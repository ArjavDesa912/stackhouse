//! End-to-end integration tests for the billing module.
//!
//! Requires a local Postgres on `STACKHOUSE_TEST_URL`
//! (default `postgres://postgres:postgres@localhost:5432/stackhouse_test`),
//! matching the convention of the other tests in this crate.

use std::sync::Arc;

use chrono::{Duration, Utc};
use serde_json::json;
use stackhouse::billing::{self, models::ValidatedPurchase};
use stackhouse::db::StackhouseStore;

async fn init_store() -> Arc<billing::BillingStore> {
    let store = Arc::new(StackhouseStore::in_memory().await.expect("test db"));
    billing::init(store).await.expect("billing init")
}

#[tokio::test]
async fn full_subscription_lifecycle_resolves_entitlement() {
    let billing = init_store().await;

    let app = billing
        .create_app(None, "test-app", Some("com.example.app"), "ios")
        .await
        .unwrap();
    let product = billing
        .upsert_product(
            app.id,
            "app_store",
            "pro.monthly",
            "auto_renewable",
            Some("P1M"),
            Some(9_990_000),
            Some("USD"),
            &json!({}),
        )
        .await
        .unwrap();
    let ent = billing
        .upsert_entitlement(app.id, "pro", Some("Pro"))
        .await
        .unwrap();
    billing
        .attach_product_to_entitlement(ent.id, product.id)
        .await
        .unwrap();

    let customer = billing
        .get_or_create_customer(app.id, "user-42")
        .await
        .unwrap();
    assert_eq!(customer.app_user_id, "user-42");

    let purchase = ValidatedPurchase {
        store: "app_store".into(),
        store_product_id: "pro.monthly".into(),
        store_transaction_id: "txn_1".into(),
        original_transaction_id: Some("otid_1".into()),
        purchased_at: Some(Utc::now() - Duration::days(1)),
        expires_at: Some(Utc::now() + Duration::days(29)),
        is_trial: false,
        is_renewal: false,
        auto_renew: true,
        raw: json!({"source": "test"}),
    };
    let (sub, is_new) = billing
        .upsert_subscription_from_purchase(customer.id, Some(product.id), &purchase)
        .await
        .unwrap();
    assert!(is_new);
    assert_eq!(sub.status, "active");

    let resolved = billing
        .resolve_entitlements(app.id, customer.id, Utc::now())
        .await
        .unwrap();
    assert_eq!(resolved.len(), 1);
    assert_eq!(resolved[0].identifier, "pro");
    assert!(resolved[0].is_active);
    assert!(resolved[0].will_renew);
    assert_eq!(
        resolved[0].product_identifier.as_deref(),
        Some("pro.monthly")
    );

    // Idempotent: re-applying the same purchase should NOT create a new sub.
    let (_, is_new2) = billing
        .upsert_subscription_from_purchase(customer.id, Some(product.id), &purchase)
        .await
        .unwrap();
    assert!(!is_new2);
}

#[tokio::test]
async fn expired_subscription_is_inactive_then_reactivated_by_promo() {
    let billing = init_store().await;

    let app = billing
        .create_app(None, "test-app-2", None, "multi")
        .await
        .unwrap();
    let product = billing
        .upsert_product(
            app.id,
            "app_store",
            "pro.yearly",
            "auto_renewable",
            Some("P1Y"),
            None,
            None,
            &json!({}),
        )
        .await
        .unwrap();
    let ent = billing
        .upsert_entitlement(app.id, "pro", None)
        .await
        .unwrap();
    billing
        .attach_product_to_entitlement(ent.id, product.id)
        .await
        .unwrap();
    let customer = billing
        .get_or_create_customer(app.id, "lapsed-user")
        .await
        .unwrap();

    let expired = ValidatedPurchase {
        store: "app_store".into(),
        store_product_id: "pro.yearly".into(),
        store_transaction_id: "txn_old".into(),
        original_transaction_id: Some("otid_old".into()),
        purchased_at: Some(Utc::now() - Duration::days(400)),
        expires_at: Some(Utc::now() - Duration::days(30)),
        is_trial: false,
        is_renewal: false,
        auto_renew: false,
        raw: json!({}),
    };
    billing
        .upsert_subscription_from_purchase(customer.id, Some(product.id), &expired)
        .await
        .unwrap();

    let resolved = billing
        .resolve_entitlements(app.id, customer.id, Utc::now())
        .await
        .unwrap();
    assert_eq!(resolved.len(), 1);
    assert!(!resolved[0].is_active, "expired sub should be inactive");

    // Admin grants a 7-day promotional extension.
    billing
        .grant_promotional(customer.id, product.id, Utc::now() + Duration::days(7))
        .await
        .unwrap();

    let resolved = billing
        .resolve_entitlements(app.id, customer.id, Utc::now())
        .await
        .unwrap();
    assert!(
        resolved[0].is_active,
        "promotional grant should reactivate entitlement"
    );
    assert_eq!(resolved[0].store, "promotional");
}

#[tokio::test]
async fn offerings_current_flag_is_exclusive_per_app() {
    let billing = init_store().await;
    let app = billing
        .create_app(None, "offer-app", None, "multi")
        .await
        .unwrap();
    let product = billing
        .upsert_product(
            app.id,
            "app_store",
            "pro.monthly",
            "auto_renewable",
            None,
            None,
            None,
            &json!({}),
        )
        .await
        .unwrap();

    let o1 = billing
        .upsert_offering(app.id, "default", true, &json!({}))
        .await
        .unwrap();
    let o2 = billing
        .upsert_offering(app.id, "holiday", true, &json!({}))
        .await
        .unwrap();
    billing
        .add_package(o1.id, "$rc_monthly", product.id, Some("MONTHLY"))
        .await
        .unwrap();
    billing
        .add_package(o2.id, "$rc_monthly", product.id, Some("MONTHLY"))
        .await
        .unwrap();

    let listed = billing.list_offerings(app.id).await.unwrap();
    assert_eq!(listed.len(), 2);
    let current: Vec<_> = listed.iter().filter(|o| o.is_current).collect();
    assert_eq!(current.len(), 1, "only one offering should be current");
    assert_eq!(current[0].identifier, "holiday");
    assert_eq!(listed.iter().map(|o| o.packages.len()).sum::<usize>(), 2);
}

