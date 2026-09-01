//! # API Key Management
//!
//! Generate, revoke, and manage API keys with scopes, expiry, and rotation.
//! Keys are stored as SHA-256 hashes (never plaintext).

use crate::auth::{extract_auth_user, AuthState};
use crate::db::{SqlValue, StackhouseStore};
use crate::error::{StackhouseError, StackhouseResult};

use axum::{
    extract::State,
    http::HeaderMap,
    response::IntoResponse,
    routing::{delete, get, post},
    Json, Router,
};
use rand::Rng;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::sync::Arc;
use tracing::info;

// ============================================================================
// Types
// ============================================================================

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApiKey {
    pub id: String,
    pub user_id: i64,
    pub name: String,
    pub prefix: String, // First 8 chars for identification
    pub key_hash: String,
    pub scopes: Vec<String>,
    pub expires_at: Option<String>,
    pub last_used_at: Option<String>,
    pub created_at: String,
    pub revoked: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApiKeyCreated {
    pub id: String,
    pub name: String,
    pub key: String, // Only returned once at creation
    pub prefix: String,
    pub scopes: Vec<String>,
    pub expires_at: Option<String>,
    pub created_at: String,
}

/// Available API key scopes
pub const VALID_SCOPES: &[&str] = &[
    "data:read",
    "data:write",
    "data:delete",
    "storage:read",
    "storage:write",
    "storage:delete",
    "auth:read",
    "auth:manage",
    "teams:read",
    "teams:manage",
    "admin:read",
    "admin:manage",
    "billing:read",
    "billing:manage",
    "vectors:read",
    "vectors:write",
    "functions:invoke",
    "functions:manage",
    "realtime:subscribe",
    "brain:query",
    "brain:manage",
    "mcp:write",
    "*", // Full access
];

// ============================================================================
// Service
// ============================================================================

#[derive(Clone)]
pub struct ApiKeyService {
    store: Arc<StackhouseStore>,
}

impl ApiKeyService {
    pub async fn new(store: Arc<StackhouseStore>) -> StackhouseResult<Self> {
        let service = Self { store };
        service.initialize_tables().await?;
        info!("🔑 API Key management initialized");
        Ok(service)
    }

    async fn initialize_tables(&self) -> StackhouseResult<()> {
        self.store
            .execute_batch(
                r#"
            CREATE TABLE IF NOT EXISTS stackhouse_api_keys (
                id TEXT PRIMARY KEY,
                user_id BIGINT NOT NULL REFERENCES stackhouse_users(id) ON DELETE CASCADE,
                name TEXT NOT NULL,
                prefix TEXT NOT NULL,
                key_hash TEXT NOT NULL UNIQUE,
                scopes TEXT NOT NULL DEFAULT '[]',
                expires_at TIMESTAMPTZ,
                last_used_at TIMESTAMPTZ,
                created_at TIMESTAMPTZ DEFAULT NOW(),
                revoked BOOLEAN DEFAULT FALSE
            );
            CREATE INDEX IF NOT EXISTS idx_api_keys_user ON stackhouse_api_keys(user_id);
            CREATE INDEX IF NOT EXISTS idx_api_keys_hash ON stackhouse_api_keys(key_hash);
            CREATE INDEX IF NOT EXISTS idx_api_keys_prefix ON stackhouse_api_keys(prefix);
        "#
                .to_string(),
            )
            .await?;
        Ok(())
    }

    /// Generate a new API key
    pub async fn create_key(
        &self,
        user_id: i64,
        name: &str,
        scopes: Vec<String>,
        expires_in_days: Option<u32>,
    ) -> StackhouseResult<ApiKeyCreated> {
        // Validate scopes
        for scope in &scopes {
            if !VALID_SCOPES.contains(&scope.as_str()) {
                return Err(StackhouseError::InvalidPayload(format!(
                    "Invalid scope: {}",
                    scope
                )));
            }
        }

        let id = uuid::Uuid::new_v4().to_string();
        let raw_key = self.generate_raw_key();
        let prefix = &raw_key[..8];
        let key_hash = Self::hash_key(&raw_key);

        let expires_at = expires_in_days
            .map(|days| (chrono::Utc::now() + chrono::Duration::days(days as i64)).to_rfc3339());

        self.store.execute(
            "INSERT INTO stackhouse_api_keys (id, user_id, name, prefix, key_hash, scopes, expires_at) VALUES (?, ?, ?, ?, ?, ?, ?::timestamptz)".to_string(),
            vec![
                SqlValue::Text(id.clone()),
                SqlValue::Integer(user_id),
                SqlValue::Text(name.to_string()),
                SqlValue::Text(prefix.to_string()),
                SqlValue::Text(key_hash),
                SqlValue::Text(serde_json::to_string(&scopes).unwrap_or_default()),
                SqlValue::Text(expires_at.clone().unwrap_or_default()),
            ],
        ).await?;

        info!("🔑 API key created: {} (prefix: {})", name, prefix);

        Ok(ApiKeyCreated {
            id,
            name: name.to_string(),
            key: format!("vdb_{}", raw_key),
            prefix: prefix.to_string(),
            scopes,
            expires_at,
            created_at: chrono::Utc::now().to_rfc3339(),
        })
    }

    /// Validate an API key and return user_id + scopes
    pub async fn validate_key(&self, raw_key: &str) -> StackhouseResult<(i64, Vec<String>)> {
        let key = raw_key.strip_prefix("vdb_").unwrap_or(raw_key);
        let key_hash = Self::hash_key(key);

        let rows = self.store.query(
            "SELECT user_id, scopes, expires_at, revoked FROM stackhouse_api_keys WHERE key_hash = ?".to_string(),
            vec![SqlValue::Text(key_hash.clone())],
        ).await?;

        if rows.is_empty() {
            return Err(StackhouseError::Unauthorized("Invalid API key".into()));
        }

        let row = &rows[0];
        let revoked = row
            .iter()
            .find(|(k, _)| k == "revoked")
            .and_then(|(_, v)| v.as_str())
            .unwrap_or("false")
            == "true";
        if revoked {
            return Err(StackhouseError::Unauthorized(
                "API key has been revoked".into(),
            ));
        }

        // Check expiry
        if let Some(expires) = row
            .iter()
            .find(|(k, _)| k == "expires_at")
            .and_then(|(_, v)| v.as_str())
        {
            if !expires.is_empty() {
                if let Ok(exp_time) = chrono::DateTime::parse_from_rfc3339(expires) {
                    if exp_time < chrono::Utc::now() {
                        return Err(StackhouseError::Unauthorized("API key has expired".into()));
                    }
                }
            }
        }

        let user_id = row
            .iter()
            .find(|(k, _)| k == "user_id")
            .and_then(|(_, v)| v.as_i64())
            .ok_or_else(|| StackhouseError::Internal(anyhow::anyhow!("Missing user_id")))?;

        let scopes: Vec<String> = row
            .iter()
            .find(|(k, _)| k == "scopes")
            .and_then(|(_, v)| v.as_str())
            .and_then(|s| serde_json::from_str(s).ok())
            .unwrap_or_default();

        // Update last_used_at
        self.store
            .execute(
                "UPDATE stackhouse_api_keys SET last_used_at = NOW() WHERE key_hash = ?"
                    .to_string(),
                vec![SqlValue::Text(key_hash)],
            )
            .await
            .ok();

        Ok((user_id, scopes))
    }

    /// Check if an API key has a required scope
    pub fn has_scope(scopes: &[String], required: &str) -> bool {
        scopes.iter().any(|s| {
            s == "*" || s == required || {
                // Check wildcard: "data:*" matches "data:read"
                if let Some(prefix) = s.strip_suffix(":*") {
                    required.starts_with(prefix)
                } else {
                    false
                }
            }
        })
    }

    /// List API keys for a user (without hashes)
    pub async fn list_keys(&self, user_id: i64) -> StackhouseResult<Vec<Value>> {
        let rows = self.store.query(
            "SELECT id, name, prefix, scopes, expires_at, last_used_at, created_at, revoked FROM stackhouse_api_keys WHERE user_id = ? ORDER BY created_at DESC".to_string(),
            vec![SqlValue::Integer(user_id)],
        ).await?;
        Ok(rows
            .into_iter()
            .map(|r| json!(r.into_iter().collect::<std::collections::HashMap<_, _>>()))
            .collect())
    }

    /// Revoke an API key
    pub async fn revoke_key(&self, key_id: &str, user_id: i64) -> StackhouseResult<()> {
        self.store
            .execute(
                "UPDATE stackhouse_api_keys SET revoked = TRUE WHERE id = ? AND user_id = ?"
                    .to_string(),
                vec![
                    SqlValue::Text(key_id.to_string()),
                    SqlValue::Integer(user_id),
                ],
            )
            .await?;
        info!("🔑 API key revoked: {}", key_id);
        Ok(())
    }

    /// Rotate an API key (revoke old, create new with same name/scopes)
    pub async fn rotate_key(&self, key_id: &str, user_id: i64) -> StackhouseResult<ApiKeyCreated> {
        let rows = self.store.query(
            "SELECT name, scopes, expires_at FROM stackhouse_api_keys WHERE id = ? AND user_id = ?".to_string(),
            vec![SqlValue::Text(key_id.to_string()), SqlValue::Integer(user_id)],
        ).await?;

        if rows.is_empty() {
            return Err(StackhouseError::NotFound("API key not found".into()));
        }

        let row = &rows[0];
        let name = row
            .iter()
            .find(|(k, _)| k == "name")
            .and_then(|(_, v)| v.as_str())
            .unwrap_or("")
            .to_string();
        let scopes: Vec<String> = row
            .iter()
            .find(|(k, _)| k == "scopes")
            .and_then(|(_, v)| v.as_str())
            .and_then(|s| serde_json::from_str(s).ok())
            .unwrap_or_default();

        // Revoke old key
        self.revoke_key(key_id, user_id).await?;

        // Create new key
        self.create_key(user_id, &name, scopes, None).await
    }

    fn generate_raw_key(&self) -> String {
        use base64::Engine;
        let mut bytes = [0u8; 32];
        rand::thread_rng().fill(&mut bytes);
        base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(bytes)
    }

    fn hash_key(key: &str) -> String {
        let hash = Sha256::digest(key.as_bytes());
        hex::encode(hash)
    }
}

// ============================================================================
// Router
// ============================================================================

#[derive(Clone)]
pub struct ApiKeyState {
    pub api_keys: Arc<ApiKeyService>,
    pub auth: AuthState,
}

#[derive(Deserialize)]
struct CreateKeyRequest {
    name: String,
    #[serde(default)]
    scopes: Vec<String>,
    #[serde(default)]
    expires_in_days: Option<u32>,
}

async fn create_key_handler(
    State(state): State<ApiKeyState>,
    headers: HeaderMap,
    Json(req): Json<CreateKeyRequest>,
) -> Result<impl IntoResponse, StackhouseError> {
    let user = extract_auth_user(&state.auth, &headers)?;
    let scopes = if req.scopes.is_empty() {
        vec!["*".to_string()]
    } else {
        req.scopes
    };
    let key = state
        .api_keys
        .create_key(user.id, &req.name, scopes, req.expires_in_days)
        .await?;
    Ok((
        axum::http::StatusCode::CREATED,
        Json(json!({"success": true, "data": key})),
    ))
}

async fn list_keys_handler(
    State(state): State<ApiKeyState>,
    headers: HeaderMap,
) -> Result<impl IntoResponse, StackhouseError> {
    let user = extract_auth_user(&state.auth, &headers)?;
    let keys = state.api_keys.list_keys(user.id).await?;
    Ok(Json(json!({"success": true, "data": keys})))
}

async fn revoke_key_handler(
    State(state): State<ApiKeyState>,
    headers: HeaderMap,
    axum::extract::Path(key_id): axum::extract::Path<String>,
) -> Result<impl IntoResponse, StackhouseError> {
    let user = extract_auth_user(&state.auth, &headers)?;
    state.api_keys.revoke_key(&key_id, user.id).await?;
    Ok(Json(json!({"success": true, "message": "Key revoked"})))
}

async fn rotate_key_handler(
    State(state): State<ApiKeyState>,
    headers: HeaderMap,
    axum::extract::Path(key_id): axum::extract::Path<String>,
) -> Result<impl IntoResponse, StackhouseError> {
    let user = extract_auth_user(&state.auth, &headers)?;
    let key = state.api_keys.rotate_key(&key_id, user.id).await?;
    Ok(Json(json!({"success": true, "data": key})))
}

async fn scopes_handler() -> impl IntoResponse {
    Json(json!({"success": true, "data": VALID_SCOPES}))
}

pub fn create_api_key_router(state: ApiKeyState) -> Router {
    Router::new()
        .route("/keys", post(create_key_handler))
        .route("/keys", get(list_keys_handler))
        .route("/keys/:id", delete(revoke_key_handler))
        .route("/keys/:id/rotate", post(rotate_key_handler))
        .route("/keys/scopes", get(scopes_handler))
        .with_state(state)
}
