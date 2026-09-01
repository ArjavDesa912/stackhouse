//! # Function-Level Secrets Vault
//!
//! Encrypted per-function secrets with rotation support and access audit.

use crate::auth::{extract_auth_user, AuthState};
use crate::db::{SqlValue, StackhouseStore};
use crate::error::{StackhouseError, StackhouseResult};

use aes_gcm_siv::aead::{Aead, KeyInit};
use aes_gcm_siv::{Aes256GcmSiv, Key, Nonce};
use axum::{
    extract::State,
    http::HeaderMap,
    response::IntoResponse,
    routing::{delete, get, post},
    Json, Router,
};
use base64::{engine::general_purpose::STANDARD, Engine as _};
use rand::RngCore;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::sync::Arc;
use tracing::info;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FunctionSecret {
    pub id: String,
    pub tenant_id: i64,
    pub name: String,
    pub function_id: Option<String>, // None = available to all functions
    pub version: u32,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Clone)]
pub struct SecretsVault {
    store: Arc<StackhouseStore>,
    encryption_key: [u8; 32],
}

impl SecretsVault {
    pub async fn new(store: Arc<StackhouseStore>) -> StackhouseResult<Self> {
        let key_str = std::env::var("STACKHOUSE_SECRETS_KEY")
            .unwrap_or_else(|_| "0123456789abcdef0123456789abcdef".into());
        let mut encryption_key = [0u8; 32];
        let key_bytes = key_str.as_bytes();
        for i in 0..32.min(key_bytes.len()) {
            encryption_key[i] = key_bytes[i];
        }

        let vault = Self {
            store,
            encryption_key,
        };
        vault.initialize_tables().await?;
        info!("🔑 Secrets vault initialized");
        Ok(vault)
    }

