//! # Real-time Metrics Dashboard
//!
//! Latency, error rate, throughput, and percentile tracking
//! with time-series storage and query capabilities.

use crate::db::{SqlValue, StackhouseStore};
use crate::error::StackhouseResult;

use chrono::Timelike;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::HashMap;
use std::sync::Arc;
use tracing::info;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MetricPoint {
    pub metric_name: String,
    pub tenant_id: i64,
    pub service: String,
    pub value: f64,
    pub labels: HashMap<String, String>,
    pub timestamp: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MetricSeries {
    pub metric_name: String,
    pub points: Vec<(String, f64)>, // (timestamp, value)
    pub labels: HashMap<String, String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DashboardQuery {
    pub tenant_id: i64,
    pub metric_names: Vec<String>,
    pub from: String,
    pub to: String,
    pub granularity: String, // "1m", "5m", "1h", "1d"
    pub service: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DashboardSummary {
    pub total_requests: u64,
    pub error_rate: f64,
    pub avg_latency_ms: f64,
    pub p50_latency_ms: f64,
    pub p95_latency_ms: f64,
    pub p99_latency_ms: f64,
    pub throughput_rps: f64,
    pub active_connections: u64,
}

#[derive(Clone)]
pub struct MetricsDashboardService {
    store: Arc<StackhouseStore>,
}

impl MetricsDashboardService {
    pub async fn new(store: Arc<StackhouseStore>) -> StackhouseResult<Self> {
        let service = Self { store };
        service.initialize_tables().await?;
        info!("📊 Metrics dashboard service initialized");
        Ok(service)
    }

    async fn initialize_tables(&self) -> StackhouseResult<()> {
        self.store.execute_batch(r#"
            CREATE TABLE IF NOT EXISTS stackhouse_metrics_time_series (
                id BIGSERIAL PRIMARY KEY,
                metric_name TEXT NOT NULL,
                tenant_id BIGINT NOT NULL,
                service TEXT NOT NULL,
                value FLOAT NOT NULL,
                labels JSONB DEFAULT '{}',
                bucket TIMESTAMPTZ NOT NULL
            );
            CREATE TABLE IF NOT EXISTS stackhouse_metrics_latency (
                id BIGSERIAL PRIMARY KEY,
                tenant_id BIGINT NOT NULL,
                service TEXT NOT NULL,
                endpoint TEXT NOT NULL,
                latency_ms FLOAT NOT NULL,
                status_code INTEGER,
                timestamp TIMESTAMPTZ DEFAULT NOW()
            );
            CREATE INDEX IF NOT EXISTS idx_metrics_series ON stackhouse_metrics_time_series(metric_name, tenant_id, bucket);
            CREATE INDEX IF NOT EXISTS idx_metrics_latency ON stackhouse_metrics_latency(tenant_id, service, timestamp);
        "#.to_string()).await?;
        Ok(())
    }

    /// Record a metric point
    pub async fn record(&self, point: MetricPoint) -> StackhouseResult<()> {
        let bucket = self.truncate_timestamp(&point.timestamp, &point.metric_name);
        self.store.execute(
            "INSERT INTO stackhouse_metrics_time_series (metric_name, tenant_id, service, value, labels, bucket) VALUES (?, ?, ?, ?, ?::jsonb, ?::timestamptz)".to_string(),
            vec![
                SqlValue::Text(point.metric_name),
                SqlValue::Integer(point.tenant_id),
                SqlValue::Text(point.service),
                SqlValue::Text(point.value.to_string()),
                SqlValue::Text(serde_json::to_string(&point.labels).unwrap_or_default()),
                SqlValue::Text(bucket),
            ],
        ).await?;
        Ok(())
    }

    /// Record an HTTP request latency
    pub async fn record_request(
        &self,
        tenant_id: i64,
        service: &str,
        endpoint: &str,
        latency_ms: f64,
        status_code: u16,
    ) -> StackhouseResult<()> {
        self.store.execute(
            "INSERT INTO stackhouse_metrics_latency (tenant_id, service, endpoint, latency_ms, status_code) VALUES (?, ?, ?, ?, ?)".to_string(),
            vec![
                SqlValue::Integer(tenant_id),
                SqlValue::Text(service.to_string()),
                SqlValue::Text(endpoint.to_string()),
                SqlValue::Text(latency_ms.to_string()),
                SqlValue::Integer(status_code as i64),
            ],
        ).await?;
        Ok(())
    }

    /// Get dashboard summary for a time range
    pub async fn get_summary(
        &self,
        tenant_id: i64,
        from: &str,
        to: &str,
    ) -> StackhouseResult<DashboardSummary> {
        let total_rows = self.store.query(
            "SELECT COUNT(*) as total FROM stackhouse_metrics_latency WHERE tenant_id = ? AND timestamp >= ?::timestamptz AND timestamp <= ?::timestamptz".to_string(),
            vec![SqlValue::Integer(tenant_id), SqlValue::Text(from.to_string()), SqlValue::Text(to.to_string())],
        ).await?;
        let total = total_rows
            .first()
            .and_then(|r| r.iter().find(|(k, _)| k == "total"))
            .and_then(|(_, v)| v.as_i64())
            .unwrap_or(0) as u64;

        let error_rows = self.store.query(
            "SELECT COUNT(*) as errors FROM stackhouse_metrics_latency WHERE tenant_id = ? AND timestamp >= ?::timestamptz AND timestamp <= ?::timestamptz AND status_code >= 500".to_string(),
            vec![SqlValue::Integer(tenant_id), SqlValue::Text(from.to_string()), SqlValue::Text(to.to_string())],
        ).await?;
        let errors = error_rows
            .first()
            .and_then(|r| r.iter().find(|(k, _)| k == "errors"))
            .and_then(|(_, v)| v.as_i64())
            .unwrap_or(0) as u64;

        let latency_rows = self
            .store
            .query(
                r#"SELECT
                AVG(latency_ms) as avg,
                PERCENTILE_CONT(0.5) WITHIN GROUP (ORDER BY latency_ms) as p50,
                PERCENTILE_CONT(0.95) WITHIN GROUP (ORDER BY latency_ms) as p95,
                PERCENTILE_CONT(0.99) WITHIN GROUP (ORDER BY latency_ms) as p99
            FROM stackhouse_metrics_latency
            WHERE tenant_id = ? AND timestamp >= ?::timestamptz AND timestamp <= ?::timestamptz"#
                    .to_string(),
                vec![
                    SqlValue::Integer(tenant_id),
                    SqlValue::Text(from.to_string()),
                    SqlValue::Text(to.to_string()),
                ],
            )
            .await?;

        let avg = latency_rows
            .first()
            .and_then(|r| r.iter().find(|(k, _)| k == "avg"))
            .and_then(|(_, v)| v.as_f64())
            .unwrap_or(0.0);
        let p50 = latency_rows
            .first()
            .and_then(|r| r.iter().find(|(k, _)| k == "p50"))
            .and_then(|(_, v)| v.as_f64())
            .unwrap_or(0.0);
        let p95 = latency_rows
            .first()
            .and_then(|r| r.iter().find(|(k, _)| k == "p95"))
            .and_then(|(_, v)| v.as_f64())
            .unwrap_or(0.0);
        let p99 = latency_rows
            .first()
            .and_then(|r| r.iter().find(|(k, _)| k == "p99"))
            .and_then(|(_, v)| v.as_f64())
            .unwrap_or(0.0);

        let error_rate = if total > 0 {
            (errors as f64 / total as f64) * 100.0
        } else {
            0.0
        };
        let from_dt = chrono::DateTime::parse_from_rfc3339(from)
            .map(|d| d.timestamp())
            .unwrap_or(0);
        let to_dt = chrono::DateTime::parse_from_rfc3339(to)
            .map(|d| d.timestamp())
            .unwrap_or(1);
        let duration_secs = (to_dt - from_dt).max(1) as f64;
        let throughput = total as f64 / duration_secs;

        Ok(DashboardSummary {
            total_requests: total,
            error_rate,
            avg_latency_ms: avg,
            p50_latency_ms: p50,
            p95_latency_ms: p95,
            p99_latency_ms: p99,
            throughput_rps: throughput,
            active_connections: 0,
        })
    }

    /// Get time-series data for charting
    pub async fn get_series(&self, q: &DashboardQuery) -> StackhouseResult<Vec<MetricSeries>> {
        let mut results = Vec::new();

        for metric in &q.metric_names {
            let service_filter = q
                .service
                .as_ref()
                .map(|s| format!("AND service = '{}'", s))
                .unwrap_or_default();
            let interval = match q.granularity.as_str() {
                "1m" => "1 minute",
                "5m" => "5 minutes",
                "1h" => "1 hour",
                "1d" => "1 day",
                _ => "5 minutes",
            };

            let rows = self.store.query(
                format!(r#"SELECT
                    date_trunc('{}', bucket) as ts,
                    AVG(value) as avg_value
                FROM stackhouse_metrics_time_series
                WHERE metric_name = ? AND tenant_id = ? AND bucket >= ?::timestamptz AND bucket <= ?::timestamptz {}
                GROUP BY date_trunc('{}', bucket)
                ORDER BY ts"#, interval, service_filter, interval),
                vec![
                    SqlValue::Text(metric.clone()),
                    SqlValue::Integer(q.tenant_id),
                    SqlValue::Text(q.from.clone()),
                    SqlValue::Text(q.to.clone()),
                ],
            ).await?;

            let points: Vec<(String, f64)> = rows
                .into_iter()
                .filter_map(|row| {
                    let ts = row
                        .iter()
                        .find(|(k, _)| k == "ts")
                        .and_then(|(_, v)| v.as_str())?;
                    let val = row
                        .iter()
                        .find(|(k, _)| k == "avg_value")
                        .and_then(|(_, v)| v.as_f64())?;
                    Some((ts.to_string(), val))
                })
                .collect();

            results.push(MetricSeries {
                metric_name: metric.clone(),
                points,
                labels: HashMap::new(),
            });
        }

        Ok(results)
    }

    fn truncate_timestamp(&self, ts: &str, _metric: &str) -> String {
        let dt: chrono::DateTime<chrono::FixedOffset> = chrono::DateTime::parse_from_rfc3339(ts)
            .unwrap_or_else(|_| chrono::DateTime::from(chrono::Utc::now()));
        let bucket = dt.with_nanosecond(0).unwrap_or(dt);
        bucket.to_rfc3339()
    }

    /// Get top slowest endpoints
    pub async fn get_slow_endpoints(
        &self,
        tenant_id: i64,
        from: &str,
        to: &str,
        limit: usize,
    ) -> StackhouseResult<Vec<Value>> {
        let rows = self
            .store
            .query(
                format!(
                    r#"SELECT service, endpoint, AVG(latency_ms) as avg_latency, COUNT(*) as count
                FROM stackhouse_metrics_latency
                WHERE tenant_id = ? AND timestamp >= ?::timestamptz AND timestamp <= ?::timestamptz
                GROUP BY service, endpoint
                ORDER BY avg_latency DESC
                LIMIT {}"#,
                    limit
                ),
                vec![
                    SqlValue::Integer(tenant_id),
                    SqlValue::Text(from.to_string()),
                    SqlValue::Text(to.to_string()),
                ],
            )
            .await?;
        Ok(rows
            .into_iter()
            .map(|r| json!(r.iter().cloned().collect::<HashMap<_, _>>()))
            .collect())
    }
}
