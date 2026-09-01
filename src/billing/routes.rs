//! Axum router for `/v1/billing`.

use axum::{
    routing::{get, post},
    Router,
};

use super::handlers::*;
use super::state::BillingState;

pub fn create_billing_router(state: BillingState) -> Router {
    Router::new()
        .route("/health", get(health_handler))
        // Customer-facing
        .route("/customers/:app_user_id", get(get_customer_handler))
        .route(
            "/customers/:app_user_id/entitlements",
            get(get_entitlements_handler),
        )
        .route(
            "/customers/:app_user_id/attributes",
            post(set_attributes_handler),
        )
        .route("/customers/:app_user_id/alias", post(add_alias_handler))
        .route(
            "/customers/:app_user_id/receipts/apple",
            post(apple_receipt_handler),
        )
        .route(
            "/customers/:app_user_id/receipts/google",
            post(google_receipt_handler),
        )
        .route(
            "/customers/:app_user_id/receipts/stripe",
            post(stripe_link_handler),
        )
        .route(
            "/customers/:app_user_id/offerings/resolve",
            get(resolve_offering_handler),
        )
        .route(
            "/customers/:app_user_id/experiments/impression",
            post(experiment_impression_handler),
        )
        .route(
            "/customers/:app_user_id/experiments/conversion",
            post(experiment_conversion_handler),
        )
        // Checkout & subscription management (JWT required)
        .route("/checkout", post(create_checkout_handler))
        .route("/cancel", post(cancel_subscription_handler))
        .route("/me", get(get_my_subscription_handler))
        // Public offerings (no auth required so paywalls can load)
        .route("/offerings", get(list_offerings_handler))
        .route("/plans", get(list_subscription_plans_handler))
        // Inbound store webhooks (signature-verified, no JWT)
        .route("/webhooks/apple", post(apple_inbound_webhook))
        .route("/webhooks/google", post(google_inbound_webhook))
        .route("/webhooks/stripe", post(stripe_inbound_webhook))
        // Admin
        .route(
            "/admin/apps",
            post(create_app_handler).get(list_apps_handler),
        )
        .route(
            "/admin/apps/:app_id/secrets",
            post(update_app_secrets_handler),
        )
        .route(
            "/admin/products",
            post(upsert_product_handler).get(list_products_handler),
        )
        .route(
            "/admin/entitlements",
            post(upsert_entitlement_handler).get(list_entitlements_handler),
        )
        .route("/admin/offerings", post(upsert_offering_handler))
        .route(
            "/admin/offerings/:offering_id/audience",
            post(set_offering_audience_handler),
        )
        .route(
            "/admin/experiments",
            post(upsert_experiment_handler).get(list_experiments_handler),
        )
        .route(
            "/admin/experiments/:id/status",
            post(update_experiment_status_handler),
        )
        .route(
            "/admin/experiments/:id/results",
            get(get_experiment_results_handler),
        )
        .route(
            "/admin/audiences",
            post(upsert_audience_handler).get(list_audiences_handler),
        )
        .route(
            "/admin/paywalls",
            post(upsert_paywall_handler).get(get_paywall_handler),
        )
        .route(
            "/admin/paywalls/:offering_id/publish",
            post(publish_paywall_handler),
        )
        .route("/admin/grant", post(grant_promotional_handler))
        .route(
            "/admin/webhook-endpoints",
            post(add_webhook_endpoint_handler),
        )
        .with_state(state)
}
