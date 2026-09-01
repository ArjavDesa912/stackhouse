//! # Multi-Provider Payment Integration
//!
//! PayPal, Paddle, and additional provider adapters alongside existing Stripe.

use crate::error::{StackhouseError, StackhouseResult};

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

// ============================================================================
// Provider Abstraction
// ============================================================================

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PaymentProvider {
    Stripe,
    PayPal,
    Paddle,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProviderConfig {
    pub provider: PaymentProvider,
    pub api_key: String,
    pub webhook_secret: String,
    pub sandbox: bool,
}

#[async_trait::async_trait]
pub trait PaymentProviderAdapter: Send + Sync {
    async fn create_checkout(
        &self,
        tenant_id: i64,
        plan_id: &str,
        return_url: &str,
    ) -> StackhouseResult<Value>;
    async fn cancel_subscription(&self, subscription_id: &str) -> StackhouseResult<()>;
    async fn get_subscription(&self, subscription_id: &str) -> StackhouseResult<Value>;
    async fn process_webhook(&self, payload: &str, signature: &str) -> StackhouseResult<Value>;
    fn provider_name(&self) -> &str;
}

// ============================================================================
// PayPal Adapter
// ============================================================================

pub struct PayPalAdapter {
    client_id: String,
    client_secret: String,
    sandbox: bool,
}

impl PayPalAdapter {
    pub fn new(client_id: &str, client_secret: &str, sandbox: bool) -> Self {
        Self {
            client_id: client_id.to_string(),
            client_secret: client_secret.to_string(),
            sandbox,
        }
    }

    fn base_url(&self) -> &str {
        if self.sandbox {
            "https://api-m.sandbox.paypal.com"
        } else {
            "https://api-m.paypal.com"
        }
    }

    async fn get_access_token(&self) -> StackhouseResult<String> {
        let client = reqwest::Client::new();
        let resp = client
            .post(format!("{}/v1/oauth2/token", self.base_url()))
            .basic_auth(&self.client_id, Some(&self.client_secret))
            .form(&[("grant_type", "client_credentials")])
            .send()
            .await
            .map_err(|e| StackhouseError::Internal(anyhow::anyhow!("PayPal auth: {}", e)))?;

        let data: Value = resp
            .json()
            .await
            .map_err(|e| StackhouseError::Internal(anyhow::anyhow!("PayPal auth parse: {}", e)))?;
        data["access_token"]
            .as_str()
            .map(String::from)
            .ok_or_else(|| {
                StackhouseError::Internal(anyhow::anyhow!("No access_token in PayPal response"))
            })
    }
}

#[async_trait::async_trait]
impl PaymentProviderAdapter for PayPalAdapter {
    async fn create_checkout(
        &self,
        _tenant_id: i64,
        plan_id: &str,
        return_url: &str,
    ) -> StackhouseResult<Value> {
        let token = self.get_access_token().await?;
        let client = reqwest::Client::new();

        let body = json!({
            "plan_id": plan_id,
            "application_context": {
                "return_url": return_url,
                "cancel_url": format!("{}/cancel", return_url),
            }
        });

        let resp = client
            .post(format!("{}/v1/billing/subscriptions", self.base_url()))
            .bearer_auth(&token)
            .json(&body)
            .send()
            .await
            .map_err(|e| StackhouseError::Internal(anyhow::anyhow!("PayPal checkout: {}", e)))?;

        let data: Value = resp.json().await.unwrap_or(json!({}));
        let approval_url = data["links"]
            .as_array()
            .and_then(|links| links.iter().find(|l| l["rel"] == "approve"))
            .and_then(|l| l["href"].as_str())
            .unwrap_or("");

        Ok(json!({
            "subscription_id": data["id"],
            "approval_url": approval_url,
            "provider": "paypal",
        }))
    }

    async fn cancel_subscription(&self, subscription_id: &str) -> StackhouseResult<()> {
        let token = self.get_access_token().await?;
        let client = reqwest::Client::new();
        client
            .post(format!(
                "{}/v1/billing/subscriptions/{}/cancel",
                self.base_url(),
                subscription_id
            ))
            .bearer_auth(&token)
            .json(&json!({"reason": "Customer requested cancellation"}))
            .send()
            .await
            .map_err(|e| StackhouseError::Internal(anyhow::anyhow!("PayPal cancel: {}", e)))?;
        Ok(())
    }

    async fn get_subscription(&self, subscription_id: &str) -> StackhouseResult<Value> {
        let token = self.get_access_token().await?;
        let client = reqwest::Client::new();
        let resp = client
            .get(format!(
                "{}/v1/billing/subscriptions/{}",
                self.base_url(),
                subscription_id
            ))
            .bearer_auth(&token)
            .send()
            .await
            .map_err(|e| StackhouseError::Internal(anyhow::anyhow!("PayPal get sub: {}", e)))?;
        let data: Value = resp.json().await.unwrap_or(json!({}));
        Ok(data)
    }

    async fn process_webhook(&self, payload: &str, _signature: &str) -> StackhouseResult<Value> {
        let event: Value = serde_json::from_str(payload)
            .map_err(|_| StackhouseError::InvalidPayload("Invalid PayPal webhook JSON".into()))?;
        Ok(event)
    }

    fn provider_name(&self) -> &str {
        "paypal"
    }
}

// ============================================================================
// Paddle Adapter
// ============================================================================

pub struct PaddleAdapter {
    api_key: String,
    sandbox: bool,
}

impl PaddleAdapter {
    pub fn new(api_key: &str, sandbox: bool) -> Self {
        Self {
            api_key: api_key.to_string(),
            sandbox,
        }
    }

    fn base_url(&self) -> &str {
        if self.sandbox {
            "https://sandbox-api.paddle.com"
        } else {
            "https://api.paddle.com"
        }
    }
}

#[async_trait::async_trait]
impl PaymentProviderAdapter for PaddleAdapter {
    async fn create_checkout(
        &self,
        _tenant_id: i64,
        plan_id: &str,
        return_url: &str,
    ) -> StackhouseResult<Value> {
        let client = reqwest::Client::new();
        let body = json!({
            "items": [{"price_id": plan_id, "quantity": 1}],
            "settings": {"success_url": return_url}
        });

        let resp = client
            .post(format!("{}/transactions", self.base_url()))
            .bearer_auth(&self.api_key)
            .json(&body)
            .send()
            .await
            .map_err(|e| StackhouseError::Internal(anyhow::anyhow!("Paddle checkout: {}", e)))?;

        let data: Value = resp.json().await.unwrap_or(json!({}));
        Ok(json!({
            "transaction_id": data["data"]["id"],
            "checkout_url": data["data"]["checkout"]["url"],
            "provider": "paddle",
        }))
    }

    async fn cancel_subscription(&self, subscription_id: &str) -> StackhouseResult<()> {
        let client = reqwest::Client::new();
        client
            .post(format!(
                "{}/subscriptions/{}/cancel",
                self.base_url(),
                subscription_id
            ))
            .bearer_auth(&self.api_key)
            .json(&json!({"effective_from": "next_billing_period"}))
            .send()
            .await
            .map_err(|e| StackhouseError::Internal(anyhow::anyhow!("Paddle cancel: {}", e)))?;
        Ok(())
    }

    async fn get_subscription(&self, subscription_id: &str) -> StackhouseResult<Value> {
        let client = reqwest::Client::new();
        let resp = client
            .get(format!(
                "{}/subscriptions/{}",
                self.base_url(),
                subscription_id
            ))
            .bearer_auth(&self.api_key)
            .send()
            .await
            .map_err(|e| StackhouseError::Internal(anyhow::anyhow!("Paddle get sub: {}", e)))?;
        let data: Value = resp.json().await.unwrap_or(json!({}));
        Ok(data)
    }

    async fn process_webhook(&self, payload: &str, _signature: &str) -> StackhouseResult<Value> {
        let event: Value = serde_json::from_str(payload)
            .map_err(|_| StackhouseError::InvalidPayload("Invalid Paddle webhook JSON".into()))?;
        Ok(event)
    }

    fn provider_name(&self) -> &str {
        "paddle"
    }
}
