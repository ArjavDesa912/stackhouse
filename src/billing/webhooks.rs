//! Outbound webhook dispatcher.
//!
//! Payloads are signed with the endpoint's secret using the header
//! `X-StackhouseBilling-Signature: t=<unix>,v1=<hex hmac-sha256>` over
//! `<t>.<body>`. A background task drains
//! `billing_webhook_deliveries` with exponential backoff.

use chrono::Utc;
use hmac::{Hmac, Mac};
use reqwest::Client;
use serde_json::{json, Value};
use sha2::Sha256;
use sqlx::Row;
use std::sync::Arc;
use std::time::Duration;
use tokio::time::sleep;
use tracing::{debug, warn};

use super::models::BillingEventType;
use super::store::BillingStore;
use crate::error::{StackhouseError, StackhouseResult};

type HmacSha256 = Hmac<Sha256>;

const BACKOFF_SECS: &[i64] = &[60, 300, 1_800, 7_200, 43_200];
const MAX_ATTEMPTS: i32 = 5;

/// Enqueue an outbound event to all active endpoints for `app_id`.
pub async fn enqueue_event(
    store: &BillingStore,
    app_id: i64,
    event_type: BillingEventType,
    payload: &Value,
) -> StackhouseResult<()> {
    let endpoints = store.list_webhook_endpoints(app_id).await?;
    let event_body = json!({
        "type": event_type.as_str(),
        "api_version": "2024-01",
        "created_at": Utc::now().to_rfc3339(),
        "data": payload,
    });
    for (id, _, _, events_filter, _) in endpoints {
        if !event_matches(&events_filter, event_type.as_str()) {
            continue;
        }
        store
            .enqueue_delivery(id, event_type.as_str(), &event_body)
            .await?;
    }
    Ok(())
}

fn event_matches(filter: &Value, event_type: &str) -> bool {
    match filter.as_array() {
        None => true,
        Some(arr) if arr.is_empty() => true,
        Some(arr) => arr.iter().any(|v| v.as_str() == Some(event_type)),
    }
}

pub fn sign_payload(secret: &str, body: &[u8], now_unix: i64) -> String {
    let mut mac =
        HmacSha256::new_from_slice(secret.as_bytes()).expect("hmac accepts any key length");
    mac.update(format!("{now_unix}.").as_bytes());
    mac.update(body);
    format!(
        "t={now_unix},v1={}",
        hex::encode(mac.finalize().into_bytes())
    )
}

/// Spawns the delivery worker. Returns a join handle that can be ignored.
pub fn spawn_dispatcher(store: Arc<BillingStore>, http: Client) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        loop {
            match drain_once(&store, &http).await {
                Ok(n) if n > 0 => debug!("billing: dispatched {n} webhook(s)"),
                Ok(_) => {}
                Err(e) => warn!("billing: dispatcher error: {e}"),
            }
            sleep(Duration::from_secs(5)).await;
        }
    })
}

async fn drain_once(store: &BillingStore, http: &Client) -> StackhouseResult<usize> {
    let rows = sqlx::query(
        r#"SELECT d.id, d.endpoint_id, d.event_type, d.payload, d.attempts, e.url, e.secret
           FROM billing_webhook_deliveries d
           JOIN billing_webhook_endpoints e ON e.id = d.endpoint_id
           WHERE d.status = 'pending' AND d.next_attempt_at <= NOW() AND e.active = TRUE
           ORDER BY d.next_attempt_at
           LIMIT 50"#,
    )
    .fetch_all(store.pool())
    .await
    .map_err(|e| StackhouseError::Database(e.to_string()))?;

    let mut count = 0;
    for row in rows {
        let id: i64 = row.get("id");
        let url: String = row.get("url");
        let secret: String = row.get("secret");
        let event_type: String = row.get("event_type");
        let payload: Value = row.get("payload");
        let attempts: i32 = row.get("attempts");

        let body_bytes = serde_json::to_vec(&payload).unwrap_or_default();
        let signature = sign_payload(&secret, &body_bytes, Utc::now().timestamp());
        let result = http
            .post(&url)
            .header("content-type", "application/json")
            .header("x-stackhousebilling-signature", signature)
            .header("x-stackhousebilling-event", &event_type)
            .body(body_bytes)
            .send()
            .await;

        match result {
            Ok(resp) if resp.status().is_success() => {
                sqlx::query(
                    r#"UPDATE billing_webhook_deliveries
                       SET status = 'delivered', attempts = attempts + 1, updated_at = NOW()
                       WHERE id = $1"#,
                )
                .bind(id)
                .execute(store.pool())
                .await
                .map_err(|e| StackhouseError::Database(e.to_string()))?;
                count += 1;
            }
            Ok(resp) => {
                record_failure(store, id, attempts, &format!("http {}", resp.status())).await?;
            }
            Err(e) => {
                record_failure(store, id, attempts, &format!("request error: {e}")).await?;
            }
        }
    }
    Ok(count)
}

async fn record_failure(
    store: &BillingStore,
    id: i64,
    attempts: i32,
    err: &str,
) -> StackhouseResult<()> {
    let next_attempts = attempts + 1;
    if next_attempts >= MAX_ATTEMPTS {
        sqlx::query(
            r#"UPDATE billing_webhook_deliveries
               SET status = 'failed', attempts = $2, last_error = $3, updated_at = NOW()
               WHERE id = $1"#,
        )
        .bind(id)
        .bind(next_attempts)
        .bind(err)
        .execute(store.pool())
        .await
        .map_err(|e| StackhouseError::Database(e.to_string()))?;
    } else {
        let backoff = BACKOFF_SECS[next_attempts as usize % BACKOFF_SECS.len()];
        sqlx::query(
            r#"UPDATE billing_webhook_deliveries
               SET attempts = $2,
                   last_error = $3,
                   next_attempt_at = NOW() + ($4::text || ' seconds')::interval,
                   updated_at = NOW()
               WHERE id = $1"#,
        )
        .bind(id)
        .bind(next_attempts)
        .bind(err)
        .bind(backoff.to_string())
        .execute(store.pool())
        .await
        .map_err(|e| StackhouseError::Database(e.to_string()))?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn signature_format_is_parseable() {
        let sig = sign_payload("secret", b"{\"x\":1}", 1_700_000_000);
        assert!(sig.starts_with("t=1700000000,v1="));
        let hex_part = sig.split("v1=").nth(1).unwrap();
        assert_eq!(hex_part.len(), 64);
    }

    #[test]
    fn event_filter_empty_array_matches_all() {
        assert!(event_matches(&json!([]), "RENEWAL"));
        assert!(event_matches(&json!(["RENEWAL"]), "RENEWAL"));
        assert!(!event_matches(&json!(["CANCELLATION"]), "RENEWAL"));
    }
}
