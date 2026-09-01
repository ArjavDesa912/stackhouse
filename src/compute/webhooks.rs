//! # Webhooks with HMAC Verification, Retry & Dead-Letter Queue
//!
//! Inbound webhook registration, HMAC signature verification,
//! exponential backoff retry, and dead-letter queue for failed deliveries.

use crate::auth::{extract_auth_user, AuthState};
use crate::db::{SqlValue, StackhouseStore};
use crate::error::{StackhouseError, StackhouseResult};

use axum::{
    extract::State,
    http::HeaderMap,
    response::IntoResponse,
    routing::{delete, get, post},
    Json, Router,
};
use hmac::{Hmac, Mac};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sha2::Sha256;
use std::sync::Arc;
use std::time::Duration;
use tracing::info;

type HmacSha256 = Hmac<Sha256>;

// ============================================================================
// Types
// ============================================================================

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WebhookEndpoint {
    pub id: String,
    pub tenant_id: i64,
    pub name: String,
    pub url: String,
    pub secret: String,
    pub events: Vec<String>,
    pub enabled: bool,
    pub retry_config: RetryConfig,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RetryConfig {
    pub max_retries: u32,
    pub initial_delay_secs: u64,
    pub max_delay_secs: u64,
    pub backoff_multiplier: f64,
}

impl Default for RetryConfig {
    fn default() -> Self {
        Self {
            max_retries: 5,
            initial_delay_secs: 5,
            max_delay_secs: 3600,
            backoff_multiplier: 2.0,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WebhookDelivery {
    pub id: String,
    pub endpoint_id: String,
    pub event_type: String,
    pub payload: Value,
    pub status: DeliveryStatus,
    pub attempts: u32,
    pub last_response_code: Option<u16>,
    pub last_response_body: Option<String>,
    pub next_retry_at: Option<String>,
    pub created_at: String,
    pub completed_at: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum DeliveryStatus {
    Pending,
    Delivering,
    Delivered,
    Retrying,
    Failed,
    DeadLetter,
}

// ============================================================================
// Webhook Service
// ============================================================================

#[derive(Clone)]
pub struct WebhookService {
    store: Arc<StackhouseStore>,
}

impl WebhookService {
    pub async fn new(store: Arc<StackhouseStore>) -> StackhouseResult<Self> {
        let service = Self { store };
        service.initialize_tables().await?;
        service.start_retry_worker();
        info!("🪝 Webhook service initialized");
        Ok(service)
    }

    async fn initialize_tables(&self) -> StackhouseResult<()> {
        self.store.execute_batch(r#"
            CREATE TABLE IF NOT EXISTS stackhouse_webhook_endpoints (
                id TEXT PRIMARY KEY,
                tenant_id BIGINT NOT NULL,
                name TEXT NOT NULL,
                url TEXT NOT NULL,
                secret TEXT NOT NULL,
                events TEXT NOT NULL DEFAULT '["*"]',
                enabled BOOLEAN DEFAULT TRUE,
                retry_max INTEGER DEFAULT 5,
                retry_initial_delay INTEGER DEFAULT 5,
                created_at TIMESTAMPTZ DEFAULT NOW()
            );
            CREATE TABLE IF NOT EXISTS stackhouse_webhook_deliveries (
                id TEXT PRIMARY KEY,
                endpoint_id TEXT NOT NULL,
                tenant_id BIGINT NOT NULL,
                event_type TEXT NOT NULL,
                payload JSONB NOT NULL,
                status TEXT NOT NULL DEFAULT 'pending',
                attempts INTEGER DEFAULT 0,
                last_response_code INTEGER,
                last_response_body TEXT,
                next_retry_at TIMESTAMPTZ,
                created_at TIMESTAMPTZ DEFAULT NOW(),
                completed_at TIMESTAMPTZ
            );
            CREATE INDEX IF NOT EXISTS idx_webhook_endpoints_tenant ON stackhouse_webhook_endpoints(tenant_id);
            CREATE INDEX IF NOT EXISTS idx_webhook_deliveries_status ON stackhouse_webhook_deliveries(status);
            CREATE INDEX IF NOT EXISTS idx_webhook_deliveries_retry ON stackhouse_webhook_deliveries(next_retry_at);
        "#.to_string()).await?;
        Ok(())
    }

    fn start_retry_worker(&self) {
        let store = Arc::clone(&self.store);
        tokio::spawn(async move {
            let mut interval = tokio::time::interval(Duration::from_secs(10));
            loop {
                interval.tick().await;
                Self::process_retries(&store).await;
            }
        });
    }

    async fn process_retries(store: &Arc<StackhouseStore>) {
        let rows = store.query(
            "SELECT d.id, d.endpoint_id, d.payload, d.attempts, e.url, e.secret, e.retry_max FROM stackhouse_webhook_deliveries d JOIN stackhouse_webhook_endpoints e ON e.id = d.endpoint_id WHERE d.status IN ('pending', 'retrying') AND (d.next_retry_at IS NULL OR d.next_retry_at <= NOW()) LIMIT 50".to_string(),
            vec![],
        ).await.unwrap_or_default();

        for row in &rows {
            let get = |key: &str| row.iter().find(|(k, _)| k == key).map(|(_, v)| v.clone());
            let delivery_id = get("id")
                .and_then(|v| v.as_str().map(String::from))
                .unwrap_or_default();
            let url = get("url")
                .and_then(|v| v.as_str().map(String::from))
                .unwrap_or_default();
            let secret = get("secret")
                .and_then(|v| v.as_str().map(String::from))
                .unwrap_or_default();
            let payload_str = get("payload")
                .and_then(|v| v.as_str().map(String::from))
                .unwrap_or_else(|| "{}".into());
            let attempts = get("attempts").and_then(|v| v.as_i64()).unwrap_or(0) as u32;
            let max_retries = get("retry_max").and_then(|v| v.as_i64()).unwrap_or(5) as u32;

            let result = Self::deliver(&url, &secret, &payload_str).await;

            match result {
                Ok(status_code) if status_code >= 200 && status_code < 300 => {
                    store.execute(
                        "UPDATE stackhouse_webhook_deliveries SET status = 'delivered', last_response_code = ?, attempts = ?, completed_at = NOW() WHERE id = ?".to_string(),
                        vec![SqlValue::Integer(status_code as i64), SqlValue::Integer((attempts + 1) as i64), SqlValue::Text(delivery_id)],
                    ).await.ok();
                }
                Ok(status_code) => {
                    if attempts + 1 >= max_retries {
                        store.execute(
                            "UPDATE stackhouse_webhook_deliveries SET status = 'dead_letter', last_response_code = ?, attempts = ? WHERE id = ?".to_string(),
                            vec![SqlValue::Integer(status_code as i64), SqlValue::Integer((attempts + 1) as i64), SqlValue::Text(delivery_id)],
                        ).await.ok();
                    } else {
                        let delay = 5u64 * 2u64.pow(attempts);
                        store.execute(
                            format!("UPDATE stackhouse_webhook_deliveries SET status = 'retrying', last_response_code = ?, attempts = ?, next_retry_at = NOW() + INTERVAL '{} seconds' WHERE id = ?", delay),
                            vec![SqlValue::Integer(status_code as i64), SqlValue::Integer((attempts + 1) as i64), SqlValue::Text(delivery_id)],
                        ).await.ok();
                    }
                }
                Err(_) => {
                    if attempts + 1 >= max_retries {
                        store.execute(
                            "UPDATE stackhouse_webhook_deliveries SET status = 'dead_letter', attempts = ? WHERE id = ?".to_string(),
                            vec![SqlValue::Integer((attempts + 1) as i64), SqlValue::Text(delivery_id)],
                        ).await.ok();
                    } else {
                        let delay = 5u64 * 2u64.pow(attempts);
                        store.execute(
                            format!("UPDATE stackhouse_webhook_deliveries SET status = 'retrying', attempts = ?, next_retry_at = NOW() + INTERVAL '{} seconds' WHERE id = ?", delay),
                            vec![SqlValue::Integer((attempts + 1) as i64), SqlValue::Text(delivery_id)],
                        ).await.ok();
                    }
                }
            }
        }
    }

    async fn deliver(url: &str, secret: &str, payload: &str) -> Result<u16, String> {
        let signature = Self::compute_signature(secret, payload);
        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(30))
            .build()
            .map_err(|e| e.to_string())?;

        let resp = client
            .post(url)
            .header("Content-Type", "application/json")
            .header("X-Webhook-Signature", format!("sha256={}", signature))
            .header(
                "X-Webhook-Timestamp",
                chrono::Utc::now().timestamp().to_string(),
            )
            .body(payload.to_string())
            .send()
            .await
            .map_err(|e| e.to_string())?;

        Ok(resp.status().as_u16())
    }

    fn compute_signature(secret: &str, payload: &str) -> String {
        let mut mac = HmacSha256::new_from_slice(secret.as_bytes()).unwrap();
        mac.update(payload.as_bytes());
        hex::encode(mac.finalize().into_bytes())
    }

    /// Verify an inbound webhook signature
    pub fn verify_signature(secret: &str, payload: &str, signature: &str) -> bool {
        let expected = Self::compute_signature(secret, payload);
        let provided = signature.strip_prefix("sha256=").unwrap_or(signature);
        expected == provided
    }

    /// Register a webhook endpoint
    pub async fn register_endpoint(
        &self,
        tenant_id: i64,
        name: &str,
        url: &str,
        events: Vec<String>,
    ) -> StackhouseResult<WebhookEndpoint> {
        let id = uuid::Uuid::new_v4().to_string();
        let secret = format!(
            "whsec_{}",
            uuid::Uuid::new_v4().to_string().replace("-", "")
        );

        self.store.execute(
            "INSERT INTO stackhouse_webhook_endpoints (id, tenant_id, name, url, secret, events) VALUES (?, ?, ?, ?, ?, ?)".to_string(),
            vec![
                SqlValue::Text(id.clone()),
                SqlValue::Integer(tenant_id),
                SqlValue::Text(name.to_string()),
                SqlValue::Text(url.to_string()),
                SqlValue::Text(secret.clone()),
                SqlValue::Text(serde_json::to_string(&events).unwrap_or_default()),
            ],
        ).await?;

        info!("🪝 Webhook endpoint registered: {} -> {}", name, url);

        Ok(WebhookEndpoint {
            id,
            tenant_id,
            name: name.to_string(),
            url: url.to_string(),
            secret,
            events,
            enabled: true,
            retry_config: RetryConfig::default(),
            created_at: chrono::Utc::now().to_rfc3339(),
        })
    }

    /// Dispatch an event to all matching endpoints
    pub async fn dispatch(
        &self,
        tenant_id: i64,
        event_type: &str,
        payload: Value,
    ) -> StackhouseResult<u32> {
        let rows = self.store.query(
            "SELECT id FROM stackhouse_webhook_endpoints WHERE tenant_id = ? AND enabled = true".to_string(),
            vec![SqlValue::Integer(tenant_id)],
        ).await?;

        let mut count = 0;
        for row in &rows {
            let endpoint_id = row
                .iter()
                .find(|(k, _)| k == "id")
                .and_then(|(_, v)| v.as_str())
                .unwrap_or("");
            let delivery_id = uuid::Uuid::new_v4().to_string();

            self.store.execute(
                "INSERT INTO stackhouse_webhook_deliveries (id, endpoint_id, tenant_id, event_type, payload) VALUES (?, ?, ?, ?, ?::jsonb)".to_string(),
                vec![
                    SqlValue::Text(delivery_id),
                    SqlValue::Text(endpoint_id.to_string()),
                    SqlValue::Integer(tenant_id),
                    SqlValue::Text(event_type.to_string()),
                    SqlValue::Text(payload.to_string()),
                ],
            ).await?;
            count += 1;
        }

        Ok(count)
    }

    /// List endpoints for a tenant
    pub async fn list_endpoints(&self, tenant_id: i64) -> StackhouseResult<Vec<Value>> {
        let rows = self.store.query(
            "SELECT id, name, url, events, enabled, created_at FROM stackhouse_webhook_endpoints WHERE tenant_id = ? ORDER BY created_at DESC".to_string(),
            vec![SqlValue::Integer(tenant_id)],
        ).await?;
        Ok(rows
            .into_iter()
            .map(|r| json!(r.into_iter().collect::<std::collections::HashMap<_, _>>()))
            .collect())
    }

    /// Get dead-letter queue
    pub async fn get_dead_letters(&self, tenant_id: i64) -> StackhouseResult<Vec<Value>> {
        let rows = self.store.query(
            "SELECT id, endpoint_id, event_type, attempts, last_response_code, created_at FROM stackhouse_webhook_deliveries WHERE tenant_id = ? AND status = 'dead_letter' ORDER BY created_at DESC LIMIT 100".to_string(),
            vec![SqlValue::Integer(tenant_id)],
        ).await?;
        Ok(rows
            .into_iter()
            .map(|r| json!(r.into_iter().collect::<std::collections::HashMap<_, _>>()))
            .collect())
    }

    /// Retry a dead-letter delivery
    pub async fn retry_dead_letter(&self, delivery_id: &str) -> StackhouseResult<()> {
        self.store.execute(
            "UPDATE stackhouse_webhook_deliveries SET status = 'pending', next_retry_at = NOW() WHERE id = ? AND status = 'dead_letter'".to_string(),
            vec![SqlValue::Text(delivery_id.to_string())],
        ).await?;
        Ok(())
    }

    /// Delete an endpoint
    pub async fn delete_endpoint(&self, endpoint_id: &str, tenant_id: i64) -> StackhouseResult<()> {
        self.store
            .execute(
                "DELETE FROM stackhouse_webhook_endpoints WHERE id = ? AND tenant_id = ?"
                    .to_string(),
                vec![
                    SqlValue::Text(endpoint_id.to_string()),
                    SqlValue::Integer(tenant_id),
                ],
            )
            .await?;
        Ok(())
    }
}

// ============================================================================
// Router
// ============================================================================

#[derive(Clone)]
pub struct WebhookState {
    pub webhooks: Arc<WebhookService>,
    pub auth: AuthState,
}

#[derive(Deserialize)]
struct RegisterEndpointRequest {
    name: String,
    url: String,
    #[serde(default = "default_events")]
    events: Vec<String>,
}
fn default_events() -> Vec<String> {
    vec!["*".into()]
}

async fn register_handler(
    State(state): State<WebhookState>,
    headers: HeaderMap,
    Json(req): Json<RegisterEndpointRequest>,
) -> Result<impl IntoResponse, StackhouseError> {
    let user = extract_auth_user(&state.auth, &headers)?;
    let endpoint = state
        .webhooks
        .register_endpoint(user.id, &req.name, &req.url, req.events)
        .await?;
    Ok(Json(json!({"success": true, "data": endpoint})))
}

async fn list_endpoints_handler(
    State(state): State<WebhookState>,
    headers: HeaderMap,
) -> Result<impl IntoResponse, StackhouseError> {
    let user = extract_auth_user(&state.auth, &headers)?;
    let endpoints = state.webhooks.list_endpoints(user.id).await?;
    Ok(Json(json!({"success": true, "data": endpoints})))
}

async fn dead_letters_handler(
    State(state): State<WebhookState>,
    headers: HeaderMap,
) -> Result<impl IntoResponse, StackhouseError> {
    let user = extract_auth_user(&state.auth, &headers)?;
    let dls = state.webhooks.get_dead_letters(user.id).await?;
    Ok(Json(json!({"success": true, "data": dls})))
}

async fn delete_endpoint_handler(
    State(state): State<WebhookState>,
    headers: HeaderMap,
    axum::extract::Path(id): axum::extract::Path<String>,
) -> Result<impl IntoResponse, StackhouseError> {
    let user = extract_auth_user(&state.auth, &headers)?;
    state.webhooks.delete_endpoint(&id, user.id).await?;
    Ok(Json(
        json!({"success": true, "message": "Endpoint deleted"}),
    ))
}

pub fn create_webhooks_router(state: WebhookState) -> Router {
    Router::new()
        .route("/endpoints", post(register_handler))
        .route("/endpoints", get(list_endpoints_handler))
        .route("/endpoints/:id", delete(delete_endpoint_handler))
        .route("/dead-letters", get(dead_letters_handler))
        .with_state(state)
}
