//! # Connection Pooling Service
//!
//! PgBouncer-compatible pool configuration, pool stats endpoints,
//! and per-tenant configurable pool sizes.

use crate::auth::{extract_auth_user, AuthState};
use crate::db::{SqlValue, StackhouseStore};
use crate::error::{StackhouseError, StackhouseResult};

use axum::{
    extract::State,
    http::HeaderMap,
    response::IntoResponse,
    routing::{get, put},
    Json, Router,
};
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::info;

// ============================================================================
// Types
// ============================================================================

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PoolConfig {
    pub tenant_id: i64,
    pub pool_mode: PoolMode,
    pub max_connections: u32,
    pub min_connections: u32,
    pub idle_timeout_secs: u64,
    pub max_lifetime_secs: u64,
    pub connection_timeout_secs: u64,
    pub statement_timeout_secs: u64,
    pub query_wait_timeout_secs: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PoolMode {
    Transaction,
    Session,
    Statement,
}

impl Default for PoolConfig {
    fn default() -> Self {
        Self {
            tenant_id: 0,
            pool_mode: PoolMode::Transaction,
            max_connections: 100,
            min_connections: 5,
            idle_timeout_secs: 600,
            max_lifetime_secs: 1800,
            connection_timeout_secs: 30,
            statement_timeout_secs: 60,
            query_wait_timeout_secs: 120,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PoolStats {
    pub total_connections: u64,
    pub active_connections: u64,
    pub idle_connections: u64,
    pub waiting_clients: u64,
    pub max_connections: u32,
    pub queries_executed: u64,
    pub queries_waiting: u64,
    pub avg_query_time_ms: f64,
    pub pool_mode: String,
    pub uptime_secs: u64,
}

// ============================================================================
// Pooling Service
// ============================================================================

#[derive(Clone)]
pub struct PoolingService {
    store: Arc<StackhouseStore>,
    stats: Arc<PoolStatsInner>,
    configs: Arc<RwLock<HashMap<i64, PoolConfig>>>,
}

struct PoolStatsInner {
    total_connections: AtomicU64,
    active_connections: AtomicU64,
    idle_connections: AtomicU64,
    waiting_clients: AtomicU64,
    queries_executed: AtomicU64,
    total_query_time_us: AtomicU64,
    start_time: std::time::Instant,
}

impl PoolingService {
    pub async fn new(store: Arc<StackhouseStore>) -> StackhouseResult<Self> {
        let service = Self {
            store,
            stats: Arc::new(PoolStatsInner {
                total_connections: AtomicU64::new(0),
                active_connections: AtomicU64::new(0),
                idle_connections: AtomicU64::new(0),
                waiting_clients: AtomicU64::new(0),
                queries_executed: AtomicU64::new(0),
                total_query_time_us: AtomicU64::new(0),
                start_time: std::time::Instant::now(),
            }),
            configs: Arc::new(RwLock::new(HashMap::new())),
        };
        service.initialize_tables().await?;
        service.load_configs().await?;
        info!("🔌 Connection pooling service initialized");
        Ok(service)
    }

    async fn initialize_tables(&self) -> StackhouseResult<()> {
        self.store
            .execute_batch(
                r#"
            CREATE TABLE IF NOT EXISTS stackhouse_pool_configs (
                tenant_id BIGINT PRIMARY KEY,
                pool_mode TEXT NOT NULL DEFAULT 'transaction',
                max_connections INTEGER NOT NULL DEFAULT 100,
                min_connections INTEGER NOT NULL DEFAULT 5,
                idle_timeout_secs INTEGER NOT NULL DEFAULT 600,
                max_lifetime_secs INTEGER NOT NULL DEFAULT 1800,
                connection_timeout_secs INTEGER NOT NULL DEFAULT 30,
                statement_timeout_secs INTEGER NOT NULL DEFAULT 60,
                query_wait_timeout_secs INTEGER NOT NULL DEFAULT 120,
                updated_at TIMESTAMPTZ DEFAULT NOW()
            );
        "#
                .to_string(),
            )
            .await?;
        Ok(())
    }

    async fn load_configs(&self) -> StackhouseResult<()> {
        let rows = self.store.query(
            "SELECT tenant_id, pool_mode, max_connections, min_connections, idle_timeout_secs, max_lifetime_secs, connection_timeout_secs, statement_timeout_secs, query_wait_timeout_secs FROM stackhouse_pool_configs".to_string(),
            vec![],
        ).await?;

        let mut configs = self.configs.write().await;
        for row in rows {
            let get = |key: &str| row.iter().find(|(k, _)| k == key).map(|(_, v)| v.clone());
            let tenant_id = get("tenant_id").and_then(|v| v.as_i64()).unwrap_or(0);
            let mode = match get("pool_mode")
                .and_then(|v| v.as_str().map(String::from))
                .unwrap_or_default()
                .as_str()
            {
                "session" => PoolMode::Session,
                "statement" => PoolMode::Statement,
                _ => PoolMode::Transaction,
            };
            configs.insert(
                tenant_id,
                PoolConfig {
                    tenant_id,
                    pool_mode: mode,
                    max_connections: get("max_connections")
                        .and_then(|v| v.as_i64())
                        .unwrap_or(100) as u32,
                    min_connections: get("min_connections").and_then(|v| v.as_i64()).unwrap_or(5)
                        as u32,
                    idle_timeout_secs: get("idle_timeout_secs")
                        .and_then(|v| v.as_i64())
                        .unwrap_or(600) as u64,
                    max_lifetime_secs: get("max_lifetime_secs")
                        .and_then(|v| v.as_i64())
                        .unwrap_or(1800) as u64,
                    connection_timeout_secs: get("connection_timeout_secs")
                        .and_then(|v| v.as_i64())
                        .unwrap_or(30) as u64,
                    statement_timeout_secs: get("statement_timeout_secs")
                        .and_then(|v| v.as_i64())
                        .unwrap_or(60) as u64,
                    query_wait_timeout_secs: get("query_wait_timeout_secs")
                        .and_then(|v| v.as_i64())
                        .unwrap_or(120) as u64,
                },
            );
        }
        Ok(())
    }

    /// Get pool config for a tenant (or default)
    pub async fn get_config(&self, tenant_id: i64) -> PoolConfig {
        let configs = self.configs.read().await;
        configs
            .get(&tenant_id)
            .cloned()
            .unwrap_or_else(|| PoolConfig {
                tenant_id,
                ..Default::default()
            })
    }

    /// Update pool config for a tenant
    pub async fn set_config(&self, config: PoolConfig) -> StackhouseResult<()> {
        let mode_str = match &config.pool_mode {
            PoolMode::Transaction => "transaction",
            PoolMode::Session => "session",
            PoolMode::Statement => "statement",
        };

        self.store.execute(
            r#"INSERT INTO stackhouse_pool_configs (tenant_id, pool_mode, max_connections, min_connections, idle_timeout_secs, max_lifetime_secs, connection_timeout_secs, statement_timeout_secs, query_wait_timeout_secs, updated_at)
               VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, NOW())
               ON CONFLICT (tenant_id) DO UPDATE SET pool_mode = EXCLUDED.pool_mode, max_connections = EXCLUDED.max_connections,
               min_connections = EXCLUDED.min_connections, idle_timeout_secs = EXCLUDED.idle_timeout_secs,
               max_lifetime_secs = EXCLUDED.max_lifetime_secs, connection_timeout_secs = EXCLUDED.connection_timeout_secs,
               statement_timeout_secs = EXCLUDED.statement_timeout_secs, query_wait_timeout_secs = EXCLUDED.query_wait_timeout_secs, updated_at = NOW()"#.to_string(),
            vec![
                SqlValue::Integer(config.tenant_id),
                SqlValue::Text(mode_str.to_string()),
                SqlValue::Integer(config.max_connections as i64),
                SqlValue::Integer(config.min_connections as i64),
                SqlValue::Integer(config.idle_timeout_secs as i64),
                SqlValue::Integer(config.max_lifetime_secs as i64),
                SqlValue::Integer(config.connection_timeout_secs as i64),
                SqlValue::Integer(config.statement_timeout_secs as i64),
                SqlValue::Integer(config.query_wait_timeout_secs as i64),
            ],
        ).await?;

        // Update in-memory cache
        let mut configs = self.configs.write().await;
        configs.insert(config.tenant_id, config);
        Ok(())
    }

    /// Record a query execution for stats
    pub fn record_query(&self, duration_us: u64) {
        self.stats.queries_executed.fetch_add(1, Ordering::Relaxed);
        self.stats
            .total_query_time_us
            .fetch_add(duration_us, Ordering::Relaxed);
    }

    /// Get current pool stats
    pub fn get_stats(&self) -> PoolStats {
        let queries = self.stats.queries_executed.load(Ordering::Relaxed);
        let total_time = self.stats.total_query_time_us.load(Ordering::Relaxed);
        let avg_time = if queries > 0 {
            (total_time as f64 / queries as f64) / 1000.0
        } else {
            0.0
        };

        PoolStats {
            total_connections: self.stats.total_connections.load(Ordering::Relaxed),
            active_connections: self.stats.active_connections.load(Ordering::Relaxed),
            idle_connections: self.stats.idle_connections.load(Ordering::Relaxed),
            waiting_clients: self.stats.waiting_clients.load(Ordering::Relaxed),
            max_connections: 100,
            queries_executed: queries,
            queries_waiting: self.stats.waiting_clients.load(Ordering::Relaxed),
            avg_query_time_ms: avg_time,
            pool_mode: "transaction".to_string(),
            uptime_secs: self.stats.start_time.elapsed().as_secs(),
        }
    }
}

// ============================================================================
// Router
// ============================================================================

#[derive(Clone)]
pub struct PoolingState {
    pub pooling: Arc<PoolingService>,
    pub auth: AuthState,
}

#[derive(Deserialize)]
struct UpdatePoolConfigRequest {
    #[serde(default = "default_pool_mode")]
    pool_mode: String,
    #[serde(default = "default_max_conn")]
    max_connections: u32,
    #[serde(default = "default_min_conn")]
    min_connections: u32,
    #[serde(default = "default_idle_timeout")]
    idle_timeout_secs: u64,
}
fn default_pool_mode() -> String {
    "transaction".to_string()
}
fn default_max_conn() -> u32 {
    100
}
fn default_min_conn() -> u32 {
    5
}
fn default_idle_timeout() -> u64 {
    600
}

async fn get_stats_handler(
    State(state): State<PoolingState>,
    headers: HeaderMap,
) -> Result<impl IntoResponse, StackhouseError> {
    let _user = extract_auth_user(&state.auth, &headers)?;
    let stats = state.pooling.get_stats();
    Ok(Json(json!({"success": true, "data": stats})))
}

async fn get_config_handler(
    State(state): State<PoolingState>,
    headers: HeaderMap,
) -> Result<impl IntoResponse, StackhouseError> {
    let user = extract_auth_user(&state.auth, &headers)?;
    let config = state.pooling.get_config(user.id).await;
    Ok(Json(json!({"success": true, "data": config})))
}

async fn update_config_handler(
    State(state): State<PoolingState>,
    headers: HeaderMap,
    Json(req): Json<UpdatePoolConfigRequest>,
) -> Result<impl IntoResponse, StackhouseError> {
    let user = extract_auth_user(&state.auth, &headers)?;
    let pool_mode = match req.pool_mode.as_str() {
        "session" => PoolMode::Session,
        "statement" => PoolMode::Statement,
        _ => PoolMode::Transaction,
    };
    let config = PoolConfig {
        tenant_id: user.id,
        pool_mode,
        max_connections: req.max_connections,
        min_connections: req.min_connections,
        idle_timeout_secs: req.idle_timeout_secs,
        ..Default::default()
    };
    state.pooling.set_config(config.clone()).await?;
    Ok(Json(json!({"success": true, "data": config})))
}

pub fn create_pooling_router(state: PoolingState) -> Router {
    Router::new()
        .route("/stats", get(get_stats_handler))
        .route("/config", get(get_config_handler))
        .route("/config", put(update_config_handler))
        .with_state(state)
}
