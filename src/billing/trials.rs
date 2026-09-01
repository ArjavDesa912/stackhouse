//! # Free Trials & Promotional Offers
//!
//! Trial management, promo codes, coupon system, and currency conversion.

use crate::db::{SqlValue, StackhouseStore};
use crate::error::{StackhouseError, StackhouseResult};

use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tracing::info;

// ============================================================================
// Trials
// ============================================================================

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Trial {
    pub id: String,
    pub tenant_id: i64,
    pub plan_id: String,
    pub status: TrialStatus,
    pub started_at: String,
    pub expires_at: String,
    pub converted: bool,
    pub converted_at: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum TrialStatus {
    Active,
    Expired,
    Converted,
    Cancelled,
}

// ============================================================================
// Promo Codes
// ============================================================================

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PromoCode {
    pub id: String,
    pub code: String,
    pub discount_type: DiscountType,
    pub discount_value: f64,
    pub currency: Option<String>,
    pub max_redemptions: Option<u32>,
    pub current_redemptions: u32,
    pub valid_from: String,
    pub valid_until: Option<String>,
    pub applicable_plans: Vec<String>,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DiscountType {
    Percentage,
    FixedAmount,
    TrialExtension { days: u32 },
}

// ============================================================================
// Multi-Currency
// ============================================================================

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CurrencyRate {
    pub from_currency: String,
    pub to_currency: String,
    pub rate: f64,
    pub updated_at: String,
}

// ============================================================================
// Service
// ============================================================================

#[derive(Clone)]
pub struct TrialsAndPromosService {
    store: Arc<StackhouseStore>,
}

impl TrialsAndPromosService {
    pub async fn new(store: Arc<StackhouseStore>) -> StackhouseResult<Self> {
        let service = Self { store };
        service.initialize_tables().await?;
        info!("🎁 Trials & promotions service initialized");
        Ok(service)
    }

    async fn initialize_tables(&self) -> StackhouseResult<()> {
        self.store
            .execute_batch(
                r#"
            CREATE TABLE IF NOT EXISTS stackhouse_trials (
                id TEXT PRIMARY KEY,
                tenant_id BIGINT NOT NULL,
                plan_id TEXT NOT NULL,
                status TEXT NOT NULL DEFAULT 'active',
                started_at TIMESTAMPTZ DEFAULT NOW(),
                expires_at TIMESTAMPTZ NOT NULL,
                converted BOOLEAN DEFAULT FALSE,
                converted_at TIMESTAMPTZ
            );
            CREATE TABLE IF NOT EXISTS stackhouse_promo_codes (
                id TEXT PRIMARY KEY,
                code TEXT NOT NULL UNIQUE,
                discount_type TEXT NOT NULL,
                discount_value FLOAT NOT NULL,
                currency TEXT,
                max_redemptions INTEGER,
                current_redemptions INTEGER DEFAULT 0,
                valid_from TIMESTAMPTZ DEFAULT NOW(),
                valid_until TIMESTAMPTZ,
                applicable_plans JSONB DEFAULT '[]',
                created_at TIMESTAMPTZ DEFAULT NOW()
            );
            CREATE TABLE IF NOT EXISTS stackhouse_promo_redemptions (
                id TEXT PRIMARY KEY,
                promo_code_id TEXT NOT NULL,
                tenant_id BIGINT NOT NULL,
                subscription_id TEXT,
                redeemed_at TIMESTAMPTZ DEFAULT NOW()
            );
            CREATE TABLE IF NOT EXISTS stackhouse_currency_rates (
                from_currency TEXT NOT NULL,
                to_currency TEXT NOT NULL,
                rate FLOAT NOT NULL,
                updated_at TIMESTAMPTZ DEFAULT NOW(),
                PRIMARY KEY (from_currency, to_currency)
            );
            CREATE INDEX IF NOT EXISTS idx_trials_tenant ON stackhouse_trials(tenant_id);
            CREATE INDEX IF NOT EXISTS idx_promo_code ON stackhouse_promo_codes(code);
        "#
                .to_string(),
            )
            .await?;
        Ok(())
    }

    // ========== TRIALS ==========

    /// Start a free trial
    pub async fn start_trial(
        &self,
        tenant_id: i64,
        plan_id: &str,
        trial_days: u32,
    ) -> StackhouseResult<Trial> {
        let id = uuid::Uuid::new_v4().to_string();
        let now = chrono::Utc::now();
        let expires_at = now + chrono::Duration::days(trial_days as i64);

        self.store.execute(
            "INSERT INTO stackhouse_trials (id, tenant_id, plan_id, expires_at) VALUES (?, ?, ?, ?::timestamptz)".to_string(),
            vec![
                SqlValue::Text(id.clone()),
                SqlValue::Integer(tenant_id),
                SqlValue::Text(plan_id.to_string()),
                SqlValue::Text(expires_at.to_rfc3339()),
            ],
        ).await?;

        Ok(Trial {
            id,
            tenant_id,
            plan_id: plan_id.to_string(),
            status: TrialStatus::Active,
            started_at: now.to_rfc3339(),
            expires_at: expires_at.to_rfc3339(),
            converted: false,
            converted_at: None,
        })
    }

    /// Convert trial to paid subscription
    pub async fn convert_trial(&self, trial_id: &str) -> StackhouseResult<()> {
        self.store.execute(
            "UPDATE stackhouse_trials SET status = 'converted', converted = TRUE, converted_at = NOW() WHERE id = ? AND status = 'active'".to_string(),
            vec![SqlValue::Text(trial_id.to_string())],
        ).await?;
        Ok(())
    }

    /// Check if tenant has active trial
    pub async fn has_active_trial(&self, tenant_id: i64) -> StackhouseResult<Option<Trial>> {
        let rows = self.store.query(
            "SELECT id, plan_id, status, started_at, expires_at FROM stackhouse_trials WHERE tenant_id = ? AND status = 'active' AND expires_at > NOW() ORDER BY started_at DESC LIMIT 1".to_string(),
            vec![SqlValue::Integer(tenant_id)],
        ).await?;
        if rows.is_empty() {
            return Ok(None);
        }
        let row = &rows[0];
        Ok(Some(Trial {
            id: row
                .iter()
                .find(|(k, _)| k == "id")
                .and_then(|(_, v)| v.as_str().map(String::from))
                .unwrap_or_default(),
            tenant_id,
            plan_id: row
                .iter()
                .find(|(k, _)| k == "plan_id")
                .and_then(|(_, v)| v.as_str().map(String::from))
                .unwrap_or_default(),
            status: TrialStatus::Active,
            started_at: row
                .iter()
                .find(|(k, _)| k == "started_at")
                .and_then(|(_, v)| v.as_str().map(String::from))
                .unwrap_or_default(),
            expires_at: row
                .iter()
                .find(|(k, _)| k == "expires_at")
                .and_then(|(_, v)| v.as_str().map(String::from))
                .unwrap_or_default(),
            converted: false,
            converted_at: None,
        }))
    }

    // ========== PROMO CODES ==========

    /// Create a promo code
    pub async fn create_promo(
        &self,
        code: &str,
        discount_type: DiscountType,
        discount_value: f64,
        max_redemptions: Option<u32>,
        valid_until: Option<&str>,
        applicable_plans: Vec<String>,
    ) -> StackhouseResult<PromoCode> {
        let id = uuid::Uuid::new_v4().to_string();
        let dt_str = serde_json::to_string(&discount_type).unwrap_or_default();

        self.store.execute(
            "INSERT INTO stackhouse_promo_codes (id, code, discount_type, discount_value, max_redemptions, valid_until, applicable_plans) VALUES (?, ?, ?, ?, ?, ?::timestamptz, ?::jsonb)".to_string(),
            vec![
                SqlValue::Text(id.clone()),
                SqlValue::Text(code.to_string()),
                SqlValue::Text(dt_str),
                SqlValue::Text(discount_value.to_string()),
                SqlValue::Integer(max_redemptions.unwrap_or(0) as i64),
                SqlValue::Text(valid_until.unwrap_or("").to_string()),
                SqlValue::Text(serde_json::to_string(&applicable_plans).unwrap_or_default()),
            ],
        ).await?;

        Ok(PromoCode {
            id,
            code: code.to_string(),
            discount_type,
            discount_value,
            currency: None,
            max_redemptions,
            current_redemptions: 0,
            valid_from: chrono::Utc::now().to_rfc3339(),
            valid_until: valid_until.map(String::from),
            applicable_plans,
            created_at: chrono::Utc::now().to_rfc3339(),
        })
    }

    /// Validate and redeem a promo code
    pub async fn redeem_promo(
        &self,
        code: &str,
        tenant_id: i64,
        _plan_id: &str,
    ) -> StackhouseResult<f64> {
        let rows = self.store.query(
            "SELECT id, discount_type, discount_value, max_redemptions, current_redemptions, valid_until, applicable_plans FROM stackhouse_promo_codes WHERE code = ?".to_string(),
            vec![SqlValue::Text(code.to_string())],
        ).await?;

        if rows.is_empty() {
            return Err(StackhouseError::NotFound("Promo code not found".into()));
        }

        let row = &rows[0];
        let promo_id = row
            .iter()
            .find(|(k, _)| k == "id")
            .and_then(|(_, v)| v.as_str())
            .unwrap_or("")
            .to_string();
        let max_r = row
            .iter()
            .find(|(k, _)| k == "max_redemptions")
            .and_then(|(_, v)| v.as_i64())
            .unwrap_or(0) as u32;
        let cur_r = row
            .iter()
            .find(|(k, _)| k == "current_redemptions")
            .and_then(|(_, v)| v.as_i64())
            .unwrap_or(0) as u32;
        let discount_value = row
            .iter()
            .find(|(k, _)| k == "discount_value")
            .and_then(|(_, v)| v.as_f64())
            .unwrap_or(0.0);

        if max_r > 0 && cur_r >= max_r {
            return Err(StackhouseError::InvalidPayload(
                "Promo code redemption limit reached".into(),
            ));
        }

        // Record redemption
        self.store.execute(
            "INSERT INTO stackhouse_promo_redemptions (id, promo_code_id, tenant_id) VALUES (?, ?, ?)".to_string(),
            vec![SqlValue::Text(uuid::Uuid::new_v4().to_string()), SqlValue::Text(promo_id.clone()), SqlValue::Integer(tenant_id)],
        ).await?;
        self.store.execute(
            "UPDATE stackhouse_promo_codes SET current_redemptions = current_redemptions + 1 WHERE id = ?".to_string(),
            vec![SqlValue::Text(promo_id)],
        ).await?;

        Ok(discount_value)
    }

    // ========== MULTI-CURRENCY ==========

    /// Convert amount between currencies
    pub async fn convert_currency(
        &self,
        amount: f64,
        from: &str,
        to: &str,
    ) -> StackhouseResult<f64> {
        if from == to {
            return Ok(amount);
        }

        let rows = self.store.query(
            "SELECT rate FROM stackhouse_currency_rates WHERE from_currency = ? AND to_currency = ?".to_string(),
            vec![SqlValue::Text(from.to_string()), SqlValue::Text(to.to_string())],
        ).await?;

        let rate = rows
            .first()
            .and_then(|r| r.iter().find(|(k, _)| k == "rate"))
            .and_then(|(_, v)| v.as_f64())
            .ok_or_else(|| StackhouseError::NotFound(format!("No rate for {} -> {}", from, to)))?;

        Ok(amount * rate)
    }

    /// Update currency rate
    pub async fn set_currency_rate(&self, from: &str, to: &str, rate: f64) -> StackhouseResult<()> {
        self.store.execute(
            "INSERT INTO stackhouse_currency_rates (from_currency, to_currency, rate) VALUES (?, ?, ?) ON CONFLICT (from_currency, to_currency) DO UPDATE SET rate = EXCLUDED.rate, updated_at = NOW()".to_string(),
            vec![
                SqlValue::Text(from.to_string()),
                SqlValue::Text(to.to_string()),
                SqlValue::Text(rate.to_string()),
            ],
        ).await?;
        Ok(())
    }
}
