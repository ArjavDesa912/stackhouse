//! Persistence layer for billing entities.
//!
//! Uses `sqlx` directly against the shared `StackhouseStore` pool.

use chrono::{DateTime, Utc};
use serde_json::{json, Value};
use sqlx::Row;
use std::collections::HashMap;
use std::sync::Arc;

use crate::db::StackhouseStore;
use crate::error::{StackhouseError, StackhouseResult};

use super::models::{
    App, Audience, Customer, Entitlement, EntitlementInfo, Experiment, ExperimentWithVariants,
    Offering, Package, Paywall, Product, Subscription, ValidatedPurchase, Variant, VariantResult,
};

#[derive(Clone)]
pub struct BillingStore {
    inner: Arc<StackhouseStore>,
}

impl BillingStore {
    pub fn new(store: Arc<StackhouseStore>) -> Self {
        Self { inner: store }
    }

    pub fn pool(&self) -> &sqlx::PgPool {
        self.inner.pool()
    }

    // ------------------------------------------------------------------
    // Apps
    // ------------------------------------------------------------------
    pub async fn create_app(
        &self,
        project_id: Option<i64>,
        name: &str,
        bundle_id: Option<&str>,
        platform: &str,
    ) -> StackhouseResult<App> {
        let row = sqlx::query(
            r#"INSERT INTO billing_apps (project_id, name, bundle_id, platform)
               VALUES ($1, $2, $3, $4)
               RETURNING id, project_id, name, bundle_id, platform, created_at"#,
        )
        .bind(project_id)
        .bind(name)
        .bind(bundle_id)
        .bind(platform)
        .fetch_one(self.pool())
        .await
        .map_err(|e| StackhouseError::Database(format!("create_app: {e}")))?;

        Ok(App {
            id: row.try_get("id")?,
            project_id: row.try_get("project_id").ok(),
            name: row.try_get("name")?,
            bundle_id: row.try_get("bundle_id").ok(),
            platform: row.try_get("platform")?,
            created_at: row.try_get("created_at")?,
        })
    }

    pub async fn list_apps(&self) -> StackhouseResult<Vec<App>> {
        let rows = sqlx::query(
            r#"SELECT id, project_id, name, bundle_id, platform, created_at
               FROM billing_apps ORDER BY id"#,
        )
        .fetch_all(self.pool())
        .await
        .map_err(|e| StackhouseError::Database(format!("list_apps: {e}")))?;

        Ok(rows
            .into_iter()
            .map(|row| App {
                id: row.get("id"),
                project_id: row.try_get("project_id").ok(),
                name: row.get("name"),
                bundle_id: row.try_get("bundle_id").ok(),
                platform: row.get("platform"),
                created_at: row.get("created_at"),
            })
            .collect())
    }

    pub async fn get_app(&self, app_id: i64) -> StackhouseResult<App> {
        let row = sqlx::query(
            r#"SELECT id, project_id, name, bundle_id, platform, created_at
               FROM billing_apps WHERE id = $1"#,
        )
        .bind(app_id)
        .fetch_optional(self.pool())
        .await
        .map_err(|e| StackhouseError::Database(format!("get_app: {e}")))?
        .ok_or_else(|| StackhouseError::NotFound(format!("billing_app {app_id}")))?;

        Ok(App {
            id: row.get("id"),
            project_id: row.try_get("project_id").ok(),
            name: row.get("name"),
            bundle_id: row.try_get("bundle_id").ok(),
            platform: row.get("platform"),
            created_at: row.get("created_at"),
        })
    }

    pub async fn get_app_secrets(
        &self,
        app_id: i64,
    ) -> StackhouseResult<(
        Option<String>,
        Option<String>,
        Option<String>,
        Option<String>,
    )> {
        let row = sqlx::query(
            r#"SELECT apple_shared_secret, google_service_account, stripe_signing_secret,
                      outbound_webhook_secret
               FROM billing_apps WHERE id = $1"#,
        )
        .bind(app_id)
        .fetch_optional(self.pool())
        .await
        .map_err(|e| StackhouseError::Database(format!("get_app_secrets: {e}")))?
        .ok_or_else(|| StackhouseError::NotFound(format!("billing_app {app_id}")))?;

        Ok((
            row.try_get("apple_shared_secret").ok(),
            row.try_get("google_service_account").ok(),
            row.try_get("stripe_signing_secret").ok(),
            row.try_get("outbound_webhook_secret").ok(),
        ))
    }

    pub async fn update_app_secrets(
        &self,
        app_id: i64,
        apple: Option<&str>,
        google: Option<&str>,
        stripe: Option<&str>,
        webhook: Option<&str>,
    ) -> StackhouseResult<()> {
        sqlx::query(
            r#"UPDATE billing_apps SET
                 apple_shared_secret = COALESCE($2, apple_shared_secret),
                 google_service_account = COALESCE($3, google_service_account),
                 stripe_signing_secret = COALESCE($4, stripe_signing_secret),
                 outbound_webhook_secret = COALESCE($5, outbound_webhook_secret)
               WHERE id = $1"#,
        )
        .bind(app_id)
        .bind(apple)
        .bind(google)
        .bind(stripe)
        .bind(webhook)
        .execute(self.pool())
        .await
        .map_err(|e| StackhouseError::Database(format!("update_app_secrets: {e}")))?;
        Ok(())
    }

    // ------------------------------------------------------------------
    // Products
    // ------------------------------------------------------------------
    pub async fn upsert_product(
        &self,
        app_id: i64,
        store: &str,
        store_product_id: &str,
        product_type: &str,
        period: Option<&str>,
        price_micros: Option<i64>,
        currency: Option<&str>,
        metadata: &Value,
    ) -> StackhouseResult<Product> {
        let row = sqlx::query(
            r#"INSERT INTO billing_products
                 (app_id, store, store_product_id, product_type, period, price_micros, currency, metadata)
               VALUES ($1,$2,$3,$4,$5,$6,$7,$8)
               ON CONFLICT (app_id, store, store_product_id) DO UPDATE SET
                 product_type = EXCLUDED.product_type,
                 period = EXCLUDED.period,
                 price_micros = EXCLUDED.price_micros,
                 currency = EXCLUDED.currency,
                 metadata = EXCLUDED.metadata
               RETURNING id, app_id, store, store_product_id, product_type, period,
                         price_micros, currency, metadata"#,
        )
        .bind(app_id)
        .bind(store)
        .bind(store_product_id)
        .bind(product_type)
        .bind(period)
        .bind(price_micros)
        .bind(currency)
        .bind(metadata)
        .fetch_one(self.pool())
        .await
        .map_err(|e| StackhouseError::Database(format!("upsert_product: {e}")))?;

        Ok(product_from_row(&row))
    }

