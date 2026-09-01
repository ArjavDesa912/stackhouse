//! # Bring Your Own Key (BYOK) Encryption
//!
//! Customer-managed encryption keys with envelope encryption.
//! Supports key registration, rotation, and per-tenant encryption.

use crate::auth::{extract_auth_user, AuthState};
use crate::db::{SqlValue, StackhouseStore};
use crate::error::{StackhouseError, StackhouseResult};

use aes_gcm_siv::{
    aead::{Aead, KeyInit},
    Aes256GcmSiv, Nonce,
};
use axum::{
    extract::State,
    http::HeaderMap,
    response::IntoResponse,
    routing::{delete, get, post},
    Json, Router,
};
use base64::{engine::general_purpose::STANDARD, Engine as _};
use rand::{rngs::OsRng, RngCore};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::sync::Arc;
use tracing::info;

// ============================================================================
// Types
// ============================================================================

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CustomerKey {
    pub id: String,
    pub tenant_id: i64,
    pub name: String,
    pub algorithm: String,
    pub key_hash: String, // SHA-256 of the key for identification
    pub status: KeyStatus,
    pub created_at: String,
    pub rotated_at: Option<String>,
    pub expires_at: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum KeyStatus {
    Active,
    Rotating,
    Retired,
    Revoked,
}

#[derive(Debug, Clone, Serialize)]
pub struct EncryptedPayload {
    pub key_id: String,
    pub algorithm: String,
    pub nonce: String,
    pub ciphertext: String,
    pub version: u32,
}

// ============================================================================
// BYOK Service
// ============================================================================

#[derive(Clone)]
pub struct ByokService {
    store: Arc<StackhouseStore>,
    master_key: Vec<u8>, // Platform master key for wrapping customer DEKs
}

impl ByokService {
    pub async fn new(store: Arc<StackhouseStore>) -> StackhouseResult<Self> {
        let master_key = std::env::var("STACKHOUSE_MASTER_ENCRYPTION_KEY")
            .map(|k| STANDARD.decode(k).unwrap_or_else(|_| Self::generate_key()))
            .unwrap_or_else(|_| Self::generate_key());

        let service = Self { store, master_key };
        service.initialize_tables().await?;
        info!("🔐 BYOK encryption service initialized");
        Ok(service)
    }

    fn generate_key() -> Vec<u8> {
        let mut key = vec![0u8; 32];
        OsRng.fill_bytes(&mut key);
        key
    }

    async fn initialize_tables(&self) -> StackhouseResult<()> {
        self.store.execute_batch(r#"
            CREATE TABLE IF NOT EXISTS stackhouse_customer_keys (
                id TEXT PRIMARY KEY,
                tenant_id BIGINT NOT NULL,
                name TEXT NOT NULL,
                algorithm TEXT NOT NULL DEFAULT 'AES-256-GCM-SIV',
                wrapped_key TEXT NOT NULL,
                key_hash TEXT NOT NULL,
                status TEXT NOT NULL DEFAULT 'active',
                version INTEGER NOT NULL DEFAULT 1,
                created_at TIMESTAMPTZ DEFAULT NOW(),
                rotated_at TIMESTAMPTZ,
                expires_at TIMESTAMPTZ
            );
            CREATE INDEX IF NOT EXISTS idx_customer_keys_tenant ON stackhouse_customer_keys(tenant_id);
            CREATE INDEX IF NOT EXISTS idx_customer_keys_status ON stackhouse_customer_keys(status);
        "#.to_string()).await?;
        Ok(())
    }

    /// Register a customer-managed encryption key
    pub async fn register_key(
        &self,
        tenant_id: i64,
        name: &str,
        raw_key: &[u8],
    ) -> StackhouseResult<CustomerKey> {
        if raw_key.len() != 32 {
            return Err(StackhouseError::InvalidPayload(
                "Key must be exactly 32 bytes (256 bits)".into(),
            ));
        }

        let key_id = uuid::Uuid::new_v4().to_string();
        let key_hash = Self::hash_key(raw_key);
        let wrapped_key = self.wrap_key(raw_key)?;

        self.store.execute(
            "INSERT INTO stackhouse_customer_keys (id, tenant_id, name, wrapped_key, key_hash) VALUES (?, ?, ?, ?, ?)".to_string(),
            vec![
                SqlValue::Text(key_id.clone()),
                SqlValue::Integer(tenant_id),
                SqlValue::Text(name.to_string()),
                SqlValue::Text(wrapped_key),
                SqlValue::Text(key_hash.clone()),
            ],
        ).await?;

        info!(
            "🔑 Customer key registered: {} for tenant {}",
            key_id, tenant_id
        );

        Ok(CustomerKey {
            id: key_id,
            tenant_id,
            name: name.to_string(),
            algorithm: "AES-256-GCM-SIV".to_string(),
            key_hash,
            status: KeyStatus::Active,
            created_at: chrono::Utc::now().to_rfc3339(),
            rotated_at: None,
            expires_at: None,
        })
    }

    /// Rotate a customer key
    pub async fn rotate_key(
        &self,
        key_id: &str,
        new_raw_key: &[u8],
    ) -> StackhouseResult<CustomerKey> {
        if new_raw_key.len() != 32 {
            return Err(StackhouseError::InvalidPayload(
                "Key must be exactly 32 bytes".into(),
            ));
        }

        let rows = self.store.query(
            "SELECT tenant_id, name FROM stackhouse_customer_keys WHERE id = ? AND status = 'active'".to_string(),
            vec![SqlValue::Text(key_id.to_string())],
        ).await?;

        if rows.is_empty() {
            return Err(StackhouseError::NotFound(
                "Key not found or not active".into(),
            ));
        }

        let row = &rows[0];
        let tenant_id = row
            .iter()
            .find(|(k, _)| k == "tenant_id")
            .and_then(|(_, v)| v.as_i64())
            .unwrap_or(0);
        let name = row
            .iter()
            .find(|(k, _)| k == "name")
            .and_then(|(_, v)| v.as_str())
            .unwrap_or("")
            .to_string();

        // Retire old key
        self.store.execute(
            "UPDATE stackhouse_customer_keys SET status = 'retired', rotated_at = NOW() WHERE id = ?".to_string(),
            vec![SqlValue::Text(key_id.to_string())],
        ).await?;

        // Register new key
        let new_key = self.register_key(tenant_id, &name, new_raw_key).await?;
        info!("🔄 Key rotated: {} -> {}", key_id, new_key.id);

        Ok(new_key)
    }

    /// Encrypt data with a tenant's key (envelope encryption)
    pub async fn encrypt(
        &self,
        tenant_id: i64,
        plaintext: &[u8],
    ) -> StackhouseResult<EncryptedPayload> {
        let key_data = self.get_active_key(tenant_id).await?;
        let raw_key = self.unwrap_key(&key_data.wrapped_key)?;

        let cipher = Aes256GcmSiv::new_from_slice(&raw_key)
            .map_err(|e| StackhouseError::Internal(anyhow::anyhow!("Cipher init failed: {}", e)))?;

        let mut nonce_bytes = [0u8; 12];
        OsRng.fill_bytes(&mut nonce_bytes);
        let nonce = Nonce::from_slice(&nonce_bytes);

        let ciphertext = cipher
            .encrypt(nonce, plaintext)
            .map_err(|e| StackhouseError::Internal(anyhow::anyhow!("Encryption failed: {}", e)))?;

        Ok(EncryptedPayload {
            key_id: key_data.key_id,
            algorithm: "AES-256-GCM-SIV".to_string(),
            nonce: STANDARD.encode(nonce_bytes),
            ciphertext: STANDARD.encode(ciphertext),
            version: 1,
        })
    }

    /// Decrypt data
    pub async fn decrypt(&self, payload: &EncryptedPayload) -> StackhouseResult<Vec<u8>> {
        let key_data = self.get_key_by_id(&payload.key_id).await?;
        let raw_key = self.unwrap_key(&key_data.wrapped_key)?;

        let cipher = Aes256GcmSiv::new_from_slice(&raw_key)
            .map_err(|e| StackhouseError::Internal(anyhow::anyhow!("Cipher init failed: {}", e)))?;

        let nonce_bytes = STANDARD.decode(&payload.nonce).map_err(|e| {
            StackhouseError::Internal(anyhow::anyhow!("Nonce decode failed: {}", e))
        })?;
        let nonce = Nonce::from_slice(&nonce_bytes);

        let ciphertext = STANDARD.decode(&payload.ciphertext).map_err(|e| {
            StackhouseError::Internal(anyhow::anyhow!("Ciphertext decode failed: {}", e))
        })?;

        cipher
            .decrypt(nonce, ciphertext.as_slice())
            .map_err(|e| StackhouseError::Internal(anyhow::anyhow!("Decryption failed: {}", e)))
    }

    /// List keys for a tenant
    pub async fn list_keys(&self, tenant_id: i64) -> StackhouseResult<Vec<Value>> {
        let rows = self.store.query(
            "SELECT id, name, algorithm, key_hash, status, created_at, rotated_at, expires_at FROM stackhouse_customer_keys WHERE tenant_id = ? ORDER BY created_at DESC".to_string(),
            vec![SqlValue::Integer(tenant_id)],
        ).await?;
        Ok(rows
            .into_iter()
            .map(|r| json!(r.into_iter().collect::<std::collections::HashMap<_, _>>()))
            .collect())
    }

    /// Revoke a key
    pub async fn revoke_key(&self, key_id: &str) -> StackhouseResult<()> {
        self.store
            .execute(
                "UPDATE stackhouse_customer_keys SET status = 'revoked' WHERE id = ?".to_string(),
                vec![SqlValue::Text(key_id.to_string())],
            )
            .await?;
        info!("⚠️ Key revoked: {}", key_id);
        Ok(())
    }

    // Internal helpers

    fn wrap_key(&self, raw_key: &[u8]) -> StackhouseResult<String> {
        let cipher = Aes256GcmSiv::new_from_slice(&self.master_key)
            .map_err(|e| StackhouseError::Internal(anyhow::anyhow!("Master cipher init: {}", e)))?;
        let mut nonce_bytes = [0u8; 12];
        OsRng.fill_bytes(&mut nonce_bytes);
        let nonce = Nonce::from_slice(&nonce_bytes);
        let wrapped = cipher.encrypt(nonce, raw_key).map_err(|e| {
            StackhouseError::Internal(anyhow::anyhow!("Key wrapping failed: {}", e))
        })?;
        let mut output = nonce_bytes.to_vec();
        output.extend_from_slice(&wrapped);
        Ok(STANDARD.encode(output))
    }

    fn unwrap_key(&self, wrapped: &str) -> StackhouseResult<Vec<u8>> {
        let data = STANDARD
            .decode(wrapped)
            .map_err(|e| StackhouseError::Internal(anyhow::anyhow!("Wrapped key decode: {}", e)))?;
        if data.len() < 12 {
            return Err(StackhouseError::Internal(anyhow::anyhow!(
                "Invalid wrapped key"
            )));
        }
        let (nonce_bytes, ciphertext) = data.split_at(12);
        let cipher = Aes256GcmSiv::new_from_slice(&self.master_key)
            .map_err(|e| StackhouseError::Internal(anyhow::anyhow!("Master cipher init: {}", e)))?;
        let nonce = Nonce::from_slice(nonce_bytes);
        cipher
            .decrypt(nonce, ciphertext)
            .map_err(|e| StackhouseError::Internal(anyhow::anyhow!("Key unwrapping failed: {}", e)))
    }

    fn hash_key(key: &[u8]) -> String {
        use sha2::{Digest, Sha256};
        let hash = Sha256::digest(key);
        hex::encode(hash)
    }

    async fn get_active_key(&self, tenant_id: i64) -> StackhouseResult<KeyRow> {
        let rows = self.store.query(
            "SELECT id, wrapped_key FROM stackhouse_customer_keys WHERE tenant_id = ? AND status = 'active' ORDER BY created_at DESC LIMIT 1".to_string(),
            vec![SqlValue::Integer(tenant_id)],
        ).await?;
        if rows.is_empty() {
            return Err(StackhouseError::NotFound("No active key for tenant".into()));
        }
        let row = &rows[0];
        Ok(KeyRow {
            key_id: row
                .iter()
                .find(|(k, _)| k == "id")
                .and_then(|(_, v)| v.as_str())
                .unwrap_or("")
                .to_string(),
            wrapped_key: row
                .iter()
                .find(|(k, _)| k == "wrapped_key")
                .and_then(|(_, v)| v.as_str())
                .unwrap_or("")
                .to_string(),
        })
    }

    async fn get_key_by_id(&self, key_id: &str) -> StackhouseResult<KeyRow> {
        let rows = self
            .store
            .query(
                "SELECT id, wrapped_key FROM stackhouse_customer_keys WHERE id = ?".to_string(),
                vec![SqlValue::Text(key_id.to_string())],
            )
            .await?;
        if rows.is_empty() {
            return Err(StackhouseError::NotFound("Key not found".into()));
        }
        let row = &rows[0];
        Ok(KeyRow {
            key_id: row
                .iter()
                .find(|(k, _)| k == "id")
                .and_then(|(_, v)| v.as_str())
                .unwrap_or("")
                .to_string(),
            wrapped_key: row
                .iter()
                .find(|(k, _)| k == "wrapped_key")
                .and_then(|(_, v)| v.as_str())
                .unwrap_or("")
                .to_string(),
        })
    }
}

struct KeyRow {
    key_id: String,
    wrapped_key: String,
}

// ============================================================================
// Router
// ============================================================================

#[derive(Clone)]
pub struct ByokState {
    pub byok: Arc<ByokService>,
    pub auth: AuthState,
}

#[derive(Deserialize)]
struct RegisterKeyRequest {
    name: String,
    key_base64: String,
}

#[derive(Deserialize)]
struct RotateKeyRequest {
    new_key_base64: String,
}

async fn register_key_handler(
    State(state): State<ByokState>,
    headers: HeaderMap,
    Json(req): Json<RegisterKeyRequest>,
) -> Result<impl IntoResponse, StackhouseError> {
    let user = extract_auth_user(&state.auth, &headers)?;
    let raw_key = STANDARD
        .decode(&req.key_base64)
        .map_err(|_| StackhouseError::InvalidPayload("Invalid base64 key".into()))?;
    let key = state
        .byok
        .register_key(user.id, &req.name, &raw_key)
        .await?;
    Ok(Json(json!({"success": true, "data": key})))
}

async fn list_keys_handler(
    State(state): State<ByokState>,
    headers: HeaderMap,
) -> Result<impl IntoResponse, StackhouseError> {
    let user = extract_auth_user(&state.auth, &headers)?;
    let keys = state.byok.list_keys(user.id).await?;
    Ok(Json(json!({"success": true, "data": keys})))
}

async fn rotate_key_handler(
    State(state): State<ByokState>,
    headers: HeaderMap,
    axum::extract::Path(key_id): axum::extract::Path<String>,
    Json(req): Json<RotateKeyRequest>,
) -> Result<impl IntoResponse, StackhouseError> {
    let _user = extract_auth_user(&state.auth, &headers)?;
    let raw_key = STANDARD
        .decode(&req.new_key_base64)
        .map_err(|_| StackhouseError::InvalidPayload("Invalid base64 key".into()))?;
    let key = state.byok.rotate_key(&key_id, &raw_key).await?;
    Ok(Json(json!({"success": true, "data": key})))
}

async fn revoke_key_handler(
    State(state): State<ByokState>,
    headers: HeaderMap,
    axum::extract::Path(key_id): axum::extract::Path<String>,
) -> Result<impl IntoResponse, StackhouseError> {
    let _user = extract_auth_user(&state.auth, &headers)?;
    state.byok.revoke_key(&key_id).await?;
    Ok(Json(json!({"success": true, "message": "Key revoked"})))
}

pub fn create_byok_router(state: ByokState) -> Router {
    Router::new()
        .route("/keys", post(register_key_handler))
        .route("/keys", get(list_keys_handler))
        .route("/keys/:id/rotate", post(rotate_key_handler))
        .route("/keys/:id", delete(revoke_key_handler))
        .with_state(state)
}
