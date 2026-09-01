//! # Error Tracking (Sentry-Compatible)
//!
//! Captures, groups, and analyzes errors with stack traces, contexts,
//! and breadcrumbs. Compatible with the Sentry SDK wire protocol.

use crate::db::{SqlValue, StackhouseStore};
use crate::error::StackhouseResult;

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::sync::Arc;
use tracing::info;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ErrorEvent {
    pub event_id: String,
    pub tenant_id: i64,
    pub project: String,
    pub level: ErrorLevel,
    pub message: String,
    pub exception: Option<ExceptionInfo>,
    pub tags: HashMap<String, String>,
    pub contexts: HashMap<String, Value>,
    pub breadcrumbs: Vec<Breadcrumb>,
    pub user: Option<ErrorUser>,
    pub request: Option<ErrorRequest>,
    pub fingerprint: Vec<String>,
    pub timestamp: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ErrorLevel {
    Fatal,
    Error,
    Warning,
    Info,
    Debug,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExceptionInfo {
    pub exception_type: String,
    pub value: String,
    pub stacktrace: Vec<StackFrame>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StackFrame {
    pub filename: String,
    pub function: String,
    pub lineno: u32,
    pub colno: Option<u32>,
    pub context_line: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Breadcrumb {
    pub category: String,
    pub message: String,
    pub level: String,
    pub timestamp: String,
    pub data: Option<Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ErrorUser {
    pub id: Option<String>,
    pub email: Option<String>,
    pub ip_address: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ErrorRequest {
    pub url: String,
    pub method: String,
    pub headers: HashMap<String, String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ErrorGroup {
    pub group_id: String,
    pub title: String,
    pub level: String,
    pub first_seen: String,
    pub last_seen: String,
    pub event_count: u64,
    pub user_count: u64,
    pub status: GroupStatus,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum GroupStatus {
    Unresolved,
    Resolved,
    Ignored,
    Muted,
}

#[derive(Clone)]
pub struct ErrorTrackingService {
    store: Arc<StackhouseStore>,
}

impl ErrorTrackingService {
    pub async fn new(store: Arc<StackhouseStore>) -> StackhouseResult<Self> {
        let service = Self { store };
        service.initialize_tables().await?;
        info!("🐛 Error tracking service initialized");
        Ok(service)
    }

    async fn initialize_tables(&self) -> StackhouseResult<()> {
        self.store.execute_batch(r#"
            CREATE TABLE IF NOT EXISTS stackhouse_error_events (
                event_id TEXT PRIMARY KEY,
                tenant_id BIGINT NOT NULL,
                group_id TEXT NOT NULL,
                project TEXT NOT NULL,
                level TEXT NOT NULL,
                message TEXT NOT NULL,
                exception JSONB,
                tags JSONB DEFAULT '{}',
                contexts JSONB DEFAULT '{}',
                breadcrumbs JSONB DEFAULT '[]',
                user_info JSONB,
                request JSONB,
                timestamp TIMESTAMPTZ DEFAULT NOW()
            );
            CREATE TABLE IF NOT EXISTS stackhouse_error_groups (
                group_id TEXT PRIMARY KEY,
                tenant_id BIGINT NOT NULL,
                project TEXT NOT NULL,
                title TEXT NOT NULL,
                level TEXT NOT NULL,
                fingerprint TEXT NOT NULL,
                first_seen TIMESTAMPTZ DEFAULT NOW(),
                last_seen TIMESTAMPTZ DEFAULT NOW(),
                event_count BIGINT DEFAULT 1,
                user_count BIGINT DEFAULT 0,
                status TEXT DEFAULT 'unresolved',
                assigned_to TEXT
            );
            CREATE INDEX IF NOT EXISTS idx_error_events_group ON stackhouse_error_events(group_id);
            CREATE INDEX IF NOT EXISTS idx_error_events_tenant ON stackhouse_error_events(tenant_id, timestamp);
            CREATE INDEX IF NOT EXISTS idx_error_groups_tenant ON stackhouse_error_groups(tenant_id, status);
        "#.to_string()).await?;
        Ok(())
    }

    /// Ingest an error event
    pub async fn capture(&self, event: ErrorEvent) -> StackhouseResult<String> {
        // Compute fingerprint for grouping
        let fingerprint_input = if !event.fingerprint.is_empty() {
            event.fingerprint.join("|")
        } else if let Some(exc) = &event.exception {
            format!(
                "{}:{}",
                exc.exception_type,
                exc.stacktrace
                    .first()
                    .map(|f| f.function.as_str())
                    .unwrap_or("")
            )
        } else {
            event.message.clone()
        };

        let mut hasher = Sha256::new();
        hasher.update(format!("{}:{}", event.tenant_id, fingerprint_input).as_bytes());
        let group_id = hex::encode(&hasher.finalize()[..16]);

        // Upsert group
        let title = event
            .exception
            .as_ref()
            .map(|e| format!("{}: {}", e.exception_type, e.value))
            .unwrap_or_else(|| event.message.clone());
        let level_str = serde_json::to_string(&event.level)
            .unwrap_or_default()
            .trim_matches('"')
            .to_string();

        self.store.execute(
            r#"INSERT INTO stackhouse_error_groups (group_id, tenant_id, project, title, level, fingerprint)
               VALUES (?, ?, ?, ?, ?, ?)
               ON CONFLICT (group_id) DO UPDATE SET
               last_seen = NOW(), event_count = stackhouse_error_groups.event_count + 1,
               level = EXCLUDED.level"#.to_string(),
            vec![
                SqlValue::Text(group_id.clone()),
                SqlValue::Integer(event.tenant_id),
                SqlValue::Text(event.project.clone()),
                SqlValue::Text(title),
                SqlValue::Text(level_str.clone()),
                SqlValue::Text(fingerprint_input),
            ],
        ).await?;

        // Store event
        self.store.execute(
            "INSERT INTO stackhouse_error_events (event_id, tenant_id, group_id, project, level, message, exception, tags, contexts, breadcrumbs, user_info, request) VALUES (?, ?, ?, ?, ?, ?, ?::jsonb, ?::jsonb, ?::jsonb, ?::jsonb, ?::jsonb, ?::jsonb)".to_string(),
            vec![
                SqlValue::Text(event.event_id.clone()),
                SqlValue::Integer(event.tenant_id),
                SqlValue::Text(group_id.clone()),
                SqlValue::Text(event.project),
                SqlValue::Text(level_str),
                SqlValue::Text(event.message),
                SqlValue::Text(serde_json::to_string(&event.exception).unwrap_or("null".into())),
                SqlValue::Text(serde_json::to_string(&event.tags).unwrap_or_default()),
                SqlValue::Text(serde_json::to_string(&event.contexts).unwrap_or_default()),
                SqlValue::Text(serde_json::to_string(&event.breadcrumbs).unwrap_or_default()),
                SqlValue::Text(serde_json::to_string(&event.user).unwrap_or("null".into())),
                SqlValue::Text(serde_json::to_string(&event.request).unwrap_or("null".into())),
            ],
        ).await?;

        Ok(group_id)
    }

    /// List error groups
    pub async fn list_groups(
        &self,
        tenant_id: i64,
        status: Option<&str>,
        limit: usize,
    ) -> StackhouseResult<Vec<Value>> {
        let (sql, params) = if let Some(st) = status {
            (
                format!("SELECT group_id, title, level, first_seen, last_seen, event_count, status FROM stackhouse_error_groups WHERE tenant_id = ? AND status = ? ORDER BY last_seen DESC LIMIT {}", limit),
                vec![SqlValue::Integer(tenant_id), SqlValue::Text(st.to_string())],
            )
        } else {
            (
                format!("SELECT group_id, title, level, first_seen, last_seen, event_count, status FROM stackhouse_error_groups WHERE tenant_id = ? ORDER BY last_seen DESC LIMIT {}", limit),
                vec![SqlValue::Integer(tenant_id)],
            )
        };
        let rows = self.store.query(sql, params).await?;
        Ok(rows
            .into_iter()
            .map(|r| json!(r.into_iter().collect::<HashMap<_, _>>()))
            .collect())
    }

    /// Get events for a group
    pub async fn get_group_events(
        &self,
        group_id: &str,
        limit: usize,
    ) -> StackhouseResult<Vec<Value>> {
        let rows = self.store.query(
            format!("SELECT event_id, level, message, exception, tags, timestamp FROM stackhouse_error_events WHERE group_id = ? ORDER BY timestamp DESC LIMIT {}", limit),
            vec![SqlValue::Text(group_id.to_string())],
        ).await?;
        Ok(rows
            .into_iter()
            .map(|r| json!(r.into_iter().collect::<HashMap<_, _>>()))
            .collect())
    }

    /// Resolve a group
    pub async fn resolve_group(&self, group_id: &str) -> StackhouseResult<()> {
        self.store
            .execute(
                "UPDATE stackhouse_error_groups SET status = 'resolved' WHERE group_id = ?"
                    .to_string(),
                vec![SqlValue::Text(group_id.to_string())],
            )
            .await?;
        Ok(())
    }
}