    pub async fn find_product(
        &self,
        app_id: i64,
        store: &str,
        store_product_id: &str,
    ) -> StackhouseResult<Option<Product>> {
        let row = sqlx::query(
            r#"SELECT id, app_id, store, store_product_id, product_type, period,
                      price_micros, currency, metadata
               FROM billing_products
               WHERE app_id = $1 AND store = $2 AND store_product_id = $3"#,
        )
        .bind(app_id)
        .bind(store)
        .bind(store_product_id)
        .fetch_optional(self.pool())
        .await
        .map_err(|e| StackhouseError::Database(format!("find_product: {e}")))?;

        Ok(row.map(|r| product_from_row(&r)))
    }

    pub async fn list_products(&self, app_id: i64) -> StackhouseResult<Vec<Product>> {
        let rows = sqlx::query(
            r#"SELECT id, app_id, store, store_product_id, product_type, period,
                      price_micros, currency, metadata
               FROM billing_products WHERE app_id = $1 ORDER BY id"#,
        )
        .bind(app_id)
        .fetch_all(self.pool())
        .await
        .map_err(|e| StackhouseError::Database(format!("list_products: {e}")))?;
        Ok(rows.iter().map(product_from_row).collect())
    }

    // ------------------------------------------------------------------
    // Entitlements
    // ------------------------------------------------------------------
    pub async fn upsert_entitlement(
        &self,
        app_id: i64,
        identifier: &str,
        display_name: Option<&str>,
    ) -> StackhouseResult<Entitlement> {
        let row = sqlx::query(
            r#"INSERT INTO billing_entitlements (app_id, identifier, display_name)
               VALUES ($1,$2,$3)
               ON CONFLICT (app_id, identifier) DO UPDATE SET
                 display_name = COALESCE(EXCLUDED.display_name, billing_entitlements.display_name)
               RETURNING id, app_id, identifier, display_name"#,
        )
        .bind(app_id)
        .bind(identifier)
        .bind(display_name)
        .fetch_one(self.pool())
        .await
        .map_err(|e| StackhouseError::Database(format!("upsert_entitlement: {e}")))?;

        Ok(Entitlement {
            id: row.get("id"),
            app_id: row.get("app_id"),
            identifier: row.get("identifier"),
            display_name: row.try_get("display_name").ok(),
        })
    }

    pub async fn list_entitlements(&self, app_id: i64) -> StackhouseResult<Vec<Entitlement>> {
        let rows = sqlx::query(
            r#"SELECT id, app_id, identifier, display_name FROM billing_entitlements
               WHERE app_id = $1 ORDER BY id"#,
        )
        .bind(app_id)
        .fetch_all(self.pool())
        .await
        .map_err(|e| StackhouseError::Database(format!("list_entitlements: {e}")))?;

        Ok(rows
            .into_iter()
            .map(|r| Entitlement {
                id: r.get("id"),
                app_id: r.get("app_id"),
                identifier: r.get("identifier"),
                display_name: r.try_get("display_name").ok(),
            })
            .collect())
    }

    pub async fn attach_product_to_entitlement(
        &self,
        entitlement_id: i64,
        product_id: i64,
    ) -> StackhouseResult<()> {
        sqlx::query(
            r#"INSERT INTO billing_entitlement_products (entitlement_id, product_id)
               VALUES ($1,$2) ON CONFLICT DO NOTHING"#,
        )
        .bind(entitlement_id)
        .bind(product_id)
        .execute(self.pool())
        .await
        .map_err(|e| StackhouseError::Database(format!("attach_product_to_entitlement: {e}")))?;
        Ok(())
    }

    /// Returns entitlements keyed by their id for the given app along with their product ids.
    pub async fn entitlement_product_map(
        &self,
        app_id: i64,
    ) -> StackhouseResult<Vec<(Entitlement, Vec<i64>)>> {
        let entitlements = self.list_entitlements(app_id).await?;
        let rows = sqlx::query(
            r#"SELECT ep.entitlement_id, ep.product_id
               FROM billing_entitlement_products ep
               JOIN billing_entitlements e ON e.id = ep.entitlement_id
               WHERE e.app_id = $1"#,
        )
        .bind(app_id)
        .fetch_all(self.pool())
        .await
        .map_err(|e| StackhouseError::Database(format!("entitlement_product_map: {e}")))?;

        let mut out: Vec<(Entitlement, Vec<i64>)> =
            entitlements.into_iter().map(|e| (e, Vec::new())).collect();
        for r in rows {
            let eid: i64 = r.get("entitlement_id");
            let pid: i64 = r.get("product_id");
            if let Some((_, v)) = out.iter_mut().find(|(e, _)| e.id == eid) {
                v.push(pid);
            }
        }
        Ok(out)
    }

    // ------------------------------------------------------------------
    // Offerings / packages
    // ------------------------------------------------------------------
    pub async fn upsert_offering(
        &self,
        app_id: i64,
        identifier: &str,
        is_current: bool,
        metadata: &Value,
    ) -> StackhouseResult<Offering> {
        if is_current {
            sqlx::query("UPDATE billing_offerings SET is_current = FALSE WHERE app_id = $1")
                .bind(app_id)
                .execute(self.pool())
                .await
                .map_err(|e| StackhouseError::Database(e.to_string()))?;
        }
        let row = sqlx::query(
            r#"INSERT INTO billing_offerings (app_id, identifier, is_current, metadata)
               VALUES ($1,$2,$3,$4)
               ON CONFLICT (app_id, identifier) DO UPDATE SET
                 is_current = EXCLUDED.is_current,
                 metadata = EXCLUDED.metadata
               RETURNING id, app_id, identifier, is_current, metadata"#,
        )
        .bind(app_id)
        .bind(identifier)
        .bind(is_current)
        .bind(metadata)
        .fetch_one(self.pool())
        .await
        .map_err(|e| StackhouseError::Database(format!("upsert_offering: {e}")))?;

        Ok(Offering {
            id: row.get("id"),
            app_id: row.get("app_id"),
            identifier: row.get("identifier"),
            is_current: row.get("is_current"),
            metadata: row.get("metadata"),
            audience_id: None,
            packages: vec![],
        })
    }

