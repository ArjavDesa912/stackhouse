//! # Admin Audit Service
//!
//! Reusable persistence for route-level privileged action auditing.

use crate::db::{SqlValue, StackhouseStore};
use crate::error::{StackhouseError, StackhouseResult};
use axum::{extract::State, response::IntoResponse, routing::get, Json, Router};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::sync::Arc;
use tracing::info;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct AdminAuditEntry {
    pub id: String,
    pub occurred_at: DateTime<Utc>,
    pub actor_user_id: i64,
    pub action: String,
    pub resource_type: String,
    pub resource_id: Option<String>,
    pub outcome: String,
    pub details: Value,
}

#[derive(Clone)]
pub struct AdminAuditService {
    store: Arc<StackhouseStore>,
}

impl AdminAuditService {
    pub async fn new(store: Arc<StackhouseStore>) -> StackhouseResult<Self> {
        let service = Self { store };
        service.initialize().await?;
        info!("Admin audit service initialized");
        Ok(service)
    }

    async fn initialize(&self) -> StackhouseResult<()> {
        self.store
            .execute_batch(
                r#"
                CREATE TABLE IF NOT EXISTS stackhouse_admin_audit_logs (
                    id TEXT PRIMARY KEY,
                    occurred_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
                    actor_user_id BIGINT NOT NULL,
                    action TEXT NOT NULL,
                    resource_type TEXT NOT NULL,
                    resource_id TEXT,
                    outcome TEXT NOT NULL,
                    details JSONB NOT NULL DEFAULT '{}'::jsonb
                );
                CREATE INDEX IF NOT EXISTS idx_stackhouse_admin_audit_occurred_at
                    ON stackhouse_admin_audit_logs(occurred_at DESC);
                "#
                .to_string(),
            )
            .await
    }

    pub async fn record(
        &self,
        actor_user_id: i64,
        action: &str,
        resource_type: &str,
        resource_id: Option<String>,
        outcome: &str,
        details: Value,
    ) -> StackhouseResult<()> {
        self.store
            .execute(
                r#"
                INSERT INTO stackhouse_admin_audit_logs
                    (id, actor_user_id, action, resource_type, resource_id, outcome, details)
                VALUES ($1, $2, $3, $4, $5, $6, $7)
                "#
                .to_string(),
                vec![
                    SqlValue::Text(uuid::Uuid::new_v4().to_string()),
                    SqlValue::Integer(actor_user_id),
                    SqlValue::Text(action.to_string()),
                    SqlValue::Text(resource_type.to_string()),
                    resource_id.map(SqlValue::Text).unwrap_or(SqlValue::Null),
                    SqlValue::Text(outcome.to_string()),
                    SqlValue::Json(details),
                ],
            )
            .await?;

        Ok(())
    }

    pub async fn list_audit(&self, limit: i64) -> StackhouseResult<Vec<Value>> {
        let rows = self.store.query(
            r#"SELECT id, occurred_at, actor_user_id, action, resource_type, resource_id, outcome, details
               FROM stackhouse_admin_audit_logs ORDER BY occurred_at DESC LIMIT ?"#
                .to_string(),
            vec![SqlValue::Integer(limit)],
        ).await?;

        let entries: Vec<Value> = rows.into_iter().map(|row| {
            let get = |key: &str| row.iter().find(|(k, _)| k == key).map(|(_, v)| v);
            json!({
                "id": get("id").and_then(|v| v.as_str()).unwrap_or(""),
                "occurred_at": get("occurred_at").and_then(|v| v.as_str()).unwrap_or(""),
                "actor_user_id": get("actor_user_id").and_then(|v| v.as_i64()).unwrap_or(0),
                "action": get("action").and_then(|v| v.as_str()).unwrap_or(""),
                "resource_type": get("resource_type").and_then(|v| v.as_str()).unwrap_or(""),
                "resource_id": get("resource_id").and_then(|v| v.as_str().map(|s| s.to_string())),
                "outcome": get("outcome").and_then(|v| v.as_str()).unwrap_or(""),
                "details": get("details").unwrap_or(&Value::Object(Default::default())).clone(),
            })
        }).collect();
        Ok(entries)
    }
}

#[derive(Clone)]
pub struct AdminState {
    pub audit: AdminAuditService,
}

/// GET /v1/admin/audit
async fn list_audit_handler(
    State(state): State<AdminState>,
) -> Result<impl IntoResponse, StackhouseError> {
    let entries = state.audit.list_audit(200).await?;
    Ok(Json(json!({
        "success": true,
        "data": entries
    })))
}

pub fn create_admin_router(state: AdminState) -> Router {
    Router::new()
        .route("/audit", get(list_audit_handler))
        .with_state(state)
}
