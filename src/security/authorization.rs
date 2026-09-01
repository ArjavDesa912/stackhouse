use crate::auth::User;
use crate::error::{StackhouseError, StackhouseResult};

use aes_gcm_siv::{
    aead::{Aead, KeyInit},
    Aes256GcmSiv, Nonce,
};
use base64::{engine::general_purpose::STANDARD, Engine as _};
use rand::{rngs::OsRng, RngCore};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::sync::OnceLock;

const DATA_ENCRYPTION_PREFIX: &str = "enc:v1:";
const DATA_ENCRYPTION_KEY_ENV: &str = "STACKHOUSE_DATA_ENCRYPTION_KEY";
const NONCE_BYTES: usize = 12;

static DATA_PROTECTOR: OnceLock<DataProtector> = OnceLock::new();

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SecurityConfig {
    #[serde(default = "default_require_service_admin_for_admin_logs")]
    pub require_service_admin_for_admin_logs: bool,
}

fn default_require_service_admin_for_admin_logs() -> bool {
    true
}

impl Default for SecurityConfig {
    fn default() -> Self {
        Self {
            require_service_admin_for_admin_logs: true,
        }
    }
}

impl SecurityConfig {
    pub fn new(require_service_admin_for_admin_logs: bool) -> Self {
        Self {
            require_service_admin_for_admin_logs,
        }
    }
}

#[derive(Clone, Debug)]
pub struct AuthorizationService {
    security_config: SecurityConfig,
    admin_emails: Vec<String>,
}

impl AuthorizationService {
    pub fn new(security_config: SecurityConfig) -> Self {
        let admin_emails = std::env::var("STACKHOUSE_ADMIN_EMAILS")
            .unwrap_or_default()
            .split(',')
            .map(|s| s.trim().to_lowercase())
            .filter(|s| !s.is_empty())
            .collect();
        Self {
            security_config,
            admin_emails,
        }
    }

    pub fn security_config(&self) -> &SecurityConfig {
        &self.security_config
    }

    pub fn require_service_admin(&self, user: &User) -> StackhouseResult<()> {
        if self.security_config.require_service_admin_for_admin_logs && !self.is_service_admin(user)
        {
            return Err(StackhouseError::Forbidden(
                "Service admin access required".to_string(),
            ));
        }

        Ok(())
    }

    pub fn require_service_admin_unconditional(&self, user: &User) -> StackhouseResult<()> {
        if !self.is_service_admin(user) {
            return Err(StackhouseError::Forbidden(
                "Service admin access required".to_string(),
            ));
        }

        Ok(())
    }

    pub fn is_service_admin(&self, user: &User) -> bool {
        user.metadata.get("service_admin").and_then(Value::as_bool) == Some(true)
            || self.admin_emails.contains(&user.email.to_lowercase())
    }
}

#[derive(Clone, Debug)]
pub struct DataProtector {
    key: [u8; 32],
}

impl DataProtector {
    pub fn new(key: [u8; 32]) -> Self {
        Self { key }
    }

    pub fn from_hex_key(hex_key: &str) -> StackhouseResult<Self> {
        let mut key = [0u8; 32];
        let decoded = hex::decode(hex_key.trim()).map_err(|e| {
            StackhouseError::Internal(anyhow::anyhow!("Invalid data encryption key: {}", e))
        })?;

        if decoded.len() != key.len() {
            return Err(StackhouseError::Internal(anyhow::anyhow!(
                "STACKHOUSE_DATA_ENCRYPTION_KEY must be a 32-byte hex value"
            )));
        }

        key.copy_from_slice(&decoded);
        Ok(Self::new(key))
    }

    pub fn from_env() -> StackhouseResult<Self> {
        let key = std::env::var(DATA_ENCRYPTION_KEY_ENV).map_err(|_| {
            StackhouseError::Internal(anyhow::anyhow!(
                "Missing STACKHOUSE_DATA_ENCRYPTION_KEY environment variable"
            ))
        })?;
        Self::from_hex_key(&key)
    }

