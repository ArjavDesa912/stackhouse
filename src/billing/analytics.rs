//! # Revenue Analytics
//!
//! MRR, churn rate, LTV, cohort analysis, expansion revenue tracking,
//! and subscription health metrics.

use crate::db::{SqlValue, StackhouseStore};
use crate::error::StackhouseResult;

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::HashMap;
use std::sync::Arc;
use tracing::info;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RevenueMetrics {
    pub mrr_cents: i64,
    pub arr_cents: i64,
    pub total_customers: u64,
    pub paying_customers: u64,
    pub churned_customers: u64,
    pub churn_rate_pct: f64,
    pub expansion_mrr_cents: i64,
    pub contraction_mrr_cents: i64,
    pub net_revenue_retention_pct: f64,
    pub avg_revenue_per_customer_cents: i64,
    pub ltv_cents: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CohortData {
    pub cohort_month: String,
    pub initial_customers: u64,
    pub retention_by_month: Vec<f64>,
    pub revenue_by_month: Vec<i64>,
}

#[derive(Clone)]
pub struct RevenueAnalyticsService {
    store: Arc<StackhouseStore>,
}

impl RevenueAnalyticsService {
    pub async fn new(store: Arc<StackhouseStore>) -> StackhouseResult<Self> {
        let service = Self { store };
        service.initialize_tables().await?;
        info!("💰 Revenue analytics service initialized");
        Ok(service)
    }

    async fn initialize_tables(&self) -> StackhouseResult<()> {
        self.store.execute_batch(r#"
            CREATE TABLE IF NOT EXISTS stackhouse_revenue_events (
                id BIGSERIAL PRIMARY KEY,
                tenant_id BIGINT NOT NULL,
                event_type TEXT NOT NULL,
                amount_cents BIGINT NOT NULL,
                currency TEXT DEFAULT 'usd',
                plan_id TEXT,
                metadata JSONB DEFAULT '{}',
                timestamp TIMESTAMPTZ DEFAULT NOW()
            );
            CREATE TABLE IF NOT EXISTS stackhouse_mrr_snapshots (
                snapshot_date DATE PRIMARY KEY,
                total_mrr_cents BIGINT DEFAULT 0,
                paying_customers BIGINT DEFAULT 0,
                new_mrr_cents BIGINT DEFAULT 0,
                expansion_mrr_cents BIGINT DEFAULT 0,
                contraction_mrr_cents BIGINT DEFAULT 0,
                churned_mrr_cents BIGINT DEFAULT 0
            );
            CREATE INDEX IF NOT EXISTS idx_revenue_events_tenant ON stackhouse_revenue_events(tenant_id);
            CREATE INDEX IF NOT EXISTS idx_revenue_events_type ON stackhouse_revenue_events(event_type, timestamp);
        "#.to_string()).await?;
        Ok(())
    }

    /// Record a revenue event
    pub async fn record_event(
        &self,
        tenant_id: i64,
        event_type: &str,
        amount_cents: i64,
        plan_id: Option<&str>,
    ) -> StackhouseResult<()> {
        self.store.execute(
            "INSERT INTO stackhouse_revenue_events (tenant_id, event_type, amount_cents, plan_id) VALUES (?, ?, ?, ?)".to_string(),
            vec![
                SqlValue::Integer(tenant_id),
                SqlValue::Text(event_type.to_string()),
                SqlValue::Integer(amount_cents),
                SqlValue::Text(plan_id.unwrap_or("").to_string()),
            ],
        ).await?;
        Ok(())
    }

    /// Get current MRR and key metrics
    pub async fn get_metrics(&self) -> StackhouseResult<Value> {
        let rows = self.store.query(
            r#"SELECT
                COALESCE(SUM(amount_cents) FILTER (WHERE event_type = 'subscription_active'), 0) as mrr,
                COUNT(DISTINCT tenant_id) FILTER (WHERE event_type = 'subscription_active') as paying,
                COUNT(DISTINCT tenant_id) FILTER (WHERE event_type = 'churned' AND timestamp > NOW() - INTERVAL '30 days') as churned,
                COALESCE(SUM(amount_cents) FILTER (WHERE event_type = 'expansion' AND timestamp > NOW() - INTERVAL '30 days'), 0) as expansion,
                COALESCE(SUM(amount_cents) FILTER (WHERE event_type = 'contraction' AND timestamp > NOW() - INTERVAL '30 days'), 0) as contraction
            FROM stackhouse_revenue_events
            WHERE timestamp > NOW() - INTERVAL '30 days'"#.to_string(),
            vec![],
        ).await?;

        if rows.is_empty() {
            return Ok(json!({}));
        }

        let row = &rows[0];
        let mrr = row
            .iter()
            .find(|(k, _)| k == "mrr")
            .and_then(|(_, v)| v.as_i64())
            .unwrap_or(0);
        let paying = row
            .iter()
            .find(|(k, _)| k == "paying")
            .and_then(|(_, v)| v.as_i64())
            .unwrap_or(0);
        let churned = row
            .iter()
            .find(|(k, _)| k == "churned")
            .and_then(|(_, v)| v.as_i64())
            .unwrap_or(0);
        let expansion = row
            .iter()
            .find(|(k, _)| k == "expansion")
            .and_then(|(_, v)| v.as_i64())
            .unwrap_or(0);
        let contraction = row
            .iter()
            .find(|(k, _)| k == "contraction")
            .and_then(|(_, v)| v.as_i64())
            .unwrap_or(0);

        let churn_rate = if paying > 0 {
            churned as f64 / paying as f64 * 100.0
        } else {
            0.0
        };
        let arpu = if paying > 0 { mrr / paying } else { 0 };
        let nrr = if mrr > 0 {
            ((mrr + expansion - contraction) as f64 / mrr as f64) * 100.0
        } else {
            100.0
        };

        Ok(json!({
            "mrr_cents": mrr,
            "arr_cents": mrr * 12,
            "paying_customers": paying,
            "churned_customers_30d": churned,
            "churn_rate_pct": churn_rate,
            "expansion_mrr_cents": expansion,
            "contraction_mrr_cents": contraction,
            "net_revenue_retention_pct": nrr,
            "avg_revenue_per_customer_cents": arpu,
        }))
    }

    /// Get MRR trend over time
    pub async fn get_mrr_trend(&self, months: u32) -> StackhouseResult<Vec<Value>> {
        let rows = self.store.query(
            format!(r#"SELECT
                date_trunc('month', timestamp) as month,
                SUM(amount_cents) FILTER (WHERE event_type = 'subscription_active') as mrr,
                COUNT(DISTINCT tenant_id) FILTER (WHERE event_type = 'subscription_active') as customers
            FROM stackhouse_revenue_events
            WHERE timestamp > NOW() - INTERVAL '{} months'
            GROUP BY date_trunc('month', timestamp)
            ORDER BY month"#, months),
            vec![],
        ).await?;
        Ok(rows
            .into_iter()
            .map(|r| json!(r.into_iter().collect::<HashMap<_, _>>()))
            .collect())
    }

    /// Get cohort retention data
    pub async fn get_cohorts(&self, months: u32) -> StackhouseResult<Vec<Value>> {
        let rows = self.store.query(
            format!(r#"WITH cohorts AS (
                SELECT tenant_id, date_trunc('month', MIN(timestamp)) as cohort_month
                FROM stackhouse_revenue_events WHERE event_type = 'subscription_active'
                GROUP BY tenant_id
            )
            SELECT
                c.cohort_month,
                COUNT(DISTINCT c.tenant_id) as initial_customers,
                COUNT(DISTINCT e.tenant_id) FILTER (WHERE e.timestamp > c.cohort_month + INTERVAL '1 month') as month_1,
                COUNT(DISTINCT e.tenant_id) FILTER (WHERE e.timestamp > c.cohort_month + INTERVAL '2 months') as month_2,
                COUNT(DISTINCT e.tenant_id) FILTER (WHERE e.timestamp > c.cohort_month + INTERVAL '3 months') as month_3
            FROM cohorts c
            LEFT JOIN stackhouse_revenue_events e ON c.tenant_id = e.tenant_id AND e.event_type = 'subscription_active'
            WHERE c.cohort_month > NOW() - INTERVAL '{} months'
            GROUP BY c.cohort_month
            ORDER BY c.cohort_month"#, months),
            vec![],
        ).await?;
        Ok(rows
            .into_iter()
            .map(|r| json!(r.into_iter().collect::<HashMap<_, _>>()))
            .collect())
    }

    /// Get plan distribution
    pub async fn get_plan_distribution(&self) -> StackhouseResult<Vec<Value>> {
        let rows = self
            .store
            .query(
                r#"SELECT plan_id, COUNT(DISTINCT tenant_id) as count, SUM(amount_cents) as revenue
            FROM stackhouse_revenue_events
            WHERE event_type = 'subscription_active' AND timestamp > NOW() - INTERVAL '30 days'
            GROUP BY plan_id ORDER BY revenue DESC"#
                    .to_string(),
                vec![],
            )
            .await?;
        Ok(rows
            .into_iter()
            .map(|r| json!(r.into_iter().collect::<HashMap<_, _>>()))
            .collect())
    }
}
