//! Axum handlers for the billing module.

use async_trait::async_trait;
use axum::{
    body::Bytes,
    extract::{FromRequestParts, Path, Query, State},
    http::{request::Parts, HeaderMap},
    response::IntoResponse,
    Json,
};
use chrono::Utc;
use serde::Deserialize;
use serde_json::{json, Value};
use sqlx::Row;

use crate::auth::{extract_auth_user, AuthUser};
use crate::error::StackhouseError;

use super::audiences::{self, AudienceContext};
use super::models::{
    BillingEventType, Experiment, Variant, STORE_APPLE, STORE_GOOGLE, STORE_STRIPE,
};
use super::state::BillingState;
use super::validators;
use super::webhooks;

// ---------------------------------------------------------------------------
// Extractors
// ---------------------------------------------------------------------------

/// Requires a valid JWT from any authenticated user.
pub struct AuthedUser(pub AuthUser);

#[async_trait]
impl FromRequestParts<BillingState> for AuthedUser {
    type Rejection = StackhouseError;

    async fn from_request_parts(
        parts: &mut Parts,
        state: &BillingState,
    ) -> Result<Self, Self::Rejection> {
        let user = extract_auth_user(&state.auth, &parts.headers)?;
        Ok(Self(user))
    }
}

/// Requires a service-admin JWT.
pub struct AdminAuth(pub AuthUser);

#[async_trait]
impl FromRequestParts<BillingState> for AdminAuth {
    type Rejection = StackhouseError;

    async fn from_request_parts(
        parts: &mut Parts,
        state: &BillingState,
    ) -> Result<Self, Self::Rejection> {
        let auth_user = extract_auth_user(&state.auth, &parts.headers)?;
        let user = state.auth.auth.get_user_by_id(auth_user.id).await?;
        state
            .authorization
            .require_service_admin_unconditional(&user)?;
        Ok(Self(auth_user))
    }
}

// ---------------------------------------------------------------------------
// Request DTOs
// ---------------------------------------------------------------------------

#[derive(Deserialize)]
pub struct CreateAppReq {
    pub name: String,
    #[serde(default)]
    pub project_id: Option<i64>,
    #[serde(default)]
    pub bundle_id: Option<String>,
    #[serde(default = "default_platform")]
    pub platform: String,
}
fn default_platform() -> String {
    "multi".into()
}

#[derive(Deserialize)]
pub struct UpdateSecretsReq {
    #[serde(default)]
    pub apple_shared_secret: Option<String>,
    #[serde(default)]
    pub google_service_account: Option<String>,
    #[serde(default)]
    pub stripe_signing_secret: Option<String>,
    #[serde(default)]
    pub outbound_webhook_secret: Option<String>,
}

#[derive(Deserialize)]
pub struct UpsertProductReq {
    pub app_id: i64,
    pub store: String,
    pub store_product_id: String,
    #[serde(default = "default_product_type")]
    pub product_type: String,
    #[serde(default)]
    pub period: Option<String>,
    #[serde(default)]
    pub price_micros: Option<i64>,
    #[serde(default)]
    pub currency: Option<String>,
    #[serde(default)]
    pub metadata: Option<Value>,
}
fn default_product_type() -> String {
    "auto_renewable".into()
}

#[derive(Deserialize)]
pub struct UpsertEntitlementReq {
    pub app_id: i64,
    pub identifier: String,
    #[serde(default)]
    pub display_name: Option<String>,
    #[serde(default)]
    pub product_ids: Vec<i64>,
}

#[derive(Deserialize)]
pub struct UpsertOfferingReq {
    pub app_id: i64,
    pub identifier: String,
    #[serde(default)]
    pub is_current: bool,
    #[serde(default)]
    pub metadata: Option<Value>,
    #[serde(default)]
    pub packages: Vec<PackageInput>,
}

#[derive(Deserialize)]
pub struct PackageInput {
    pub identifier: String,
    pub product_id: i64,
    #[serde(default)]
    pub package_type: Option<String>,
}

#[derive(Deserialize)]
pub struct AppleReceiptReq {
    pub app_id: i64,
    pub receipt_data: String,
}

#[derive(Deserialize)]
pub struct GoogleReceiptReq {
    pub app_id: i64,
    pub package_name: String,
    pub subscription_id: String,
    pub purchase_token: String,
    #[serde(default)]
    pub access_token: Option<String>,
}

#[derive(Deserialize)]
pub struct StripeLinkReq {
    pub app_id: i64,
    pub event: Value,
}

#[derive(Deserialize)]
pub struct AttributesReq {
    pub app_id: i64,
    pub attributes: Value,
}

#[derive(Deserialize)]
pub struct AliasReq {
    pub app_id: i64,
    pub alias: String,
}

#[derive(Deserialize)]
pub struct GrantPromotionalReq {
    pub app_id: i64,
    pub app_user_id: String,
    pub product_id: i64,
    /// Days from now after which the grant expires.
    pub duration_days: i64,
}

#[derive(Deserialize)]
pub struct AddWebhookEndpointReq {
    pub app_id: i64,
    pub url: String,
    pub secret: String,
    #[serde(default)]
    pub events: Vec<String>,
}

#[derive(Deserialize)]
pub struct OfferingsQuery {
    pub app_id: i64,
}

#[derive(Deserialize)]
pub struct AppIdQuery {
    pub app_id: i64,
}

// ---------------------------------------------------------------------------
// Growth suite DTOs
// ---------------------------------------------------------------------------

#[derive(Deserialize)]
pub struct ResolveOfferingQuery {
    pub app_id: i64,
    #[serde(default)]
    pub country: Option<String>,
    #[serde(default)]
    pub app_version: Option<String>,
}