    pub async fn add_package(
        &self,
        offering_id: i64,
        identifier: &str,
        product_id: i64,
        package_type: Option<&str>,
    ) -> StackhouseResult<Package> {
        let row = sqlx::query(
            r#"INSERT INTO billing_packages (offering_id, identifier, product_id, package_type)
               VALUES ($1,$2,$3,$4)
               ON CONFLICT (offering_id, identifier) DO UPDATE SET
                 product_id = EXCLUDED.product_id,
                 package_type = EXCLUDED.package_type
               RETURNING id, offering_id, identifier, product_id, package_type"#,
        )
        .bind(offering_id)
        .bind(identifier)
        .bind(product_id)
        .bind(package_type)
        .fetch_one(self.pool())
        .await
        .map_err(|e| StackhouseError::Database(format!("add_package: {e}")))?;

        Ok(Package {
            id: row.get("id"),
            offering_id: row.get("offering_id"),
            identifier: row.get("identifier"),
            product_id: row.get("product_id"),
            package_type: row.try_get("package_type").ok(),
        })
    }

    pub async fn list_offerings(&self, app_id: i64) -> StackhouseResult<Vec<Offering>> {
        let rows = sqlx::query(
            r#"SELECT id, app_id, identifier, is_current, metadata, audience_id
               FROM billing_offerings WHERE app_id = $1 ORDER BY id"#,
        )
        .bind(app_id)
        .fetch_all(self.pool())
        .await
        .map_err(|e| StackhouseError::Database(e.to_string()))?;

        let offerings: Vec<Offering> = rows.iter().map(offering_from_row).collect();

        let mut packages = packages_for_offering_ids(
            self.pool(),
            &offerings.iter().map(|o| o.id).collect::<Vec<_>>(),
        )
        .await?;

        let mut result = offerings;
        for offering in result.iter_mut() {
            offering.packages = packages.remove(&offering.id).unwrap_or_default();
        }
        Ok(result)
    }

    pub async fn get_offering_by_id(&self, offering_id: i64) -> StackhouseResult<Offering> {
        let row = sqlx::query(
            r#"SELECT id, app_id, identifier, is_current, metadata, audience_id
               FROM billing_offerings WHERE id = $1"#,
        )
        .bind(offering_id)
        .fetch_optional(self.pool())
        .await
        .map_err(|e| StackhouseError::Database(format!("get_offering_by_id: {e}")))?
        .ok_or_else(|| StackhouseError::NotFound(format!("offering {offering_id}")))?;

        let mut offering = offering_from_row(&row);
        let mut packages = packages_for_offering_ids(self.pool(), &[offering.id]).await?;
        offering.packages = packages.remove(&offering.id).unwrap_or_default();
        Ok(offering)
    }

    pub async fn get_current_offering(&self, app_id: i64) -> StackhouseResult<Option<Offering>> {
        let row = sqlx::query(
            r#"SELECT id, app_id, identifier, is_current, metadata, audience_id
               FROM billing_offerings WHERE app_id = $1 AND is_current = TRUE"#,
        )
        .bind(app_id)
        .fetch_optional(self.pool())
        .await
        .map_err(|e| StackhouseError::Database(format!("get_current_offering: {e}")))?;

        let Some(row) = row else {
            return Ok(None);
        };
        let mut offering = offering_from_row(&row);
        let mut packages = packages_for_offering_ids(self.pool(), &[offering.id]).await?;
        offering.packages = packages.remove(&offering.id).unwrap_or_default();
        Ok(Some(offering))
    }

    // ------------------------------------------------------------------
    // Customers
    // ------------------------------------------------------------------
    pub async fn get_or_create_customer(
        &self,
        app_id: i64,
        app_user_id: &str,
    ) -> StackhouseResult<Customer> {
        let row = sqlx::query(
            r#"INSERT INTO billing_customers (app_id, app_user_id)
               VALUES ($1,$2)
               ON CONFLICT (app_id, app_user_id) DO UPDATE SET
                 last_seen_at = NOW()
               RETURNING id, app_id, app_user_id, aliases, attributes, first_seen_at, last_seen_at"#,
        )
        .bind(app_id)
        .bind(app_user_id)
        .fetch_one(self.pool())
        .await
        .map_err(|e| StackhouseError::Database(format!("get_or_create_customer: {e}")))?;

        Ok(customer_from_row(&row))
    }

    pub async fn set_attributes(
        &self,
        customer_id: i64,
        attributes: &Value,
    ) -> StackhouseResult<()> {
        sqlx::query(
            r#"UPDATE billing_customers
               SET attributes = billing_customers.attributes || $2::jsonb,
                   last_seen_at = NOW()
               WHERE id = $1"#,
        )
        .bind(customer_id)
        .bind(attributes)
        .execute(self.pool())
        .await
        .map_err(|e| StackhouseError::Database(format!("set_attributes: {e}")))?;
        Ok(())
    }

    pub async fn add_alias(&self, customer_id: i64, alias: &str) -> StackhouseResult<()> {
        sqlx::query(
            r#"UPDATE billing_customers
               SET aliases = (
                 CASE WHEN aliases @> to_jsonb($2::text)
                      THEN aliases
                      ELSE aliases || to_jsonb($2::text) END
               )
               WHERE id = $1"#,
        )
        .bind(customer_id)
        .bind(alias)
        .execute(self.pool())
        .await
        .map_err(|e| StackhouseError::Database(format!("add_alias: {e}")))?;
        Ok(())
    }

