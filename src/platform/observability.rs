//! # Observability: Distributed Tracing, Alerting, Status Pages, Error Tracking
//!
//! OpenTelemetry-compatible tracing, custom alerting rules, public status pages,
//! structured error tracking, and query performance insights.

use crate::db::{SqlValue, StackhouseStore};
use crate::error::StackhouseResult;

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::RwLock;
use tracing::{debug, info};

// ============================================================================
// Distributed Tracing
// ============================================================================

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TraceSpan {
    pub trace_id: String,
    pub span_id: String,
    pub parent_span_id: Option<String>,
    pub operation: String,
    pub service: String,
    pub status: SpanStatus,
    pub duration_ms: u64,
    pub attributes: HashMap<String, String>,
    pub events: Vec<SpanEvent>,
    pub started_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SpanStatus {
    Ok,
    Error,
    Unset,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SpanEvent {
    pub name: String,
    pub timestamp: String,
    pub attributes: HashMap<String, String>,
}

// ============================================================================
// Alerting
// ============================================================================

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AlertRule {
    pub id: String,
    pub tenant_id: i64,
    pub name: String,
    pub condition: AlertCondition,
    pub channels: Vec<AlertChannel>,
    pub cooldown_secs: u64,
    pub enabled: bool,
    pub last_triggered: Option<String>,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AlertCondition {
    ErrorRateAbove {
        threshold_pct: f64,
        window_secs: u64,
    },
    LatencyAbove {
        threshold_ms: u64,
        percentile: u8,
    },
    StatusCodeSpike {
        code: u16,
        threshold_per_min: u64,
    },
    CustomMetric {
        metric: String,
        op: String,
        value: f64,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AlertChannel {
    Email { address: String },
    Slack { webhook_url: String },
    PagerDuty { integration_key: String },
    Webhook { url: String },
}

// ============================================================================
// Status Page
// ============================================================================

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StatusPage {
    pub id: String,
    pub tenant_id: i64,
    pub slug: String,
    pub title: String,
    pub components: Vec<StatusComponent>,
    pub incidents: Vec<Incident>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StatusComponent {
    pub id: String,
    pub name: String,
    pub status: ComponentStatus,
    pub description: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ComponentStatus {
    Operational,
    DegradedPerformance,
    PartialOutage,
    MajorOutage,
    Maintenance,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Incident {
    pub id: String,
    pub title: String,
    pub status: String,
    pub impact: String,
    pub updates: Vec<IncidentUpdate>,
    pub created_at: String,
    pub resolved_at: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IncidentUpdate {
    pub message: String,
    pub status: String,
    pub timestamp: String,
}

// ============================================================================
// Query Insights
// ============================================================================

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QueryInsight {
    pub query_hash: String,
    pub query_text: String,
    pub calls: u64,
    pub total_time_ms: u64,
    pub avg_time_ms: f64,
    pub max_time_ms: u64,
    pub rows_returned: u64,
}

// ============================================================================
// Observability Service
// ============================================================================

#[derive(Clone)]
pub struct ObservabilityService {
    store: Arc<StackhouseStore>,
    alert_rules: Arc<RwLock<Vec<AlertRule>>>,
}

impl ObservabilityService {
    pub async fn new(store: Arc<StackhouseStore>) -> StackhouseResult<Self> {
        let service = Self {
            store,
            alert_rules: Arc::new(RwLock::new(Vec::new())),
        };
        service.initialize_tables().await?;
        service.load_alert_rules().await?;
        service.start_alert_evaluator();
        info!("📊 Observability service initialized");
        Ok(service)
    }

    async fn initialize_tables(&self) -> StackhouseResult<()> {
        self.store.execute_batch(r#"
            CREATE TABLE IF NOT EXISTS stackhouse_trace_spans (
                trace_id TEXT NOT NULL,
                span_id TEXT NOT NULL,
                parent_span_id TEXT,
                operation TEXT NOT NULL,
                service TEXT NOT NULL,
                status TEXT DEFAULT 'ok',
                duration_ms BIGINT,
                attributes JSONB DEFAULT '{}',
                events JSONB DEFAULT '[]',
                tenant_id BIGINT NOT NULL,
                started_at TIMESTAMPTZ DEFAULT NOW(),
                PRIMARY KEY (trace_id, span_id)
            );
            CREATE TABLE IF NOT EXISTS stackhouse_alert_rules (
                id TEXT PRIMARY KEY,
                tenant_id BIGINT NOT NULL,
                name TEXT NOT NULL,
                condition_json JSONB NOT NULL,
                channels JSONB NOT NULL DEFAULT '[]',
                cooldown_secs INTEGER DEFAULT 300,
                enabled BOOLEAN DEFAULT TRUE,
                last_triggered TIMESTAMPTZ,
                created_at TIMESTAMPTZ DEFAULT NOW()
            );
            CREATE TABLE IF NOT EXISTS stackhouse_alert_history (
                id TEXT PRIMARY KEY,
                rule_id TEXT NOT NULL,
                tenant_id BIGINT NOT NULL,
                message TEXT NOT NULL,
                triggered_at TIMESTAMPTZ DEFAULT NOW()
            );
            CREATE TABLE IF NOT EXISTS stackhouse_status_components (
                id TEXT PRIMARY KEY,
                tenant_id BIGINT NOT NULL,
                name TEXT NOT NULL,
                status TEXT DEFAULT 'operational',
                description TEXT DEFAULT '',
                display_order INTEGER DEFAULT 0
            );
            CREATE TABLE IF NOT EXISTS stackhouse_incidents (
                id TEXT PRIMARY KEY,
                tenant_id BIGINT NOT NULL,
                title TEXT NOT NULL,
                status TEXT DEFAULT 'investigating',
                impact TEXT DEFAULT 'minor',
                updates JSONB DEFAULT '[]',
                created_at TIMESTAMPTZ DEFAULT NOW(),
                resolved_at TIMESTAMPTZ
            );
            CREATE TABLE IF NOT EXISTS stackhouse_query_insights (
                query_hash TEXT NOT NULL,
                tenant_id BIGINT NOT NULL,
                query_text TEXT NOT NULL,
                calls BIGINT DEFAULT 0,
                total_time_ms BIGINT DEFAULT 0,
                max_time_ms BIGINT DEFAULT 0,
                rows_returned BIGINT DEFAULT 0,
                last_seen TIMESTAMPTZ DEFAULT NOW(),
                PRIMARY KEY (query_hash, tenant_id)
            );
            CREATE INDEX IF NOT EXISTS idx_spans_trace ON stackhouse_trace_spans(trace_id);
            CREATE INDEX IF NOT EXISTS idx_spans_tenant_time ON stackhouse_trace_spans(tenant_id, started_at);
            CREATE INDEX IF NOT EXISTS idx_alerts_tenant ON stackhouse_alert_rules(tenant_id);
            CREATE INDEX IF NOT EXISTS idx_query_insights_slow ON stackhouse_query_insights(tenant_id, total_time_ms DESC);
        "#.to_string()).await?;
        Ok(())
    }

    async fn load_alert_rules(&self) -> StackhouseResult<()> {
        let rows = self.store.query(
            "SELECT id, tenant_id, name, condition_json, channels, cooldown_secs, enabled, last_triggered, created_at FROM stackhouse_alert_rules WHERE enabled = true".to_string(),
            vec![],
        ).await?;
        let mut rules = self.alert_rules.write().await;
        for row in rows {
            let get = |key: &str| row.iter().find(|(k, _)| k == key).map(|(_, v)| v.clone());
            let id = get("id")
                .and_then(|v| v.as_str().map(String::from))
                .unwrap_or_default();
            let tenant_id = get("tenant_id").and_then(|v| v.as_i64()).unwrap_or(0);
            let name = get("name")
                .and_then(|v| v.as_str().map(String::from))
                .unwrap_or_default();
            let condition: AlertCondition = get("condition_json")
                .and_then(|v| v.as_str().and_then(|s| serde_json::from_str(s).ok()))
                .unwrap_or(AlertCondition::ErrorRateAbove {
                    threshold_pct: 0.0,
                    window_secs: 300,
                });
            let channels: Vec<AlertChannel> = get("channels")
                .and_then(|v| v.as_str().and_then(|s| serde_json::from_str(s).ok()))
                .unwrap_or_default();
            let cooldown_secs = get("cooldown_secs").and_then(|v| v.as_i64()).unwrap_or(300) as u64;
            let last_triggered = get("last_triggered").and_then(|v| v.as_str().map(String::from));
            let created_at = get("created_at")
                .and_then(|v| v.as_str().map(String::from))
                .unwrap_or_default();

            rules.push(AlertRule {
                id,
                tenant_id,
                name,
                condition,
                channels,
                cooldown_secs,
                enabled: true,
                last_triggered,
                created_at,
            });
        }
        info!("📊 Loaded {} alert rules", rules.len());
        Ok(())
    }

    fn start_alert_evaluator(&self) {
        let store = Arc::clone(&self.store);
        let alert_rules = Arc::clone(&self.alert_rules);
        tokio::spawn(async move {
            let mut interval = tokio::time::interval(Duration::from_secs(60));
            loop {
                interval.tick().await;
                let rules = alert_rules.read().await;
                for rule in rules.iter() {
                    if let Err(e) = Self::evaluate_rule(&store, rule).await {
                        debug!("Alert rule '{}' evaluation error: {}", rule.name, e);
                    }
                }
            }
        });
    }

    async fn evaluate_rule(store: &Arc<StackhouseStore>, rule: &AlertRule) -> StackhouseResult<()> {
        // Check error rate from recent spans
        let error_count = store.query(
            "SELECT count(*) as cnt FROM stackhouse_trace_spans WHERE tenant_id = ? AND status != 'ok' AND started_at > NOW() - INTERVAL '5 minutes'".to_string(),
            vec![SqlValue::Integer(rule.tenant_id)],
        ).await?;
        let total_count = store.query(
            "SELECT count(*) as cnt FROM stackhouse_trace_spans WHERE tenant_id = ? AND started_at > NOW() - INTERVAL '5 minutes'".to_string(),
            vec![SqlValue::Integer(rule.tenant_id)],
        ).await?;

        let errors = error_count
            .first()
            .and_then(|r| r.iter().find(|(k, _)| k == "cnt"))
            .and_then(|(_, v)| v.as_i64())
            .unwrap_or(0);
        let total = total_count
            .first()
            .and_then(|r| r.iter().find(|(k, _)| k == "cnt"))
            .and_then(|(_, v)| v.as_i64())
            .unwrap_or(0);

        if total > 0 {
            let error_rate = errors as f64 / total as f64;
            match &rule.condition {
                AlertCondition::ErrorRateAbove { threshold_pct, .. } => {
                    let threshold = threshold_pct / 100.0;
                    if error_rate > threshold {
                        info!(
                            "🚨 Alert '{}' triggered: error rate {:.1}% exceeds threshold {:.1}%",
                            rule.name,
                            error_rate * 100.0,
                            threshold_pct
                        );
                        Self::record_alert(store, rule, &format!(
                            "Error rate {:.1}% exceeds threshold {:.1}% ({} errors out of {} requests)",
                            error_rate * 100.0, threshold_pct, errors, total
                        )).await?;
                    }
                }
                AlertCondition::LatencyAbove { threshold_ms, .. } => {
                    let slow = store.query(
                        format!("SELECT count(*) as cnt FROM stackhouse_trace_spans WHERE tenant_id = {} AND duration_ms > {} AND started_at > NOW() - INTERVAL '5 minutes'", rule.tenant_id, threshold_ms),
                        vec![],
                    ).await?;
                    let slow_count = slow
                        .first()
                        .and_then(|r| r.iter().find(|(k, _)| k == "cnt"))
                        .and_then(|(_, v)| v.as_i64())
                        .unwrap_or(0);
                    if slow_count > 0 {
                        info!(
                            "🚨 Alert '{}' triggered: {} requests exceeded {}ms latency",
                            rule.name, slow_count, threshold_ms
                        );
                        Self::record_alert(
                            store,
                            rule,
                            &format!(
                                "{} requests exceeded {}ms latency threshold",
                                slow_count, threshold_ms
                            ),
                        )
                        .await?;
                    }
                }
                AlertCondition::StatusCodeSpike { .. } | AlertCondition::CustomMetric { .. } => {}
            }
        }
        Ok(())
    }

    async fn record_alert(
        store: &Arc<StackhouseStore>,
        rule: &AlertRule,
        message: &str,
    ) -> StackhouseResult<()> {
        let id = uuid::Uuid::new_v4().to_string();
        store.execute(
            "INSERT INTO stackhouse_alert_history (id, rule_id, tenant_id, message) VALUES (?, ?, ?, ?)".to_string(),
            vec![
                SqlValue::Text(id),
                SqlValue::Text(rule.id.clone()),
                SqlValue::Integer(rule.tenant_id),
                SqlValue::Text(message.to_string()),
            ],
        ).await?;
        // Update last_triggered timestamp
        store
            .execute(
                "UPDATE stackhouse_alert_rules SET last_triggered = NOW() WHERE id = ?".to_string(),
                vec![SqlValue::Text(rule.id.clone())],
            )
            .await?;
        Ok(())
    }

    /// Record a trace span
    pub async fn record_span(&self, tenant_id: i64, span: TraceSpan) -> StackhouseResult<()> {
        self.store.execute(
            "INSERT INTO stackhouse_trace_spans (trace_id, span_id, parent_span_id, operation, service, status, duration_ms, attributes, events, tenant_id, started_at) VALUES (?, ?, ?, ?, ?, ?, ?, ?::jsonb, ?::jsonb, ?, ?::timestamptz)".to_string(),
            vec![
                SqlValue::Text(span.trace_id),
                SqlValue::Text(span.span_id),
                SqlValue::Text(span.parent_span_id.unwrap_or_default()),
                SqlValue::Text(span.operation),
                SqlValue::Text(span.service),
                SqlValue::Text(serde_json::to_string(&span.status).unwrap_or_default().trim_matches('"').to_string()),
                SqlValue::Integer(span.duration_ms as i64),
                SqlValue::Text(serde_json::to_string(&span.attributes).unwrap_or_default()),
                SqlValue::Text(serde_json::to_string(&span.events).unwrap_or_default()),
                SqlValue::Integer(tenant_id),
                SqlValue::Text(span.started_at),
            ],
        ).await?;
        Ok(())
    }

    /// Get a full trace
    pub async fn get_trace(&self, trace_id: &str) -> StackhouseResult<Vec<Value>> {
        let rows = self.store.query(
            "SELECT span_id, parent_span_id, operation, service, status, duration_ms, attributes, started_at FROM stackhouse_trace_spans WHERE trace_id = ? ORDER BY started_at".to_string(),
            vec![SqlValue::Text(trace_id.to_string())],
        ).await?;
        Ok(rows
            .into_iter()
            .map(|r| json!(r.into_iter().collect::<std::collections::HashMap<_, _>>()))
            .collect())
    }

    /// Create an alert rule
    pub async fn create_alert(
        &self,
        tenant_id: i64,
        name: &str,
        condition: AlertCondition,
        channels: Vec<AlertChannel>,
    ) -> StackhouseResult<AlertRule> {
        let id = uuid::Uuid::new_v4().to_string();
        self.store.execute(
            "INSERT INTO stackhouse_alert_rules (id, tenant_id, name, condition_json, channels) VALUES (?, ?, ?, ?::jsonb, ?::jsonb)".to_string(),
            vec![
                SqlValue::Text(id.clone()),
                SqlValue::Integer(tenant_id),
                SqlValue::Text(name.to_string()),
                SqlValue::Text(serde_json::to_string(&condition).unwrap_or_default()),
                SqlValue::Text(serde_json::to_string(&channels).unwrap_or_default()),
            ],
        ).await?;

        Ok(AlertRule {
            id,
            tenant_id,
            name: name.to_string(),
            condition,
            channels,
            cooldown_secs: 300,
            enabled: true,
            last_triggered: None,
            created_at: chrono::Utc::now().to_rfc3339(),
        })
    }

    /// Record a query for insights
    pub async fn record_query(
        &self,
        tenant_id: i64,
        query_text: &str,
        duration_ms: u64,
        rows: u64,
    ) -> StackhouseResult<()> {
        let hash = format!("{:x}", md5::compute(query_text));
        self.store.execute(
            r#"INSERT INTO stackhouse_query_insights (query_hash, tenant_id, query_text, calls, total_time_ms, max_time_ms, rows_returned)
               VALUES (?, ?, ?, 1, ?, ?, ?)
               ON CONFLICT (query_hash, tenant_id) DO UPDATE SET
               calls = stackhouse_query_insights.calls + 1,
               total_time_ms = stackhouse_query_insights.total_time_ms + EXCLUDED.total_time_ms,
               max_time_ms = GREATEST(stackhouse_query_insights.max_time_ms, EXCLUDED.max_time_ms),
               rows_returned = stackhouse_query_insights.rows_returned + EXCLUDED.rows_returned,
               last_seen = NOW()"#.to_string(),
            vec![
                SqlValue::Text(hash),
                SqlValue::Integer(tenant_id),
                SqlValue::Text(query_text.to_string()),
                SqlValue::Integer(duration_ms as i64),
                SqlValue::Integer(duration_ms as i64),
                SqlValue::Integer(rows as i64),
            ],
        ).await?;
        Ok(())
    }

    /// Get slow queries
    pub async fn get_slow_queries(
        &self,
        tenant_id: i64,
        limit: usize,
    ) -> StackhouseResult<Vec<Value>> {
        let rows = self.store.query(
            format!("SELECT query_hash, query_text, calls, total_time_ms, total_time_ms/GREATEST(calls,1) as avg_time_ms, max_time_ms FROM stackhouse_query_insights WHERE tenant_id = ? ORDER BY total_time_ms DESC LIMIT {}", limit),
            vec![SqlValue::Integer(tenant_id)],
        ).await?;
        Ok(rows
            .into_iter()
            .map(|r| json!(r.into_iter().collect::<std::collections::HashMap<_, _>>()))
            .collect())
    }

    /// Create/update status component
    pub async fn set_component_status(
        &self,
        tenant_id: i64,
        name: &str,
        status: ComponentStatus,
    ) -> StackhouseResult<()> {
        let id = format!("{}_{}", tenant_id, name.replace(' ', "_").to_lowercase());
        let status_str = serde_json::to_string(&status)
            .unwrap_or_default()
            .trim_matches('"')
            .to_string();
        self.store.execute(
            "INSERT INTO stackhouse_status_components (id, tenant_id, name, status) VALUES (?, ?, ?, ?) ON CONFLICT (id) DO UPDATE SET status = EXCLUDED.status".to_string(),
            vec![
                SqlValue::Text(id),
                SqlValue::Integer(tenant_id),
                SqlValue::Text(name.to_string()),
                SqlValue::Text(status_str),
            ],
        ).await?;
        Ok(())
    }

    /// Get public status page data
    pub async fn get_status_page(&self, tenant_id: i64) -> StackhouseResult<Value> {
        let components = self.store.query(
            "SELECT id, name, status, description FROM stackhouse_status_components WHERE tenant_id = ? ORDER BY display_order".to_string(),
            vec![SqlValue::Integer(tenant_id)],
        ).await?;
        let incidents = self.store.query(
            "SELECT id, title, status, impact, updates, created_at, resolved_at FROM stackhouse_incidents WHERE tenant_id = ? AND (resolved_at IS NULL OR resolved_at > NOW() - INTERVAL '7 days') ORDER BY created_at DESC LIMIT 10".to_string(),
            vec![SqlValue::Integer(tenant_id)],
        ).await?;

        Ok(json!({
            "components": components.into_iter().map(|r| json!(r.into_iter().collect::<std::collections::HashMap<_, _>>())).collect::<Vec<_>>(),
            "incidents": incidents.into_iter().map(|r| json!(r.into_iter().collect::<std::collections::HashMap<_, _>>())).collect::<Vec<_>>(),
        }))
    }
}