#[derive(Deserialize)]
pub struct ExperimentEventReq {
    pub app_id: i64,
    #[serde(default)]
    pub metadata: Option<Value>,
}

#[derive(Deserialize)]
pub struct UpsertExperimentReq {
    pub app_id: i64,
    pub identifier: String,
    #[serde(default)]
    pub metric: Option<String>,
    #[serde(default)]
    pub audience_id: Option<i64>,
    #[serde(default)]
    pub variants: Vec<VariantInput>,
}

#[derive(Deserialize)]
pub struct VariantInput {
    pub identifier: String,
    pub offering_id: i64,
    #[serde(default)]
    pub is_control: bool,
    pub traffic_weight: i32,
}

#[derive(Deserialize)]
pub struct UpdateExperimentStatusReq {
    pub status: String,
}

#[derive(Deserialize)]
pub struct UpsertAudienceReq {
    pub app_id: i64,
    pub identifier: String,
    #[serde(default)]
    pub display_name: Option<String>,
    #[serde(default)]
    pub rules: Option<Value>,
}

#[derive(Deserialize)]
pub struct SetOfferingAudienceReq {
    pub audience_id: Option<i64>,
}

#[derive(Deserialize)]
pub struct UpsertPaywallReq {
    pub offering_id: i64,
    #[serde(default)]
    pub template: Option<String>,
    #[serde(default)]
    pub config: Option<Value>,
    #[serde(default)]
    pub draft_config: Option<Value>,
}

#[derive(Deserialize)]
pub struct PaywallQuery {
    pub offering_id: i64,
}

// ---------------------------------------------------------------------------
// Checkout & Subscription Management DTOs
// ---------------------------------------------------------------------------

#[derive(Deserialize)]
pub struct CreateCheckoutReq {
    pub app_id: i64,
    /// Stripe price ID (e.g. `price_1O...`) or product ID mapped to a price.
    pub price_id: String,
    /// The user's ID within the app (used as `client_reference_id`).
    pub app_user_id: String,
    /// Optional email to prefill on the Stripe checkout page.
    #[serde(default)]
    pub customer_email: Option<String>,
    /// URL to redirect to after successful payment.
    pub success_url: String,
    /// URL to redirect to if the user cancels.
    pub cancel_url: String,
}

#[derive(Deserialize)]
pub struct CancelSubscriptionReq {
    pub app_id: i64,
    pub app_user_id: String,
}

// ---------------------------------------------------------------------------
// Health
// ---------------------------------------------------------------------------

pub async fn health_handler() -> impl IntoResponse {
    Json(json!({"success": true, "module": "stackhouse-billing"}))
}

// ---------------------------------------------------------------------------
// Admin: apps
// ---------------------------------------------------------------------------

pub async fn create_app_handler(
    State(state): State<BillingState>,
    AdminAuth(_): AdminAuth,
    Json(req): Json<CreateAppReq>,
) -> Result<impl IntoResponse, StackhouseError> {
    let app = state
        .store
        .create_app(
            req.project_id,
            &req.name,
            req.bundle_id.as_deref(),
            &req.platform,
        )
        .await?;
    Ok(Json(json!({"success": true, "data": app})))
}

pub async fn list_apps_handler(
    State(state): State<BillingState>,
    AdminAuth(_): AdminAuth,
) -> Result<impl IntoResponse, StackhouseError> {
    let apps = state.store.list_apps().await?;
    Ok(Json(json!({"success": true, "data": apps})))
}

pub async fn update_app_secrets_handler(
    State(state): State<BillingState>,
    AdminAuth(_): AdminAuth,
    Path(app_id): Path<i64>,
    Json(req): Json<UpdateSecretsReq>,
) -> Result<impl IntoResponse, StackhouseError> {
    state
        .store
        .update_app_secrets(
            app_id,
            req.apple_shared_secret.as_deref(),
            req.google_service_account.as_deref(),
            req.stripe_signing_secret.as_deref(),
            req.outbound_webhook_secret.as_deref(),
        )
        .await?;
    Ok(Json(json!({"success": true})))
}

// ---------------------------------------------------------------------------
// Admin: products
// ---------------------------------------------------------------------------

pub async fn upsert_product_handler(
    State(state): State<BillingState>,
    AdminAuth(_): AdminAuth,
    Json(req): Json<UpsertProductReq>,
) -> Result<impl IntoResponse, StackhouseError> {
    let meta = req.metadata.unwrap_or_else(|| json!({}));
    let product = state
        .store
        .upsert_product(
            req.app_id,
            &req.store,
            &req.store_product_id,
            &req.product_type,
            req.period.as_deref(),
            req.price_micros,
            req.currency.as_deref(),
            &meta,
        )
        .await?;
    Ok(Json(json!({"success": true, "data": product})))
}

pub async fn list_products_handler(
    State(state): State<BillingState>,
    AdminAuth(_): AdminAuth,
    Query(q): Query<AppIdQuery>,
) -> Result<impl IntoResponse, StackhouseError> {
    let products = state.store.list_products(q.app_id).await?;
    Ok(Json(json!({"success": true, "data": products})))
}

// ---------------------------------------------------------------------------
// Admin: entitlements
// ---------------------------------------------------------------------------

pub async fn upsert_entitlement_handler(
    State(state): State<BillingState>,
    AdminAuth(_): AdminAuth,
    Json(req): Json<UpsertEntitlementReq>,
) -> Result<impl IntoResponse, StackhouseError> {
    let ent = state
        .store
        .upsert_entitlement(req.app_id, &req.identifier, req.display_name.as_deref())
        .await?;
    for pid in req.product_ids {
        state
            .store
            .attach_product_to_entitlement(ent.id, pid)
            .await?;
    }
    Ok(Json(json!({"success": true, "data": ent})))
}

