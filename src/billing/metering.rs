//! # Metered Billing Webhook Events
//!
//! Syncs usage metering data to the database via webhook events,
//! enabling per-API-call, per-GB, per-agent-run billing.

use crate::db::{SqlValue, StackhouseStore};
use crate::error::StackhouseResult;

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::HashMap;
use std::sync::Arc;
use tracing::info;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MeteringEvent {
    pub event_id: String,
    pub tenant_id: i64,
    pub meter_id: String,
    pub quantity: f64,
    pub properties: HashMap<String, String>,
    pub timestamp: String,
    pub source: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MeterConfig {
    pub id: String,
    pub name: String,
    pub aggregate: AggregateType,
    pub reset_period: ResetPeriod,
    pub property_filters: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AggregateType {
    Sum,
    Count,
    Max,
    UniqueCount,
    LastValue,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ResetPeriod {
    Monthly,
    Weekly,
    Daily,
    Never,
}

#[derive(Clone)]
pub struct MeteringService {
    store: Arc<StackhouseStore>,
}

impl MeteringService {
    pub async fn new(store: Arc<StackhouseStore>) -> StackhouseResult<Self> {
        let service = Self { store };
        service.initialize_tables().await?;
        info!("📏 Metering service initialized");
        Ok(service)
    }

    async fn initialize_tables(&self) -> StackhouseResult<()> {
        self.store.execute_batch(r#"
            CREATE TABLE IF NOT EXISTS stackhouse_metering_events (
                event_id TEXT PRIMARY KEY,
                tenant_id BIGINT NOT NULL,
                meter_id TEXT NOT NULL,
                quantity FLOAT NOT NULL,
                properties JSONB DEFAULT '{}',
                source TEXT DEFAULT 'api',
                timestamp TIMESTAMPTZ DEFAULT NOW()
            );
            CREATE TABLE IF NOT EXISTS stackhouse_meter_configs (
                id TEXT PRIMARY KEY,
                name TEXT NOT NULL,
                aggregate TEXT NOT NULL DEFAULT 'sum',
                reset_period TEXT NOT NULL DEFAULT 'monthly',
                property_filters JSONB DEFAULT '[]',
                created_at TIMESTAMPTZ DEFAULT NOW()
            );
            CREATE TABLE IF NOT EXISTS stackhouse_metering_totals (
                tenant_id BIGINT NOT NULL,
                meter_id TEXT NOT NULL,
                period_start TIMESTAMPTZ NOT NULL,
                total FLOAT DEFAULT 0,
                event_count BIGINT DEFAULT 0,
                last_updated TIMESTAMPTZ DEFAULT NOW(),
                PRIMARY KEY (tenant_id, meter_id, period_start)
            );
            CREATE INDEX IF NOT EXISTS idx_metering_events_tenant ON stackhouse_metering_events(tenant_id, meter_id, timestamp);
        "#.to_string()).await?;
        Ok(())
    }

    /// Ingest a metering event
    pub async fn ingest(&self, event: MeteringEvent) -> StackhouseResult<()> {
        // Insert event (idempotent on event_id)
        self.store.execute(
            "INSERT INTO stackhouse_metering_events (event_id, tenant_id, meter_id, quantity, properties, source, timestamp) VALUES (?, ?, ?, ?, ?::jsonb, ?, ?::timestamptz) ON CONFLICT (event_id) DO NOTHING".to_string(),
            vec![
                SqlValue::Text(event.event_id),
                SqlValue::Integer(event.tenant_id),
                SqlValue::Text(event.meter_id.clone()),
                SqlValue::Text(event.quantity.to_string()),
                SqlValue::Text(serde_json::to_string(&event.properties).unwrap_or_default()),
                SqlValue::Text(event.source),
                SqlValue::Text(event.timestamp),
            ],
        ).await?;

        // Update running total
        self.store.execute(
            r#"INSERT INTO stackhouse_metering_totals (tenant_id, meter_id, period_start, total, event_count)
               VALUES (?, ?, date_trunc('month', NOW()), ?, 1)
               ON CONFLICT (tenant_id, meter_id, period_start) DO UPDATE
               SET total = stackhouse_metering_totals.total + EXCLUDED.total,
                   event_count = stackhouse_metering_totals.event_count + 1,
                   last_updated = NOW()"#.to_string(),
            vec![
                SqlValue::Integer(event.tenant_id),
                SqlValue::Text(event.meter_id),
                SqlValue::Text(event.quantity.to_string()),
            ],
        ).await?;

        Ok(())
    }

    /// Batch ingest multiple events
    pub async fn ingest_batch(&self, events: Vec<MeteringEvent>) -> StackhouseResult<u32> {
        let mut ingested = 0;
        for event in events {
            self.ingest(event).await?;
            ingested += 1;
        }
        Ok(ingested)
    }

    /// Get current period usage for a tenant/meter
    pub async fn get_usage(&self, tenant_id: i64, meter_id: &str) -> StackhouseResult<f64> {
        let rows = self.store.query(
            "SELECT total FROM stackhouse_metering_totals WHERE tenant_id = ? AND meter_id = ? AND period_start = date_trunc('month', NOW())".to_string(),
            vec![SqlValue::Integer(tenant_id), SqlValue::Text(meter_id.to_string())],
        ).await?;
        Ok(rows
            .first()
            .and_then(|r| r.iter().find(|(k, _)| k == "total"))
            .and_then(|(_, v)| v.as_f64())
            .unwrap_or(0.0))
    }

    /// Get usage breakdown by meter for a tenant
    pub async fn get_usage_breakdown(&self, tenant_id: i64) -> StackhouseResult<Vec<Value>> {
        let rows = self.store.query(
            "SELECT meter_id, total, event_count, last_updated FROM stackhouse_metering_totals WHERE tenant_id = ? AND period_start = date_trunc('month', NOW())".to_string(),
            vec![SqlValue::Integer(tenant_id)],
        ).await?;
        Ok(rows
            .into_iter()
            .map(|r| json!(r.into_iter().collect::<HashMap<_, _>>()))
            .collect())
    }

    /// Create/update a meter configuration
    pub async fn upsert_meter(&self, config: &MeterConfig) -> StackhouseResult<()> {
        let agg_str = serde_json::to_string(&config.aggregate)
            .unwrap_or_default()
            .trim_matches('"')
            .to_string();
        let period_str = serde_json::to_string(&config.reset_period)
            .unwrap_or_default()
            .trim_matches('"')
            .to_string();
        self.store.execute(
            "INSERT INTO stackhouse_meter_configs (id, name, aggregate, reset_period, property_filters) VALUES (?, ?, ?, ?, ?::jsonb) ON CONFLICT (id) DO UPDATE SET name = EXCLUDED.name, aggregate = EXCLUDED.aggregate, reset_period = EXCLUDED.reset_period".to_string(),
            vec![
                SqlValue::Text(config.id.clone()),
                SqlValue::Text(config.name.clone()),
                SqlValue::Text(agg_str),
                SqlValue::Text(period_str),
                SqlValue::Text(serde_json::to_string(&config.property_filters).unwrap_or_default()),
            ],
        ).await?;
        Ok(())
    }
}