    // ------------------------------------------------------------------
    // Subscriptions & transactions
    // ------------------------------------------------------------------
    pub async fn upsert_subscription_from_purchase(
        &self,
        customer_id: i64,
        product_id: Option<i64>,
        purchase: &ValidatedPurchase,
    ) -> StackhouseResult<(Subscription, bool)> {
        // Try find existing by (customer, store, original_transaction_id).
        let otid = purchase
            .original_transaction_id
            .clone()
            .unwrap_or_else(|| purchase.store_transaction_id.clone());

        let existing = sqlx::query(
            r#"SELECT id FROM billing_subscriptions
               WHERE customer_id = $1 AND store = $2 AND original_transaction_id = $3"#,
        )
        .bind(customer_id)
        .bind(&purchase.store)
        .bind(&otid)
        .fetch_optional(self.pool())
        .await
        .map_err(|e| StackhouseError::Database(e.to_string()))?;

        let now: DateTime<Utc> = Utc::now();
        let status = if purchase.expires_at.map(|e| e > now).unwrap_or(true) {
            "active"
        } else {
            "expired"
        };

        let (sub_row, is_new) = if let Some(row) = existing {
            let id: i64 = row.get("id");
            let updated = sqlx::query(
                r#"UPDATE billing_subscriptions SET
                     product_id = COALESCE($2, product_id),
                     current_period_start = COALESCE($3, current_period_start),
                     current_period_end = COALESCE($4, current_period_end),
                     status = $5,
                     auto_renew = $6,
                     is_trial = $7,
                     updated_at = NOW()
                   WHERE id = $1
                   RETURNING id, customer_id, product_id, store, original_transaction_id,
                             current_period_start, current_period_end, status, auto_renew,
                             unsubscribe_detected_at, billing_issues_detected_at,
                             grace_period_expires_at, is_trial, updated_at"#,
            )
            .bind(id)
            .bind(product_id)
            .bind(purchase.purchased_at)
            .bind(purchase.expires_at)
            .bind(status)
            .bind(purchase.auto_renew)
            .bind(purchase.is_trial)
            .fetch_one(self.pool())
            .await
            .map_err(|e| StackhouseError::Database(e.to_string()))?;
            (updated, false)
        } else {
            let inserted = sqlx::query(
                r#"INSERT INTO billing_subscriptions
                     (customer_id, product_id, store, original_transaction_id,
                      current_period_start, current_period_end, status, auto_renew, is_trial)
                   VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9)
                   RETURNING id, customer_id, product_id, store, original_transaction_id,
                             current_period_start, current_period_end, status, auto_renew,
                             unsubscribe_detected_at, billing_issues_detected_at,
                             grace_period_expires_at, is_trial, updated_at"#,
            )
            .bind(customer_id)
            .bind(product_id)
            .bind(&purchase.store)
            .bind(&otid)
            .bind(purchase.purchased_at)
            .bind(purchase.expires_at)
            .bind(status)
            .bind(purchase.auto_renew)
            .bind(purchase.is_trial)
            .fetch_one(self.pool())
            .await
            .map_err(|e| StackhouseError::Database(e.to_string()))?;
            (inserted, true)
        };

        let sub = subscription_from_row(&sub_row);

