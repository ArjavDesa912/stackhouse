//! # Database Backup Module (Stackhouse-Backup)
//!
//! Database backup and point-in-time recovery (PITR).
//! Supports scheduled backups, manual snapshots, and restore operations.

#[path = "backups/pitr.rs"]
pub mod pitr;
pub use pitr::*;

use crate::api::admin::AdminAuditService;
use crate::auth::{extract_auth_user, AuthState, AuthUser};
use crate::authorization::{data_protector, AuthorizationService};
use crate::db::{SqlValue, StackhouseStore};
use crate::error::{StackhouseError, StackhouseResult};

use axum::{
    body::{to_bytes, Body},
    extract::State,
    http::{HeaderMap, Request},
    response::IntoResponse,
    routing::{get, post},
    Json, Router,
};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::path::PathBuf;
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};
use tracing::info;

#[derive(Clone)]
pub struct BackupService {
    store: Arc<StackhouseStore>,
    backup_path: PathBuf,
    db_url: String,
}

#[derive(Clone, Debug, Serialize)]
pub struct BackupInfo {
    pub id: String,
    pub name: String,
    pub size_bytes: u64,
    pub created_at: u64,
    pub backup_type: String,
    pub status: String,
}

impl BackupService {
    pub async fn new(
        store: Arc<StackhouseStore>,
        backup_path: PathBuf,
        db_url: String,
    ) -> StackhouseResult<Self> {
        std::fs::create_dir_all(&backup_path).ok();
        let service = Self {
            store,
            backup_path,
            db_url,
        };
        service.initialize_tables().await?;
        info!(
            "💾 Stackhouse-Backup initialized (path: {:?})",
            service.backup_path
        );
        Ok(service)
    }

    async fn initialize_tables(&self) -> StackhouseResult<()> {
        self.store
            .execute_batch(
                r#"
            CREATE TABLE IF NOT EXISTS stackhouse_backups (
                id TEXT PRIMARY KEY,
                name TEXT NOT NULL,
                size_bytes BIGINT DEFAULT 0,
                created_at BIGINT NOT NULL,
                backup_type TEXT DEFAULT 'manual',
                status TEXT DEFAULT 'pending',
                file_path TEXT,
                metadata JSONB DEFAULT '{}'
            );
            "#
                .to_string(),
            )
            .await?;
        Ok(())
    }

    /// Create a logical SQL dump backup
    pub async fn create_backup(&self, name: &str) -> StackhouseResult<BackupInfo> {
        let backup_id = uuid::Uuid::new_v4().to_string();
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs();
        let file_name = format!("{}_{}.sql", name, now);
        let file_path = self.backup_path.join(&file_name);

        // Record backup
        self.store.execute(
            "INSERT INTO stackhouse_backups (id, name, created_at, backup_type, status, file_path) VALUES ($1, $2, $3, 'manual', 'in_progress', $4)".to_string(),
            vec![
                SqlValue::Text(backup_id.clone()),
                SqlValue::Text(name.to_string()),
                SqlValue::Integer(now as i64),
                SqlValue::Text(file_path.to_string_lossy().to_string()),
            ],
        ).await?;

        // Create logical backup by dumping all user tables
        let tables = self.store.list_tables().await?;
        let mut sql_dump = String::new();
        sql_dump.push_str(&format!("-- Stackhouse Backup: {}\n", name));
        sql_dump.push_str(&format!("-- Created: {}\n", now));
        sql_dump.push_str("-- Format: SQL\n\n");
        sql_dump.push_str("BEGIN;\n\n");

        for table in &tables {
            if table.starts_with("stackhouse_") || table.starts_with("pg_") {
                continue;
            }

            // Get CREATE TABLE statement
            let schema = self.store.query_simple(format!(
                "SELECT column_name, data_type, is_nullable FROM information_schema.columns WHERE table_name = '{}' ORDER BY ordinal_position",
                table
            )).await.unwrap_or_default();

            sql_dump.push_str(&format!("-- Table: {}\n", table));
            sql_dump.push_str(&format!("CREATE TABLE IF NOT EXISTS \"{}\" (\n", table));
            let cols: Vec<String> = schema
                .iter()
                .map(|row| {
                    let col_name = row
                        .iter()
                        .find(|(k, _)| k == "column_name")
                        .and_then(|(_, v)| v.as_str())
                        .unwrap_or("?");
                    let data_type = row
                        .iter()
                        .find(|(k, _)| k == "data_type")
                        .and_then(|(_, v)| v.as_str())
                        .unwrap_or("text");
                    let nullable = row
                        .iter()
                        .find(|(k, _)| k == "is_nullable")
                        .and_then(|(_, v)| v.as_str())
                        .unwrap_or("YES");
                    let null_str = if nullable == "NO" { " NOT NULL" } else { "" };
                    format!("  \"{}\" {}{}", col_name, data_type, null_str)
                })
                .collect();
            sql_dump.push_str(&cols.join(",\n"));
            sql_dump.push_str("\n);\n\n");

            // Dump data
            let rows = self
                .store
                .query_simple(format!("SELECT * FROM \"{}\"", table))
                .await
                .unwrap_or_default();
            for row in rows {
                let cols: Vec<String> = row.iter().map(|(k, _)| format!("\"{}\"", k)).collect();
                let vals: Vec<String> = row
                    .iter()
                    .map(|(_, v)| match v {
                        Value::Null => "NULL".to_string(),
                        Value::String(s) => format!("'{}'", s.replace('\'', "''")),
                        Value::Number(n) => n.to_string(),
                        Value::Bool(b) => b.to_string(),
                        _ => format!("'{}'", v.to_string().replace('\'', "''")),
                    })
                    .collect();
                sql_dump.push_str(&format!(
                    "INSERT INTO \"{}\" ({}) VALUES ({});\n",
                    table,
                    cols.join(", "),
                    vals.join(", ")
                ));
            }
            sql_dump.push('\n');
        }

        sql_dump.push_str("COMMIT;\n");

        // Write encrypted backup artifact to disk
        let encrypted_dump = data_protector()?.encrypt_bytes(sql_dump.as_bytes())?;
        let size = encrypted_dump.len() as u64;
        std::fs::write(&file_path, &encrypted_dump).map_err(|e| {
            StackhouseError::Internal(anyhow::anyhow!("Backup write failed: {}", e))
        })?;

        // Update status
        self.store
            .execute(
                "UPDATE stackhouse_backups SET status = 'completed', size_bytes = $1 WHERE id = $2"
                    .to_string(),
                vec![
                    SqlValue::Integer(size as i64),
                    SqlValue::Text(backup_id.clone()),
                ],
            )
            .await?;

        info!("💾 Backup '{}' created ({} bytes)", name, size);

        Ok(BackupInfo {
            id: backup_id,
            name: name.to_string(),
            size_bytes: size,
            created_at: now,
            backup_type: "manual".to_string(),
            status: "completed".to_string(),
        })
    }

