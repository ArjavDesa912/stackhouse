//! # Function Execution Logs, Metrics & Cold-Start Profiling
//!
//! Captures invocation logs, latency distributions, cold-start timing,
//! error rates, and memory usage for edge functions.

use crate::db::{SqlValue, StackhouseStore};
use crate::error::StackhouseResult;

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::RwLock;
use tracing::info;

// ============================================================================
// Types
// ============================================================================

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FunctionInvocationLog {
    pub id: String,
    pub function_id: String,
    pub tenant_id: i64,
    pub status: InvocationResult,
    pub duration_ms: u64,
    pub cold_start: bool,
    pub cold_start_ms: Option<u64>,
    pub memory_used_mb: f64,
    pub request_size_bytes: u64,
    pub response_size_bytes: u64,
    pub error: Option<String>,
    pub logs: Vec<LogLine>,
    pub timestamp: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum InvocationResult {
    Success,
    Error,
    Timeout,
    OOM,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LogLine {
    pub level: String,
    pub message: String,
    pub timestamp: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FunctionMetricsSummary {
    pub function_id: String,
    pub total_invocations: u64,
    pub success_count: u64,
    pub error_count: u64,
    pub timeout_count: u64,
    pub avg_duration_ms: f64,
    pub p50_duration_ms: u64,
    pub p95_duration_ms: u64,
    pub p99_duration_ms: u64,
    pub cold_start_rate: f64,
    pub avg_cold_start_ms: f64,
    pub avg_memory_mb: f64,
    pub total_compute_ms: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ColdStartProfile {
    pub function_id: String,
    pub avg_cold_start_ms: f64,
    pub min_cold_start_ms: u64,
    pub max_cold_start_ms: u64,
    pub cold_start_count: u64,
    pub warm_start_count: u64,
    pub cold_start_pct: f64,
}

// ============================================================================
// Service
// ============================================================================

#[derive(Clone)]
pub struct FnMetricsService {
    store: Arc<StackhouseStore>,
    // In-memory buffer for batching writes
    buffer: Arc<RwLock<Vec<FunctionInvocationLog>>>,
}

impl FnMetricsService {
    pub async fn new(store: Arc<StackhouseStore>) -> StackhouseResult<Self> {
        let service = Self {
            store,
            buffer: Arc::new(RwLock::new(Vec::new())),
        };
        service.initialize_tables().await?;
        service.start_flush_worker();
        info!("📈 Function metrics service initialized");
        Ok(service)
    }

    async fn initialize_tables(&self) -> StackhouseResult<()> {
        self.store.execute_batch(r#"
            CREATE TABLE IF NOT EXISTS stackhouse_fn_invocations (
                id TEXT PRIMARY KEY,
                function_id TEXT NOT NULL,
                tenant_id BIGINT NOT NULL,
                status TEXT NOT NULL,
                duration_ms BIGINT NOT NULL,
                cold_start BOOLEAN DEFAULT FALSE,
                cold_start_ms BIGINT,
                memory_used_mb FLOAT,
                request_size_bytes BIGINT DEFAULT 0,
                response_size_bytes BIGINT DEFAULT 0,
                error TEXT,
                logs JSONB DEFAULT '[]',
                timestamp TIMESTAMPTZ DEFAULT NOW()
            );
            CREATE TABLE IF NOT EXISTS stackhouse_fn_metrics_rollup (
                function_id TEXT NOT NULL,
                tenant_id BIGINT NOT NULL,
                period TEXT NOT NULL,
                period_start TIMESTAMPTZ NOT NULL,
                invocations BIGINT DEFAULT 0,
                errors BIGINT DEFAULT 0,
                timeouts BIGINT DEFAULT 0,
                total_duration_ms BIGINT DEFAULT 0,
                cold_starts BIGINT DEFAULT 0,
                max_duration_ms BIGINT DEFAULT 0,
                PRIMARY KEY (function_id, tenant_id, period, period_start)
            );
            CREATE INDEX IF NOT EXISTS idx_fn_invocations_fn ON stackhouse_fn_invocations(function_id);
            CREATE INDEX IF NOT EXISTS idx_fn_invocations_tenant ON stackhouse_fn_invocations(tenant_id, timestamp);
        "#.to_string()).await?;
        Ok(())
    }

    fn start_flush_worker(&self) {
        let store = Arc::clone(&self.store);
        let buffer = Arc::clone(&self.buffer);
        tokio::spawn(async move {
            let mut interval = tokio::time::interval(Duration::from_secs(5));
            loop {
                interval.tick().await;
                let logs: Vec<FunctionInvocationLog> = {
                    let mut buf = buffer.write().await;
                    std::mem::take(&mut *buf)
                };
                for log in logs {
                    let status_str = serde_json::to_string(&log.status)
                        .unwrap_or_default()
                        .trim_matches('"')
                        .to_string();
                    store.execute(
                        "INSERT INTO stackhouse_fn_invocations (id, function_id, tenant_id, status, duration_ms, cold_start, cold_start_ms, memory_used_mb, request_size_bytes, response_size_bytes, error, logs) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?::jsonb)".to_string(),
                        vec![
                            SqlValue::Text(log.id),
                            SqlValue::Text(log.function_id),
                            SqlValue::Integer(log.tenant_id),
                            SqlValue::Text(status_str),
                            SqlValue::Integer(log.duration_ms as i64),
                            SqlValue::Text(log.cold_start.to_string()),
                            SqlValue::Integer(log.cold_start_ms.unwrap_or(0) as i64),
                            SqlValue::Text(log.memory_used_mb.to_string()),
                            SqlValue::Integer(log.request_size_bytes as i64),
                            SqlValue::Integer(log.response_size_bytes as i64),
                            SqlValue::Text(log.error.unwrap_or_default()),
                            SqlValue::Text(serde_json::to_string(&log.logs).unwrap_or_default()),
                        ],
                    ).await.ok();
                }
            }
        });
    }

    /// Record a function invocation
    pub async fn record_invocation(&self, log: FunctionInvocationLog) {
        self.buffer.write().await.push(log);
    }

    /// Get metrics summary for a function
    pub async fn get_metrics(&self, function_id: &str, tenant_id: i64) -> StackhouseResult<Value> {
        let rows = self
            .store
            .query(
                r#"SELECT
                COUNT(*) as total,
                COUNT(*) FILTER (WHERE status = 'success') as success,
                COUNT(*) FILTER (WHERE status = 'error') as errors,
                COUNT(*) FILTER (WHERE status = 'timeout') as timeouts,
                AVG(duration_ms) as avg_ms,
                PERCENTILE_CONT(0.5) WITHIN GROUP (ORDER BY duration_ms) as p50,
                PERCENTILE_CONT(0.95) WITHIN GROUP (ORDER BY duration_ms) as p95,
                PERCENTILE_CONT(0.99) WITHIN GROUP (ORDER BY duration_ms) as p99,
                AVG(CASE WHEN cold_start THEN 1.0 ELSE 0.0 END) as cold_rate,
                AVG(CASE WHEN cold_start THEN cold_start_ms ELSE NULL END) as avg_cold,
                AVG(memory_used_mb) as avg_mem
            FROM stackhouse_fn_invocations
            WHERE function_id = ? AND tenant_id = ?"#
                    .to_string(),
                vec![
                    SqlValue::Text(function_id.to_string()),
                    SqlValue::Integer(tenant_id),
                ],
            )
            .await?;

        if rows.is_empty() {
            return Ok(json!({}));
        }
        Ok(json!(rows[0].iter().cloned().collect::<HashMap<_, _>>()))
    }

    /// Get cold-start profile
    pub async fn get_cold_start_profile(
        &self,
        function_id: &str,
        tenant_id: i64,
    ) -> StackhouseResult<Value> {
        let rows = self
            .store
            .query(
                r#"SELECT
                AVG(cold_start_ms) FILTER (WHERE cold_start) as avg_cold_ms,
                MIN(cold_start_ms) FILTER (WHERE cold_start) as min_cold_ms,
                MAX(cold_start_ms) FILTER (WHERE cold_start) as max_cold_ms,
                COUNT(*) FILTER (WHERE cold_start) as cold_count,
                COUNT(*) FILTER (WHERE NOT cold_start) as warm_count
            FROM stackhouse_fn_invocations
            WHERE function_id = ? AND tenant_id = ?"#
                    .to_string(),
                vec![
                    SqlValue::Text(function_id.to_string()),
                    SqlValue::Integer(tenant_id),
                ],
            )
            .await?;

        if rows.is_empty() {
            return Ok(json!({}));
        }
        Ok(json!(rows[0].iter().cloned().collect::<HashMap<_, _>>()))
    }

    /// Get recent logs for a function
    pub async fn get_logs(
        &self,
        function_id: &str,
        tenant_id: i64,
        limit: usize,
    ) -> StackhouseResult<Vec<Value>> {
        let rows = self.store.query(
            format!("SELECT id, status, duration_ms, cold_start, error, logs, timestamp FROM stackhouse_fn_invocations WHERE function_id = ? AND tenant_id = ? ORDER BY timestamp DESC LIMIT {}", limit),
            vec![SqlValue::Text(function_id.to_string()), SqlValue::Integer(tenant_id)],
        ).await?;
        Ok(rows
            .into_iter()
            .map(|r| json!(r.into_iter().collect::<HashMap<_, _>>()))
            .collect())
    }
}
