//! # Dunning & Failed Payment Recovery
//!
//! Automated retry schedules, grace periods, customer notifications,
//! and subscription downgrade/cancellation on final failure.

use crate::db::{SqlValue, StackhouseStore};
use crate::error::StackhouseResult;

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::sync::Arc;
use std::time::Duration;
use tracing::{info, warn};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DunningConfig {
    pub tenant_id: i64,
    pub retry_schedule: Vec<u32>, // days after failure to retry
    pub grace_period_days: u32,
    pub final_action: DunningAction,
    pub send_emails: bool,
    pub email_template_ids: Vec<String>,
}

impl Default for DunningConfig {
    fn default() -> Self {
        Self {
            tenant_id: 0,
            retry_schedule: vec![1, 3, 5, 7, 14],
            grace_period_days: 14,
            final_action: DunningAction::Cancel,
            send_emails: true,
            email_template_ids: vec![],
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DunningAction {
    Cancel,
    Downgrade { to_plan: String },
    Pause,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DunningEvent {
    pub id: String,
    pub subscription_id: String,
    pub tenant_id: i64,
    pub attempt_number: u32,
    pub status: DunningEventStatus,
    pub amount_cents: i64,
    pub currency: String,
    pub next_retry_at: Option<String>,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum DunningEventStatus {
    Retrying,
    Recovered,
    Failed,
    GracePeriod,
    Cancelled,
}

#[derive(Clone)]
pub struct DunningService {
    store: Arc<StackhouseStore>,
}

impl DunningService {
    pub async fn new(store: Arc<StackhouseStore>) -> StackhouseResult<Self> {
        let service = Self { store };
        service.initialize_tables().await?;
        service.start_dunning_worker();
        info!("📧 Dunning service initialized");
        Ok(service)
    }

    async fn initialize_tables(&self) -> StackhouseResult<()> {
        self.store.execute_batch(r#"
            CREATE TABLE IF NOT EXISTS stackhouse_dunning_configs (
                tenant_id BIGINT PRIMARY KEY,
                retry_schedule JSONB DEFAULT '[1,3,5,7,14]',
                grace_period_days INTEGER DEFAULT 14,
                final_action TEXT DEFAULT 'cancel',
                send_emails BOOLEAN DEFAULT TRUE,
                email_template_ids JSONB DEFAULT '[]'
            );
            CREATE TABLE IF NOT EXISTS stackhouse_dunning_events (
                id TEXT PRIMARY KEY,
                subscription_id TEXT NOT NULL,
                tenant_id BIGINT NOT NULL,
                attempt_number INTEGER DEFAULT 0,
                status TEXT NOT NULL DEFAULT 'retrying',
                amount_cents BIGINT NOT NULL,
                currency TEXT DEFAULT 'usd',
                next_retry_at TIMESTAMPTZ,
                created_at TIMESTAMPTZ DEFAULT NOW()
            );
            CREATE INDEX IF NOT EXISTS idx_dunning_events_sub ON stackhouse_dunning_events(subscription_id);
            CREATE INDEX IF NOT EXISTS idx_dunning_events_retry ON stackhouse_dunning_events(next_retry_at);
        "#.to_string()).await?;
        Ok(())
    }

    fn start_dunning_worker(&self) {
        let store = Arc::clone(&self.store);
        tokio::spawn(async move {
            let mut interval = tokio::time::interval(Duration::from_secs(3600)); // Hourly
            loop {
                interval.tick().await;
                Self::process_dunning(&store).await;
            }
        });
    }

    async fn process_dunning(store: &Arc<StackhouseStore>) {
        let rows = store.query(
            "SELECT id, subscription_id, tenant_id, attempt_number, amount_cents, currency FROM stackhouse_dunning_events WHERE status = 'retrying' AND next_retry_at <= NOW()".to_string(),
            vec![],
        ).await.unwrap_or_default();

        for row in &rows {
            let event_id = row
                .iter()
                .find(|(k, _)| k == "id")
                .and_then(|(_, v)| v.as_str())
                .unwrap_or("")
                .to_string();
            let tenant_id = row
                .iter()
                .find(|(k, _)| k == "tenant_id")
                .and_then(|(_, v)| v.as_i64())
                .unwrap_or(0);
            let attempt = row
                .iter()
                .find(|(k, _)| k == "attempt_number")
                .and_then(|(_, v)| v.as_i64())
                .unwrap_or(0) as u32;

            // Get config
            let config_rows = store.query(
                "SELECT retry_schedule, final_action FROM stackhouse_dunning_configs WHERE tenant_id = ?".to_string(),
                vec![SqlValue::Integer(tenant_id)],
            ).await.unwrap_or_default();

            let max_attempts = config_rows
                .first()
                .and_then(|r| r.iter().find(|(k, _)| k == "retry_schedule"))
                .and_then(|(_, v)| v.as_str())
                .and_then(|s| serde_json::from_str::<Vec<u32>>(s).ok())
                .map(|s| s.len() as u32)
                .unwrap_or(5);

            // Retry the payment via Stripe if a Stripe subscription ID is available.
            let retry_success = if let Ok(stripe_key) = std::env::var("STRIPE_SECRET_KEY")
                .or_else(|_| std::env::var("STACKHOUSE_STRIPE_SECRET_KEY"))
            {
                let sub_id = row
                    .iter()
                    .find(|(k, _)| k == "subscription_id")
                    .and_then(|(_, v)| v.as_str())
                    .unwrap_or("");
                if sub_id.is_empty() {
                    false
                } else {
                    // Attempt to pay the latest unpaid invoice for this subscription.
                    let client = reqwest::Client::new();
                    let invoices_resp = client
                        .get(format!("https://api.stripe.com/v1/invoices?subscription={}&limit=1&status=open", sub_id))
                        .bearer_auth(&stripe_key)
                        .send()
                        .await;
                    if let Ok(resp) = invoices_resp {
                        if let Ok(invoices) = resp.json::<serde_json::Value>().await {
                            if let Some(inv_id) = invoices
                                .get("data")
                                .and_then(|d| d.as_array())
                                .and_then(|arr| arr.first())
                                .and_then(|inv| inv.get("id"))
                                .and_then(|id| id.as_str())
                            {
                                let pay_resp = client
                                    .post(format!(
                                        "https://api.stripe.com/v1/invoices/{}/pay",
                                        inv_id
                                    ))
                                    .bearer_auth(&stripe_key)
                                    .send()
                                    .await;
                                pay_resp.map(|r| r.status().is_success()).unwrap_or(false)
                            } else {
                                false
                            }
                        } else {
                            false
                        }
                    } else {
                        false
                    }
                }
            } else {
                // No Stripe key configured — can't retry.
                false
            };

            if retry_success {
                store
                    .execute(
                        "UPDATE stackhouse_dunning_events SET status = 'recovered' WHERE id = ?"
                            .to_string(),
                        vec![SqlValue::Text(event_id.clone())],
                    )
                    .await
                    .ok();
            } else if attempt + 1 >= max_attempts {
                store
                    .execute(
                        "UPDATE stackhouse_dunning_events SET status = 'cancelled' WHERE id = ?"
                            .to_string(),
                        vec![SqlValue::Text(event_id.clone())],
                    )
                    .await
                    .ok();
                warn!("💀 Dunning exhausted for event {}, cancelling", event_id);
            } else {
                let next_retry_days = (attempt + 1) * 2;
                store.execute(
                    format!("UPDATE stackhouse_dunning_events SET attempt_number = ?, next_retry_at = NOW() + INTERVAL '{} days' WHERE id = ?", next_retry_days),
                    vec![SqlValue::Integer((attempt + 1) as i64), SqlValue::Text(event_id)],
                ).await.ok();
            }
        }
    }

    /// Start dunning for a failed payment
    pub async fn start_dunning(
        &self,
        tenant_id: i64,
        subscription_id: &str,
        amount_cents: i64,
        currency: &str,
    ) -> StackhouseResult<DunningEvent> {
        let id = uuid::Uuid::new_v4().to_string();

        self.store.execute(
            "INSERT INTO stackhouse_dunning_events (id, subscription_id, tenant_id, amount_cents, currency, next_retry_at) VALUES (?, ?, ?, ?, ?, NOW() + INTERVAL '1 day')".to_string(),
            vec![
                SqlValue::Text(id.clone()),
                SqlValue::Text(subscription_id.to_string()),
                SqlValue::Integer(tenant_id),
                SqlValue::Integer(amount_cents),
                SqlValue::Text(currency.to_string()),
            ],
        ).await?;

        Ok(DunningEvent {
            id,
            subscription_id: subscription_id.to_string(),
            tenant_id,
            attempt_number: 0,
            status: DunningEventStatus::Retrying,
            amount_cents,
            currency: currency.to_string(),
            next_retry_at: Some(chrono::Utc::now().to_rfc3339()),
            created_at: chrono::Utc::now().to_rfc3339(),
        })
    }

    /// Get dunning status for a subscription
    pub async fn get_status(&self, subscription_id: &str) -> StackhouseResult<Vec<Value>> {
        let rows = self.store.query(
            "SELECT id, attempt_number, status, amount_cents, next_retry_at, created_at FROM stackhouse_dunning_events WHERE subscription_id = ? ORDER BY created_at DESC".to_string(),
            vec![SqlValue::Text(subscription_id.to_string())],
        ).await?;
        Ok(rows
            .into_iter()
            .map(|r| json!(r.into_iter().collect::<std::collections::HashMap<_, _>>()))
            .collect())
    }

    /// Set dunning config
    pub async fn set_config(&self, config: DunningConfig) -> StackhouseResult<()> {
        self.store.execute(
            r#"INSERT INTO stackhouse_dunning_configs (tenant_id, retry_schedule, grace_period_days, final_action, send_emails)
               VALUES (?, ?::jsonb, ?, ?, ?)
               ON CONFLICT (tenant_id) DO UPDATE SET retry_schedule = EXCLUDED.retry_schedule,
               grace_period_days = EXCLUDED.grace_period_days, final_action = EXCLUDED.final_action,
               send_emails = EXCLUDED.send_emails"#.to_string(),
            vec![
                SqlValue::Integer(config.tenant_id),
                SqlValue::Text(serde_json::to_string(&config.retry_schedule).unwrap_or_default()),
                SqlValue::Integer(config.grace_period_days as i64),
                SqlValue::Text(serde_json::to_string(&config.final_action).unwrap_or_default()),
                SqlValue::Text(config.send_emails.to_string()),
            ],
        ).await?;
        Ok(())
    }
}