    pub async fn list_backups(&self) -> StackhouseResult<Vec<Value>> {
        let rows = self.store.query_simple(
            "SELECT id, name, size_bytes, created_at, backup_type, status FROM stackhouse_backups ORDER BY created_at DESC LIMIT 50".to_string(),
        ).await?;

        Ok(rows
            .into_iter()
            .map(|row| {
                let mut obj = serde_json::Map::new();
                for (k, v) in row {
                    obj.insert(k, v);
                }
                Value::Object(obj)
            })
            .collect())
    }

    pub async fn restore_backup(&self, backup_id: &str) -> StackhouseResult<()> {
        let rows = self
            .store
            .query(
                "SELECT file_path FROM stackhouse_backups WHERE id = $1 AND status = 'completed'"
                    .to_string(),
                vec![SqlValue::Text(backup_id.to_string())],
            )
            .await?;

        let file_path = rows
            .first()
            .and_then(|r| r.iter().find(|(k, _)| k == "file_path"))
            .and_then(|(_, v)| v.as_str())
            .ok_or_else(|| StackhouseError::TableNotFound("Backup not found".to_string()))?;

        let encrypted = std::fs::read(file_path)
            .map_err(|e| StackhouseError::Internal(anyhow::anyhow!("Backup read failed: {}", e)))?;
        let sql = String::from_utf8(data_protector()?.decrypt_bytes(&encrypted)?).map_err(|e| {
            StackhouseError::Internal(anyhow::anyhow!("Backup decode failed: {}", e))
        })?;

        self.store.execute_batch(sql).await?;
        info!("💾 Restored backup: {}", backup_id);
        Ok(())
    }

    pub async fn delete_backup(&self, backup_id: &str) -> StackhouseResult<()> {
        let rows = self
            .store
            .query(
                "SELECT file_path FROM stackhouse_backups WHERE id = $1".to_string(),
                vec![SqlValue::Text(backup_id.to_string())],
            )
            .await?;

        if let Some(path) = rows
            .first()
            .and_then(|r| r.iter().find(|(k, _)| k == "file_path"))
            .and_then(|(_, v)| v.as_str())
        {
            std::fs::remove_file(path).ok();
        }

        self.store
            .execute(
                "DELETE FROM stackhouse_backups WHERE id = $1".to_string(),
                vec![SqlValue::Text(backup_id.to_string())],
            )
            .await?;

        Ok(())
    }
}