        // Record transaction (idempotent on store_transaction_id).
        sqlx::query(
            r#"INSERT INTO billing_transactions
                 (subscription_id, customer_id, product_id, store, store_transaction_id,
                  purchased_at, expires_at, is_trial, is_renewal, raw)
               VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10)
               ON CONFLICT (store, store_transaction_id) DO NOTHING"#,
        )
        .bind(sub.id)
        .bind(customer_id)
        .bind(product_id)
        .bind(&purchase.store)
        .bind(&purchase.store_transaction_id)
        .bind(purchase.purchased_at)
        .bind(purchase.expires_at)
        .bind(purchase.is_trial)
        .bind(purchase.is_renewal)
        .bind(&purchase.raw)
        .execute(self.pool())
        .await
        .map_err(|e| StackhouseError::Database(e.to_string()))?;

        Ok((sub, is_new))
    }

    pub async fn list_subscriptions(
        &self,
        customer_id: i64,
    ) -> StackhouseResult<Vec<Subscription>> {
        let rows = sqlx::query(
            r#"SELECT id, customer_id, product_id, store, original_transaction_id,
                      current_period_start, current_period_end, status, auto_renew,
                      unsubscribe_detected_at, billing_issues_detected_at,
                      grace_period_expires_at, is_trial, updated_at
               FROM billing_subscriptions WHERE customer_id = $1 ORDER BY id"#,
        )
        .bind(customer_id)
        .fetch_all(self.pool())
        .await
        .map_err(|e| StackhouseError::Database(e.to_string()))?;

        Ok(rows.iter().map(subscription_from_row).collect())
    }

    pub async fn mark_subscription_cancelled(
        &self,
        customer_id: i64,
        store: &str,
        original_transaction_id: &str,
    ) -> StackhouseResult<()> {
        sqlx::query(
            r#"UPDATE billing_subscriptions
               SET auto_renew = FALSE,
                   unsubscribe_detected_at = NOW(),
                   updated_at = NOW()
               WHERE customer_id = $1 AND store = $2 AND original_transaction_id = $3"#,
        )
        .bind(customer_id)
        .bind(store)
        .bind(original_transaction_id)
        .execute(self.pool())
        .await
        .map_err(|e| StackhouseError::Database(e.to_string()))?;
        Ok(())
    }

    pub async fn record_receipt(
        &self,
        customer_id: i64,
        store: &str,
        raw: &str,
        validation_result: Option<&Value>,
    ) -> StackhouseResult<i64> {
        let row = sqlx::query(
            r#"INSERT INTO billing_receipts (customer_id, store, raw, validated_at, validation_result)
               VALUES ($1,$2,$3, CASE WHEN $4::jsonb IS NULL THEN NULL ELSE NOW() END, $4)
               RETURNING id"#,
        )
        .bind(customer_id)
        .bind(store)
        .bind(raw)
        .bind(validation_result)
        .fetch_one(self.pool())
        .await
        .map_err(|e| StackhouseError::Database(e.to_string()))?;
        Ok(row.get("id"))
    }

    // ------------------------------------------------------------------
    // Derived / helpers for entitlement resolution
    // ------------------------------------------------------------------
    pub async fn resolve_entitlements(
        &self,
        app_id: i64,
        customer_id: i64,
        now: DateTime<Utc>,
    ) -> StackhouseResult<Vec<EntitlementInfo>> {
        let subs = self.list_subscriptions(customer_id).await?;
        let entitlements = self.entitlement_product_map(app_id).await?;
        let products = self.list_products(app_id).await?;
        Ok(super::entitlements::resolve(
            &entitlements,
            &subs,
            &products,
            now,
        ))
    }

    // ------------------------------------------------------------------
    // Webhook endpoints
    // ------------------------------------------------------------------
    pub async fn add_webhook_endpoint(
        &self,
        app_id: i64,
        url: &str,
        secret: &str,
        events: &Value,
    ) -> StackhouseResult<i64> {
        let row = sqlx::query(
            r#"INSERT INTO billing_webhook_endpoints (app_id, url, secret, events)
               VALUES ($1,$2,$3,$4) RETURNING id"#,
        )
        .bind(app_id)
        .bind(url)
        .bind(secret)
        .bind(events)
        .fetch_one(self.pool())
        .await
        .map_err(|e| StackhouseError::Database(e.to_string()))?;
        Ok(row.get("id"))
    }

    pub async fn list_webhook_endpoints(
        &self,
        app_id: i64,
    ) -> StackhouseResult<Vec<(i64, String, String, Value, bool)>> {
        let rows = sqlx::query(
            r#"SELECT id, url, secret, events, active
               FROM billing_webhook_endpoints WHERE app_id = $1 AND active = TRUE"#,
        )
        .bind(app_id)
        .fetch_all(self.pool())
        .await
        .map_err(|e| StackhouseError::Database(e.to_string()))?;
        Ok(rows
            .into_iter()
            .map(|r| {
                (
                    r.get("id"),
                    r.get("url"),
                    r.get("secret"),
                    r.get("events"),
                    r.get("active"),
                )
            })
            .collect())
    }

    /// Insert a promotional "subscription" that grants an entitlement until
    /// `expires_at` without any real store transaction.
    pub async fn grant_promotional(
        &self,
        customer_id: i64,
        product_id: i64,
        expires_at: DateTime<Utc>,
    ) -> StackhouseResult<super::models::Subscription> {
        let original_txn = format!(
            "promo_{customer_id}_{product_id}_{}",
            expires_at.timestamp()
        );
        let row = sqlx::query(
            r#"INSERT INTO billing_subscriptions
                 (customer_id, product_id, store, original_transaction_id,
                  current_period_start, current_period_end, status, auto_renew, is_trial)
               VALUES ($1,$2,'promotional',$3, NOW(), $4, 'active', FALSE, FALSE)
               ON CONFLICT (customer_id, store, original_transaction_id) DO UPDATE SET
                 current_period_end = EXCLUDED.current_period_end,
                 status = 'active',
                 updated_at = NOW()
               RETURNING id, customer_id, product_id, store, original_transaction_id,
                         current_period_start, current_period_end, status, auto_renew,
                         unsubscribe_detected_at, billing_issues_detected_at,
                         grace_period_expires_at, is_trial, updated_at"#,
        )
        .bind(customer_id)
        .bind(product_id)
        .bind(&original_txn)
        .bind(expires_at)
        .fetch_one(self.pool())
        .await
        .map_err(|e| StackhouseError::Database(format!("grant_promotional: {e}")))?;

        Ok(subscription_from_row(&row))
    }

    pub async fn enqueue_delivery(
        &self,
        endpoint_id: i64,
        event_type: &str,
        payload: &Value,
    ) -> StackhouseResult<()> {
        sqlx::query(
            r#"INSERT INTO billing_webhook_deliveries (endpoint_id, event_type, payload)
               VALUES ($1,$2,$3)"#,
        )
        .bind(endpoint_id)
        .bind(event_type)
        .bind(payload)
        .execute(self.pool())
        .await
        .map_err(|e| StackhouseError::Database(e.to_string()))?;
        Ok(())
    }

    /// Query subscription plans from the stackhouse_subscription_plans table.
    pub async fn query_plans(&self) -> StackhouseResult<Vec<Value>> {
        let rows = sqlx::query(
            r#"SELECT id, name, tier, description, base_price_cents,
                      billing_interval, features, limits
               FROM stackhouse_subscription_plans
               WHERE is_active = true
               ORDER BY base_price_cents"#,
        )
        .fetch_all(self.pool())
        .await
        .map_err(|e| StackhouseError::Database(e.to_string()))?;

        let plans: Vec<Value> = rows
            .iter()
            .map(|row| {
                json!({
                    "id": row.try_get::<String, _>("id").unwrap_or_default(),
                    "name": row.try_get::<String, _>("name").unwrap_or_default(),
                    "tier": row.try_get::<String, _>("tier").unwrap_or_default(),
                    "description": row.try_get::<String, _>("description").unwrap_or_default(),
                    "base_price_cents": row.try_get::<i64, _>("base_price_cents").unwrap_or(0),
                    "billing_interval": row.try_get::<String, _>("billing_interval").unwrap_or("monthly".into()),
                    "features": row.try_get::<Value, _>("features").unwrap_or(json!([])),
                    "limits": row.try_get::<Value, _>("limits").unwrap_or(json!({})),
                })
            })
            .collect();
        Ok(plans)
    }
}

// ---------------------------------------------------------------------------
// Row helpers
// ---------------------------------------------------------------------------
fn offering_from_row(row: &sqlx::postgres::PgRow) -> Offering {
    Offering {
        id: row.get("id"),
        app_id: row.get("app_id"),
        identifier: row.get("identifier"),
        is_current: row.get("is_current"),
        metadata: row.get("metadata"),
        audience_id: row.try_get("audience_id").ok(),
        packages: vec![],
    }
}

async fn packages_for_offering_ids(
    pool: &sqlx::PgPool,
    ids: &[i64],
) -> StackhouseResult<HashMap<i64, Vec<Package>>> {
    if ids.is_empty() {
        return Ok(HashMap::new());
    }

    let mut packages: HashMap<i64, Vec<Package>> = HashMap::new();
    for id in ids {
        packages.insert(*id, Vec::new());
    }

    let rows = sqlx::query(
        r#"SELECT id, offering_id, identifier, product_id, package_type
           FROM billing_packages WHERE offering_id = ANY($1) ORDER BY id"#,
    )
    .bind(ids.to_vec())
    .fetch_all(pool)
    .await
    .map_err(|e| StackhouseError::Database(format!("packages_for_offering_ids: {e}")))?;

    for r in rows {
        let offering_id: i64 = r.get("offering_id");
        packages.entry(offering_id).or_default().push(Package {
            id: r.get("id"),
            offering_id,
            identifier: r.get("identifier"),
            product_id: r.get("product_id"),
            package_type: r.try_get("package_type").ok(),
        });
    }

    Ok(packages)
}

