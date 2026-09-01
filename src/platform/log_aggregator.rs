//! # Centralized Structured Logs with Full-Text Query
//!
//! Aggregates logs from all services with full-text search, filtering,
//! and retention policies. Compatible with OpenTelemetry log format.

use crate::db::{SqlValue, StackhouseStore};
use crate::error::StackhouseResult;

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::HashMap;
use std::sync::Arc;
use tracing::info;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LogEntry {
    pub id: String,
    pub tenant_id: Option<i64>,
    pub service: String,
    pub level: String,
    pub message: String,
    pub attributes: HashMap<String, Value>,
    pub trace_id: Option<String>,
    pub span_id: Option<String>,
    pub timestamp: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LogQuery {
    pub tenant_id: Option<i64>,
    pub service: Option<String>,
    pub level: Option<String>,
    pub message_query: Option<String>,
    pub from: Option<String>,
    pub to: Option<String>,
    pub attributes: Option<HashMap<String, String>>,
    pub limit: usize,
}

#[derive(Clone)]
pub struct LogAggregator {
    store: Arc<StackhouseStore>,
}

impl LogAggregator {
    pub async fn new(store: Arc<StackhouseStore>) -> StackhouseResult<Self> {
        let service = Self { store };
        service.initialize_tables().await?;
        info!("📝 Log aggregator initialized");
        Ok(service)
    }

    async fn initialize_tables(&self) -> StackhouseResult<()> {
        self.store.execute_batch(r#"
            CREATE TABLE IF NOT EXISTS stackhouse_structured_logs (
                id TEXT PRIMARY KEY,
                tenant_id BIGINT,
                service TEXT NOT NULL,
                level TEXT NOT NULL,
                message TEXT NOT NULL,
                attributes JSONB DEFAULT '{}',
                trace_id TEXT,
                span_id TEXT,
                timestamp TIMESTAMPTZ DEFAULT NOW()
            );
            CREATE INDEX IF NOT EXISTS idx_logs_tenant ON stackhouse_structured_logs(tenant_id, timestamp);
            CREATE INDEX IF NOT EXISTS idx_logs_service ON stackhouse_structured_logs(service, timestamp);
            CREATE INDEX IF NOT EXISTS idx_logs_level ON stackhouse_structured_logs(level, timestamp);
            CREATE INDEX IF NOT EXISTS idx_logs_trace ON stackhouse_structured_logs(trace_id);
            CREATE INDEX IF NOT EXISTS idx_logs_message ON stackhouse_structured_logs USING gin(to_tsvector('english', message));
        "#.to_string()).await?;
        Ok(())
    }

    /// Ingest a log entry
    pub async fn ingest(&self, entry: LogEntry) -> StackhouseResult<()> {
        self.store.execute(
            "INSERT INTO stackhouse_structured_logs (id, tenant_id, service, level, message, attributes, trace_id, span_id, timestamp) VALUES (?, ?, ?, ?, ?, ?::jsonb, ?, ?, ?::timestamptz)".to_string(),
            vec![
                SqlValue::Text(entry.id),
                SqlValue::Text(entry.tenant_id.map(|t| t.to_string()).unwrap_or_default()),
                SqlValue::Text(entry.service),
                SqlValue::Text(entry.level),
                SqlValue::Text(entry.message),
                SqlValue::Text(serde_json::to_string(&entry.attributes).unwrap_or_default()),
                SqlValue::Text(entry.trace_id.unwrap_or_default()),
                SqlValue::Text(entry.span_id.unwrap_or_default()),
                SqlValue::Text(entry.timestamp),
            ],
        ).await?;
        Ok(())
    }

    /// Batch ingest logs
    pub async fn ingest_batch(&self, entries: Vec<LogEntry>) -> StackhouseResult<u32> {
        let mut count = 0u32;
        for entry in entries {
            self.ingest(entry).await?;
            count += 1;
        }
        Ok(count)
    }

    /// Query logs with full-text and filters
    pub async fn query(&self, q: &LogQuery) -> StackhouseResult<Vec<Value>> {
        let mut conditions = vec!["1=1".to_string()];
        let mut params = Vec::new();

        if let Some(tenant) = q.tenant_id {
            conditions.push("tenant_id = ?".to_string());
            params.push(SqlValue::Integer(tenant));
        }
        if let Some(service) = &q.service {
            conditions.push("service = ?".to_string());
            params.push(SqlValue::Text(service.clone()));
        }
        if let Some(level) = &q.level {
            conditions.push("level = ?".to_string());
            params.push(SqlValue::Text(level.clone()));
        }
        if let Some(msg) = &q.message_query {
            conditions.push(
                "to_tsvector('english', message) @@ plainto_tsquery('english', ?)".to_string(),
            );
            params.push(SqlValue::Text(msg.clone()));
        }
        if let Some(from) = &q.from {
            conditions.push("timestamp >= ?::timestamptz".to_string());
            params.push(SqlValue::Text(from.clone()));
        }
        if let Some(to) = &q.to {
            conditions.push("timestamp <= ?::timestamptz".to_string());
            params.push(SqlValue::Text(to.clone()));
        }
        if let Some(attrs) = &q.attributes {
            for (k, v) in attrs {
                conditions.push(format!("attributes @> ?::jsonb"));
                params.push(SqlValue::Text(json!({k: v}).to_string()));
            }
        }

        let sql = format!(
            "SELECT id, service, level, message, attributes, trace_id, timestamp FROM stackhouse_structured_logs WHERE {} ORDER BY timestamp DESC LIMIT {}",
            conditions.join(" AND "), q.limit
        );

        let rows = self.store.query(sql, params).await?;
        Ok(rows
            .into_iter()
            .map(|r| json!(r.iter().cloned().collect::<HashMap<_, _>>()))
            .collect())
    }

    /// Get log summary stats
    pub async fn get_summary(
        &self,
        tenant_id: Option<i64>,
        from: &str,
        to: &str,
    ) -> StackhouseResult<Value> {
        let tenant_filter = tenant_id
            .map(|t| format!("AND tenant_id = {}", t))
            .unwrap_or_default();
        let rows = self
            .store
            .query(
                format!(
                    r#"SELECT
                COUNT(*) as total,
                COUNT(*) FILTER (WHERE level = 'ERROR') as errors,
                COUNT(*) FILTER (WHERE level = 'WARN') as warnings,
                COUNT(*) FILTER (WHERE level = 'INFO') as infos,
                COUNT(DISTINCT service) as services,
                COUNT(DISTINCT trace_id) as traces
            FROM stackhouse_structured_logs
            WHERE timestamp >= ?::timestamptz AND timestamp <= ?::timestamptz {}"#,
                    tenant_filter
                ),
                vec![
                    SqlValue::Text(from.to_string()),
                    SqlValue::Text(to.to_string()),
                ],
            )
            .await?;
        Ok(json!(rows[0].iter().cloned().collect::<HashMap<_, _>>()))
    }

    /// Cleanup old logs
    pub async fn cleanup(&self, retention_days: u32) -> StackhouseResult<u64> {
        self.store.execute(
            format!("DELETE FROM stackhouse_structured_logs WHERE timestamp < NOW() - INTERVAL '{} days'", retention_days),
            vec![],
        ).await?;
        Ok(0)
    }
}
