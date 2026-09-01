//! # Tenant Resource Quotas & Rate Limits
//!
//! Configurable per-tenant, per-plan resource quotas with enforcement.
//! Supports rate limiting (requests/min), storage caps, row limits, and
//! compute time budgets.

use crate::db::{SqlValue, StackhouseStore};
use crate::error::StackhouseResult;

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{info, warn};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TenantQuota {
    pub tenant_id: i64,
    pub plan_id: String,
    pub limits: QuotaLimits,
    pub current_usage: QuotaUsage,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QuotaLimits {
    pub requests_per_minute: u64,
    pub requests_per_day: u64,
    pub storage_bytes: u64,
    pub database_rows: u64,
    pub bandwidth_bytes_per_month: u64,
    pub function_invocations_per_day: u64,
    pub compute_seconds_per_month: u64,
    pub vector_collections: u32,
    pub vector_documents: u64,
    pub team_members: u32,
    pub api_keys: u32,
}

impl Default for QuotaLimits {
    fn default() -> Self {
        Self {
            requests_per_minute: 1000,
            requests_per_day: 100_000,
            storage_bytes: 10 * 1024 * 1024 * 1024, // 10GB
            database_rows: 1_000_000,
            bandwidth_bytes_per_month: 100 * 1024 * 1024 * 1024, // 100GB
            function_invocations_per_day: 10_000,
            compute_seconds_per_month: 36_000,
            vector_collections: 50,
            vector_documents: 1_000_000,
            team_members: 25,
            api_keys: 50,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct QuotaUsage {
    pub requests_this_minute: u64,
    pub requests_today: u64,
    pub storage_bytes_used: u64,
    pub database_rows_used: u64,
    pub bandwidth_this_month: u64,
    pub functions_today: u64,
    pub compute_this_month: u64,
    pub vector_collections_used: u32,
    pub vector_documents_used: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QuotaCheckResult {
    pub allowed: bool,
    pub resource: String,
    pub current: u64,
    pub limit: u64,
    pub remaining: u64,
    pub reset_at: Option<String>,
}

#[derive(Clone)]
pub struct QuotaService {
    store: Arc<StackhouseStore>,
    rate_counters: Arc<RwLock<HashMap<String, RateCounter>>>,
}

#[derive(Clone, Debug)]
struct RateCounter {
    count: u64,
    window_start: std::time::Instant,
}

impl QuotaService {
    pub async fn new(store: Arc<StackhouseStore>) -> StackhouseResult<Self> {
        let service = Self {
            store,
            rate_counters: Arc::new(RwLock::new(HashMap::new())),
        };
        service.initialize_tables().await?;
        info!("🚦 Quota service initialized");
        Ok(service)
    }

    async fn initialize_tables(&self) -> StackhouseResult<()> {
        self.store
            .execute_batch(
                r#"
            CREATE TABLE IF NOT EXISTS stackhouse_tenant_quotas (
                tenant_id BIGINT PRIMARY KEY,
                plan_id TEXT NOT NULL,
                limits JSONB NOT NULL,
                created_at TIMESTAMPTZ DEFAULT NOW(),
                updated_at TIMESTAMPTZ DEFAULT NOW()
            );
            CREATE TABLE IF NOT EXISTS stackhouse_quota_usage (
                tenant_id BIGINT NOT NULL,
                resource TEXT NOT NULL,
                period TEXT NOT NULL,
                period_start TIMESTAMPTZ NOT NULL,
                used BIGINT DEFAULT 0,
                PRIMARY KEY (tenant_id, resource, period_start)
            );
        "#
                .to_string(),
            )
            .await?;
        Ok(())
    }

    /// Set quota limits for a tenant
    pub async fn set_limits(
        &self,
        tenant_id: i64,
        plan_id: &str,
        limits: &QuotaLimits,
    ) -> StackhouseResult<()> {
        self.store.execute(
            "INSERT INTO stackhouse_tenant_quotas (tenant_id, plan_id, limits) VALUES (?, ?, ?::jsonb) ON CONFLICT (tenant_id) DO UPDATE SET plan_id = EXCLUDED.plan_id, limits = EXCLUDED.limits, updated_at = NOW()".to_string(),
            vec![
                SqlValue::Integer(tenant_id),
                SqlValue::Text(plan_id.to_string()),
                SqlValue::Text(serde_json::to_string(limits).unwrap_or_default()),
            ],
        ).await?;
        Ok(())
    }

    /// Check if a request is within rate limits
    pub async fn check_rate_limit(
        &self,
        tenant_id: i64,
        resource: &str,
    ) -> StackhouseResult<QuotaCheckResult> {
        let limits = self.get_limits(tenant_id).await?;
        let limit = match resource {
            "requests_per_minute" => limits.requests_per_minute,
            "requests_per_day" => limits.requests_per_day,
            "function_invocations" => limits.function_invocations_per_day,
            _ => u64::MAX,
        };

        let key = format!("{}:{}", tenant_id, resource);
        let mut counters = self.rate_counters.write().await;
        let counter = counters.entry(key).or_insert(RateCounter {
            count: 0,
            window_start: std::time::Instant::now(),
        });

        // Reset window if expired
        let window_duration = match resource {
            "requests_per_minute" => std::time::Duration::from_secs(60),
            _ => std::time::Duration::from_secs(86400),
        };

        if counter.window_start.elapsed() > window_duration {
            counter.count = 0;
            counter.window_start = std::time::Instant::now();
        }

        counter.count += 1;
        let allowed = counter.count <= limit;
        let remaining = limit.saturating_sub(counter.count);

        if !allowed {
            warn!(
                "🚫 Rate limit exceeded: tenant={}, resource={}",
                tenant_id, resource
            );
        }

        Ok(QuotaCheckResult {
            allowed,
            resource: resource.to_string(),
            current: counter.count,
            limit,
            remaining,
            reset_at: None,
        })
    }

    /// Check a resource quota (storage, rows, etc.)
    pub async fn check_quota(
        &self,
        tenant_id: i64,
        resource: &str,
        current_usage: u64,
    ) -> StackhouseResult<QuotaCheckResult> {
        let limits = self.get_limits(tenant_id).await?;
        let limit = match resource {
            "storage_bytes" => limits.storage_bytes,
            "database_rows" => limits.database_rows,
            "vector_collections" => limits.vector_collections as u64,
            "vector_documents" => limits.vector_documents,
            "team_members" => limits.team_members as u64,
            "api_keys" => limits.api_keys as u64,
            _ => u64::MAX,
        };

        let allowed = current_usage < limit;
        let remaining = limit.saturating_sub(current_usage);

        Ok(QuotaCheckResult {
            allowed,
            resource: resource.to_string(),
            current: current_usage,
            limit,
            remaining,
            reset_at: None,
        })
    }

    /// Increment usage counter
    pub async fn increment_usage(
        &self,
        tenant_id: i64,
        resource: &str,
        amount: u64,
    ) -> StackhouseResult<()> {
        self.store.execute(
            r#"INSERT INTO stackhouse_quota_usage (tenant_id, resource, period, period_start, used)
               VALUES (?, ?, 'monthly', date_trunc('month', NOW()), ?)
               ON CONFLICT (tenant_id, resource, period_start) DO UPDATE
               SET used = stackhouse_quota_usage.used + EXCLUDED.used"#.to_string(),
            vec![
                SqlValue::Integer(tenant_id),
                SqlValue::Text(resource.to_string()),
                SqlValue::Integer(amount as i64),
            ],
        ).await?;
        Ok(())
    }

    async fn get_limits(&self, tenant_id: i64) -> StackhouseResult<QuotaLimits> {
        let rows = self
            .store
            .query(
                "SELECT limits FROM stackhouse_tenant_quotas WHERE tenant_id = ?".to_string(),
                vec![SqlValue::Integer(tenant_id)],
            )
            .await?;
        if let Some(row) = rows.first() {
            let limits_str = row
                .iter()
                .find(|(k, _)| k == "limits")
                .and_then(|(_, v)| v.as_str())
                .unwrap_or("{}");
            Ok(serde_json::from_str(limits_str).unwrap_or_default())
        } else {
            Ok(QuotaLimits::default())
        }
    }

    /// Get usage summary for a tenant
    pub async fn get_usage_summary(&self, tenant_id: i64) -> StackhouseResult<Vec<Value>> {
        let rows = self.store.query(
            "SELECT resource, used FROM stackhouse_quota_usage WHERE tenant_id = ? AND period_start = date_trunc('month', NOW())".to_string(),
            vec![SqlValue::Integer(tenant_id)],
        ).await?;
        Ok(rows
            .into_iter()
            .map(|r| json!(r.into_iter().collect::<HashMap<_, _>>()))
            .collect())
    }
}