impl BillingStore {
    // ------------------------------------------------------------------
    // Audiences
    // ------------------------------------------------------------------
    pub async fn upsert_audience(
        &self,
        app_id: i64,
        identifier: &str,
        display_name: Option<&str>,
        rules: &Value,
    ) -> StackhouseResult<Audience> {
        let row = sqlx::query(
            r#"INSERT INTO billing_audiences (app_id, identifier, display_name, rules)
               VALUES ($1,$2,$3,$4)
               ON CONFLICT (app_id, identifier) DO UPDATE SET
                 display_name = COALESCE(EXCLUDED.display_name, billing_audiences.display_name),
                 rules = EXCLUDED.rules
               RETURNING id, app_id, identifier, display_name, rules"#,
        )
        .bind(app_id)
        .bind(identifier)
        .bind(display_name)
        .bind(rules)
        .fetch_one(self.pool())
        .await
        .map_err(|e| StackhouseError::Database(format!("upsert_audience: {e}")))?;

        Ok(Audience {
            id: row.get("id"),
            app_id: row.get("app_id"),
            identifier: row.get("identifier"),
            display_name: row.try_get("display_name").ok(),
            rules: row.get("rules"),
        })
    }

    pub async fn list_audiences(&self, app_id: i64) -> StackhouseResult<Vec<Audience>> {
        let rows = sqlx::query(
            r#"SELECT id, app_id, identifier, display_name, rules
               FROM billing_audiences WHERE app_id = $1 ORDER BY id"#,
        )
        .bind(app_id)
        .fetch_all(self.pool())
        .await
        .map_err(|e| StackhouseError::Database(format!("list_audiences: {e}")))?;

        Ok(rows
            .into_iter()
            .map(|r| Audience {
                id: r.get("id"),
                app_id: r.get("app_id"),
                identifier: r.get("identifier"),
                display_name: r.try_get("display_name").ok(),
                rules: r.get("rules"),
            })
            .collect())
    }