    async fn initialize_tables(&self) -> StackhouseResult<()> {
        self.store.execute_batch(r#"
            CREATE TABLE IF NOT EXISTS stackhouse_function_secrets (
                id TEXT PRIMARY KEY,
                tenant_id BIGINT NOT NULL,
                name TEXT NOT NULL,
                function_id TEXT,
                encrypted_value TEXT NOT NULL,
                nonce TEXT NOT NULL,
                version INTEGER DEFAULT 1,
                created_at TIMESTAMPTZ DEFAULT NOW(),
                updated_at TIMESTAMPTZ DEFAULT NOW(),
                UNIQUE(tenant_id, name, function_id)
            );
            CREATE TABLE IF NOT EXISTS stackhouse_secret_access_log (
                id BIGSERIAL PRIMARY KEY,
                secret_id TEXT NOT NULL,
                function_id TEXT,
                action TEXT NOT NULL,
                timestamp TIMESTAMPTZ DEFAULT NOW()
            );
            CREATE INDEX IF NOT EXISTS idx_secrets_tenant ON stackhouse_function_secrets(tenant_id);
            CREATE INDEX IF NOT EXISTS idx_secrets_function ON stackhouse_function_secrets(function_id);
        "#.to_string()).await?;
        Ok(())
    }

    fn encrypt(&self, plaintext: &str) -> StackhouseResult<(String, String)> {
        let key = Key::<Aes256GcmSiv>::from_slice(&self.encryption_key);
        let cipher = Aes256GcmSiv::new(key);

        let mut nonce_bytes = [0u8; 12];
        rand::rngs::OsRng.fill_bytes(&mut nonce_bytes);
        let nonce = Nonce::from_slice(&nonce_bytes);

        let ciphertext = cipher
            .encrypt(nonce, plaintext.as_bytes())
            .map_err(|e| StackhouseError::Internal(anyhow::anyhow!("Encryption failed: {}", e)))?;

        Ok((STANDARD.encode(&ciphertext), STANDARD.encode(&nonce_bytes)))
    }

    fn decrypt(&self, ciphertext_b64: &str, nonce_b64: &str) -> StackhouseResult<String> {
        let key = Key::<Aes256GcmSiv>::from_slice(&self.encryption_key);
        let cipher = Aes256GcmSiv::new(key);

        let ciphertext = STANDARD.decode(ciphertext_b64).map_err(|_| {
            StackhouseError::Internal(anyhow::anyhow!("Invalid ciphertext encoding"))
        })?;
        let nonce_bytes = STANDARD
            .decode(nonce_b64)
            .map_err(|_| StackhouseError::Internal(anyhow::anyhow!("Invalid nonce encoding")))?;
        let nonce = Nonce::from_slice(&nonce_bytes);

        let plaintext = cipher
            .decrypt(nonce, ciphertext.as_ref())
            .map_err(|e| StackhouseError::Internal(anyhow::anyhow!("Decryption failed: {}", e)))?;

        String::from_utf8(plaintext).map_err(|_| {
            StackhouseError::Internal(anyhow::anyhow!("Decrypted value not valid UTF-8"))
        })
    }

    /// Store a secret
    pub async fn set_secret(
        &self,
        tenant_id: i64,
        name: &str,
        value: &str,
        function_id: Option<&str>,
    ) -> StackhouseResult<FunctionSecret> {
        let (encrypted, nonce) = self.encrypt(value)?;
        let id = uuid::Uuid::new_v4().to_string();
        let func_id = function_id.unwrap_or("");

        self.store.execute(
            r#"INSERT INTO stackhouse_function_secrets (id, tenant_id, name, function_id, encrypted_value, nonce)
               VALUES (?, ?, ?, ?, ?, ?)
               ON CONFLICT (tenant_id, name, function_id) DO UPDATE SET
               encrypted_value = EXCLUDED.encrypted_value, nonce = EXCLUDED.nonce,
               version = stackhouse_function_secrets.version + 1, updated_at = NOW()"#.to_string(),
            vec![
                SqlValue::Text(id.clone()),
                SqlValue::Integer(tenant_id),
                SqlValue::Text(name.to_string()),
                SqlValue::Text(func_id.to_string()),
                SqlValue::Text(encrypted),
                SqlValue::Text(nonce),
            ],
        ).await?;

        Ok(FunctionSecret {
            id,
            tenant_id,
            name: name.to_string(),
            function_id: function_id.map(String::from),
            version: 1,
            created_at: chrono::Utc::now().to_rfc3339(),
            updated_at: chrono::Utc::now().to_rfc3339(),
        })
    }

    /// Get a secret value (decrypted)
    pub async fn get_secret(
        &self,
        tenant_id: i64,
        name: &str,
        function_id: Option<&str>,
    ) -> StackhouseResult<String> {
        let func_id = function_id.unwrap_or("");
        let rows = self.store.query(
            "SELECT id, encrypted_value, nonce FROM stackhouse_function_secrets WHERE tenant_id = ? AND name = ? AND (function_id = ? OR function_id = '')".to_string(),
            vec![
                SqlValue::Integer(tenant_id),
                SqlValue::Text(name.to_string()),
                SqlValue::Text(func_id.to_string()),
            ],
        ).await?;

        if rows.is_empty() {
            return Err(StackhouseError::NotFound(format!(
                "Secret '{}' not found",
                name
            )));
        }

        let row = &rows[0];
        let secret_id = row
            .iter()
            .find(|(k, _)| k == "id")
            .and_then(|(_, v)| v.as_str())
            .unwrap_or("");
        let encrypted = row
            .iter()
            .find(|(k, _)| k == "encrypted_value")
            .and_then(|(_, v)| v.as_str())
            .unwrap_or("");
        let nonce = row
            .iter()
            .find(|(k, _)| k == "nonce")
            .and_then(|(_, v)| v.as_str())
            .unwrap_or("");

        // Audit log
        self.store.execute(
            "INSERT INTO stackhouse_secret_access_log (secret_id, function_id, action) VALUES (?, ?, 'read')".to_string(),
            vec![SqlValue::Text(secret_id.to_string()), SqlValue::Text(func_id.to_string())],
        ).await.ok();

        self.decrypt(encrypted, nonce)
    }

    /// List secrets (names only, no values)
    pub async fn list_secrets(
        &self,
        tenant_id: i64,
        function_id: Option<&str>,
    ) -> StackhouseResult<Vec<Value>> {
        let query = if let Some(fid) = function_id {
            self.store.query(
                "SELECT id, name, function_id, version, created_at, updated_at FROM stackhouse_function_secrets WHERE tenant_id = ? AND (function_id = ? OR function_id = '') ORDER BY name".to_string(),
                vec![SqlValue::Integer(tenant_id), SqlValue::Text(fid.to_string())],
            ).await?
        } else {
            self.store.query(
                "SELECT id, name, function_id, version, created_at, updated_at FROM stackhouse_function_secrets WHERE tenant_id = ? ORDER BY name".to_string(),
                vec![SqlValue::Integer(tenant_id)],
            ).await?
        };
        Ok(query
            .into_iter()
            .map(|r| json!(r.into_iter().collect::<std::collections::HashMap<_, _>>()))
            .collect())
    }

    /// Delete a secret
    pub async fn delete_secret(
        &self,
        tenant_id: i64,
        name: &str,
        function_id: Option<&str>,
    ) -> StackhouseResult<()> {
        let func_id = function_id.unwrap_or("");
        self.store.execute(
            "DELETE FROM stackhouse_function_secrets WHERE tenant_id = ? AND name = ? AND function_id = ?".to_string(),
            vec![
                SqlValue::Integer(tenant_id),
                SqlValue::Text(name.to_string()),
                SqlValue::Text(func_id.to_string()),
            ],
        ).await?;
        Ok(())
    }
}

// ============================================================================
// Router
// ============================================================================

#[derive(Clone)]
pub struct SecretsState {
    pub vault: Arc<SecretsVault>,
    pub auth: AuthState,
}

#[derive(Deserialize)]
struct SetSecretRequest {
    name: String,
    value: String,
    #[serde(default)]
    function_id: Option<String>,
}

#[derive(Deserialize)]
struct GetSecretQuery {
    name: String,
    #[serde(default)]
    function_id: Option<String>,
}

async fn set_secret_handler(
    State(state): State<SecretsState>,
    headers: HeaderMap,
    Json(req): Json<SetSecretRequest>,
) -> Result<impl IntoResponse, StackhouseError> {
    let user = extract_auth_user(&state.auth, &headers)?;
    let secret = state
        .vault
        .set_secret(user.id, &req.name, &req.value, req.function_id.as_deref())
        .await?;
    Ok(Json(json!({"success": true, "data": secret})))
}

async fn list_secrets_handler(
    State(state): State<SecretsState>,
    headers: HeaderMap,
) -> Result<impl IntoResponse, StackhouseError> {
    let user = extract_auth_user(&state.auth, &headers)?;
    let secrets = state.vault.list_secrets(user.id, None).await?;
    Ok(Json(json!({"success": true, "data": secrets})))
}

async fn delete_secret_handler(
    State(state): State<SecretsState>,
    headers: HeaderMap,
    axum::extract::Path(name): axum::extract::Path<String>,
) -> Result<impl IntoResponse, StackhouseError> {
    let user = extract_auth_user(&state.auth, &headers)?;
    state.vault.delete_secret(user.id, &name, None).await?;
    Ok(Json(json!({"success": true, "message": "Secret deleted"})))
}

pub fn create_secrets_router(state: SecretsState) -> Router {
    Router::new()
        .route("/", post(set_secret_handler))
        .route("/", get(list_secrets_handler))
        .route("/:name", delete(delete_secret_handler))
        .with_state(state)
}