pub async fn list_entitlements_handler(
    State(state): State<BillingState>,
    AdminAuth(_): AdminAuth,
    Query(q): Query<AppIdQuery>,
) -> Result<impl IntoResponse, StackhouseError> {
    let ents = state.store.entitlement_product_map(q.app_id).await?;
    let payload: Vec<Value> = ents
        .into_iter()
        .map(|(e, pids)| json!({"entitlement": e, "product_ids": pids}))
        .collect();
    Ok(Json(json!({"success": true, "data": payload})))
}

// ---------------------------------------------------------------------------
// Admin: offerings
// ---------------------------------------------------------------------------

pub async fn upsert_offering_handler(
    State(state): State<BillingState>,
    AdminAuth(_): AdminAuth,
    Json(req): Json<UpsertOfferingReq>,
) -> Result<impl IntoResponse, StackhouseError> {
    let meta = req.metadata.unwrap_or_else(|| json!({}));
    let offering = state
        .store
        .upsert_offering(req.app_id, &req.identifier, req.is_current, &meta)
        .await?;
    let mut packages = Vec::new();
    for pkg in req.packages {
        let created = state
            .store
            .add_package(
                offering.id,
                &pkg.identifier,
                pkg.product_id,
                pkg.package_type.as_deref(),
            )
            .await?;
        packages.push(created);
    }
    let mut out = offering;
    out.packages = packages;
    Ok(Json(json!({"success": true, "data": out})))
}

pub async fn list_offerings_handler(
    State(state): State<BillingState>,
    Query(q): Query<OfferingsQuery>,
) -> Result<impl IntoResponse, StackhouseError> {
    let offerings = state.store.list_offerings(q.app_id).await?;
    Ok(Json(json!({"success": true, "data": offerings})))
}

// ---------------------------------------------------------------------------
// Admin: webhook endpoints
// ---------------------------------------------------------------------------

pub async fn grant_promotional_handler(
    State(state): State<BillingState>,
    AdminAuth(_): AdminAuth,
    Json(req): Json<GrantPromotionalReq>,
) -> Result<impl IntoResponse, StackhouseError> {
    let customer = state
        .store
        .get_or_create_customer(req.app_id, &req.app_user_id)
        .await?;
    let expires_at = Utc::now() + chrono::Duration::days(req.duration_days);
    let sub = state
        .store
        .grant_promotional(customer.id, req.product_id, expires_at)
        .await?;
    dispatch_lifecycle_event(&state, req.app_id, &sub, true, false).await;
    Ok(Json(json!({"success": true, "data": sub})))
}

pub async fn add_webhook_endpoint_handler(
    State(state): State<BillingState>,
    AdminAuth(_): AdminAuth,
    Json(req): Json<AddWebhookEndpointReq>,
) -> Result<impl IntoResponse, StackhouseError> {
    let events = json!(req.events);
    let id = state
        .store
        .add_webhook_endpoint(req.app_id, &req.url, &req.secret, &events)
        .await?;
    Ok(Json(json!({"success": true, "data": {"id": id}})))
}

// ---------------------------------------------------------------------------
// Customer-facing endpoints (JWT required)
// ---------------------------------------------------------------------------

pub async fn get_customer_handler(
    State(state): State<BillingState>,
    AuthedUser(_): AuthedUser,
    Path(app_user_id): Path<String>,
    Query(q): Query<AppIdQuery>,
) -> Result<impl IntoResponse, StackhouseError> {
    let customer = state
        .store
        .get_or_create_customer(q.app_id, &app_user_id)
        .await?;
    let subs = state.store.list_subscriptions(customer.id).await?;
    let entitlements = state
        .store
        .resolve_entitlements(q.app_id, customer.id, Utc::now())
        .await?;
    Ok(Json(json!({
        "success": true,
        "data": {
            "customer": customer,
            "subscriptions": subs,
            "entitlements": entitlements,
        }
    })))
}

pub async fn get_entitlements_handler(
    State(state): State<BillingState>,
    AuthedUser(_): AuthedUser,
    Path(app_user_id): Path<String>,
    Query(q): Query<AppIdQuery>,
) -> Result<impl IntoResponse, StackhouseError> {
    let customer = state
        .store
        .get_or_create_customer(q.app_id, &app_user_id)
        .await?;
    let entitlements = state
        .store
        .resolve_entitlements(q.app_id, customer.id, Utc::now())
        .await?;
    Ok(Json(json!({"success": true, "data": entitlements})))
}

pub async fn set_attributes_handler(
    State(state): State<BillingState>,
    AuthedUser(_): AuthedUser,
    Path(app_user_id): Path<String>,
    Json(req): Json<AttributesReq>,
) -> Result<impl IntoResponse, StackhouseError> {
    let customer = state
        .store
        .get_or_create_customer(req.app_id, &app_user_id)
        .await?;
    state
        .store
        .set_attributes(customer.id, &req.attributes)
        .await?;
    Ok(Json(json!({"success": true})))
}

pub async fn add_alias_handler(
    State(state): State<BillingState>,
    AuthedUser(_): AuthedUser,
    Path(app_user_id): Path<String>,
    Json(req): Json<AliasReq>,
) -> Result<impl IntoResponse, StackhouseError> {
    let customer = state
        .store
        .get_or_create_customer(req.app_id, &app_user_id)
        .await?;
    state.store.add_alias(customer.id, &req.alias).await?;
    Ok(Json(json!({"success": true})))
}

