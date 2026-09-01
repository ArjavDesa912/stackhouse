//! # Subscription Plans, Tiers, and Add-ons
//!
//! Full plan and tier management with add-on billing, plan upgrades/downgrades,
//! proration, and feature gating.

use crate::db::{SqlValue, StackhouseStore};
use crate::error::StackhouseResult;

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::HashMap;
use std::sync::Arc;
use tracing::info;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SubscriptionPlan {
    pub id: String,
    pub name: String,
    pub tier: PlanTier,
    pub description: String,
    pub base_price_cents: i64,
    pub billing_interval: BillingInterval,
    pub features: Vec<PlanFeature>,
    pub limits: PlanLimits,
    pub is_active: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PlanTier {
    Free,
    Starter,
    Pro,
    Enterprise,
    Custom,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BillingInterval {
    Monthly,
    Yearly,
    Weekly,
    Daily,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlanFeature {
    pub key: String,
    pub name: String,
    pub included: bool,
    pub limit: Option<u64>,
    pub overage_price_cents: Option<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct PlanLimits {
    pub seats: u32,
    pub storage_gb: u32,
    pub api_calls_per_month: u64,
    pub vector_operations_per_month: u64,
    pub vector_documents: u64,
    pub bandwidth_gb: u32,
    pub functions: u32,
    pub concurrent_jobs: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AddOn {
    pub id: String,
    pub name: String,
    pub description: String,
    pub price_cents: i64,
    pub billing_interval: BillingInterval,
    pub applies_to_tiers: Vec<String>,
    pub feature_override: Option<PlanFeature>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TenantSubscription {
    pub tenant_id: i64,
    pub plan_id: String,
    pub add_on_ids: Vec<String>,
    pub status: SubscriptionStatus,
    pub started_at: String,
    pub next_billing_at: String,
    pub cancelled_at: Option<String>,
    pub proration_cents: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SubscriptionStatus {
    Active,
    Trialing,
    PastDue,
    Cancelled,
    Paused,
}

#[derive(Clone)]
pub struct SubscriptionPlanService {
    store: Arc<StackhouseStore>,
}

impl SubscriptionPlanService {
    pub async fn new(store: Arc<StackhouseStore>) -> StackhouseResult<Self> {
        let service = Self { store };
        service.initialize_tables().await?;
        info!("📋 Subscription plan service initialized");
        Ok(service)
    }

    async fn initialize_tables(&self) -> StackhouseResult<()> {
        self.store.execute_batch(r#"
            CREATE TABLE IF NOT EXISTS stackhouse_subscription_plans (
                id TEXT PRIMARY KEY,
                name TEXT NOT NULL,
                tier TEXT NOT NULL,
                description TEXT DEFAULT '',
                base_price_cents BIGINT NOT NULL DEFAULT 0,
                billing_interval TEXT NOT NULL DEFAULT 'monthly',
                features JSONB DEFAULT '[]',
                limits JSONB DEFAULT '{}',
                is_active BOOLEAN DEFAULT TRUE,
                created_at TIMESTAMPTZ DEFAULT NOW()
            );
            CREATE TABLE IF NOT EXISTS stackhouse_add_ons (
                id TEXT PRIMARY KEY,
                name TEXT NOT NULL,
                description TEXT DEFAULT '',
                price_cents BIGINT NOT NULL,
                billing_interval TEXT NOT NULL DEFAULT 'monthly',
                applies_to_tiers JSONB DEFAULT '[]',
                feature_override JSONB,
                is_active BOOLEAN DEFAULT TRUE
            );
            CREATE TABLE IF NOT EXISTS stackhouse_tenant_subscriptions (
                tenant_id BIGINT PRIMARY KEY,
                plan_id TEXT NOT NULL,
                add_on_ids JSONB DEFAULT '[]',
                status TEXT DEFAULT 'active',
                started_at TIMESTAMPTZ DEFAULT NOW(),
                next_billing_at TIMESTAMPTZ,
                cancelled_at TIMESTAMPTZ,
                proration_cents BIGINT DEFAULT 0
            );
            CREATE INDEX IF NOT EXISTS idx_plans_active ON stackhouse_subscription_plans(is_active);
            CREATE INDEX IF NOT EXISTS idx_tenant_sub_status ON stackhouse_tenant_subscriptions(status);
        "#.to_string()).await?;
        Ok(())
    }

    pub async fn create_plan(&self, plan: &SubscriptionPlan) -> StackhouseResult<()> {
        let tier_str = serde_json::to_string(&plan.tier)
            .unwrap_or_default()
            .trim_matches('"')
            .to_string();
        let interval_str = serde_json::to_string(&plan.billing_interval)
            .unwrap_or_default()
            .trim_matches('"')
            .to_string();
        self.store.execute(
            "INSERT INTO stackhouse_subscription_plans (id, name, tier, description, base_price_cents, billing_interval, features, limits, is_active) VALUES (?, ?, ?, ?, ?, ?, ?::jsonb, ?::jsonb, ?)".to_string(),
            vec![
                SqlValue::Text(plan.id.clone()),
                SqlValue::Text(plan.name.clone()),
                SqlValue::Text(tier_str),
                SqlValue::Text(plan.description.clone()),
                SqlValue::Integer(plan.base_price_cents),
                SqlValue::Text(interval_str),
                SqlValue::Text(serde_json::to_string(&plan.features).unwrap_or_default()),
                SqlValue::Text(serde_json::to_string(&plan.limits).unwrap_or_default()),
                SqlValue::Text(plan.is_active.to_string()),
            ],
        ).await?;
        Ok(())
    }

    pub async fn create_add_on(&self, add_on: &AddOn) -> StackhouseResult<()> {
        let interval_str = serde_json::to_string(&add_on.billing_interval)
            .unwrap_or_default()
            .trim_matches('"')
            .to_string();
        self.store.execute(
            "INSERT INTO stackhouse_add_ons (id, name, description, price_cents, billing_interval, applies_to_tiers, feature_override, is_active) VALUES (?, ?, ?, ?, ?, ?::jsonb, ?::jsonb, true)".to_string(),
            vec![
                SqlValue::Text(add_on.id.clone()),
                SqlValue::Text(add_on.name.clone()),
                SqlValue::Text(add_on.description.clone()),
                SqlValue::Integer(add_on.price_cents),
                SqlValue::Text(interval_str),
                SqlValue::Text(serde_json::to_string(&add_on.applies_to_tiers).unwrap_or_default()),
                SqlValue::Text(serde_json::to_string(&add_on.feature_override).unwrap_or("null".into())),
            ],
        ).await?;
        Ok(())
    }

    pub async fn subscribe(
        &self,
        tenant_id: i64,
        plan_id: &str,
        add_on_ids: Vec<String>,
    ) -> StackhouseResult<TenantSubscription> {
        let next_billing = (chrono::Utc::now() + chrono::Duration::days(30)).to_rfc3339();
        let sub = TenantSubscription {
            tenant_id,
            plan_id: plan_id.to_string(),
            add_on_ids,
            status: SubscriptionStatus::Active,
            started_at: chrono::Utc::now().to_rfc3339(),
            next_billing_at: next_billing.clone(),
            cancelled_at: None,
            proration_cents: 0,
        };
        self.store.execute(
            "INSERT INTO stackhouse_tenant_subscriptions (tenant_id, plan_id, add_on_ids, status, next_billing_at) VALUES (?, ?, ?::jsonb, 'active', ?::timestamptz) ON CONFLICT (tenant_id) DO UPDATE SET plan_id = EXCLUDED.plan_id, add_on_ids = EXCLUDED.add_on_ids, status = 'active'".to_string(),
            vec![
                SqlValue::Integer(tenant_id),
                SqlValue::Text(plan_id.to_string()),
                SqlValue::Text(serde_json::to_string(&sub.add_on_ids).unwrap_or_default()),
                SqlValue::Text(next_billing),
            ],
        ).await?;
        Ok(sub)
    }

    pub async fn change_plan(&self, tenant_id: i64, new_plan_id: &str) -> StackhouseResult<Value> {
        // Get current subscription and plan prices
        let current = self.store.query(
            "SELECT plan_id, started_at, next_billing_at FROM stackhouse_tenant_subscriptions WHERE tenant_id = ?".to_string(),
            vec![SqlValue::Integer(tenant_id)],
        ).await?;

        fn get_string(r: &Vec<(String, Value)>, key: &str) -> String {
            r.iter()
                .find(|(k, _)| k == key)
                .and_then(|(_, v)| v.as_str())
                .map(|s| s.to_string())
                .unwrap_or_default()
        }

        fn get_i64(r: &Vec<(String, Value)>, key: &str) -> i64 {
            r.iter()
                .find(|(k, _)| k == key)
                .and_then(|(_, v)| v.as_i64())
                .unwrap_or(0)
        }

        let old_plan_id = current
            .first()
            .map(|r| get_string(r, "plan_id"))
            .unwrap_or_default();
        let started_at_str = current
            .first()
            .map(|r| get_string(r, "started_at"))
            .unwrap_or_default();
        let next_billing_at_str = current
            .first()
            .map(|r| get_string(r, "next_billing_at"))
            .unwrap_or_default();

        let old_price = if old_plan_id.is_empty() {
            0i64
        } else {
            let rows = self
                .store
                .query(
                    "SELECT base_price_cents FROM stackhouse_subscription_plans WHERE id = ?"
                        .to_string(),
                    vec![SqlValue::Text(old_plan_id.clone())],
                )
                .await?;
            rows.first()
                .map(|r| get_i64(r, "base_price_cents"))
                .unwrap_or(0)
        };

        let new_price = {
            let rows = self
                .store
                .query(
                    "SELECT base_price_cents FROM stackhouse_subscription_plans WHERE id = ?"
                        .to_string(),
                    vec![SqlValue::Text(new_plan_id.to_string())],
                )
                .await?;
            rows.first()
                .map(|r| get_i64(r, "base_price_cents"))
                .unwrap_or(0)
        };

        // Calculate the prorated credit/charge for the unused portion of the current billing period.
        let (proration, next_billing) =
            if started_at_str.is_empty() || next_billing_at_str.is_empty() {
                (
                    0i64,
                    (chrono::Utc::now() + chrono::Duration::days(30)).to_rfc3339(),
                )
            } else {
                let started_at = chrono::DateTime::parse_from_rfc3339(&started_at_str)
                    .map(|d| d.with_timezone(&chrono::Utc))
                    .unwrap_or_else(|_| chrono::Utc::now());
                let next_billing_at = chrono::DateTime::parse_from_rfc3339(&next_billing_at_str)
                    .map(|d| d.with_timezone(&chrono::Utc))
                    .unwrap_or_else(|_| chrono::Utc::now() + chrono::Duration::days(30));

                let now = chrono::Utc::now();
                let total_days = (next_billing_at - started_at).num_days().max(1);
                let used_days = (now - started_at).num_days().max(0);
                let unused_days = (total_days - used_days).max(0);

                let proration = (old_price - new_price) * unused_days / total_days;
                (proration, next_billing_at.to_rfc3339())
            };

        self.store.execute(
            "UPDATE stackhouse_tenant_subscriptions SET plan_id = ?, proration_cents = ?, next_billing_at = ?::timestamptz WHERE tenant_id = ?".to_string(),
            vec![
                SqlValue::Text(new_plan_id.to_string()),
                SqlValue::Integer(proration),
                SqlValue::Text(next_billing),
                SqlValue::Integer(tenant_id),
            ],
        ).await?;

        Ok(json!({
            "tenant_id": tenant_id,
            "old_plan": old_plan_id,
            "new_plan": new_plan_id,
            "proration_cents": proration,
        }))
    }

    pub async fn cancel(&self, tenant_id: i64, end_of_period: bool) -> StackhouseResult<()> {
        if end_of_period {
            self.store.execute(
                "UPDATE stackhouse_tenant_subscriptions SET status = 'cancelled', cancelled_at = NOW() WHERE tenant_id = ?".to_string(),
                vec![SqlValue::Integer(tenant_id)],
            ).await?;
        } else {
            self.store.execute(
                "UPDATE stackhouse_tenant_subscriptions SET status = 'cancelled', cancelled_at = NOW(), next_billing_at = NOW() WHERE tenant_id = ?".to_string(),
                vec![SqlValue::Integer(tenant_id)],
            ).await?;
        }
        Ok(())
    }

    pub async fn list_plans(&self) -> StackhouseResult<Vec<Value>> {
        let rows = self.store.query(
            "SELECT id, name, tier, description, base_price_cents, billing_interval, features, limits FROM stackhouse_subscription_plans WHERE is_active = true ORDER BY base_price_cents".to_string(),
            vec![],
        ).await?;
        Ok(rows
            .into_iter()
            .map(|r| json!(r.into_iter().collect::<HashMap<_, _>>()))
            .collect())
    }

    pub async fn get_tenant_subscription(&self, tenant_id: i64) -> StackhouseResult<Option<Value>> {
        let rows = self
            .store
            .query(
                r#"SELECT ts.*, sp.name as plan_name, sp.base_price_cents, sp.billing_interval
               FROM stackhouse_tenant_subscriptions ts
               JOIN stackhouse_subscription_plans sp ON ts.plan_id = sp.id
               WHERE ts.tenant_id = ?"#
                    .to_string(),
                vec![SqlValue::Integer(tenant_id)],
            )
            .await?;
        Ok(rows
            .first()
            .map(|r| json!(r.iter().cloned().collect::<HashMap<_, _>>())))
    }

    pub async fn pause(&self, tenant_id: i64) -> StackhouseResult<()> {
        self.store
            .execute(
                "UPDATE stackhouse_tenant_subscriptions SET status = 'paused' WHERE tenant_id = ?"
                    .to_string(),
                vec![SqlValue::Integer(tenant_id)],
            )
            .await?;
        Ok(())
    }

    pub async fn resume(&self, tenant_id: i64) -> StackhouseResult<()> {
        self.store.execute(
            "UPDATE stackhouse_tenant_subscriptions SET status = 'active', next_billing_at = ?::timestamptz WHERE tenant_id = ?".to_string(),
            vec![
                SqlValue::Text((chrono::Utc::now() + chrono::Duration::days(30)).to_rfc3339()),
                SqlValue::Integer(tenant_id),
            ],
        ).await?;
        Ok(())
    }
}
