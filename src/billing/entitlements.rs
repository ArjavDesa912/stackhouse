//! Pure entitlement resolver.
//!
//! Given a customer's subscriptions + the app's entitlement/product mapping,
//! compute the currently-active entitlements at `now`.

use chrono::{DateTime, Utc};
use std::collections::HashMap;

use super::models::{Entitlement, EntitlementInfo, Product, Subscription};

/// Resolves entitlements for a customer.
///
/// `entitlements` is `[(Entitlement, Vec<product_id>)]`, `subs` are all the
/// customer's subscriptions, `products` is the app's product catalogue.
pub fn resolve(
    entitlements: &[(Entitlement, Vec<i64>)],
    subs: &[Subscription],
    products: &[Product],
    now: DateTime<Utc>,
) -> Vec<EntitlementInfo> {
    let product_index: HashMap<i64, &Product> = products.iter().map(|p| (p.id, p)).collect();

    let mut out = Vec::with_capacity(entitlements.len());
    for (ent, product_ids) in entitlements {
        // Find the best candidate subscription for this entitlement: active one
        // with the latest expires_at.
        let mut best: Option<&Subscription> = None;
        for sub in subs {
            let Some(pid) = sub.product_id else { continue };
            if !product_ids.contains(&pid) {
                continue;
            }
            if is_more_relevant(best, sub, now) {
                best = Some(sub);
            }
        }

        let (
            is_active,
            will_renew,
            period_type,
            latest_purchase,
            expires,
            grace,
            store,
            product_ident,
        ) = match best {
            None => (
                false,
                false,
                "normal".to_string(),
                None,
                None,
                None,
                "unknown".to_string(),
                None,
            ),
            Some(s) => {
                let expired = s.current_period_end.map(|e| e <= now).unwrap_or(false);
                let in_grace = s.grace_period_expires_at.map(|g| g > now).unwrap_or(false);
                let active = !expired || in_grace;
                let ptype = if s.is_trial {
                    "trial"
                } else if in_grace {
                    "grace"
                } else {
                    "normal"
                };
                let prod_ident = s
                    .product_id
                    .and_then(|pid| product_index.get(&pid))
                    .map(|p| p.store_product_id.clone());
                (
                    active,
                    s.auto_renew,
                    ptype.to_string(),
                    s.current_period_start,
                    s.current_period_end,
                    s.grace_period_expires_at,
                    s.store.clone(),
                    prod_ident,
                )
            }
        };

        out.push(EntitlementInfo {
            identifier: ent.identifier.clone(),
            is_active,
            will_renew,
            period_type,
            latest_purchase_date: latest_purchase,
            expires_date: expires,
            grace_period_expires_date: grace,
            store,
            product_identifier: product_ident,
        });
    }
    out
}

fn is_more_relevant(
    current: Option<&Subscription>,
    candidate: &Subscription,
    now: DateTime<Utc>,
) -> bool {
    let Some(cur) = current else { return true };
    let cand_active = candidate
        .current_period_end
        .map(|e| e > now)
        .unwrap_or(true);
    let cur_active = cur.current_period_end.map(|e| e > now).unwrap_or(true);
    if cand_active != cur_active {
        return cand_active;
    }
    // Prefer later expiry.
    match (candidate.current_period_end, cur.current_period_end) {
        (Some(a), Some(b)) => a > b,
        (Some(_), None) => true,
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Duration;
    use serde_json::json;

    fn mk_product(id: i64, ident: &str) -> Product {
        Product {
            id,
            app_id: 1,
            store: "app_store".into(),
            store_product_id: ident.into(),
            product_type: "auto_renewable".into(),
            period: Some("P1M".into()),
            price_micros: Some(9_990_000),
            currency: Some("USD".into()),
            metadata: json!({}),
        }
    }

    fn mk_sub(id: i64, product_id: i64, expires: DateTime<Utc>, renew: bool) -> Subscription {
        Subscription {
            id,
            customer_id: 1,
            product_id: Some(product_id),
            store: "app_store".into(),
            original_transaction_id: Some(format!("otid_{id}")),
            current_period_start: Some(expires - Duration::days(30)),
            current_period_end: Some(expires),
            status: "active".into(),
            auto_renew: renew,
            unsubscribe_detected_at: None,
            billing_issues_detected_at: None,
            grace_period_expires_at: None,
            is_trial: false,
            updated_at: Utc::now(),
        }
    }

    #[test]
    fn active_subscription_yields_active_entitlement() {
        let now = Utc::now();
        let products = vec![mk_product(10, "pro.monthly")];
        let entitlements = vec![(
            Entitlement {
                id: 1,
                app_id: 1,
                identifier: "pro".into(),
                display_name: Some("Pro".into()),
            },
            vec![10],
        )];
        let subs = vec![mk_sub(1, 10, now + Duration::days(5), true)];
        let resolved = resolve(&entitlements, &subs, &products, now);
        assert_eq!(resolved.len(), 1);
        assert!(resolved[0].is_active);
        assert!(resolved[0].will_renew);
        assert_eq!(
            resolved[0].product_identifier.as_deref(),
            Some("pro.monthly")
        );
    }

    #[test]
    fn expired_subscription_is_inactive_unless_in_grace() {
        let now = Utc::now();
        let products = vec![mk_product(10, "pro.monthly")];
        let entitlements = vec![(
            Entitlement {
                id: 1,
                app_id: 1,
                identifier: "pro".into(),
                display_name: None,
            },
            vec![10],
        )];
        let mut sub = mk_sub(1, 10, now - Duration::days(1), false);
        let out = resolve(&entitlements, &[sub.clone()], &products, now);
        assert!(!out[0].is_active);

        sub.grace_period_expires_at = Some(now + Duration::days(2));
        let out = resolve(&entitlements, &[sub], &products, now);
        assert!(out[0].is_active);
        assert_eq!(out[0].period_type, "grace");
    }
}