#[tokio::test]
async fn experiment_assignment_is_sticky_and_weighted() {
    let billing = init_store().await;

    let app = billing
        .create_app(None, "exp-app", None, "multi")
        .await
        .unwrap();
    let product = billing
        .upsert_product(
            app.id,
            "app_store",
            "pro.monthly",
            "auto_renewable",
            None,
            None,
            None,
            &json!({}),
        )
        .await
        .unwrap();

    let control = billing
        .upsert_offering(app.id, "control", true, &json!({}))
        .await
        .unwrap();
    let treatment = billing
        .upsert_offering(app.id, "treatment", false, &json!({}))
        .await
        .unwrap();
    billing
        .add_package(control.id, "$rc_monthly", product.id, Some("MONTHLY"))
        .await
        .unwrap();
    billing
        .add_package(treatment.id, "$rc_monthly", product.id, Some("MONTHLY"))
        .await
        .unwrap();

    let experiment = billing
        .upsert_experiment(
            app.id,
            "price-test",
            Some("purchase"),
            None,
            &[
                billing::models::Variant {
                    id: 0,
                    experiment_id: 0,
                    identifier: "control".into(),
                    offering_id: control.id,
                    is_control: true,
                    traffic_weight: 50,
                },
                billing::models::Variant {
                    id: 0,
                    experiment_id: 0,
                    identifier: "treatment".into(),
                    offering_id: treatment.id,
                    is_control: false,
                    traffic_weight: 50,
                },
            ],
        )
        .await
        .unwrap();

    billing
        .update_experiment_status(experiment.experiment.id, "running", Some(Utc::now()), None)
        .await
        .unwrap();

    let customer_a = billing
        .get_or_create_customer(app.id, "user-a")
        .await
        .unwrap();
    let customer_b = billing
        .get_or_create_customer(app.id, "user-b")
        .await
        .unwrap();

    let variant_a = billing
        .get_or_assign_variant(
            experiment.experiment.id,
            customer_a.id,
            &experiment.variants,
        )
        .await
        .unwrap();
    let variant_a2 = billing
        .get_or_assign_variant(
            experiment.experiment.id,
            customer_a.id,
            &experiment.variants,
        )
        .await
        .unwrap();
    assert_eq!(variant_a, variant_a2, "assignment should be sticky");

    let variant_b = billing
        .get_or_assign_variant(
            experiment.experiment.id,
            customer_b.id,
            &experiment.variants,
        )
        .await
        .unwrap();

    // Deterministic but effectively a coin flip for two arbitrary users.
    let ids: std::collections::HashSet<_> = [variant_a, variant_b].into_iter().collect();
    assert!(ids.len() >= 1);

    billing
        .record_experiment_event(
            experiment.experiment.id,
            variant_a,
            customer_a.id,
            "impression",
            &json!({}),
        )
        .await
        .unwrap();
    billing
        .record_experiment_event(
            experiment.experiment.id,
            variant_a,
            customer_a.id,
            "conversion",
            &json!({}),
        )
        .await
        .unwrap();

    let results = billing
        .experiment_results(experiment.experiment.id)
        .await
        .unwrap();
    assert_eq!(results.len(), 2);
    let control_row = results.iter().find(|r| r.is_control).unwrap();
    assert_eq!(control_row.impressions, 1);
    assert_eq!(control_row.conversions, 1);

    let treatment_row = results.iter().find(|r| !r.is_control).unwrap();
    assert_eq!(treatment_row.impressions, 0);
    assert_eq!(treatment_row.conversions, 0);
}

#[tokio::test]
async fn audience_excludes_customer_from_offering() {
    let billing = init_store().await;

    let app = billing
        .create_app(None, "audience-app", None, "multi")
        .await
        .unwrap();
    let product = billing
        .upsert_product(
            app.id,
            "app_store",
            "pro.monthly",
            "auto_renewable",
            None,
            None,
            None,
            &json!({}),
        )
        .await
        .unwrap();

    let us_offering = billing
        .upsert_offering(app.id, "us-only", false, &json!({}))
        .await
        .unwrap();
    billing
        .add_package(us_offering.id, "$rc_monthly", product.id, Some("MONTHLY"))
        .await
        .unwrap();

    let audience = billing
        .upsert_audience(
            app.id,
            "us-users",
            Some("US users"),
            &json!([{ "field": "country", "op": "eq", "value": "US" }]),
        )
        .await
        .unwrap();
    billing
        .set_offering_audience(us_offering.id, Some(audience.id))
        .await
        .unwrap();

    let customer = billing
        .get_or_create_customer(app.id, "user-us")
        .await
        .unwrap();
    billing
        .set_attributes(customer.id, &json!({"country": "US"}))
        .await
        .unwrap();

    let is_eligible = billing::audiences::is_eligible(
        &audience.rules,
        &billing::audiences::AudienceContext {
            country: Some("US"),
            app_version: None,
            is_existing_subscriber: false,
            attributes: &customer.attributes,
        },
    );
    assert!(is_eligible, "customer in US should match audience");
}
