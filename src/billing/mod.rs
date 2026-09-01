//! # Stackhouse-Billing
//!
//! Native RevenueCat-style subscription backend for Stackhouse.
//!
//! Provides:
//! - App / product / entitlement / offering / package configuration
//! - Customer + subscription + transaction persistence
//! - Apple App Store, Google Play, and Stripe receipt / webhook adapters
//! - Entitlement resolver
//! - Outbound webhook dispatcher with retry
//!
//! Mounted at `/v1/billing` behind the `--enable-billing` flag.

pub mod analytics;
pub mod audiences;
pub mod dunning;
pub mod entitlement_engine;
pub mod entitlements;
pub mod experiments;
pub mod handlers;
pub mod invoices;
pub mod metering;
pub mod models;
pub mod providers;
pub mod routes;
pub mod schema;
pub mod state;
pub mod store;
pub mod subscription_plans;
pub mod trials;
pub mod usage_billing;
pub mod validators;
pub mod webhooks;

#[cfg(test)]
pub mod tests;

pub use analytics::*;
pub use entitlement_engine::*;
pub use invoices::*;
pub use metering::*;
pub use routes::create_billing_router;
pub use state::{BillingConfig, BillingState};
pub use store::BillingStore;
pub use subscription_plans::*;
pub use usage_billing::*;

use crate::db::StackhouseStore;
use crate::error::StackhouseResult;
use std::sync::Arc;
use tracing::info;

/// Initialise billing: runs idempotent migrations and returns a shared store.
pub async fn init(store: Arc<StackhouseStore>) -> StackhouseResult<Arc<BillingStore>> {
    info!("💳 Initialising Stackhouse-Billing (RevenueCat-style subscriptions)");
    store.execute_batch(schema::MIGRATIONS.to_string()).await?;
    seed_default_plans(&store).await?;
    Ok(Arc::new(BillingStore::new(store)))
}

/// Seed default subscription plans if none exist yet.
async fn seed_default_plans(store: &StackhouseStore) -> StackhouseResult<()> {
    let existing = store
        .query(
            "SELECT count(*) as cnt FROM stackhouse_subscription_plans".to_string(),
            vec![],
        )
        .await?;

    let count = existing
        .first()
        .and_then(|r| r.iter().find(|(k, _)| k == "cnt"))
        .and_then(|(_, v)| v.as_i64())
        .unwrap_or(0);

    if count > 0 {
        return Ok(());
    }

    info!("📋 Seeding default subscription plans...");

    let plans = vec![
        (r#"('free', 'Free', 'free', 'Get started with Stackhouse', 0, 'monthly',
            '[{"key":"tables","name":"Tables","included":true,"limit":5},
              {"key":"api_calls","name":"API Calls/mo","included":true,"limit":10000},
              {"key":"vector_operations","name":"Vector Operations/mo","included":true,"limit":100}]'::jsonb,
            '{"seats":1,"storage_gb":1,"api_calls_per_month":10000,"vector_operations_per_month":100,"vector_documents":1000,"bandwidth_gb":5,"functions":5,"concurrent_jobs":1}'::jsonb)"#),
        (r#"('starter', 'Starter', 'starter', 'For small teams building apps', 2900, 'monthly',
            '[{"key":"tables","name":"Tables","included":true,"limit":50},
              {"key":"api_calls","name":"API Calls/mo","included":true,"limit":100000},
              {"key":"vector_operations","name":"Vector Operations/mo","included":true,"limit":1000},
              {"key":"realtime","name":"Realtime Streams","included":true}]'::jsonb,
            '{"seats":5,"storage_gb":10,"api_calls_per_month":100000,"vector_operations_per_month":1000,"vector_documents":10000,"bandwidth_gb":50,"functions":25,"concurrent_jobs":5}'::jsonb)"#),
        (r#"('pro', 'Pro', 'pro', 'For growing teams that need more power', 9900, 'monthly',
            '[{"key":"tables","name":"Tables","included":true,"limit":500},
              {"key":"api_calls","name":"API Calls/mo","included":true,"limit":1000000},
              {"key":"vector_operations","name":"Vector Operations/mo","included":true,"limit":10000},
              {"key":"realtime","name":"Realtime Streams","included":true},
              {"key":"worksheets","name":"Worksheets & Dashboards","included":true}]'::jsonb,
            '{"seats":25,"storage_gb":100,"api_calls_per_month":1000000,"vector_operations_per_month":10000,"vector_documents":100000,"bandwidth_gb":500,"functions":100,"concurrent_jobs":20}'::jsonb)"#),
        (r#"('enterprise', 'Enterprise', 'enterprise', 'Unlimited scale with priority support', 49900, 'monthly',
            '[{"key":"tables","name":"Tables","included":true},
              {"key":"api_calls","name":"API Calls/mo","included":true},
              {"key":"vector_operations","name":"Vector Operations/mo","included":true},
              {"key":"realtime","name":"Realtime Streams","included":true},
              {"key":"worksheets","name":"Worksheets & Dashboards","included":true},
              {"key":"sso","name":"SSO & SAML","included":true},
              {"key":"audit_log","name":"Audit Logs","included":true},
              {"key":"priority_support","name":"Priority Support","included":true}]'::jsonb,
            '{"seats":0,"storage_gb":0,"api_calls_per_month":0,"vector_operations_per_month":0,"vector_documents":0,"bandwidth_gb":0,"functions":0,"concurrent_jobs":0}'::jsonb)"#),
    ];

    for plan_sql in &plans {
        store.execute(
            format!("INSERT INTO stackhouse_subscription_plans (id, name, tier, description, base_price_cents, billing_interval, features, limits, is_active) VALUES {}", plan_sql),
            vec![],
        ).await?;
    }

    info!("✅ Seeded 4 default plans: Free, Starter ($29), Pro ($99), Enterprise ($499)");
    Ok(())
}
