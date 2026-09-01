//! # Captcha Verification Module (Stackhouse-Captcha)
//!
//! Server-side captcha verification supporting hCaptcha, reCAPTCHA v2/v3, and Turnstile.
//! Can be used as middleware or called explicitly from auth handlers.

use crate::error::{StackhouseError, StackhouseResult};

use axum::extract::State;

use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tracing::{info, warn};

// ============================================================================
// Configuration
// ============================================================================

#[derive(Clone, Debug)]
pub enum CaptchaProvider {
    HCaptcha,
    ReCaptchaV2,
    ReCaptchaV3,
    Turnstile,
}

#[derive(Clone, Debug)]
pub struct CaptchaConfig {
    pub provider: CaptchaProvider,
    pub secret_key: String,
    pub site_key: String,
    pub enabled: bool,
    pub min_score: f64, // For reCAPTCHA v3 (0.0 - 1.0)
}

impl CaptchaConfig {
    pub fn from_env() -> Self {
        let provider = match std::env::var("STACKHOUSE_CAPTCHA_PROVIDER")
            .unwrap_or_default()
            .as_str()
        {
            "hcaptcha" => CaptchaProvider::HCaptcha,
            "recaptcha_v2" => CaptchaProvider::ReCaptchaV2,
            "recaptcha_v3" => CaptchaProvider::ReCaptchaV3,
            "turnstile" => CaptchaProvider::Turnstile,
            _ => CaptchaProvider::Turnstile,
        };

        let secret_key = std::env::var("STACKHOUSE_CAPTCHA_SECRET").unwrap_or_default();
        let site_key = std::env::var("STACKHOUSE_CAPTCHA_SITE_KEY").unwrap_or_default();
        let enabled = !secret_key.is_empty();
        let min_score = std::env::var("STACKHOUSE_CAPTCHA_MIN_SCORE")
            .ok()
            .and_then(|s| s.parse().ok())
            .unwrap_or(0.5);

        Self {
            provider,
            secret_key,
            site_key,
            enabled,
            min_score,
        }
    }
}

// ============================================================================
// Captcha Service
// ============================================================================

#[derive(Clone)]
pub struct CaptchaService {
    config: CaptchaConfig,
    http_client: reqwest::Client,
}

#[derive(Deserialize)]
struct HCaptchaResponse {
    success: bool,
    #[serde(default)]
    #[serde(rename = "error-codes")]
    error_codes: Vec<String>,
}

#[derive(Deserialize)]
struct ReCaptchaResponse {
    success: bool,
    #[serde(default)]
    score: Option<f64>,
    #[serde(default)]
    #[serde(rename = "error-codes")]
    error_codes: Vec<String>,
}

#[derive(Deserialize)]
struct TurnstileResponse {
    success: bool,
    #[serde(default)]
    #[serde(rename = "error-codes")]
    error_codes: Vec<String>,
}

#[derive(Serialize)]
pub struct CaptchaInfo {
    pub provider: String,
    pub site_key: String,
    pub enabled: bool,
}

impl CaptchaService {
    pub fn new(config: CaptchaConfig) -> Self {
        info!(
            "🛡️ Stackhouse-Captcha initialized (enabled: {}, provider: {:?})",
            config.enabled, config.provider
        );
        Self {
            config,
            http_client: reqwest::Client::new(),
        }
    }

    /// Get captcha configuration for the client
    pub fn get_info(&self) -> CaptchaInfo {
        CaptchaInfo {
            provider: match self.config.provider {
                CaptchaProvider::HCaptcha => "hcaptcha".to_string(),
                CaptchaProvider::ReCaptchaV2 => "recaptcha_v2".to_string(),
                CaptchaProvider::ReCaptchaV3 => "recaptcha_v3".to_string(),
                CaptchaProvider::Turnstile => "turnstile".to_string(),
            },
            site_key: self.config.site_key.clone(),
            enabled: self.config.enabled,
        }
    }

    /// Verify a captcha token. Returns Ok(()) on success, Err on failure.
    /// If captcha is not enabled, always returns Ok(()).
    pub async fn verify(&self, token: &str, remote_ip: Option<&str>) -> StackhouseResult<()> {
        if !self.config.enabled {
            return Ok(());
        }

        if token.is_empty() {
            return Err(StackhouseError::InvalidPayload(
                "Captcha token is required".to_string(),
            ));
        }

        match self.config.provider {
            CaptchaProvider::HCaptcha => self.verify_hcaptcha(token, remote_ip).await,
            CaptchaProvider::ReCaptchaV2 | CaptchaProvider::ReCaptchaV3 => {
                self.verify_recaptcha(token, remote_ip).await
            }
            CaptchaProvider::Turnstile => self.verify_turnstile(token, remote_ip).await,
        }
    }