    pub async fn set_offering_audience(
        &self,
        offering_id: i64,
        audience_id: Option<i64>,
    ) -> StackhouseResult<()> {
        sqlx::query(r#"UPDATE billing_offerings SET audience_id = $1 WHERE id = $2"#)
            .bind(audience_id)
            .bind(offering_id)
            .execute(self.pool())
            .await
            .map_err(|e| StackhouseError::Database(format!("set_offering_audience: {e}")))?;
        Ok(())
    }

    // ------------------------------------------------------------------
    // Experiments
    // ------------------------------------------------------------------
    pub async fn upsert_experiment(
        &self,
        app_id: i64,
        identifier: &str,
        metric: Option<&str>,
        audience_id: Option<i64>,
        variants: &[Variant],
    ) -> StackhouseResult<ExperimentWithVariants> {
        let row = sqlx::query(
            r#"INSERT INTO billing_experiments (app_id, identifier, metric, audience_id)
               VALUES ($1,$2,$3,$4)
               ON CONFLICT (app_id, identifier) DO UPDATE SET
                 metric = EXCLUDED.metric,
                 audience_id = EXCLUDED.audience_id
               RETURNING id, app_id, identifier, status, metric, audience_id, started_at, ended_at"#,
        )
        .bind(app_id)
        .bind(identifier)
        .bind(metric)
        .bind(audience_id)
        .fetch_one(self.pool())
        .await
        .map_err(|e| StackhouseError::Database(format!("upsert_experiment: {e}")))?;

        let experiment = experiment_from_row(&row);

        // Replace variants so the saved experiment always reflects the latest input.
        sqlx::query("DELETE FROM billing_experiment_variants WHERE experiment_id = $1")
            .bind(experiment.id)
            .execute(self.pool())
            .await
            .map_err(|e| {
                StackhouseError::Database(format!("upsert_experiment: delete variants: {e}"))
            })?;

        let mut inserted = Vec::with_capacity(variants.len());
        for v in variants {
            let row = sqlx::query(
                r#"INSERT INTO billing_experiment_variants
                     (experiment_id, identifier, offering_id, is_control, traffic_weight)
                   VALUES ($1,$2,$3,$4,$5)
                   RETURNING id, experiment_id, identifier, offering_id, is_control, traffic_weight"#,
            )
            .bind(experiment.id)
            .bind(&v.identifier)
            .bind(v.offering_id)
            .bind(v.is_control)
            .bind(v.traffic_weight)
            .fetch_one(self.pool())
            .await
            .map_err(|e| StackhouseError::Database(format!("upsert_experiment variant: {e}")))?;

            inserted.push(variant_from_row(&row));
        }

        Ok(ExperimentWithVariants {
            experiment,
            variants: inserted,
        })
    }

    pub async fn list_experiments(
        &self,
        app_id: i64,
    ) -> StackhouseResult<Vec<ExperimentWithVariants>> {
        let rows = sqlx::query(
            r#"SELECT id, app_id, identifier, status, metric, audience_id, started_at, ended_at
               FROM billing_experiments WHERE app_id = $1 ORDER BY id"#,
        )
        .bind(app_id)
        .fetch_all(self.pool())
        .await
        .map_err(|e| StackhouseError::Database(format!("list_experiments: {e}")))?;

        let mut out = Vec::with_capacity(rows.len());
        for r in rows {
            let experiment = experiment_from_row(&r);
            let variants = self.variants_for_experiment(experiment.id).await?;
            out.push(ExperimentWithVariants {
                experiment,
                variants,
            });
        }
        Ok(out)
    }

    pub async fn list_running_experiments(
        &self,
        app_id: i64,
    ) -> StackhouseResult<Vec<ExperimentWithVariants>> {
        let rows = sqlx::query(
            r#"SELECT id, app_id, identifier, status, metric, audience_id, started_at, ended_at
               FROM billing_experiments WHERE app_id = $1 AND status = 'running' ORDER BY id"#,
        )
        .bind(app_id)
        .fetch_all(self.pool())
        .await
        .map_err(|e| StackhouseError::Database(format!("list_running_experiments: {e}")))?;

        let mut out = Vec::with_capacity(rows.len());
        for r in rows {
            let experiment = experiment_from_row(&r);
            let variants = self.variants_for_experiment(experiment.id).await?;
            out.push(ExperimentWithVariants {
                experiment,
                variants,
            });
        }
        Ok(out)
    }

    pub async fn get_experiment(
        &self,
        experiment_id: i64,
    ) -> StackhouseResult<ExperimentWithVariants> {
        let row = sqlx::query(
            r#"SELECT id, app_id, identifier, status, metric, audience_id, started_at, ended_at
               FROM billing_experiments WHERE id = $1"#,
        )
        .bind(experiment_id)
        .fetch_optional(self.pool())
        .await
        .map_err(|e| StackhouseError::Database(format!("get_experiment: {e}")))?
        .ok_or_else(|| StackhouseError::NotFound(format!("experiment {experiment_id}")))?;

        let experiment = experiment_from_row(&row);
        let variants = self.variants_for_experiment(experiment.id).await?;
        Ok(ExperimentWithVariants {
            experiment,
            variants,
        })
    }

    pub async fn variants_for_experiment(
        &self,
        experiment_id: i64,
    ) -> StackhouseResult<Vec<Variant>> {
        let rows = sqlx::query(
            r#"SELECT id, experiment_id, identifier, offering_id, is_control, traffic_weight
               FROM billing_experiment_variants WHERE experiment_id = $1 ORDER BY id"#,
        )
        .bind(experiment_id)
        .fetch_all(self.pool())
        .await
        .map_err(|e| StackhouseError::Database(format!("variants_for_experiment: {e}")))?;

        Ok(rows.iter().map(variant_from_row).collect())
    }

    pub async fn update_experiment_status(
        &self,
        experiment_id: i64,
        status: &str,
        started_at: Option<DateTime<Utc>>,
        ended_at: Option<DateTime<Utc>>,
    ) -> StackhouseResult<ExperimentWithVariants> {
        sqlx::query(
            r#"UPDATE billing_experiments
               SET status = $2, started_at = $3, ended_at = $4
               WHERE id = $1"#,
        )
        .bind(experiment_id)
        .bind(status)
        .bind(started_at)
        .bind(ended_at)
        .execute(self.pool())
        .await
        .map_err(|e| StackhouseError::Database(format!("update_experiment_status: {e}")))?;

        self.get_experiment(experiment_id).await
    }

    pub async fn get_or_assign_variant(
        &self,
        experiment_id: i64,
        customer_id: i64,
        variants: &[Variant],
    ) -> StackhouseResult<i64> {
        // Fast path: existing assignment.
        let existing = sqlx::query(
            r#"SELECT variant_id FROM billing_experiment_assignments
               WHERE experiment_id = $1 AND customer_id = $2"#,
        )
        .bind(experiment_id)
        .bind(customer_id)
        .fetch_optional(self.pool())
        .await
        .map_err(|e| StackhouseError::Database(format!("get_or_assign_variant: {e}")))?;

        if let Some(row) = existing {
            return Ok(row.get("variant_id"));
        }

        let Some(variant_id) =
            super::experiments::assign_variant(experiment_id, customer_id, variants)
        else {
            return Err(StackhouseError::InvalidPayload(
                "experiment has no variants".into(),
            ));
        };

        // Race-safe insert: if another request assigned concurrently, return that one.
        let inserted = sqlx::query(
            r#"INSERT INTO billing_experiment_assignments (experiment_id, customer_id, variant_id)
               VALUES ($1,$2,$3)
               ON CONFLICT (experiment_id, customer_id) DO NOTHING
               RETURNING variant_id"#,
        )
        .bind(experiment_id)
        .bind(customer_id)
        .bind(variant_id)
        .fetch_optional(self.pool())
        .await
        .map_err(|e| StackhouseError::Database(format!("get_or_assign_variant: {e}")))?;

        if let Some(row) = inserted {
            Ok(row.get("variant_id"))
        } else {
            let row = sqlx::query(
                r#"SELECT variant_id FROM billing_experiment_assignments
                   WHERE experiment_id = $1 AND customer_id = $2"#,
            )
            .bind(experiment_id)
            .bind(customer_id)
            .fetch_one(self.pool())
            .await
            .map_err(|e| StackhouseError::Database(format!("get_or_assign_variant: {e}")))?;
            Ok(row.get("variant_id"))
        }
    }

    pub async fn record_experiment_event(
        &self,
        experiment_id: i64,
        variant_id: i64,
        customer_id: i64,
        event_type: &str,
        metadata: &Value,
    ) -> StackhouseResult<()> {
        sqlx::query(
            r#"INSERT INTO billing_experiment_events
                 (experiment_id, variant_id, customer_id, event_type, metadata)
               VALUES ($1,$2,$3,$4,$5)"#,
        )
        .bind(experiment_id)
        .bind(variant_id)
        .bind(customer_id)
        .bind(event_type)
        .bind(metadata)
        .execute(self.pool())
        .await
        .map_err(|e| StackhouseError::Database(format!("record_experiment_event: {e}")))?;
        Ok(())
    }

    pub async fn experiment_results(
        &self,
        experiment_id: i64,
    ) -> StackhouseResult<Vec<VariantResult>> {
        let variants = self.variants_for_experiment(experiment_id).await?;

        let rows = sqlx::query(
            r#"SELECT
                 variant_id,
                 COUNT(DISTINCT customer_id) FILTER (WHERE event_type = 'impression') AS impressions,
                 COUNT(DISTINCT customer_id) FILTER (WHERE event_type = 'conversion') AS conversions
               FROM billing_experiment_events
               WHERE experiment_id = $1
               GROUP BY variant_id"#,
        )
        .bind(experiment_id)
        .fetch_all(self.pool())
        .await
        .map_err(|e| StackhouseError::Database(format!("experiment_results: {e}")))?;

        let mut counts: HashMap<i64, (i64, i64)> = HashMap::new();
        for r in rows {
            let variant_id: i64 = r.get("variant_id");
            let impressions: i64 = r.try_get("impressions").unwrap_or(0);
            let conversions: i64 = r.try_get("conversions").unwrap_or(0);
            counts.insert(variant_id, (impressions, conversions));
        }

        let control = variants.iter().find(|v| v.is_control);
        let control_id = control.map(|c| c.id);
        let control_counts = control_id.and_then(|id| counts.get(&id)).copied();

        Ok(variants
            .into_iter()
            .map(|v| {
                let (impressions, conversions) = counts.get(&v.id).copied().unwrap_or((0, 0));
                let conversion_rate = if impressions > 0 {
                    conversions as f64 / impressions as f64
                } else {
                    0.0
                };

                let z_score = if let (Some(cid), Some(cc)) = (control_id, control_counts) {
                    if v.id != cid {
                        super::experiments::confidence_vs_control(
                            super::experiments::VariantCounts {
                                impressions: cc.0,
                                conversions: cc.1,
                            },
                            super::experiments::VariantCounts {
                                impressions,
                                conversions,
                            },
                        )
                    } else {
                        None
                    }
                } else {
                    None
                };

                VariantResult {
                    variant_id: v.id,
                    identifier: v.identifier,
                    offering_id: v.offering_id,
                    is_control: v.is_control,
                    impressions,
                    conversions,
                    conversion_rate,
                    z_score,
                }
            })
            .collect())
    }

    // ------------------------------------------------------------------
    // Paywalls
    // ------------------------------------------------------------------
    pub async fn upsert_paywall(
        &self,
        offering_id: i64,
        template: Option<&str>,
        config: &Value,
        draft_config: Option<&Value>,
    ) -> StackhouseResult<Paywall> {
        let row = sqlx::query(
            r#"INSERT INTO billing_paywalls (offering_id, template, config, draft_config)
               VALUES ($1,$2,$3,$4)
               ON CONFLICT (offering_id) DO UPDATE SET
                 template = EXCLUDED.template,
                 draft_config = EXCLUDED.draft_config,
                 updated_at = NOW()
               RETURNING id, offering_id, template, config, draft_config, is_published"#,
        )
        .bind(offering_id)
        .bind(template)
        .bind(config)
        .bind(draft_config)
        .fetch_one(self.pool())
        .await
        .map_err(|e| StackhouseError::Database(format!("upsert_paywall: {e}")))?;

        Ok(paywall_from_row(&row))
    }

    pub async fn get_paywall(&self, offering_id: i64) -> StackhouseResult<Option<Paywall>> {
        let row = sqlx::query(
            r#"SELECT id, offering_id, template, config, draft_config, is_published
               FROM billing_paywalls WHERE offering_id = $1"#,
        )
        .bind(offering_id)
        .fetch_optional(self.pool())
        .await
        .map_err(|e| StackhouseError::Database(format!("get_paywall: {e}")))?;

        Ok(row.as_ref().map(paywall_from_row))
    }

    pub async fn publish_draft_paywall(&self, offering_id: i64) -> StackhouseResult<Paywall> {
        sqlx::query(
            r#"UPDATE billing_paywalls
               SET config = COALESCE(draft_config, config),
                   draft_config = NULL,
                   is_published = TRUE,
                   updated_at = NOW()
               WHERE offering_id = $1"#,
        )
        .bind(offering_id)
        .execute(self.pool())
        .await
        .map_err(|e| StackhouseError::Database(format!("publish_draft_paywall: {e}")))?;

        self.get_paywall(offering_id)
            .await?
            .ok_or_else(|| StackhouseError::NotFound(format!("paywall for offering {offering_id}")))
    }

    pub async fn customer_has_active_subscription(
        &self,
        customer_id: i64,
    ) -> StackhouseResult<bool> {
        let now = Utc::now();
        let row = sqlx::query(
            r#"SELECT EXISTS(
                 SELECT 1 FROM billing_subscriptions
                 WHERE customer_id = $1
                   AND status = 'active'
                   AND (current_period_end IS NULL OR current_period_end > $2)
               ) AS has_active"#,
        )
        .bind(customer_id)
        .bind(now)
        .fetch_one(self.pool())
        .await
        .map_err(|e| StackhouseError::Database(format!("customer_has_active_subscription: {e}")))?;

        Ok(row.get::<bool, _>("has_active"))
    }
}

