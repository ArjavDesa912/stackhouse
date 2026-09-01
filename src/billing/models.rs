//! Domain model types for the billing module.
//!
//! The fields map 1:1 to columns in `billing_*` tables (see `schema.rs`).

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;

pub const STORE_APPLE: &str = "app_store";
pub const STORE_GOOGLE: &str = "play_store";
pub const STORE_STRIPE: &str = "stripe";
pub const STORE_PROMOTIONAL: &str = "promotional";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct App {
    pub id: i64,
    pub project_id: Option<i64>,
    pub name: String,
    pub bundle_id: Option<String>,
    pub platform: String,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Product {
    pub id: i64,
    pub app_id: i64,
    pub store: String,
    pub store_product_id: String,
    pub product_type: String,
    pub period: Option<String>,
    pub price_micros: Option<i64>,
    pub currency: Option<String>,
    pub metadata: Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Entitlement {
    pub id: i64,
    pub app_id: i64,
    pub identifier: String,
    pub display_name: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Offering {
    pub id: i64,
    pub app_id: i64,
    pub identifier: String,
    pub is_current: bool,
    pub metadata: Value,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub audience_id: Option<i64>,
    #[serde(default)]
    pub packages: Vec<Package>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Package {
    pub id: i64,
    pub offering_id: i64,
    pub identifier: String,
    pub product_id: i64,
    pub package_type: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Customer {
    pub id: i64,
    pub app_id: i64,
    pub app_user_id: String,
    pub aliases: Value,
    pub attributes: Value,
    pub first_seen_at: DateTime<Utc>,
    pub last_seen_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Subscription {
    pub id: i64,
    pub customer_id: i64,
    pub product_id: Option<i64>,
    pub store: String,
    pub original_transaction_id: Option<String>,
    pub current_period_start: Option<DateTime<Utc>>,
    pub current_period_end: Option<DateTime<Utc>>,
    pub status: String,
    pub auto_renew: bool,
    pub unsubscribe_detected_at: Option<DateTime<Utc>>,
    pub billing_issues_detected_at: Option<DateTime<Utc>>,
    pub grace_period_expires_at: Option<DateTime<Utc>>,
    pub is_trial: bool,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Transaction {
    pub id: i64,
    pub subscription_id: Option<i64>,
    pub customer_id: i64,
    pub product_id: Option<i64>,
    pub store: String,
    pub store_transaction_id: String,
    pub purchased_at: Option<DateTime<Utc>>,
    pub expires_at: Option<DateTime<Utc>>,
    pub price_micros: Option<i64>,
    pub currency: Option<String>,
    pub is_trial: bool,
    pub is_renewal: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EntitlementInfo {
    pub identifier: String,
    pub is_active: bool,
    pub will_renew: bool,
    pub period_type: String,
    pub latest_purchase_date: Option<DateTime<Utc>>,
    pub expires_date: Option<DateTime<Utc>>,
    pub grace_period_expires_date: Option<DateTime<Utc>>,
    pub store: String,
    pub product_identifier: Option<String>,
}

/// Parsed result from a store-specific receipt validator.
#[derive(Debug, Clone)]
pub struct ValidatedPurchase {
    pub store: String,
    pub store_product_id: String,
    pub store_transaction_id: String,
    pub original_transaction_id: Option<String>,
    pub purchased_at: Option<DateTime<Utc>>,
    pub expires_at: Option<DateTime<Utc>>,
    pub is_trial: bool,
    pub is_renewal: bool,
    pub auto_renew: bool,
    pub raw: Value,
}

/// Lifecycle events dispatched via outbound webhooks.
#[derive(Debug, Clone, Copy)]
pub enum BillingEventType {
    InitialPurchase,
    Renewal,
    Cancellation,
    Uncancellation,
    NonRenewingPurchase,
    Expiration,
    BillingIssue,
    ProductChange,
    Transfer,
    SubscriptionPaused,
    Refund,
    ExperimentImpression,
    ExperimentConversion,
}

impl BillingEventType {
    pub fn as_str(&self) -> &'static str {
        match self {
            BillingEventType::InitialPurchase => "INITIAL_PURCHASE",
            BillingEventType::Renewal => "RENEWAL",
            BillingEventType::Cancellation => "CANCELLATION",
            BillingEventType::Uncancellation => "UNCANCELLATION",
            BillingEventType::NonRenewingPurchase => "NON_RENEWING_PURCHASE",
            BillingEventType::Expiration => "EXPIRATION",
            BillingEventType::BillingIssue => "BILLING_ISSUE",
            BillingEventType::ProductChange => "PRODUCT_CHANGE",
            BillingEventType::Transfer => "TRANSFER",
            BillingEventType::SubscriptionPaused => "SUBSCRIPTION_PAUSED",
            BillingEventType::Refund => "REFUND",
            BillingEventType::ExperimentImpression => "EXPERIMENT_IMPRESSION",
            BillingEventType::ExperimentConversion => "EXPERIMENT_CONVERSION",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Audience {
    pub id: i64,
    pub app_id: i64,
    pub identifier: String,
    pub display_name: Option<String>,
    pub rules: Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Experiment {
    pub id: i64,
    pub app_id: i64,
    pub identifier: String,
    pub status: String,
    pub metric: Option<String>,
    pub audience_id: Option<i64>,
    pub started_at: Option<DateTime<Utc>>,
    pub ended_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Variant {
    pub id: i64,
    pub experiment_id: i64,
    pub identifier: String,
    pub offering_id: i64,
    pub is_control: bool,
    pub traffic_weight: i32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExperimentWithVariants {
    #[serde(flatten)]
    pub experiment: Experiment,
    pub variants: Vec<Variant>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Paywall {
    pub id: i64,
    pub offering_id: i64,
    pub template: Option<String>,
    pub config: Value,
    pub draft_config: Option<Value>,
    pub is_published: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VariantResult {
    pub variant_id: i64,
    pub identifier: String,
    pub offering_id: i64,
    pub is_control: bool,
    pub impressions: i64,
    pub conversions: i64,
    pub conversion_rate: f64,
    pub z_score: Option<f64>,
}