pub async fn apple_receipt_handler(
    State(state): State<BillingState>,
    AuthedUser(_): AuthedUser,
    Path(app_user_id): Path<String>,
    Json(req): Json<AppleReceiptReq>,
) -> Result<impl IntoResponse, StackhouseError> {
    let (apple_secret, _, _, _) = state.store.get_app_secrets(req.app_id).await?;
    let secret = apple_secret.or_else(|| state.config.default_apple_shared_secret.clone());
    let purchases =
        validators::apple_verify_receipt(&state.http, &req.receipt_data, secret.as_deref()).await?;

    let customer = state
        .store
        .get_or_create_customer(req.app_id, &app_user_id)
        .await?;
    state
        .store
        .record_receipt(customer.id, STORE_APPLE, &req.receipt_data, None)
        .await?;

    let mut applied = 0usize;
    let mut new_count = 0usize;
    for purchase in &purchases {
        let product = state
            .store
            .upsert_product(
                req.app_id,
                STORE_APPLE,
                &purchase.store_product_id,
                "auto_renewable",
                None,
                None,
                None,
                &json!({}),
            )
            .await?;
        let (sub, is_new) = state
            .store
            .upsert_subscription_from_purchase(customer.id, Some(product.id), purchase)
            .await?;
        dispatch_lifecycle_event(&state, req.app_id, &sub, is_new, purchase.is_renewal).await;
        applied += 1;
        if is_new {
            new_count += 1;
        }
    }

    let entitlements = state
        .store
        .resolve_entitlements(req.app_id, customer.id, Utc::now())
        .await?;
    Ok(Json(json!({
        "success": true,
        "applied": applied,
        "new": new_count,
        "data": { "entitlements": entitlements }
    })))
}

pub async fn google_receipt_handler(
    State(state): State<BillingState>,
    AuthedUser(_): AuthedUser,
    Path(app_user_id): Path<String>,
    Json(req): Json<GoogleReceiptReq>,
) -> Result<impl IntoResponse, StackhouseError> {
    let access_token = req
        .access_token
        .or_else(|| state.config.default_google_access_token.clone())
        .ok_or_else(|| {
            StackhouseError::InvalidPayload(
                "Google access_token required (no default configured)".into(),
            )
        })?;

    let purchase = validators::google_verify_subscription(
        &state.http,
        &access_token,
        &req.package_name,
        &req.subscription_id,
        &req.purchase_token,
    )
    .await?;

    let customer = state
        .store
        .get_or_create_customer(req.app_id, &app_user_id)
        .await?;
    state
        .store
        .record_receipt(
            customer.id,
            STORE_GOOGLE,
            &req.purchase_token,
            Some(&purchase.raw),
        )
        .await?;

    let product = state
        .store
        .upsert_product(
            req.app_id,
            STORE_GOOGLE,
            &purchase.store_product_id,
            "auto_renewable",
            None,
            None,
            None,
            &json!({}),
        )
        .await?;
    let (sub, is_new) = state
        .store
        .upsert_subscription_from_purchase(customer.id, Some(product.id), &purchase)
        .await?;
    dispatch_lifecycle_event(&state, req.app_id, &sub, is_new, purchase.is_renewal).await;

    let entitlements = state
        .store
        .resolve_entitlements(req.app_id, customer.id, Utc::now())
        .await?;
    Ok(Json(json!({
        "success": true,
        "data": { "subscription": sub, "entitlements": entitlements }
    })))
}

pub async fn stripe_link_handler(
    State(state): State<BillingState>,
    AuthedUser(_): AuthedUser,
    Path(app_user_id): Path<String>,
    Json(req): Json<StripeLinkReq>,
) -> Result<impl IntoResponse, StackhouseError> {
    let purchase = validators::stripe_event_to_purchase(&req.event)
        .ok_or_else(|| StackhouseError::InvalidPayload("Unsupported stripe event".into()))?;
    let customer = state
        .store
        .get_or_create_customer(req.app_id, &app_user_id)
        .await?;
    let product = state
        .store
        .upsert_product(
            req.app_id,
            STORE_STRIPE,
            &purchase.store_product_id,
            "auto_renewable",
            None,
            None,
            None,
            &json!({}),
        )
        .await?;
    let (sub, is_new) = state
        .store
        .upsert_subscription_from_purchase(customer.id, Some(product.id), &purchase)
        .await?;
    dispatch_lifecycle_event(&state, req.app_id, &sub, is_new, purchase.is_renewal).await;
    Ok(Json(json!({"success": true, "data": sub})))
}

// ---------------------------------------------------------------------------
// Inbound store webhooks (unauthenticated; signature verified)
// ---------------------------------------------------------------------------

#[derive(Deserialize)]
pub struct AppleInboundReq {
    pub app_id: i64,
    #[serde(rename = "signedPayload")]
    pub signed_payload: String,
}

pub async fn apple_inbound_webhook(
    State(_state): State<BillingState>,
    Json(req): Json<AppleInboundReq>,
) -> Result<impl IntoResponse, StackhouseError> {
    let payload = validators::apple_decode_jws_payload(&req.signed_payload)?;
    // Decode nested signedTransactionInfo if present.
    let mut txn_info = None;
    if let Some(data) = payload.get("data") {
        if let Some(s) = data.get("signedTransactionInfo").and_then(|v| v.as_str()) {
            txn_info = validators::apple_decode_jws_payload(s).ok();
        }
    }
    Ok(Json(json!({
        "success": true,
        "app_id": req.app_id,
        "notification": payload,
        "transaction_info": txn_info,
    })))
}

#[derive(Deserialize)]
pub struct GoogleInboundReq {
    pub app_id: i64,
    pub message: Value,
}