fn product_from_row(row: &sqlx::postgres::PgRow) -> Product {
    Product {
        id: row.get("id"),
        app_id: row.get("app_id"),
        store: row.get("store"),
        store_product_id: row.get("store_product_id"),
        product_type: row.get("product_type"),
        period: row.try_get("period").ok(),
        price_micros: row.try_get("price_micros").ok(),
        currency: row.try_get("currency").ok(),
        metadata: row.get("metadata"),
    }
}

fn customer_from_row(row: &sqlx::postgres::PgRow) -> Customer {
    Customer {
        id: row.get("id"),
        app_id: row.get("app_id"),
        app_user_id: row.get("app_user_id"),
        aliases: row.get("aliases"),
        attributes: row.get("attributes"),
        first_seen_at: row.get("first_seen_at"),
        last_seen_at: row.get("last_seen_at"),
    }
}

fn subscription_from_row(row: &sqlx::postgres::PgRow) -> Subscription {
    Subscription {
        id: row.get("id"),
        customer_id: row.get("customer_id"),
        product_id: row.try_get("product_id").ok(),
        store: row.get("store"),
        original_transaction_id: row.try_get("original_transaction_id").ok(),
        current_period_start: row.try_get("current_period_start").ok(),
        current_period_end: row.try_get("current_period_end").ok(),
        status: row.get("status"),
        auto_renew: row.get("auto_renew"),
        unsubscribe_detected_at: row.try_get("unsubscribe_detected_at").ok(),
        billing_issues_detected_at: row.try_get("billing_issues_detected_at").ok(),
        grace_period_expires_at: row.try_get("grace_period_expires_at").ok(),
        is_trial: row.get("is_trial"),
        updated_at: row.get("updated_at"),
    }
}

fn experiment_from_row(row: &sqlx::postgres::PgRow) -> Experiment {
    Experiment {
        id: row.get("id"),
        app_id: row.get("app_id"),
        identifier: row.get("identifier"),
        status: row.get("status"),
        metric: row.try_get("metric").ok(),
        audience_id: row.try_get("audience_id").ok(),
        started_at: row.try_get("started_at").ok(),
        ended_at: row.try_get("ended_at").ok(),
    }
}

fn variant_from_row(row: &sqlx::postgres::PgRow) -> Variant {
    Variant {
        id: row.get("id"),
        experiment_id: row.get("experiment_id"),
        identifier: row.get("identifier"),
        offering_id: row.get("offering_id"),
        is_control: row.get("is_control"),
        traffic_weight: row.get("traffic_weight"),
    }
}

fn paywall_from_row(row: &sqlx::postgres::PgRow) -> Paywall {
    Paywall {
        id: row.get("id"),
        offering_id: row.get("offering_id"),
        template: row.try_get("template").ok(),
        config: row.get("config"),
        draft_config: row.try_get("draft_config").ok(),
        is_published: row.get("is_published"),
    }
}
