//! # Structured Logging & Log Drains Module (Stackhouse-Logging)
//!
//! Production-grade structured logging with JSON output and webhook-based log drains.
//! Exports logs to external services (Datadog, Logtail, custom webhooks).
//!
//! ## Features
//! - JSON structured log format
//! - Configurable log drains (webhook, stdout, file)
//! - Log level filtering
//! - Buffered async log shipping
//! - Query logs via API

use crate::api::admin::AdminAuditService;
use crate::auth::{extract_auth_user, AuthState, AuthUser};
use crate::authorization::AuthorizationService;
use crate::db::{SqlValue, StackhouseStore};
use crate::error::{StackhouseError, StackhouseResult};

use axum::{
    extract::{Query, State},
    http::HeaderMap,
    response::IntoResponse,
    routing::get,
    Json, Router,
};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};
use tokio::sync::mpsc;
use tracing::info;

// ============================================================================
// Log Drain Configuration
// ============================================================================

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct LogDrainConfig {
    pub name: String,
    pub drain_type: DrainType,
    pub url: Option<String>,
    pub api_key: Option<String>,
    pub min_level: LogLevel,
    pub enabled: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum DrainType {
    Webhook,
    Stdout,
    Database,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, PartialOrd)]
#[serde(rename_all = "lowercase")]
pub enum LogLevel {
    Debug = 0,
    Info = 1,
    Warn = 2,
    Error = 3,
    Fatal = 4,
}

// ============================================================================
// Log Entry
// ============================================================================

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct LogEntry {
    pub timestamp: u64,
    pub level: LogLevel,
    pub message: String,
    pub service: String,
    #[serde(default)]
    pub metadata: Value,
    #[serde(default)]
    pub request_id: Option<String>,
    #[serde(default)]
    pub user_id: Option<i64>,
}

// ============================================================================
// Log Drain Service
// ============================================================================

#[derive(Clone)]
pub struct LogDrainService {
    store: Arc<StackhouseStore>,
    drains: Vec<LogDrainConfig>,
    log_sender: mpsc::Sender<LogEntry>,
    http_client: reqwest::Client,
}

impl LogDrainService {
    pub async fn new(
        store: Arc<StackhouseStore>,
        drains: Vec<LogDrainConfig>,
    ) -> StackhouseResult<Self> {
        let (tx, mut rx) = mpsc::channel::<LogEntry>(10_000);
        let http_client = reqwest::Client::new();

        let service = Self {
            store: Arc::clone(&store),
            drains: drains.clone(),
            log_sender: tx,
            http_client: http_client.clone(),
        };

        service.initialize_tables().await?;

        // Background task to process log entries
        let drains_clone = drains;
        let store_clone = Arc::clone(&store);
        tokio::spawn(async move {
            let mut buffer: Vec<LogEntry> = Vec::with_capacity(100);

            loop {
                // Batch receive
                match tokio::time::timeout(tokio::time::Duration::from_secs(5), rx.recv()).await {
                    Ok(Some(entry)) => {
                        buffer.push(entry);
                        // Drain remaining available entries
                        while buffer.len() < 100 {
                            match rx.try_recv() {
                                Ok(e) => buffer.push(e),
                                Err(_) => break,
                            }
                        }
                    }
                    Ok(None) => break, // Channel closed
                    Err(_) => {}       // Timeout - flush what we have
                }

                if !buffer.is_empty() {
                    for drain in &drains_clone {
                        if !drain.enabled {
                            continue;
                        }
                        let filtered: Vec<&LogEntry> = buffer
                            .iter()
                            .filter(|e| e.level >= drain.min_level)
                            .collect();

                        if filtered.is_empty() {
                            continue;
                        }

                        match drain.drain_type {
                            DrainType::Webhook => {
                                if let Some(url) = &drain.url {
                                    let payload = json!({
                                        "source": "stackhouse",
                                        "logs": filtered,
                                    });
                                    let mut req = http_client.post(url).json(&payload);
                                    if let Some(key) = &drain.api_key {
                                        req =
                                            req.header("Authorization", format!("Bearer {}", key));
                                    }
                                    if let Err(e) = req.send().await {
                                        eprintln!("Log drain webhook error: {}", e);
                                    }
                                }
                            }
                            DrainType::Stdout => {
                                for entry in &filtered {
                                    println!(
                                        "{}",
                                        serde_json::to_string(entry).unwrap_or_default()
                                    );
                                }
                            }
                            DrainType::Database => {
                                for entry in &filtered {
                                    let _ = store_clone.execute(
                                        "INSERT INTO stackhouse_logs (timestamp, level, message, service, metadata) VALUES ($1, $2, $3, $4, $5)".to_string(),
                                        vec![
                                            SqlValue::Integer(entry.timestamp as i64),
                                            SqlValue::Text(serde_json::to_string(&entry.level).unwrap_or_default()),
                                            SqlValue::Text(entry.message.clone()),
                                            SqlValue::Text(entry.service.clone()),
                                            SqlValue::Json(entry.metadata.clone()),
                                        ],
                                    ).await;
                                }
                            }
                        }
                    }
                    buffer.clear();
                }
            }
        });

        info!(
            "📋 Stackhouse-LogDrain initialized with {} drain(s)",
            service.drains.len()
        );
        Ok(service)
    }

