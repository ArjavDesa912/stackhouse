//! # Usage-Based Billing
//!
//! Per-seat, per-API-call, per-GB, per-agent-run metering with
//! configurable pricing tiers and overage handling.

use crate::db::{SqlValue, StackhouseStore};
use crate::error::StackhouseResult;

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::HashMap;
use std::sync::Arc;
use tracing::info;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UsageMeter {
    pub id: String,
    pub tenant_id: i64,
    pub meter_name: String,
    pub meter_type: MeterType,
    pub unit: String,
    pub pricing: UsagePricing,
    pub current_usage: f64,
    pub billing_period_start: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MeterType {
    Counter,  // Incremental (API calls)
    Gauge,    // Point-in-time (seats, storage GB)
    Duration, // Time-based (compute minutes)
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UsagePricing {
    pub tiers: Vec<PricingTier>,
    pub overage_rate: f64,
    pub included_amount: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PricingTier {
    pub up_to: Option<f64>,
    pub unit_price: f64,
    pub flat_fee: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UsageEvent {
    pub tenant_id: i64,
    pub meter_name: String,
    pub value: f64,
    pub properties: HashMap<String, String>,
    pub timestamp: String,
    pub idempotency_key: Option<String>,
}

#[derive(Clone)]
pub struct UsageBillingService {
    store: Arc<StackhouseStore>,
}

impl UsageBillingService {
    pub async fn new(store: Arc<StackhouseStore>) -> StackhouseResult<Self> {
        let service = Self { store };
        service.initialize_tables().await?;
        info!("📊 Usage billing service initialized");
        Ok(service)
    }

    async fn initialize_tables(&self) -> StackhouseResult<()> {
        self.store.execute_batch(r#"
            CREATE TABLE IF NOT EXISTS stackhouse_usage_meters (
                id TEXT PRIMARY KEY,
                tenant_id BIGINT NOT NULL,
                meter_name TEXT NOT NULL,
                meter_type TEXT NOT NULL,
                unit TEXT NOT NULL,
                pricing JSONB NOT NULL,
                created_at TIMESTAMPTZ DEFAULT NOW(),
                UNIQUE(tenant_id, meter_name)
            );
            CREATE TABLE IF NOT EXISTS stackhouse_usage_events (
                id BIGSERIAL PRIMARY KEY,
                tenant_id BIGINT NOT NULL,
                meter_name TEXT NOT NULL,
                value FLOAT NOT NULL,
                properties JSONB DEFAULT '{}',
                idempotency_key TEXT,
                timestamp TIMESTAMPTZ DEFAULT NOW(),
                UNIQUE(idempotency_key)
            );
            CREATE TABLE IF NOT EXISTS stackhouse_usage_aggregates (
                tenant_id BIGINT NOT NULL,
                meter_name TEXT NOT NULL,
                period_start TIMESTAMPTZ NOT NULL,
                period_end TIMESTAMPTZ NOT NULL,
                total_usage FLOAT DEFAULT 0,
                calculated_cost FLOAT DEFAULT 0,
                PRIMARY KEY (tenant_id, meter_name, period_start)
            );
            CREATE INDEX IF NOT EXISTS idx_usage_events_tenant ON stackhouse_usage_events(tenant_id, meter_name, timestamp);
        "#.to_string()).await?;
        Ok(())
    }

    /// Define a usage meter
    pub async fn create_meter(
        &self,
        tenant_id: i64,
        name: &str,
        meter_type: MeterType,
        unit: &str,
        pricing: UsagePricing,
    ) -> StackhouseResult<UsageMeter> {
        let id = uuid::Uuid::new_v4().to_string();
        let type_str = serde_json::to_string(&meter_type)
            .unwrap_or_default()
            .trim_matches('"')
            .to_string();
        self.store.execute(
            "INSERT INTO stackhouse_usage_meters (id, tenant_id, meter_name, meter_type, unit, pricing) VALUES (?, ?, ?, ?, ?, ?::jsonb)".to_string(),
            vec![
                SqlValue::Text(id.clone()),
                SqlValue::Integer(tenant_id),
                SqlValue::Text(name.to_string()),
                SqlValue::Text(type_str),
                SqlValue::Text(unit.to_string()),
                SqlValue::Text(serde_json::to_string(&pricing).unwrap_or_default()),
            ],
        ).await?;
        Ok(UsageMeter {
            id,
            tenant_id,
            meter_name: name.to_string(),
            meter_type,
            unit: unit.to_string(),
            pricing,
            current_usage: 0.0,
            billing_period_start: chrono::Utc::now().to_rfc3339(),
        })
    }

    /// Record a usage event
    pub async fn record_usage(&self, event: UsageEvent) -> StackhouseResult<()> {
        self.store.execute(
            "INSERT INTO stackhouse_usage_events (tenant_id, meter_name, value, properties, idempotency_key, timestamp) VALUES (?, ?, ?, ?::jsonb, ?, ?::timestamptz) ON CONFLICT (idempotency_key) DO NOTHING".to_string(),
            vec![
                SqlValue::Integer(event.tenant_id),
                SqlValue::Text(event.meter_name),
                SqlValue::Text(event.value.to_string()),
                SqlValue::Text(serde_json::to_string(&event.properties).unwrap_or_default()),
                SqlValue::Text(event.idempotency_key.unwrap_or_else(|| uuid::Uuid::new_v4().to_string())),
                SqlValue::Text(event.timestamp),
            ],
        ).await?;
        Ok(())
    }

    /// Get current period usage for a meter
    pub async fn get_current_usage(
        &self,
        tenant_id: i64,
        meter_name: &str,
    ) -> StackhouseResult<f64> {
        let rows = self.store.query(
            "SELECT COALESCE(SUM(value), 0) as total FROM stackhouse_usage_events WHERE tenant_id = ? AND meter_name = ? AND timestamp >= date_trunc('month', NOW())".to_string(),
            vec![SqlValue::Integer(tenant_id), SqlValue::Text(meter_name.to_string())],
        ).await?;
        let total = rows
            .first()
            .and_then(|r| r.iter().find(|(k, _)| k == "total"))
            .and_then(|(_, v)| v.as_f64())
            .unwrap_or(0.0);
        Ok(total)
    }

    /// Calculate cost for usage using tiered pricing
    pub fn calculate_cost(usage: f64, pricing: &UsagePricing) -> f64 {
        let billable = (usage - pricing.included_amount).max(0.0);
        if billable <= 0.0 {
            return 0.0;
        }

        let mut remaining = billable;
        let mut cost = 0.0;
        let mut prev_limit = 0.0;

        for tier in &pricing.tiers {
            let tier_limit = tier.up_to.unwrap_or(f64::MAX) - prev_limit;
            let tier_usage = remaining.min(tier_limit);
            cost += tier.flat_fee + (tier_usage * tier.unit_price);
            remaining -= tier_usage;
            prev_limit = tier.up_to.unwrap_or(f64::MAX);
            if remaining <= 0.0 {
                break;
            }
        }

        if remaining > 0.0 {
            cost += remaining * pricing.overage_rate;
        }

        cost
    }

    /// Get usage summary for billing period
    pub async fn get_billing_summary(&self, tenant_id: i64) -> StackhouseResult<Vec<Value>> {
        let rows = self.store.query(
            r#"SELECT m.meter_name, m.unit, m.pricing,
                COALESCE(SUM(e.value), 0) as usage
            FROM stackhouse_usage_meters m
            LEFT JOIN stackhouse_usage_events e ON m.tenant_id = e.tenant_id AND m.meter_name = e.meter_name
                AND e.timestamp >= date_trunc('month', NOW())
            WHERE m.tenant_id = ?
            GROUP BY m.meter_name, m.unit, m.pricing"#.to_string(),
            vec![SqlValue::Integer(tenant_id)],
        ).await?;
        Ok(rows
            .into_iter()
            .map(|r| json!(r.into_iter().collect::<HashMap<_, _>>()))
            .collect())
    }
}
