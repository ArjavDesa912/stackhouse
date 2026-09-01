//! # GDPR & CCPA Compliance Tools
//!
//! Provides data deletion API (right to erasure), data export,
//! consent management, and data processing records.

use crate::auth::{extract_auth_user, AuthState};
use crate::db::{SqlValue, StackhouseStore};
use crate::error::{StackhouseError, StackhouseResult};

use axum::{
    extract::State,
    http::HeaderMap,
    response::IntoResponse,
    routing::{get, post},
    Json, Router,
};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::sync::Arc;
use tracing::info;

// ============================================================================
// Types
// ============================================================================

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DataExport {
    pub user_id: i64,
    pub export_id: String,
    pub status: ExportStatus,
    pub format: ExportFormat,
    pub requested_at: String,
    pub completed_at: Option<String>,
    pub download_url: Option<String>,
    pub expires_at: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ExportStatus {
    Pending,
    Processing,
    Completed,
    Failed,
    Expired,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ExportFormat {
    Json,
    Csv,
    Zip,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeletionRequest {
    pub user_id: i64,
    pub request_id: String,
    pub status: DeletionStatus,
    pub tables_processed: Vec<String>,
    pub requested_at: String,
    pub completed_at: Option<String>,
    pub verification_token: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DeletionStatus {
    PendingVerification,
    Scheduled,
    Processing,
    Completed,
    Failed,
    Cancelled,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConsentRecord {
    pub id: i64,
    pub user_id: i64,
    pub consent_type: String,
    pub granted: bool,
    pub granted_at: Option<String>,
    pub revoked_at: Option<String>,
    pub ip_address: Option<String>,
    pub user_agent: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DataProcessingRecord {
    pub id: i64,
    pub purpose: String,
    pub legal_basis: String,
    pub data_categories: Vec<String>,
    pub recipients: Vec<String>,
    pub retention_period: String,
    pub cross_border_transfers: bool,
    pub created_at: String,
}

// ============================================================================
// GDPR Service
// ============================================================================

#[derive(Clone)]
pub struct GdprService {
    store: Arc<StackhouseStore>,
}

impl GdprService {
    pub async fn new(store: Arc<StackhouseStore>) -> StackhouseResult<Self> {
        let service = Self { store };
        service.initialize_tables().await?;
        info!("🔒 GDPR/CCPA compliance module initialized");
        Ok(service)
    }

    async fn initialize_tables(&self) -> StackhouseResult<()> {
        self.store
            .execute_batch(
                r#"
            CREATE TABLE IF NOT EXISTS stackhouse_data_exports (
                id BIGSERIAL PRIMARY KEY,
                user_id BIGINT NOT NULL REFERENCES stackhouse_users(id),
                export_id TEXT UNIQUE NOT NULL,
                status TEXT NOT NULL DEFAULT 'pending',
                format TEXT NOT NULL DEFAULT 'json',
                download_url TEXT,
                requested_at TIMESTAMPTZ DEFAULT NOW(),
                completed_at TIMESTAMPTZ,
                expires_at TIMESTAMPTZ
            );
            CREATE TABLE IF NOT EXISTS stackhouse_deletion_requests (
                id BIGSERIAL PRIMARY KEY,
                user_id BIGINT NOT NULL REFERENCES stackhouse_users(id),
                request_id TEXT UNIQUE NOT NULL,
                status TEXT NOT NULL DEFAULT 'pending_verification',
                tables_processed TEXT DEFAULT '[]',
                verification_token TEXT,
                requested_at TIMESTAMPTZ DEFAULT NOW(),
                scheduled_at TIMESTAMPTZ,
                completed_at TIMESTAMPTZ
            );
            CREATE TABLE IF NOT EXISTS stackhouse_consent_records (
                id BIGSERIAL PRIMARY KEY,
                user_id BIGINT NOT NULL REFERENCES stackhouse_users(id),
                consent_type TEXT NOT NULL,
                granted BOOLEAN NOT NULL DEFAULT FALSE,
                granted_at TIMESTAMPTZ,
                revoked_at TIMESTAMPTZ,
                ip_address TEXT,
                user_agent TEXT,
                created_at TIMESTAMPTZ DEFAULT NOW(),
                UNIQUE(user_id, consent_type)
            );
            CREATE TABLE IF NOT EXISTS stackhouse_data_processing_records (
                id BIGSERIAL PRIMARY KEY,
                purpose TEXT NOT NULL,
                legal_basis TEXT NOT NULL,
                data_categories TEXT NOT NULL DEFAULT '[]',
                recipients TEXT NOT NULL DEFAULT '[]',
                retention_period TEXT NOT NULL,
                cross_border_transfers BOOLEAN DEFAULT FALSE,
                created_at TIMESTAMPTZ DEFAULT NOW(),
                updated_at TIMESTAMPTZ DEFAULT NOW()
            );
        "#
                .to_string(),
            )
            .await?;
        Ok(())
    }

    /// Request full data export for a user
    pub async fn request_export(
        &self,
        user_id: i64,
        format: ExportFormat,
    ) -> StackhouseResult<DataExport> {
        let export_id = uuid::Uuid::new_v4().to_string();
        let format_str = serde_json::to_string(&format).unwrap_or_else(|_| "\"json\"".into());
        let format_str = format_str.trim_matches('"');

        self.store
            .execute(
                "INSERT INTO stackhouse_data_exports (user_id, export_id, format) VALUES (?, ?, ?)"
                    .to_string(),
                vec![
                    SqlValue::Integer(user_id),
                    SqlValue::Text(export_id.clone()),
                    SqlValue::Text(format_str.to_string()),
                ],
            )
            .await?;

        // Spawn async export job
        let store = Arc::clone(&self.store);
        let eid = export_id.clone();
        tokio::spawn(async move {
            let _ = Self::process_export(store, user_id, &eid).await;
        });

        Ok(DataExport {
            user_id,
            export_id,
            status: ExportStatus::Pending,
            format,
            requested_at: chrono::Utc::now().to_rfc3339(),
            completed_at: None,
            download_url: None,
            expires_at: None,
        })
    }

    async fn process_export(
        store: Arc<StackhouseStore>,
        user_id: i64,
        export_id: &str,
    ) -> StackhouseResult<()> {
        // Update status to processing
        store
            .execute(
                "UPDATE stackhouse_data_exports SET status = 'processing' WHERE export_id = ?"
                    .to_string(),
                vec![SqlValue::Text(export_id.to_string())],
            )
            .await?;

        // Collect all user data from all tables
        let mut export_data = json!({});

        // User profile
        let user_rows = store.query(
            "SELECT id, email, metadata, created_at, updated_at FROM stackhouse_users WHERE id = ?".to_string(),
            vec![SqlValue::Integer(user_id)],
        ).await?;
        export_data["profile"] = json!(user_rows);

        // Sessions
        let session_rows = store
            .query(
                "SELECT id, created_at, expires_at FROM stackhouse_sessions WHERE user_id = ?"
                    .to_string(),
                vec![SqlValue::Integer(user_id)],
            )
            .await?;
        export_data["sessions"] = json!(session_rows);

        // Team memberships
        let team_rows = store.query(
            "SELECT tm.team_id, t.name, tm.role, tm.joined_at FROM stackhouse_team_members tm JOIN stackhouse_teams t ON t.id = tm.team_id WHERE tm.user_id = ?".to_string(),
            vec![SqlValue::Integer(user_id)],
        ).await.unwrap_or_default();
        export_data["teams"] = json!(team_rows);

        // Consent records
        let consent_rows = store.query(
            "SELECT consent_type, granted, granted_at, revoked_at FROM stackhouse_consent_records WHERE user_id = ?".to_string(),
            vec![SqlValue::Integer(user_id)],
        ).await.unwrap_or_default();
        export_data["consents"] = json!(consent_rows);

        // Mark as completed
        let expires = chrono::Utc::now() + chrono::Duration::days(7);
        store.execute(
            "UPDATE stackhouse_data_exports SET status = 'completed', completed_at = NOW(), expires_at = ?::timestamptz WHERE export_id = ?".to_string(),
            vec![
                SqlValue::Text(expires.to_rfc3339()),
                SqlValue::Text(export_id.to_string()),
            ],
        ).await?;

        Ok(())
    }

    /// Request data deletion (right to erasure)
    pub async fn request_deletion(&self, user_id: i64) -> StackhouseResult<DeletionRequest> {
        let request_id = uuid::Uuid::new_v4().to_string();
        let verification_token = uuid::Uuid::new_v4().to_string();

        self.store.execute(
            "INSERT INTO stackhouse_deletion_requests (user_id, request_id, status, verification_token) VALUES (?, ?, 'pending_verification', ?)".to_string(),
            vec![
                SqlValue::Integer(user_id),
                SqlValue::Text(request_id.clone()),
                SqlValue::Text(verification_token.clone()),
            ],
        ).await?;

        info!("🗑️ Data deletion requested for user_id={}", user_id);

        Ok(DeletionRequest {
            user_id,
            request_id,
            status: DeletionStatus::PendingVerification,
            tables_processed: vec![],
            requested_at: chrono::Utc::now().to_rfc3339(),
            completed_at: None,
            verification_token: Some(verification_token),
        })
    }

    /// Confirm and execute deletion
    pub async fn confirm_deletion(
        &self,
        request_id: &str,
        token: &str,
    ) -> StackhouseResult<DeletionRequest> {
        let rows = self.store.query(
            "SELECT user_id, verification_token FROM stackhouse_deletion_requests WHERE request_id = ? AND status = 'pending_verification'".to_string(),
            vec![SqlValue::Text(request_id.to_string())],
        ).await?;

        if rows.is_empty() {
            return Err(StackhouseError::NotFound(
                "Deletion request not found".into(),
            ));
        }

        let row = &rows[0];
        let stored_token = row
            .iter()
            .find(|(k, _)| k == "verification_token")
            .and_then(|(_, v)| v.as_str())
            .unwrap_or("");
        let user_id = row
            .iter()
            .find(|(k, _)| k == "user_id")
            .and_then(|(_, v)| v.as_i64())
            .unwrap_or(0);

        if stored_token != token {
            return Err(StackhouseError::Unauthorized(
                "Invalid verification token".into(),
            ));
        }

        // Execute cascade deletion
        let tables_processed = self.execute_deletion(user_id).await?;

        self.store.execute(
            "UPDATE stackhouse_deletion_requests SET status = 'completed', tables_processed = ?, completed_at = NOW() WHERE request_id = ?".to_string(),
            vec![
                SqlValue::Text(serde_json::to_string(&tables_processed).unwrap_or_default()),
                SqlValue::Text(request_id.to_string()),
            ],
        ).await?;

        info!("✅ Data deletion completed for user_id={}", user_id);

        Ok(DeletionRequest {
            user_id,
            request_id: request_id.to_string(),
            status: DeletionStatus::Completed,
            tables_processed,
            requested_at: String::new(),
            completed_at: Some(chrono::Utc::now().to_rfc3339()),
            verification_token: None,
        })
    }

    async fn execute_deletion(&self, user_id: i64) -> StackhouseResult<Vec<String>> {
        let mut processed = Vec::new();

        // Delete in dependency order (children first)
        let tables_to_clean = vec![
            "stackhouse_consent_records",
            "stackhouse_data_exports",
            "stackhouse_team_invites",
            "stackhouse_team_members",
            "stackhouse_sessions",
        ];

        for table in &tables_to_clean {
            self.store
                .execute(
                    format!("DELETE FROM {} WHERE user_id = ?", table),
                    vec![SqlValue::Integer(user_id)],
                )
                .await
                .ok();
            processed.push(table.to_string());
        }

        // Anonymize user record (don't fully delete to preserve referential integrity)
        self.store.execute(
            "UPDATE stackhouse_users SET email = ?, password_hash = 'DELETED', metadata = '{}', updated_at = NOW() WHERE id = ?".to_string(),
            vec![
                SqlValue::Text(format!("deleted_{}@anonymized.local", user_id)),
                SqlValue::Integer(user_id),
            ],
        ).await?;
        processed.push("stackhouse_users (anonymized)".to_string());

        Ok(processed)
    }

    /// Record user consent
    pub async fn record_consent(
        &self,
        user_id: i64,
        consent_type: &str,
        granted: bool,
        ip: Option<&str>,
        ua: Option<&str>,
    ) -> StackhouseResult<()> {
        self.store.execute(
            r#"INSERT INTO stackhouse_consent_records (user_id, consent_type, granted, granted_at, ip_address, user_agent)
               VALUES (?, ?, ?, NOW(), ?, ?)
               ON CONFLICT (user_id, consent_type)
               DO UPDATE SET granted = EXCLUDED.granted, granted_at = CASE WHEN EXCLUDED.granted THEN NOW() ELSE stackhouse_consent_records.granted_at END,
               revoked_at = CASE WHEN NOT EXCLUDED.granted THEN NOW() ELSE NULL END"#.to_string(),
            vec![
                SqlValue::Integer(user_id),
                SqlValue::Text(consent_type.to_string()),
                SqlValue::Text(granted.to_string()),
                SqlValue::Text(ip.unwrap_or("").to_string()),
                SqlValue::Text(ua.unwrap_or("").to_string()),
            ],
        ).await?;
        Ok(())
    }

    /// Get all consents for a user
    pub async fn get_consents(&self, user_id: i64) -> StackhouseResult<Vec<Value>> {
        let rows = self.store.query(
            "SELECT consent_type, granted, granted_at, revoked_at FROM stackhouse_consent_records WHERE user_id = ? ORDER BY consent_type".to_string(),
            vec![SqlValue::Integer(user_id)],
        ).await?;
        Ok(rows
            .into_iter()
            .map(|r| json!(r.into_iter().collect::<std::collections::HashMap<_, _>>()))
            .collect())
    }

    /// Get export status
    pub async fn get_export_status(&self, export_id: &str) -> StackhouseResult<Value> {
        let rows = self.store.query(
            "SELECT export_id, status, format, requested_at, completed_at, expires_at FROM stackhouse_data_exports WHERE export_id = ?".to_string(),
            vec![SqlValue::Text(export_id.to_string())],
        ).await?;
        if rows.is_empty() {
            return Err(StackhouseError::NotFound("Export not found".into()));
        }
        Ok(json!(rows[0]
            .iter()
            .cloned()
            .collect::<std::collections::HashMap<_, _>>()))
    }
}

// ============================================================================
// State & Router
// ============================================================================

#[derive(Clone)]
pub struct GdprState {
    pub gdpr: Arc<GdprService>,
    pub auth: AuthState,
}

#[derive(Deserialize)]
struct ExportRequest {
    #[serde(default = "default_format")]
    format: String,
}
fn default_format() -> String {
    "json".to_string()
}

#[derive(Deserialize)]
struct ConfirmDeletionRequest {
    request_id: String,
    verification_token: String,
}

#[derive(Deserialize)]
struct ConsentRequest {
    consent_type: String,
    granted: bool,
}

async fn request_export_handler(
    State(state): State<GdprState>,
    headers: HeaderMap,
    Json(req): Json<ExportRequest>,
) -> Result<impl IntoResponse, StackhouseError> {
    let user = extract_auth_user(&state.auth, &headers)?;
    let format = match req.format.as_str() {
        "csv" => ExportFormat::Csv,
        "zip" => ExportFormat::Zip,
        _ => ExportFormat::Json,
    };
    let export = state.gdpr.request_export(user.id, format).await?;
    Ok(Json(json!({"success": true, "data": export})))
}

async fn get_export_handler(
    State(state): State<GdprState>,
    headers: HeaderMap,
    axum::extract::Path(export_id): axum::extract::Path<String>,
) -> Result<impl IntoResponse, StackhouseError> {
    let _user = extract_auth_user(&state.auth, &headers)?;
    let export = state.gdpr.get_export_status(&export_id).await?;
    Ok(Json(json!({"success": true, "data": export})))
}

async fn request_deletion_handler(
    State(state): State<GdprState>,
    headers: HeaderMap,
) -> Result<impl IntoResponse, StackhouseError> {
    let user = extract_auth_user(&state.auth, &headers)?;
    let deletion = state.gdpr.request_deletion(user.id).await?;
    Ok(Json(json!({"success": true, "data": deletion})))
}

async fn confirm_deletion_handler(
    State(state): State<GdprState>,
    Json(req): Json<ConfirmDeletionRequest>,
) -> Result<impl IntoResponse, StackhouseError> {
    let result = state
        .gdpr
        .confirm_deletion(&req.request_id, &req.verification_token)
        .await?;
    Ok(Json(json!({"success": true, "data": result})))
}

async fn record_consent_handler(
    State(state): State<GdprState>,
    headers: HeaderMap,
    Json(req): Json<ConsentRequest>,
) -> Result<impl IntoResponse, StackhouseError> {
    let user = extract_auth_user(&state.auth, &headers)?;
    let ip = headers.get("x-forwarded-for").and_then(|v| v.to_str().ok());
    let ua = headers.get("user-agent").and_then(|v| v.to_str().ok());
    state
        .gdpr
        .record_consent(user.id, &req.consent_type, req.granted, ip, ua)
        .await?;
    Ok(Json(
        json!({"success": true, "message": "Consent recorded"}),
    ))
}

async fn get_consents_handler(
    State(state): State<GdprState>,
    headers: HeaderMap,
) -> Result<impl IntoResponse, StackhouseError> {
    let user = extract_auth_user(&state.auth, &headers)?;
    let consents = state.gdpr.get_consents(user.id).await?;
    Ok(Json(json!({"success": true, "data": consents})))
}

pub fn create_gdpr_router(state: GdprState) -> Router {
    Router::new()
        .route("/export", post(request_export_handler))
        .route("/export/:id", get(get_export_handler))
        .route("/delete", post(request_deletion_handler))
        .route("/delete/confirm", post(confirm_deletion_handler))
        .route("/consent", post(record_consent_handler))
        .route("/consent", get(get_consents_handler))
        .with_state(state)
}