    async fn initialize_tables(&self) -> StackhouseResult<()> {
        self.store
            .execute_batch(
                r#"
            CREATE TABLE IF NOT EXISTS stackhouse_logs (
                id BIGSERIAL PRIMARY KEY,
                timestamp BIGINT NOT NULL,
                level TEXT NOT NULL,
                message TEXT NOT NULL,
                service TEXT DEFAULT 'stackhouse',
                metadata JSONB DEFAULT '{}',
                created_at TIMESTAMPTZ DEFAULT NOW()
            );
            CREATE INDEX IF NOT EXISTS idx_stackhouse_logs_ts ON stackhouse_logs(timestamp DESC);
            CREATE INDEX IF NOT EXISTS idx_stackhouse_logs_level ON stackhouse_logs(level);
            "#
                .to_string(),
            )
            .await?;
        Ok(())
    }

    /// Log an entry (non-blocking, sent via channel)
    pub fn log(
        &self,
        level: LogLevel,
        message: impl Into<String>,
        service: impl Into<String>,
        metadata: Value,
    ) {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs();
        let entry = LogEntry {
            timestamp: now,
            level,
            message: message.into(),
            service: service.into(),
            metadata,
            request_id: None,
            user_id: None,
        };
        let _ = self.log_sender.try_send(entry);
    }

    /// Query stored logs
    pub async fn query_logs(
        &self,
        level: Option<&str>,
        service: Option<&str>,
        limit: i64,
        offset: i64,
    ) -> StackhouseResult<Vec<Value>> {
        let mut sql = "SELECT * FROM stackhouse_logs".to_string();
        let mut conditions = Vec::new();
        let mut params: Vec<SqlValue> = Vec::new();
        let mut param_idx = 1;

        if let Some(lvl) = level {
            conditions.push(format!("level = ${}", param_idx));
            params.push(SqlValue::Text(format!("\"{}\"", lvl)));
            param_idx += 1;
        }
        if let Some(svc) = service {
            conditions.push(format!("service = ${}", param_idx));
            params.push(SqlValue::Text(svc.to_string()));
            let _param_idx = param_idx + 1;
        }

        if !conditions.is_empty() {
            sql.push_str(&format!(" WHERE {}", conditions.join(" AND ")));
        }

        sql.push_str(&format!(
            " ORDER BY timestamp DESC LIMIT {} OFFSET {}",
            limit.min(1000),
            offset.max(0)
        ));

        let rows = self.store.query(sql, params).await?;
        let results: Vec<Value> = rows
            .into_iter()
            .map(|row| {
                let mut obj = serde_json::Map::new();
                for (k, v) in row {
                    obj.insert(k, v);
                }
                Value::Object(obj)
            })
            .collect();

        Ok(results)
    }
}

// ============================================================================
// State & Handlers
// ============================================================================

#[derive(Clone)]
pub struct LogDrainState {
    pub log_drain: LogDrainService,
    pub auth: AuthState,
    pub authorization: AuthorizationService,
    pub admin_audit: Arc<AdminAuditService>,
}

#[derive(Deserialize)]
struct LogQueryParams {
    #[serde(default)]
    level: Option<String>,
    #[serde(default)]
    service: Option<String>,
    #[serde(default = "default_limit")]
    limit: i64,
    #[serde(default)]
    offset: i64,
}

fn default_limit() -> i64 {
    100
}

async fn query_logs_handler(
    State(state): State<LogDrainState>,
    headers: HeaderMap,
    Query(params): Query<LogQueryParams>,
) -> Result<impl IntoResponse, StackhouseError> {
    let auth_user = authorize_service_admin(&state, &headers).await?;

    let logs = state
        .log_drain
        .query_logs(
            params.level.as_deref(),
            params.service.as_deref(),
            params.limit,
            params.offset,
        )
        .await?;
    state
        .admin_audit
        .record(
            auth_user.id,
            "log_drain.query_logs",
            "log",
            None,
            "success",
            json!({
                "route": "/v1/admin/logs",
                "level": params.level,
                "service": params.service,
                "limit": params.limit,
                "offset": params.offset,
                "count": logs.len(),
            }),
        )
        .await?;

    Ok(Json(json!({
        "success": true,
        "data": logs,
        "count": logs.len()
    })))
}

#[derive(Deserialize)]
struct AddDrainRequest {
    name: String,
    drain_type: DrainType,
    url: Option<String>,
    api_key: Option<String>,
    #[serde(default = "default_info_level")]
    min_level: LogLevel,
}

fn default_info_level() -> LogLevel {
    LogLevel::Info
}

async fn list_drains_handler(
    State(state): State<LogDrainState>,
    headers: HeaderMap,
) -> Result<impl IntoResponse, StackhouseError> {
    // Reuse the same service-admin gate as /logs.
    let auth_user = authorize_service_admin(&state, &headers).await?;

    let drains: Vec<Value> = state
        .log_drain
        .drains
        .iter()
        .map(|d| {
            json!({
                "name": d.name,
                "type": d.drain_type,
                "enabled": d.enabled,
                "min_level": d.min_level,
            })
        })
        .collect();
    state
        .admin_audit
        .record(
            auth_user.id,
            "log_drain.list_drains",
            "log_drain",
            None,
            "success",
            json!({"route": "/v1/admin/logs/drains", "count": drains.len()}),
        )
        .await?;

    Ok(Json(json!({
        "success": true,
        "data": drains
    })))
}

async fn authorize_service_admin(
    state: &LogDrainState,
    headers: &HeaderMap,
) -> Result<AuthUser, StackhouseError> {
    let auth_user = extract_auth_user(&state.auth, headers)?;
    let user = state.auth.auth.get_user_by_id(auth_user.id).await?;
    state
        .authorization
        .require_service_admin_unconditional(&user)?;
    Ok(auth_user)
}

pub fn create_log_drain_router(state: LogDrainState) -> Router {
    Router::new()
        .route("/logs", get(query_logs_handler))
        .route("/logs/drains", get(list_drains_handler))
        .with_state(state)
}