pub async fn google_inbound_webhook(
    State(_state): State<BillingState>,
    Json(req): Json<GoogleInboundReq>,
) -> Result<Json<Value>, StackhouseError> {
    // Pub/Sub envelope: message.data is base64-encoded JSON.
    use base64::{engine::general_purpose::STANDARD, Engine as _};
    let data_str = req
        .message
        .get("data")
        .and_then(|v| v.as_str())
        .ok_or_else(|| StackhouseError::InvalidPayload("google pubsub: no data".into()))?;
    let decoded = STANDARD
        .decode(data_str)
        .map_err(|e| StackhouseError::InvalidPayload(format!("google pubsub b64: {e}")))?;
    let notif: Value = serde_json::from_slice(&decoded)?;
    Ok(Json(json!({
        "success": true,
        "app_id": req.app_id,
        "notification": notif
    })))
}

#[derive(Deserialize, Default)]
pub struct StripeWebhookQuery {
    #[serde(default)]
    pub app_id: Option<i64>,
}

pub async fn stripe_inbound_webhook(
    State(state): State<BillingState>,
    Query(q): Query<StripeWebhookQuery>,
    headers: HeaderMap,
    body: Bytes,
) -> Result<impl IntoResponse, StackhouseError> {
    let sig_header = headers
        .get("stripe-signature")
        .and_then(|h| h.to_str().ok())
        .ok_or_else(|| StackhouseError::Unauthorized("Missing Stripe-Signature".into()))?;

    // Parse the event body first so we can extract app_id from metadata if not in query.
    let event: Value = serde_json::from_slice(&body)?;

    // Resolve app_id: query param > event metadata > error.
    let app_id = q
        .app_id
        .or_else(|| {
            event
                .get("data")
                .and_then(|d| d.get("object"))
                .and_then(|o| o.get("metadata"))
                .and_then(|m| m.get("app_id"))
                .and_then(|v| v.as_str())
                .and_then(|s| s.parse::<i64>().ok())
        })
        .ok_or_else(|| {
            StackhouseError::InvalidPayload("app_id required (query param or metadata)".into())
        })?;

    let (_, _, stripe_secret, _) = state.store.get_app_secrets(app_id).await?;
    let secret = stripe_secret
        .or_else(|| state.config.default_stripe_signing_secret.clone())
        .ok_or_else(|| {
            StackhouseError::Unauthorized("No Stripe signing secret configured".into())
        })?;

    // Verify signature after we know we have a valid event.
    validators::stripe_verify_signature(&body, sig_header, &secret, 300, Utc::now().timestamp())?;

    if let Some(purchase) = validators::stripe_event_to_purchase(&event) {
        // Best-effort: try to pull `client_reference_id` or `metadata.app_user_id`.
        let obj = event.get("data").and_then(|d| d.get("object"));
        let app_user_id = obj
            .and_then(|o| {
                o.get("metadata")
                    .and_then(|m| m.get("app_user_id"))
                    .and_then(|v| v.as_str())
                    .map(|s| s.to_string())
                    .or_else(|| {
                        o.get("client_reference_id")
                            .and_then(|v| v.as_str())
                            .map(|s| s.to_string())
                    })
            })
            .unwrap_or_else(|| format!("stripe:{}", purchase.store_transaction_id));
        let customer = state
            .store
            .get_or_create_customer(app_id, &app_user_id)
            .await?;
        let product = state
            .store
            .upsert_product(
                app_id,
                STORE_STRIPE,
                &purchase.store_product_id,
                "auto_renewable",
                None,
                None,
                None,
                &json!({}),
            )
            .await?;
        let (sub, is_new) = state
            .store
            .upsert_subscription_from_purchase(customer.id, Some(product.id), &purchase)
            .await?;
        dispatch_lifecycle_event(&state, app_id, &sub, is_new, purchase.is_renewal).await;
    }
    Ok(Json(json!({"success": true})))
}

// ---------------------------------------------------------------------------
// Checkout & Subscription Management (customer-facing, JWT required)
// ---------------------------------------------------------------------------

pub async fn create_checkout_handler(
    State(_state): State<BillingState>,
    AuthedUser(_): AuthedUser,
    Json(req): Json<CreateCheckoutReq>,
) -> Result<impl IntoResponse, StackhouseError> {
    let stripe_secret_key = std::env::var("STRIPE_SECRET_KEY")
        .or_else(|_| std::env::var("STACKHOUSE_STRIPE_SECRET_KEY"))
        .map_err(|_| {
            StackhouseError::Internal(anyhow::anyhow!(
                "STRIPE_SECRET_KEY not configured on the server"
            ))
        })?;

    let mut form = vec![
        ("mode", "subscription".to_string()),
        ("line_items[0][price]", req.price_id.clone()),
        ("line_items[0][quantity]", "1".to_string()),
        ("client_reference_id", req.app_user_id.clone()),
        ("success_url", req.success_url.clone()),
        ("cancel_url", req.cancel_url.clone()),
    ];

    if let Some(email) = &req.customer_email {
        form.push(("customer_email", email.clone()));
    }

    // Also store app_id in metadata so the webhook can resolve it.
    form.push(("metadata[app_id]", req.app_id.to_string()));
    form.push(("metadata[app_user_id]", req.app_user_id.clone()));

    let client = reqwest::Client::new();
    let resp = client
        .post("https://api.stripe.com/v1/checkout/sessions")
        .bearer_auth(&stripe_secret_key)
        .form(&form)
        .send()
        .await
        .map_err(|e| StackhouseError::Internal(anyhow::anyhow!("Stripe checkout: {e}")))?;

    let status = resp.status();
    let body: Value = resp
        .json()
        .await
        .map_err(|e| StackhouseError::Internal(anyhow::anyhow!("Stripe checkout parse: {e}")))?;

    if !status.is_success() {
        let msg = body
            .get("error")
            .and_then(|e| e.get("message"))
            .and_then(|m| m.as_str())
            .unwrap_or("Stripe checkout failed");
        return Err(StackhouseError::Internal(anyhow::anyhow!("{}", msg)));
    }

    let checkout_url = body.get("url").and_then(|u| u.as_str()).unwrap_or("");
    let session_id = body.get("id").and_then(|i| i.as_str()).unwrap_or("");

    Ok(Json(json!({
        "success": true,
        "data": {
            "checkout_url": checkout_url,
            "session_id": session_id,
        }
    })))
}