// ============================================================================
// Handlers & Router
// ============================================================================

#[derive(Clone)]
pub struct BackupState {
    pub backup: Arc<BackupService>,
    pub pitr: Arc<PitrService>,
    pub auth: AuthState,
    pub authorization: AuthorizationService,
    pub admin_audit: Arc<AdminAuditService>,
}

#[derive(Deserialize)]
struct CreateBackupRequest {
    name: String,
}

async fn create_backup_handler(
    State(state): State<BackupState>,
    request: Request<Body>,
) -> Result<impl IntoResponse, StackhouseError> {
    let auth_user = require_service_admin(&state, request.headers()).await?;
    let body = to_bytes(request.into_body(), usize::MAX)
        .await
        .map_err(|e| StackhouseError::InvalidPayload(format!("Invalid request body: {}", e)))?;
    let req: CreateBackupRequest = serde_json::from_slice(&body)?;
    let info = state.backup.create_backup(&req.name).await?;
    state
        .admin_audit
        .record(
            auth_user.id,
            "backup.create",
            "backup",
            Some(info.id.clone()),
            "success",
            json!({
                "route": "/v1/admin/backups",
                "name": info.name,
            }),
        )
        .await?;
    Ok(Json(json!({"success": true, "data": info})))
}

async fn list_backups_handler(
    State(state): State<BackupState>,
    headers: HeaderMap,
) -> Result<impl IntoResponse, StackhouseError> {
    let auth_user = require_service_admin(&state, &headers).await?;
    let backups = state.backup.list_backups().await?;
    state
        .admin_audit
        .record(
            auth_user.id,
            "backup.list",
            "backup",
            None,
            "success",
            json!({"route": "/v1/admin/backups", "count": backups.len()}),
        )
        .await?;
    Ok(Json(json!({"success": true, "data": backups})))
}

async fn restore_handler(
    State(state): State<BackupState>,
    headers: HeaderMap,
    axum::extract::Path(id): axum::extract::Path<String>,
) -> Result<impl IntoResponse, StackhouseError> {
    let auth_user = require_service_admin(&state, &headers).await?;
    state.backup.restore_backup(&id).await?;
    state
        .admin_audit
        .record(
            auth_user.id,
            "backup.restore",
            "backup",
            Some(id.clone()),
            "success",
            json!({"route": "/v1/admin/backups/:id/restore"}),
        )
        .await?;
    Ok(Json(json!({"success": true, "message": "Backup restored"})))
}

#[derive(Deserialize)]
struct PitrRestoreRequest {
    target_time: String,
}

async fn pitr_restore_handler(
    State(state): State<BackupState>,
    headers: HeaderMap,
    Json(req): Json<PitrRestoreRequest>,
) -> Result<impl IntoResponse, StackhouseError> {
    let auth_user = require_service_admin(&state, &headers).await?;
    let result = state
        .pitr
        .restore_to(auth_user.id, &req.target_time)
        .await?;
    state
        .admin_audit
        .record(
            auth_user.id,
            "backup.pitr_restore",
            "backup",
            None,
            "success",
            json!({
                "route": "/v1/admin/backups/pitr/restore",
                "target_time": req.target_time,
            }),
        )
        .await?;
    Ok(Json(json!({"success": true, "data": result})))
}

async fn delete_handler(
    State(state): State<BackupState>,
    headers: HeaderMap,
    axum::extract::Path(id): axum::extract::Path<String>,
) -> Result<impl IntoResponse, StackhouseError> {
    let auth_user = require_service_admin(&state, &headers).await?;
    state.backup.delete_backup(&id).await?;
    state
        .admin_audit
        .record(
            auth_user.id,
            "backup.delete",
            "backup",
            Some(id.clone()),
            "success",
            json!({"route": "/v1/admin/backups/:id"}),
        )
        .await?;
    Ok(Json(json!({"success": true})))
}

pub fn create_backup_router(state: BackupState) -> Router {
    Router::new()
        .route(
            "/backups",
            get(list_backups_handler).post(create_backup_handler),
        )
        .route("/backups/:id/restore", post(restore_handler))
        .route("/backups/pitr/restore", post(pitr_restore_handler))
        .route("/backups/:id", axum::routing::delete(delete_handler))
        .with_state(state)
}

async fn require_service_admin(
    state: &BackupState,
    headers: &HeaderMap,
) -> Result<AuthUser, StackhouseError> {
    let auth_user = extract_auth_user(&state.auth, headers)?;
    let user = state.auth.auth.get_user_by_id(auth_user.id).await?;
    state
        .authorization
        .require_service_admin_unconditional(&user)?;
    Ok(auth_user)
}