    async fn verify_hcaptcha(&self, token: &str, remote_ip: Option<&str>) -> StackhouseResult<()> {
        let mut params = vec![
            ("secret", self.config.secret_key.as_str()),
            ("response", token),
        ];
        let ip_str;
        if let Some(ip) = remote_ip {
            ip_str = ip.to_string();
            params.push(("remoteip", &ip_str));
        }

        let response: HCaptchaResponse = self
            .http_client
            .post("https://hcaptcha.com/siteverify")
            .form(&params)
            .send()
            .await
            .map_err(|e| {
                StackhouseError::Internal(anyhow::anyhow!("hCaptcha verify failed: {}", e))
            })?
            .json()
            .await
            .map_err(|e| {
                StackhouseError::Internal(anyhow::anyhow!("hCaptcha parse failed: {}", e))
            })?;

        if !response.success {
            warn!("hCaptcha verification failed: {:?}", response.error_codes);
            return Err(StackhouseError::InvalidPayload(
                "Captcha verification failed".to_string(),
            ));
        }

        Ok(())
    }

    async fn verify_recaptcha(&self, token: &str, remote_ip: Option<&str>) -> StackhouseResult<()> {
        let mut params = vec![
            ("secret", self.config.secret_key.as_str()),
            ("response", token),
        ];
        let ip_str;
        if let Some(ip) = remote_ip {
            ip_str = ip.to_string();
            params.push(("remoteip", &ip_str));
        }

        let response: ReCaptchaResponse = self
            .http_client
            .post("https://www.google.com/recaptcha/api/siteverify")
            .form(&params)
            .send()
            .await
            .map_err(|e| {
                StackhouseError::Internal(anyhow::anyhow!("reCAPTCHA verify failed: {}", e))
            })?
            .json()
            .await
            .map_err(|e| {
                StackhouseError::Internal(anyhow::anyhow!("reCAPTCHA parse failed: {}", e))
            })?;

        if !response.success {
            warn!("reCAPTCHA verification failed: {:?}", response.error_codes);
            return Err(StackhouseError::InvalidPayload(
                "Captcha verification failed".to_string(),
            ));
        }

        // For v3, check score
        if let (CaptchaProvider::ReCaptchaV3, Some(score)) = (&self.config.provider, response.score)
        {
            if score < self.config.min_score {
                warn!(
                    "reCAPTCHA v3 score too low: {} < {}",
                    score, self.config.min_score
                );
                return Err(StackhouseError::InvalidPayload(
                    "Captcha score too low".to_string(),
                ));
            }
        }

        Ok(())
    }

    async fn verify_turnstile(&self, token: &str, remote_ip: Option<&str>) -> StackhouseResult<()> {
        let mut params = vec![
            ("secret", self.config.secret_key.as_str()),
            ("response", token),
        ];
        let ip_str;
        if let Some(ip) = remote_ip {
            ip_str = ip.to_string();
            params.push(("remoteip", &ip_str));
        }

        let response: TurnstileResponse = self
            .http_client
            .post("https://challenges.cloudflare.com/turnstile/v0/siteverify")
            .form(&params)
            .send()
            .await
            .map_err(|e| {
                StackhouseError::Internal(anyhow::anyhow!("Turnstile verify failed: {}", e))
            })?
            .json()
            .await
            .map_err(|e| {
                StackhouseError::Internal(anyhow::anyhow!("Turnstile parse failed: {}", e))
            })?;

        if !response.success {
            warn!("Turnstile verification failed: {:?}", response.error_codes);
            return Err(StackhouseError::InvalidPayload(
                "Captcha verification failed".to_string(),
            ));
        }

        Ok(())
    }
}

// ============================================================================
// Handlers & Router
// ============================================================================

#[derive(Clone)]
pub struct CaptchaState {
    pub captcha: Arc<CaptchaService>,
}

/// GET /v1/auth/captcha - Get captcha configuration
async fn captcha_info_handler(
    State(state): State<CaptchaState>,
) -> impl axum::response::IntoResponse {
    axum::Json(serde_json::json!({
        "success": true,
        "data": state.captcha.get_info()
    }))
}

pub fn create_captcha_router(state: CaptchaState) -> axum::Router {
    axum::Router::new()
        .route("/captcha", axum::routing::get(captcha_info_handler))
        .with_state(state)
}