pub async fn cancel_subscription_handler(
    State(state): State<BillingState>,
    AuthedUser(_): AuthedUser,
    Json(req): Json<CancelSubscriptionReq>,
) -> Result<impl IntoResponse, StackhouseError> {
    let stripe_secret_key = std::env::var("STRIPE_SECRET_KEY")
        .or_else(|_| std::env::var("STACKHOUSE_STRIPE_SECRET_KEY"))
        .map_err(|_| {
            StackhouseError::Internal(anyhow::anyhow!(
                "STRIPE_SECRET_KEY not configured on the server"
            ))
        })?;

    // Find the customer's active Stripe subscription.
    let customer = state
        .store
        .get_or_create_customer(req.app_id, &req.app_user_id)
        .await?;
    let subs = state.store.list_subscriptions(customer.id).await?;

    let stripe_sub = subs
        .iter()
        .find(|s| s.store == STORE_STRIPE && s.status == "active")
        .ok_or_else(|| StackhouseError::NotFound("No active Stripe subscription found".into()))?;

    let stripe_sub_id = stripe_sub
        .original_transaction_id
        .as_ref()
        .ok_or_else(|| StackhouseError::NotFound("Subscription missing Stripe ID".into()))?;

    // Call Stripe API to cancel at period end.
    let client = reqwest::Client::new();
    let resp = client
        .delete(format!(
            "https://api.stripe.com/v1/subscriptions/{}",
            stripe_sub_id
        ))
        .bearer_auth(&stripe_secret_key)
        .send()
        .await
        .map_err(|e| StackhouseError::Internal(anyhow::anyhow!("Stripe cancel: {e}")))?;

    if !resp.status().is_success() {
        let body: Value = resp.json().await.unwrap_or(json!({}));
        let msg = body
            .get("error")
            .and_then(|e| e.get("message"))
            .and_then(|m| m.as_str())
            .unwrap_or("Stripe cancel failed");
        return Err(StackhouseError::Internal(anyhow::anyhow!("{}", msg)));
    }

    // Update local subscription record.
    state
        .store
        .mark_subscription_cancelled(customer.id, STORE_STRIPE, stripe_sub_id)
        .await?;

    // Dispatch lifecycle event.
    dispatch_lifecycle_event(&state, req.app_id, stripe_sub, false, false).await;

    Ok(Json(json!({"success": true, "data": {"cancelled": true}})))
}

/// List subscription plans (public, no auth required).
pub async fn list_subscription_plans_handler(
    State(state): State<BillingState>,
) -> Result<impl IntoResponse, StackhouseError> {
    let rows = state.store.query_plans().await?;
    Ok(Json(json!({"success": true, "data": rows})))
}