    pub fn encrypt_string(&self, plaintext: &str) -> StackhouseResult<String> {
        let encrypted = self.encrypt_bytes(plaintext.as_bytes())?;
        String::from_utf8(encrypted).map_err(|e| {
            StackhouseError::Internal(anyhow::anyhow!("Encrypted output was not UTF-8: {}", e))
        })
    }

    pub fn decrypt_string(&self, encrypted: &str) -> StackhouseResult<String> {
        let decrypted = self.decrypt_bytes(encrypted.as_bytes())?;
        String::from_utf8(decrypted).map_err(|e| {
            StackhouseError::Internal(anyhow::anyhow!("Decrypted output was not UTF-8: {}", e))
        })
    }

    pub fn encrypt_bytes(&self, plaintext: &[u8]) -> StackhouseResult<Vec<u8>> {
        let cipher = self.cipher()?;
        let mut nonce = [0u8; NONCE_BYTES];
        OsRng.fill_bytes(&mut nonce);

        let ciphertext = cipher
            .encrypt(Nonce::from_slice(&nonce), plaintext)
            .map_err(|e| {
                StackhouseError::Internal(anyhow::anyhow!("Data encryption failed: {}", e))
            })?;

        let mut encoded =
            Vec::with_capacity(DATA_ENCRYPTION_PREFIX.len() + NONCE_BYTES + ciphertext.len());
        encoded.extend_from_slice(DATA_ENCRYPTION_PREFIX.as_bytes());
        encoded.extend_from_slice(
            STANDARD
                .encode([nonce.as_slice(), ciphertext.as_slice()].concat())
                .as_bytes(),
        );
        Ok(encoded)
    }

    pub fn decrypt_bytes(&self, encrypted: &[u8]) -> StackhouseResult<Vec<u8>> {
        if !encrypted.starts_with(DATA_ENCRYPTION_PREFIX.as_bytes()) {
            return Ok(encrypted.to_vec());
        }

        let payload = &encrypted[DATA_ENCRYPTION_PREFIX.len()..];
        let decoded = STANDARD.decode(payload).map_err(|e| {
            StackhouseError::Internal(anyhow::anyhow!("Invalid encrypted payload: {}", e))
        })?;

        if decoded.len() < NONCE_BYTES {
            return Err(StackhouseError::Internal(anyhow::anyhow!(
                "Encrypted payload is too short"
            )));
        }

        let (nonce, ciphertext) = decoded.split_at(NONCE_BYTES);
        let cipher = self.cipher()?;
        cipher
            .decrypt(Nonce::from_slice(nonce), ciphertext)
            .map_err(|e| {
                StackhouseError::Internal(anyhow::anyhow!("Data decryption failed: {}", e))
            })
    }

    fn cipher(&self) -> StackhouseResult<Aes256GcmSiv> {
        Aes256GcmSiv::new_from_slice(&self.key).map_err(|e| {
            StackhouseError::Internal(anyhow::anyhow!("Data cipher init failed: {}", e))
        })
    }
}

pub fn init_data_protector_from_env() -> StackhouseResult<&'static DataProtector> {
    if let Some(protector) = DATA_PROTECTOR.get() {
        return Ok(protector);
    }

    let protector = DataProtector::from_env()?;
    DATA_PROTECTOR
        .set(protector)
        .map_err(|_| StackhouseError::Conflict("Data protector already initialized".to_string()))?;

    DATA_PROTECTOR.get().ok_or_else(|| {
        StackhouseError::Internal(anyhow::anyhow!("Data protector initialization failed"))
    })
}

pub fn data_protector() -> StackhouseResult<&'static DataProtector> {
    if let Some(protector) = DATA_PROTECTOR.get() {
        return Ok(protector);
    }

    init_data_protector_from_env()
}
