//! # Impersonation / Support Login
//!
//! Admin impersonation endpoint with full immutable audit trail.
//! Time-limited impersonation tokens that cannot be confused with real user tokens.

use crate::auth::{extract_auth_user, AuthService, AuthState};
use crate::db::{SqlValue, StackhouseStore};
use crate::error::{StackhouseError, StackhouseResult};

use axum::{
    extract::State,
    http::HeaderMap,
    response::IntoResponse,
    routing::{get, post},
    Json, Router,
};
use jsonwebtoken::{encode, EncodingKey, Header};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use tracing::{info, warn};

// ============================================================================
// Types
// ============================================================================

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ImpersonationSession {
    pub id: String,
    pub admin_user_id: i64,
    pub admin_email: String,
    pub target_user_id: i64,
    pub target_email: String,
    pub reason: String,
    pub token: String,
    pub expires_at: String,
    pub created_at: String,
    pub ended_at: Option<String>,
    pub active: bool,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ImpersonationClaims {
    pub sub: i64,      // Target user ID
    pub email: String, // Target email
    pub exp: u64,
    pub iat: u64,
    pub impersonator_id: i64,
    pub impersonator_email: String,
    pub session_id: String,
    pub is_impersonation: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuditEntry {
    pub id: String,
    pub session_id: String,
    pub admin_user_id: i64,
    pub target_user_id: i64,
    pub action: String,
    pub details: Value,
    pub ip_address: Option<String>,
    pub user_agent: Option<String>,
    pub timestamp: String,
}

// ============================================================================
// Impersonation Service
// ============================================================================

const MAX_IMPERSONATION_DURATION: Duration = Duration::from_secs(3600); // 1 hour max

#[derive(Clone)]
pub struct ImpersonationService {
    store: Arc<StackhouseStore>,
    auth: AuthService,
    jwt_secret: Vec<u8>,
    allowed_admin_emails: Vec<String>,
}

impl ImpersonationService {
    pub async fn new(
        store: Arc<StackhouseStore>,
        auth: AuthService,
        jwt_secret: Vec<u8>,
    ) -> StackhouseResult<Self> {
        let allowed_admin_emails = std::env::var("STACKHOUSE_IMPERSONATION_ADMINS")
            .unwrap_or_else(|_| std::env::var("STACKHOUSE_ADMIN_EMAILS").unwrap_or_default())
            .split(',')
            .map(|s| s.trim().to_lowercase())
            .filter(|s| !s.is_empty())
            .collect();

        let service = Self {
            store,
            auth,
            jwt_secret,
            allowed_admin_emails,
        };
        service.initialize_tables().await?;
        info!(
            "👤 Impersonation service initialized ({} authorized admins)",
            service.allowed_admin_emails.len()
        );
        Ok(service)
    }

    async fn initialize_tables(&self) -> StackhouseResult<()> {
        self.store.execute_batch(r#"
            CREATE TABLE IF NOT EXISTS stackhouse_impersonation_sessions (
                id TEXT PRIMARY KEY,
                admin_user_id BIGINT NOT NULL,
                admin_email TEXT NOT NULL,
                target_user_id BIGINT NOT NULL,
                target_email TEXT NOT NULL,
                reason TEXT NOT NULL,
                expires_at TIMESTAMPTZ NOT NULL,
                created_at TIMESTAMPTZ DEFAULT NOW(),
                ended_at TIMESTAMPTZ,
                active BOOLEAN DEFAULT TRUE
            );
            CREATE TABLE IF NOT EXISTS stackhouse_impersonation_audit (
                id TEXT PRIMARY KEY,
                session_id TEXT NOT NULL,
                admin_user_id BIGINT NOT NULL,
                target_user_id BIGINT NOT NULL,
                action TEXT NOT NULL,
                details JSONB DEFAULT '{}',
                ip_address TEXT,
                user_agent TEXT,
                timestamp TIMESTAMPTZ DEFAULT NOW()
            );
            CREATE INDEX IF NOT EXISTS idx_impersonation_sessions_admin ON stackhouse_impersonation_sessions(admin_user_id);
            CREATE INDEX IF NOT EXISTS idx_impersonation_audit_session ON stackhouse_impersonation_audit(session_id);
            CREATE INDEX IF NOT EXISTS idx_impersonation_audit_time ON stackhouse_impersonation_audit(timestamp);
        "#.to_string()).await?;
        Ok(())
    }

    /// Start an impersonation session
    pub async fn start_impersonation(
        &self,
        admin_id: i64,
        admin_email: &str,
        target_user_id: i64,
        reason: &str,
        ip: Option<&str>,
        ua: Option<&str>,
    ) -> StackhouseResult<ImpersonationSession> {
        // Check authorization
        if !self
            .allowed_admin_emails
            .contains(&admin_email.to_lowercase())
        {
            warn!("⚠️ Unauthorized impersonation attempt by: {}", admin_email);
            return Err(StackhouseError::Unauthorized(
                "Not authorized to impersonate users".into(),
            ));
        }

        if reason.len() < 10 {
            return Err(StackhouseError::InvalidPayload(
                "Reason must be at least 10 characters".into(),
            ));
        }

        // Cannot impersonate yourself
        if admin_id == target_user_id {
            return Err(StackhouseError::InvalidPayload(
                "Cannot impersonate yourself".into(),
            ));
        }

        // Get target user
        let target_user = self.auth.get_user_by_id(target_user_id).await?;

        let session_id = uuid::Uuid::new_v4().to_string();
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(|e| StackhouseError::Internal(anyhow::anyhow!("Time error: {}", e)))?;
        let expires_at = now + MAX_IMPERSONATION_DURATION;

        // Generate impersonation token
        let claims = ImpersonationClaims {
            sub: target_user_id,
            email: target_user.email.clone(),
            iat: now.as_secs(),
            exp: expires_at.as_secs(),
            impersonator_id: admin_id,
            impersonator_email: admin_email.to_string(),
            session_id: session_id.clone(),
            is_impersonation: true,
        };

        let token = encode(
            &Header::default(),
            &claims,
            &EncodingKey::from_secret(&self.jwt_secret),
        )
        .map_err(|e| {
            StackhouseError::Internal(anyhow::anyhow!("Token generation failed: {}", e))
        })?;

        let expires_str = chrono::DateTime::from_timestamp(expires_at.as_secs() as i64, 0)
            .map(|dt| dt.to_rfc3339())
            .unwrap_or_default();

        // Store session
        self.store.execute(
            "INSERT INTO stackhouse_impersonation_sessions (id, admin_user_id, admin_email, target_user_id, target_email, reason, expires_at) VALUES (?, ?, ?, ?, ?, ?, ?::timestamptz)".to_string(),
            vec![
                SqlValue::Text(session_id.clone()),
                SqlValue::Integer(admin_id),
                SqlValue::Text(admin_email.to_string()),
                SqlValue::Integer(target_user_id),
                SqlValue::Text(target_user.email.clone()),
                SqlValue::Text(reason.to_string()),
                SqlValue::Text(expires_str.clone()),
            ],
        ).await?;

        // Audit log
        self.log_action(
            &session_id,
            admin_id,
            target_user_id,
            "impersonation_started",
            json!({
                "reason": reason,
                "expires_at": expires_str,
            }),
            ip,
            ua,
        )
        .await?;

        info!(
            "👤 Impersonation started: {} -> {} (reason: {})",
            admin_email, target_user.email, reason
        );

        Ok(ImpersonationSession {
            id: session_id,
            admin_user_id: admin_id,
            admin_email: admin_email.to_string(),
            target_user_id,
            target_email: target_user.email,
            reason: reason.to_string(),
            token,
            expires_at: expires_str,
            created_at: chrono::Utc::now().to_rfc3339(),
            ended_at: None,
            active: true,
        })
    }

    /// End an impersonation session
    pub async fn end_impersonation(
        &self,
        session_id: &str,
        admin_id: i64,
        ip: Option<&str>,
        ua: Option<&str>,
    ) -> StackhouseResult<()> {
        let rows = self.store.query(
            "SELECT target_user_id FROM stackhouse_impersonation_sessions WHERE id = ? AND admin_user_id = ? AND active = true".to_string(),
            vec![SqlValue::Text(session_id.to_string()), SqlValue::Integer(admin_id)],
        ).await?;

        if rows.is_empty() {
            return Err(StackhouseError::NotFound(
                "Active impersonation session not found".into(),
            ));
        }

        let target_id = rows[0]
            .iter()
            .find(|(k, _)| k == "target_user_id")
            .and_then(|(_, v)| v.as_i64())
            .unwrap_or(0);

        self.store.execute(
            "UPDATE stackhouse_impersonation_sessions SET active = FALSE, ended_at = NOW() WHERE id = ?".to_string(),
            vec![SqlValue::Text(session_id.to_string())],
        ).await?;

        self.log_action(
            session_id,
            admin_id,
            target_id,
            "impersonation_ended",
            json!({}),
            ip,
            ua,
        )
        .await?;
        info!("👤 Impersonation ended: session {}", session_id);
        Ok(())
    }

    /// List impersonation sessions (for audit)
    pub async fn list_sessions(&self, limit: usize) -> StackhouseResult<Vec<Value>> {
        let rows = self.store.query(
            format!("SELECT id, admin_email, target_email, reason, created_at, ended_at, active FROM stackhouse_impersonation_sessions ORDER BY created_at DESC LIMIT {}", limit),
            vec![],
        ).await?;
        Ok(rows
            .into_iter()
            .map(|r| json!(r.into_iter().collect::<std::collections::HashMap<_, _>>()))
            .collect())
    }

    /// Get audit trail for a session
    pub async fn get_audit_trail(&self, session_id: &str) -> StackhouseResult<Vec<Value>> {
        let rows = self.store.query(
            "SELECT id, action, details, ip_address, user_agent, timestamp FROM stackhouse_impersonation_audit WHERE session_id = ? ORDER BY timestamp".to_string(),
            vec![SqlValue::Text(session_id.to_string())],
        ).await?;
        Ok(rows
            .into_iter()
            .map(|r| json!(r.into_iter().collect::<std::collections::HashMap<_, _>>()))
            .collect())
    }

    async fn log_action(
        &self,
        session_id: &str,
        admin_id: i64,
        target_id: i64,
        action: &str,
        details: Value,
        ip: Option<&str>,
        ua: Option<&str>,
    ) -> StackhouseResult<()> {
        let id = uuid::Uuid::new_v4().to_string();
        self.store.execute(
            "INSERT INTO stackhouse_impersonation_audit (id, session_id, admin_user_id, target_user_id, action, details, ip_address, user_agent) VALUES (?, ?, ?, ?, ?, ?, ?, ?)".to_string(),
            vec![
                SqlValue::Text(id),
                SqlValue::Text(session_id.to_string()),
                SqlValue::Integer(admin_id),
                SqlValue::Integer(target_id),
                SqlValue::Text(action.to_string()),
                SqlValue::Text(details.to_string()),
                SqlValue::Text(ip.unwrap_or("").to_string()),
                SqlValue::Text(ua.unwrap_or("").to_string()),
            ],
        ).await?;
        Ok(())
    }
}

// ============================================================================
// Router
// ============================================================================

#[derive(Clone)]
pub struct ImpersonationState {
    pub impersonation: Arc<ImpersonationService>,
    pub auth: AuthState,
}

#[derive(Deserialize)]
struct StartImpersonationRequest {
    target_user_id: i64,
    reason: String,
}

async fn start_handler(
    State(state): State<ImpersonationState>,
    headers: HeaderMap,
    Json(req): Json<StartImpersonationRequest>,
) -> Result<impl IntoResponse, StackhouseError> {
    let admin = extract_auth_user(&state.auth, &headers)?;
    let ip = headers.get("x-forwarded-for").and_then(|v| v.to_str().ok());
    let ua = headers.get("user-agent").and_then(|v| v.to_str().ok());
    let session = state
        .impersonation
        .start_impersonation(
            admin.id,
            &admin.email,
            req.target_user_id,
            &req.reason,
            ip,
            ua,
        )
        .await?;
    Ok(Json(json!({"success": true, "data": session})))
}

async fn end_handler(
    State(state): State<ImpersonationState>,
    headers: HeaderMap,
    axum::extract::Path(session_id): axum::extract::Path<String>,
) -> Result<impl IntoResponse, StackhouseError> {
    let admin = extract_auth_user(&state.auth, &headers)?;
    let ip = headers.get("x-forwarded-for").and_then(|v| v.to_str().ok());
    let ua = headers.get("user-agent").and_then(|v| v.to_str().ok());
    state
        .impersonation
        .end_impersonation(&session_id, admin.id, ip, ua)
        .await?;
    Ok(Json(
        json!({"success": true, "message": "Impersonation ended"}),
    ))
}

async fn list_sessions_handler(
    State(state): State<ImpersonationState>,
    headers: HeaderMap,
) -> Result<impl IntoResponse, StackhouseError> {
    let _admin = extract_auth_user(&state.auth, &headers)?;
    let sessions = state.impersonation.list_sessions(50).await?;
    Ok(Json(json!({"success": true, "data": sessions})))
}

async fn audit_trail_handler(
    State(state): State<ImpersonationState>,
    headers: HeaderMap,
    axum::extract::Path(session_id): axum::extract::Path<String>,
) -> Result<impl IntoResponse, StackhouseError> {
    let _admin = extract_auth_user(&state.auth, &headers)?;
    let trail = state.impersonation.get_audit_trail(&session_id).await?;
    Ok(Json(json!({"success": true, "data": trail})))
}

pub fn create_impersonation_router(state: ImpersonationState) -> Router {
    Router::new()
        .route("/impersonate", post(start_handler))
        .route("/impersonate/:session_id/end", post(end_handler))
        .route("/impersonate/sessions", get(list_sessions_handler))
        .route("/impersonate/:session_id/audit", get(audit_trail_handler))
        .with_state(state)
}