/// Get current user's subscription and entitlements.
pub async fn get_my_subscription_handler(
    State(state): State<BillingState>,
    AuthedUser(user): AuthedUser,
    Query(q): Query<AppIdQuery>,
) -> Result<impl IntoResponse, StackhouseError> {
    let app_user_id = user.id.to_string();
    let customer = state
        .store
        .get_or_create_customer(q.app_id, &app_user_id)
        .await?;
    let subs = state.store.list_subscriptions(customer.id).await?;
    let entitlements = state
        .store
        .resolve_entitlements(q.app_id, customer.id, Utc::now())
        .await?;
    Ok(Json(json!({
        "success": true,
        "data": {
            "customer": customer,
            "subscriptions": subs,
            "entitlements": entitlements,
        }
    })))
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

async fn dispatch_lifecycle_event(
    state: &BillingState,
    app_id: i64,
    sub: &super::models::Subscription,
    is_new: bool,
    is_renewal: bool,
) {
    let event_type = if is_new {
        BillingEventType::InitialPurchase
    } else if is_renewal {
        BillingEventType::Renewal
    } else if !sub.auto_renew {
        BillingEventType::Cancellation
    } else {
        BillingEventType::ProductChange
    };
    if let Err(e) = webhooks::enqueue_event(
        &state.store,
        app_id,
        event_type,
        &json!({"subscription": sub}),
    )
    .await
    {
        tracing::warn!("billing: failed to enqueue webhook: {e}");
    }
}

async fn dispatch_experiment_event(
    state: &BillingState,
    app_id: i64,
    event_type: BillingEventType,
    experiment: &Experiment,
    variant: &Variant,
    customer: &super::models::Customer,
    metadata: &Value,
) {
    let payload = json!({
        "experiment_id": experiment.id,
        "experiment_identifier": experiment.identifier,
        "variant_id": variant.id,
        "variant_identifier": variant.identifier,
        "customer_id": customer.id,
        "app_user_id": customer.app_user_id,
        "metadata": metadata,
    });
    if let Err(e) = webhooks::enqueue_event(&state.store, app_id, event_type, &payload).await {
        tracing::warn!("billing: failed to enqueue experiment webhook: {e}");
    }
}

// ---------------------------------------------------------------------------
// Customer: offering resolution
// ---------------------------------------------------------------------------

pub async fn resolve_offering_handler(
    State(state): State<BillingState>,
    AuthedUser(_): AuthedUser,
    Path(app_user_id): Path<String>,
    Query(q): Query<ResolveOfferingQuery>,
) -> Result<impl IntoResponse, StackhouseError> {
    let customer = state
        .store
        .get_or_create_customer(q.app_id, &app_user_id)
        .await?;

    let is_existing_subscriber = state
        .store
        .customer_has_active_subscription(customer.id)
        .await?;

    let running = state.store.list_running_experiments(q.app_id).await?;

    for ev in running {
        if let Some(audience_id) = ev.experiment.audience_id {
            let audience = state
                .store
                .list_audiences(q.app_id)
                .await?
                .into_iter()
                .find(|a| a.id == audience_id)
                .ok_or_else(|| StackhouseError::NotFound(format!("audience {audience_id}")))?;

            let ctx = AudienceContext {
                country: q.country.as_deref(),
                app_version: q.app_version.as_deref(),
                is_existing_subscriber,
                attributes: &customer.attributes,
            };

            if !audiences::is_eligible(&audience.rules, &ctx) {
                continue;
            }
        }

        let variant_id = state
            .store
            .get_or_assign_variant(ev.experiment.id, customer.id, &ev.variants)
            .await?;

        let variant = ev
            .variants
            .iter()
            .find(|v| v.id == variant_id)
            .ok_or_else(|| StackhouseError::NotFound(format!("variant {variant_id}")))?;

        let offering = state.store.get_offering_by_id(variant.offering_id).await?;
        let paywall = state.store.get_paywall(offering.id).await?;

        return Ok(Json(json!({
            "success": true,
            "data": {
                "offering": offering,
                "paywall": paywall,
                "experiment": {
                    "id": ev.experiment.id,
                    "identifier": ev.experiment.identifier,
                    "variant_id": variant.id,
                    "variant_identifier": variant.identifier,
                }
            }
        })));
    }

    // No running experiment applies — fall back to the app's current offering.
    let offering = state
        .store
        .get_current_offering(q.app_id)
        .await?
        .ok_or_else(|| StackhouseError::NotFound("no current offering configured".into()))?;
    let paywall = state.store.get_paywall(offering.id).await?;

    Ok(Json(json!({
        "success": true,
        "data": {
            "offering": offering,
            "paywall": paywall,
            "experiment": null
        }
    })))
}

pub async fn experiment_impression_handler(
    State(state): State<BillingState>,
    AuthedUser(_): AuthedUser,
    Path(app_user_id): Path<String>,
    Json(req): Json<ExperimentEventReq>,
) -> Result<impl IntoResponse, StackhouseError> {
    let customer = state
        .store
        .get_or_create_customer(req.app_id, &app_user_id)
        .await?;

    let running = state.store.list_running_experiments(req.app_id).await?;
    for ev in running {
        let assignment = sqlx::query(
            r#"SELECT variant_id FROM billing_experiment_assignments
               WHERE experiment_id = $1 AND customer_id = $2"#,
        )
        .bind(ev.experiment.id)
        .bind(customer.id)
        .fetch_optional(state.store.pool())
        .await
        .map_err(|e| StackhouseError::Database(format!("experiment_impression: {e}")))?;

        let Some(row) = assignment else {
            continue;
        };

        let variant_id: i64 = row.get("variant_id");
        let variant = ev
            .variants
            .iter()
            .find(|v| v.id == variant_id)
            .ok_or_else(|| StackhouseError::NotFound(format!("variant {variant_id}")))?;

        state
            .store
            .record_experiment_event(
                ev.experiment.id,
                variant.id,
                customer.id,
                "impression",
                &req.metadata.unwrap_or_else(|| json!({})),
            )
            .await?;

        dispatch_experiment_event(
            &state,
            req.app_id,
            BillingEventType::ExperimentImpression,
            &ev.experiment,
            variant,
            &customer,
            &json!({}),
        )
        .await;

        return Ok(Json(json!({"success": true})));
    }

    Err(StackhouseError::NotFound(
        "no running experiment assignment for this customer".into(),
    ))
}

pub async fn experiment_conversion_handler(
    State(state): State<BillingState>,
    AuthedUser(_): AuthedUser,
    Path(app_user_id): Path<String>,
    Json(req): Json<ExperimentEventReq>,
) -> Result<impl IntoResponse, StackhouseError> {
    let customer = state
        .store
        .get_or_create_customer(req.app_id, &app_user_id)
        .await?;

    let running = state.store.list_running_experiments(req.app_id).await?;
    for ev in running {
        let assignment = sqlx::query(
            r#"SELECT variant_id FROM billing_experiment_assignments
               WHERE experiment_id = $1 AND customer_id = $2"#,
        )
        .bind(ev.experiment.id)
        .bind(customer.id)
        .fetch_optional(state.store.pool())
        .await
        .map_err(|e| StackhouseError::Database(format!("experiment_conversion: {e}")))?;

        let Some(row) = assignment else {
            continue;
        };

        let variant_id: i64 = row.get("variant_id");
        let variant = ev
            .variants
            .iter()
            .find(|v| v.id == variant_id)
            .ok_or_else(|| StackhouseError::NotFound(format!("variant {variant_id}")))?;

        state
            .store
            .record_experiment_event(
                ev.experiment.id,
                variant.id,
                customer.id,
                "conversion",
                &req.metadata.unwrap_or_else(|| json!({})),
            )
            .await?;

        dispatch_experiment_event(
            &state,
            req.app_id,
            BillingEventType::ExperimentConversion,
            &ev.experiment,
            variant,
            &customer,
            &json!({}),
        )
        .await;

        return Ok(Json(json!({"success": true})));
    }

    Err(StackhouseError::NotFound(
        "no running experiment assignment for this customer".into(),
    ))
}

// ---------------------------------------------------------------------------
// Admin: audiences
// ---------------------------------------------------------------------------

pub async fn upsert_audience_handler(
    State(state): State<BillingState>,
    AdminAuth(_): AdminAuth,
    Json(req): Json<UpsertAudienceReq>,
) -> Result<impl IntoResponse, StackhouseError> {
    let rules = req.rules.unwrap_or_else(|| json!([]));
    let audience = state
        .store
        .upsert_audience(
            req.app_id,
            &req.identifier,
            req.display_name.as_deref(),
            &rules,
        )
        .await?;
    Ok(Json(json!({"success": true, "data": audience})))
}

pub async fn list_audiences_handler(
    State(state): State<BillingState>,
    AdminAuth(_): AdminAuth,
    Query(q): Query<AppIdQuery>,
) -> Result<impl IntoResponse, StackhouseError> {
    let audiences = state.store.list_audiences(q.app_id).await?;
    Ok(Json(json!({"success": true, "data": audiences})))
}

// ---------------------------------------------------------------------------
// Admin: experiments
// ---------------------------------------------------------------------------

pub async fn upsert_experiment_handler(
    State(state): State<BillingState>,
    AdminAuth(_): AdminAuth,
    Json(req): Json<UpsertExperimentReq>,
) -> Result<impl IntoResponse, StackhouseError> {
    if req.variants.is_empty() {
        return Err(StackhouseError::InvalidPayload(
            "experiment must have at least one variant".into(),
        ));
    }

    let variants: Vec<Variant> = req
        .variants
        .into_iter()
        .map(|v| Variant {
            id: 0,
            experiment_id: 0,
            identifier: v.identifier,
            offering_id: v.offering_id,
            is_control: v.is_control,
            traffic_weight: v.traffic_weight,
        })
        .collect();

    validators::validate_traffic_weights(&variants)?;

    let experiment = state
        .store
        .upsert_experiment(
            req.app_id,
            &req.identifier,
            req.metric.as_deref(),
            req.audience_id,
            &variants,
        )
        .await?;
    Ok(Json(json!({"success": true, "data": experiment})))
}

pub async fn list_experiments_handler(
    State(state): State<BillingState>,
    AdminAuth(_): AdminAuth,
    Query(q): Query<AppIdQuery>,
) -> Result<impl IntoResponse, StackhouseError> {
    let experiments = state.store.list_experiments(q.app_id).await?;
    Ok(Json(json!({"success": true, "data": experiments})))
}

pub async fn update_experiment_status_handler(
    State(state): State<BillingState>,
    AdminAuth(_): AdminAuth,
    Path(experiment_id): Path<i64>,
    Json(req): Json<UpdateExperimentStatusReq>,
) -> Result<impl IntoResponse, StackhouseError> {
    let current = state.store.get_experiment(experiment_id).await?;

    if req.status == "running" && current.experiment.status != "running" {
        validators::validate_traffic_weights(&current.variants)?;
    }

    let started_at = if req.status == "running" {
        Some(Utc::now())
    } else {
        current.experiment.started_at
    };
    let ended_at = if req.status == "completed" || req.status == "paused" {
        Some(Utc::now())
    } else {
        current.experiment.ended_at
    };

    let experiment = state
        .store
        .update_experiment_status(experiment_id, &req.status, started_at, ended_at)
        .await?;
    Ok(Json(json!({"success": true, "data": experiment})))
}

pub async fn get_experiment_results_handler(
    State(state): State<BillingState>,
    AdminAuth(_): AdminAuth,
    Path(experiment_id): Path<i64>,
) -> Result<impl IntoResponse, StackhouseError> {
    let results = state.store.experiment_results(experiment_id).await?;
    Ok(Json(json!({"success": true, "data": results})))
}

// ---------------------------------------------------------------------------
// Admin: offering audience
// ---------------------------------------------------------------------------

pub async fn set_offering_audience_handler(
    State(state): State<BillingState>,
    AdminAuth(_): AdminAuth,
    Path(offering_id): Path<i64>,
    Json(req): Json<SetOfferingAudienceReq>,
) -> Result<impl IntoResponse, StackhouseError> {
    state
        .store
        .set_offering_audience(offering_id, req.audience_id)
        .await?;
    Ok(Json(json!({"success": true})))
}

// ---------------------------------------------------------------------------
// Admin: paywalls
// ---------------------------------------------------------------------------

pub async fn upsert_paywall_handler(
    State(state): State<BillingState>,
    AdminAuth(_): AdminAuth,
    Json(req): Json<UpsertPaywallReq>,
) -> Result<impl IntoResponse, StackhouseError> {
    let config = req.config.unwrap_or_else(|| json!({}));
    let draft = req.draft_config.as_ref();
    let paywall = state
        .store
        .upsert_paywall(req.offering_id, req.template.as_deref(), &config, draft)
        .await?;
    Ok(Json(json!({"success": true, "data": paywall})))
}

pub async fn get_paywall_handler(
    State(state): State<BillingState>,
    AdminAuth(_): AdminAuth,
    Query(q): Query<PaywallQuery>,
) -> Result<impl IntoResponse, StackhouseError> {
    let paywall = state.store.get_paywall(q.offering_id).await?;
    Ok(Json(json!({"success": true, "data": paywall})))
}

pub async fn publish_paywall_handler(
    State(state): State<BillingState>,
    AdminAuth(_): AdminAuth,
    Path(offering_id): Path<i64>,
) -> Result<impl IntoResponse, StackhouseError> {
    let paywall = state.store.publish_draft_paywall(offering_id).await?;
    Ok(Json(json!({"success": true, "data": paywall})))
}
