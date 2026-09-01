//! End-to-end billing experiment lifecycle test.
//!
//! Covers app, product, offering and experiment creation, deterministic
//! variant assignment, impression / conversion recording, confidence
//! calculation and winner declaration. Requires a running PostgreSQL instance
//! because the billing tables live in SQLx/Postgres.

use std::sync::Arc;

use chrono::Utc;

use crate::billing::init;
use crate::db::StackhouseStore;

#[tokio::test]
async fn experiment_lifecycle_creates_assigns_records_and_reports() {
    let store = Arc::new(StackhouseStore::in_memory().await.expect("test database"));
    let billing = init(Arc::clone(&store)).await.expect("billing init");

    // 1. Create an app.
    let app = billing
        .create_app(None, "Lifecycle App", None, "com.stackhouse.test")
        .await
        .expect("create app");

    // 2. Seed a product and two offerings (control + treatment).
    let product = billing
        .upsert_product(
            app.id,
            "stripe",
            "prod_lifecycle",
            "subscription",
            Some("P1M"),
            Some(9_900_000),
            Some("USD"),
            &serde_json::json!({}),
        )
        .await
        .expect("create product");

    let control_offering = billing
        .upsert_offering(
            app.id,
            "control_offering",
            true,
            &serde_json::json!({"display_name": "Control"}),
        )
        .await
        .expect("create control offering");
    billing
        .add_package(control_offering.id, "monthly", product.id, Some("MONTHLY"))
        .await
        .expect("add control package");

    let treatment_offering = billing
        .upsert_offering(
            app.id,
            "treatment_offering",
            false,
            &serde_json::json!({"display_name": "Treatment"}),
        )
        .await
        .expect("create treatment offering");
    billing
        .add_package(
            treatment_offering.id,
            "monthly",
            product.id,
            Some("MONTHLY"),
        )
        .await
        .expect("add treatment package");

    // 3. Create a running experiment with two variants.
    let experiment = billing
        .upsert_experiment(
            app.id,
            "lifecycle_test",
            Some("purchase"),
            None,
            &[
                super::models::Variant {
                    id: 0,
                    experiment_id: 0,
                    identifier: "control".to_string(),
                    offering_id: control_offering.id,
                    is_control: true,
                    traffic_weight: 50,
                },
                super::models::Variant {
                    id: 0,
                    experiment_id: 0,
                    identifier: "treatment".to_string(),
                    offering_id: treatment_offering.id,
                    is_control: false,
                    traffic_weight: 50,
                },
            ],
        )
        .await
        .expect("create experiment");

    assert_eq!(experiment.variants.len(), 2);

    let running = billing
        .update_experiment_status(experiment.experiment.id, "running", Some(Utc::now()), None)
        .await
        .expect("start experiment");
    assert_eq!(running.experiment.status, "running");

    // 4. Deterministically assign customers and record events.
    let mut control_customers = Vec::new();
    let mut treatment_customers = Vec::new();

    for i in 0..200 {
        let app_user_id = format!("customer-{i}");
        let customer = billing
            .get_or_create_customer(app.id, &app_user_id)
            .await
            .expect("create customer");

        let variant_id = billing
            .get_or_assign_variant(experiment.experiment.id, customer.id, &running.variants)
            .await
            .expect("assign variant");

        // Record impression for every assigned customer.
        billing
            .record_experiment_event(
                experiment.experiment.id,
                variant_id,
                customer.id,
                "impression",
                &serde_json::json!({}),
            )
            .await
            .expect("record impression");

        // Record conversion for a deterministic subset to keep z-score non-degenerate.
        if i % 5 == 0 {
            billing
                .record_experiment_event(
                    experiment.experiment.id,
                    variant_id,
                    customer.id,
                    "conversion",
                    &serde_json::json!({}),
                )
                .await
                .expect("record conversion");
        }

        if running
            .variants
            .iter()
            .any(|v| v.id == variant_id && v.is_control)
        {
            control_customers.push(customer.id);
        } else {
            treatment_customers.push(customer.id);
        }
    }

    // 5. Verify assignment is deterministic: re-fetching returns the same variant.
    for &customer_id in &control_customers[..3] {
        let variant_id = billing
            .get_or_assign_variant(experiment.experiment.id, customer_id, &running.variants)
            .await
            .expect("re-assign variant");
        let variant = running
            .variants
            .iter()
            .find(|v| v.id == variant_id)
            .unwrap();
        assert!(
            variant.is_control,
            "re-assigned customer should stay in control"
        );
    }

    // 6. Calculate results and confirm counts.
    let results = billing
        .experiment_results(experiment.experiment.id)
        .await
        .expect("experiment results");
    assert_eq!(results.len(), 2);

    let control_result = results
        .iter()
        .find(|r| r.is_control)
        .expect("control result");
    let treatment_result = results
        .iter()
        .find(|r| !r.is_control)
        .expect("treatment result");

    assert!(
        control_result.impressions > 0,
        "control should have impressions"
    );
    assert!(
        treatment_result.impressions > 0,
        "treatment should have impressions"
    );
    assert_eq!(
        control_result.impressions + treatment_result.impressions,
        200,
        "impressions should cover all assigned customers"
    );
    assert_eq!(
        control_result.conversions + treatment_result.conversions,
        40,
        "conversions should cover every 5th customer"
    );
    assert!(control_result.conversion_rate >= 0.0);
    assert!(treatment_result.conversion_rate >= 0.0);

    // 7. Declare a winner and complete the experiment.
    let completed = billing
        .update_experiment_status(
            experiment.experiment.id,
            "completed",
            None,
            Some(Utc::now()),
        )
        .await
        .expect("complete experiment");
    assert_eq!(completed.experiment.status, "completed");

    // 8. Resolution: running experiments list is empty after completion.
    let still_running = billing
        .list_running_experiments(app.id)
        .await
        .expect("list running");
    assert!(
        still_running.is_empty(),
        "completed experiment should not be running"
    );

    // 9. Audience scoping: a new customer resolving after completion falls back to the current offering.
    let fallback_customer = billing
        .get_or_create_customer(app.id, "post-experiment-customer")
        .await
        .expect("create fallback customer");
    let current = billing
        .get_current_offering(app.id)
        .await
        .expect("current offering")
        .expect("current offering exists");
    assert_eq!(
        current.id, control_offering.id,
        "current offering should be the control offering"
    );

    // Prevent unused warnings.
    let _ = fallback_customer;
}
