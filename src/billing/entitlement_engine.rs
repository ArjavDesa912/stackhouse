//! # Entitlement Engine
//!
//! Resolves `user.can("feature_x")` by checking plan, add-ons, usage limits,
//! and custom overrides in a single fast call.

use crate::db::{SqlValue, StackhouseStore};
use crate::error::StackhouseResult;

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::info;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Entitlement {
    pub feature: String,
    pub entitled: bool,
    pub limit: Option<f64>,
    pub current_usage: Option<f64>,
    pub source: EntitlementSource,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EntitlementSource {
    Plan,
    AddOn,
    Override,
    Trial,
    Legacy,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlanDefinition {
    pub id: String,
    pub name: String,
    pub tier: u32,
    pub features: HashMap<String, FeatureGrant>,
    pub limits: HashMap<String, f64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FeatureGrant {
    pub enabled: bool,
    pub limit: Option<f64>,
    pub meter_name: Option<String>,
}

#[derive(Clone)]
pub struct EntitlementEngine {
    store: Arc<StackhouseStore>,
    plan_cache: Arc<RwLock<HashMap<String, PlanDefinition>>>,
}

impl EntitlementEngine {
    pub async fn new(store: Arc<StackhouseStore>) -> StackhouseResult<Self> {
        let engine = Self {
            store,
            plan_cache: Arc::new(RwLock::new(HashMap::new())),
        };
        engine.initialize_tables().await?;
        engine.load_plans().await?;
        info!("🎫 Entitlement engine initialized");
        Ok(engine)
    }

    async fn initialize_tables(&self) -> StackhouseResult<()> {
        self.store
            .execute_batch(
                r#"
            CREATE TABLE IF NOT EXISTS stackhouse_plans (
                id TEXT PRIMARY KEY,
                name TEXT NOT NULL,
                tier INTEGER DEFAULT 0,
                features JSONB NOT NULL DEFAULT '{}',
                limits JSONB NOT NULL DEFAULT '{}',
                created_at TIMESTAMPTZ DEFAULT NOW()
            );
            CREATE TABLE IF NOT EXISTS stackhouse_tenant_plans (
                tenant_id BIGINT PRIMARY KEY,
                plan_id TEXT NOT NULL,
                add_ons JSONB DEFAULT '[]',
                overrides JSONB DEFAULT '{}',
                started_at TIMESTAMPTZ DEFAULT NOW(),
                expires_at TIMESTAMPTZ
            );
            CREATE TABLE IF NOT EXISTS stackhouse_entitlement_overrides (
                tenant_id BIGINT NOT NULL,
                feature TEXT NOT NULL,
                enabled BOOLEAN NOT NULL,
                custom_limit FLOAT,
                reason TEXT,
                expires_at TIMESTAMPTZ,
                PRIMARY KEY (tenant_id, feature)
            );
        "#
                .to_string(),
            )
            .await?;
        Ok(())
    }

    async fn load_plans(&self) -> StackhouseResult<()> {
        let rows = self
            .store
            .query(
                "SELECT id, name, tier, features, limits FROM stackhouse_plans".to_string(),
                vec![],
            )
            .await?;
        let mut cache = self.plan_cache.write().await;
        for row in rows {
            let id = row
                .iter()
                .find(|(k, _)| k == "id")
                .and_then(|(_, v)| v.as_str())
                .unwrap_or("")
                .to_string();
            let name = row
                .iter()
                .find(|(k, _)| k == "name")
                .and_then(|(_, v)| v.as_str())
                .unwrap_or("")
                .to_string();
            let tier = row
                .iter()
                .find(|(k, _)| k == "tier")
                .and_then(|(_, v)| v.as_i64())
                .unwrap_or(0) as u32;
            let features_str = row
                .iter()
                .find(|(k, _)| k == "features")
                .and_then(|(_, v)| v.as_str())
                .unwrap_or("{}");
            let limits_str = row
                .iter()
                .find(|(k, _)| k == "limits")
                .and_then(|(_, v)| v.as_str())
                .unwrap_or("{}");

            cache.insert(
                id.clone(),
                PlanDefinition {
                    id,
                    name,
                    tier,
                    features: serde_json::from_str(features_str).unwrap_or_default(),
                    limits: serde_json::from_str(limits_str).unwrap_or_default(),
                },
            );
        }
        Ok(())
    }

    /// Check if a tenant is entitled to a feature: `user.can("feature_x")`
    pub async fn check(&self, tenant_id: i64, feature: &str) -> StackhouseResult<Entitlement> {
        // 1. Check overrides first (highest priority)
        let override_rows = self.store.query(
            "SELECT enabled, custom_limit FROM stackhouse_entitlement_overrides WHERE tenant_id = ? AND feature = ? AND (expires_at IS NULL OR expires_at > NOW())".to_string(),
            vec![SqlValue::Integer(tenant_id), SqlValue::Text(feature.to_string())],
        ).await?;

        if let Some(row) = override_rows.first() {
            let enabled = row
                .iter()
                .find(|(k, _)| k == "enabled")
                .and_then(|(_, v)| v.as_str())
                .map(|s| s == "true" || s == "t")
                .unwrap_or(false);
            let limit = row
                .iter()
                .find(|(k, _)| k == "custom_limit")
                .and_then(|(_, v)| v.as_f64());
            return Ok(Entitlement {
                feature: feature.to_string(),
                entitled: enabled,
                limit,
                current_usage: None,
                source: EntitlementSource::Override,
            });
        }

        // 2. Check tenant's plan
        let plan_rows = self.store.query(
            "SELECT plan_id, add_ons FROM stackhouse_tenant_plans WHERE tenant_id = ? AND (expires_at IS NULL OR expires_at > NOW())".to_string(),
            vec![SqlValue::Integer(tenant_id)],
        ).await?;

        if let Some(row) = plan_rows.first() {
            let plan_id = row
                .iter()
                .find(|(k, _)| k == "plan_id")
                .and_then(|(_, v)| v.as_str())
                .unwrap_or("");
            let cache = self.plan_cache.read().await;

            if let Some(plan) = cache.get(plan_id) {
                if let Some(grant) = plan.features.get(feature) {
                    return Ok(Entitlement {
                        feature: feature.to_string(),
                        entitled: grant.enabled,
                        limit: grant.limit,
                        current_usage: None,
                        source: EntitlementSource::Plan,
                    });
                }
            }
        }

        // 3. Not entitled
        Ok(Entitlement {
            feature: feature.to_string(),
            entitled: false,
            limit: None,
            current_usage: None,
            source: EntitlementSource::Plan,
        })
    }

    /// Batch check multiple features
    pub async fn check_many(
        &self,
        tenant_id: i64,
        features: &[&str],
    ) -> StackhouseResult<HashMap<String, Entitlement>> {
        let mut results = HashMap::new();
        for feature in features {
            results.insert(feature.to_string(), self.check(tenant_id, feature).await?);
        }
        Ok(results)
    }

    /// Create/update a plan definition
    pub async fn upsert_plan(&self, plan: &PlanDefinition) -> StackhouseResult<()> {
        self.store.execute(
            "INSERT INTO stackhouse_plans (id, name, tier, features, limits) VALUES (?, ?, ?, ?::jsonb, ?::jsonb) ON CONFLICT (id) DO UPDATE SET name = EXCLUDED.name, tier = EXCLUDED.tier, features = EXCLUDED.features, limits = EXCLUDED.limits".to_string(),
            vec![
                SqlValue::Text(plan.id.clone()),
                SqlValue::Text(plan.name.clone()),
                SqlValue::Integer(plan.tier as i64),
                SqlValue::Text(serde_json::to_string(&plan.features).unwrap_or_default()),
                SqlValue::Text(serde_json::to_string(&plan.limits).unwrap_or_default()),
            ],
        ).await?;
        self.plan_cache
            .write()
            .await
            .insert(plan.id.clone(), plan.clone());
        Ok(())
    }

    /// Assign a plan to a tenant
    pub async fn assign_plan(&self, tenant_id: i64, plan_id: &str) -> StackhouseResult<()> {
        self.store.execute(
            "INSERT INTO stackhouse_tenant_plans (tenant_id, plan_id) VALUES (?, ?) ON CONFLICT (tenant_id) DO UPDATE SET plan_id = EXCLUDED.plan_id, started_at = NOW()".to_string(),
            vec![SqlValue::Integer(tenant_id), SqlValue::Text(plan_id.to_string())],
        ).await?;
        Ok(())
    }

    /// Set a feature override for a tenant
    pub async fn set_override(
        &self,
        tenant_id: i64,
        feature: &str,
        enabled: bool,
        limit: Option<f64>,
        reason: &str,
    ) -> StackhouseResult<()> {
        self.store.execute(
            "INSERT INTO stackhouse_entitlement_overrides (tenant_id, feature, enabled, custom_limit, reason) VALUES (?, ?, ?, ?, ?) ON CONFLICT (tenant_id, feature) DO UPDATE SET enabled = EXCLUDED.enabled, custom_limit = EXCLUDED.custom_limit, reason = EXCLUDED.reason".to_string(),
            vec![
                SqlValue::Integer(tenant_id),
                SqlValue::Text(feature.to_string()),
                SqlValue::Text(enabled.to_string()),
                SqlValue::Text(limit.map(|l| l.to_string()).unwrap_or_default()),
                SqlValue::Text(reason.to_string()),
            ],
        ).await?;
        Ok(())
    }
}
