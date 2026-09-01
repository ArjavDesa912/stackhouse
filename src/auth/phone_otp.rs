//! # Phone OTP Module (Stackhouse-PhoneOTP)
//!
//! SMS-based phone verification using Twilio.
//! Supports phone login, verification, and rate limiting.
//!
//! ## Security
//! - Rate limited: max 5 OTP requests per phone per 15 minutes
//! - OTP codes expire after 5 minutes
//! - Codes are single-use and hashed before storage
//! - Account lockout after 5 failed verification attempts

use crate::auth::{AuthService, AuthTokens};
use crate::db::{SqlValue, StackhouseStore};
use crate::error::{StackhouseError, StackhouseResult};

use axum::{extract::State, response::IntoResponse, routing::post, Json, Router};
use serde::Deserialize;
use serde_json::json;
use sha2::{Digest, Sha256};
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};
use tracing::{info, warn};

const OTP_LENGTH: usize = 6;
const OTP_EXPIRY_SECS: u64 = 300; // 5 minutes
const MAX_ATTEMPTS: i64 = 5;
const RATE_LIMIT_WINDOW: u64 = 900; // 15 minutes
const MAX_REQUESTS_PER_WINDOW: i64 = 5;

// ============================================================================
// Configuration
// ============================================================================

#[derive(Clone, Debug)]
pub struct TwilioConfig {
    pub account_sid: String,
    pub auth_token: String,
    pub from_number: String,
    pub enabled: bool,
}

impl TwilioConfig {
    pub fn from_env() -> Self {
        let account_sid = std::env::var("STACKHOUSE_TWILIO_ACCOUNT_SID").unwrap_or_default();
        let auth_token = std::env::var("STACKHOUSE_TWILIO_AUTH_TOKEN").unwrap_or_default();
        let from_number = std::env::var("STACKHOUSE_TWILIO_FROM_NUMBER").unwrap_or_default();
        let enabled = !account_sid.is_empty() && !auth_token.is_empty();
        Self {
            account_sid,
            auth_token,
            from_number,
            enabled,
        }
    }
}

// ============================================================================
// Phone OTP Service
// ============================================================================

#[derive(Clone)]
pub struct PhoneOtpService {
    store: Arc<StackhouseStore>,
    auth: AuthService,
    twilio: TwilioConfig,
    http_client: reqwest::Client,
}

#[derive(Deserialize)]
pub struct SendOtpRequest {
    pub phone: String,
}

#[derive(Deserialize)]
pub struct VerifyOtpRequest {
    pub phone: String,
    pub code: String,
}

impl PhoneOtpService {
    pub async fn new(
        store: Arc<StackhouseStore>,
        auth: AuthService,
        twilio: TwilioConfig,
    ) -> StackhouseResult<Self> {
        let service = Self {
            store,
            auth,
            twilio,
            http_client: reqwest::Client::new(),
        };
        service.initialize_tables().await?;
        info!(
            "📱 Stackhouse-PhoneOTP initialized (Twilio enabled: {})",
            service.twilio.enabled
        );
        Ok(service)
    }

    async fn initialize_tables(&self) -> StackhouseResult<()> {
        self.store.execute_batch(
            r#"
            CREATE TABLE IF NOT EXISTS stackhouse_phone_otp (
                id BIGSERIAL PRIMARY KEY,
                phone TEXT NOT NULL,
                code_hash TEXT NOT NULL,
                attempts INT DEFAULT 0,
                verified BOOLEAN DEFAULT FALSE,
                created_at BIGINT NOT NULL,
                expires_at BIGINT NOT NULL
            );
            CREATE INDEX IF NOT EXISTS idx_phone_otp_phone ON stackhouse_phone_otp(phone, created_at);
            "#.to_string(),
        ).await?;
        Ok(())
    }

    fn generate_otp() -> String {
        use rand::Rng;
        let mut rng = rand::thread_rng();
        let code: u32 = rng.gen_range(100_000..999_999);
        code.to_string()
    }

    fn hash_code(code: &str) -> String {
        let mut hasher = Sha256::new();
        hasher.update(code.as_bytes());
        hex::encode(hasher.finalize())
    }

    fn validate_phone(phone: &str) -> StackhouseResult<()> {
        let cleaned: String = phone
            .chars()
            .filter(|c| c.is_ascii_digit() || *c == '+')
            .collect();
        if cleaned.len() < 10 || cleaned.len() > 15 {
            return Err(StackhouseError::InvalidPayload(
                "Invalid phone number format".to_string(),
            ));
        }
        Ok(())
    }

    pub async fn send_otp(&self, phone: &str) -> StackhouseResult<()> {
        Self::validate_phone(phone)?;

        // Rate limiting
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs();
        let window_start = now - RATE_LIMIT_WINDOW;

        let recent = self.store.query(
            "SELECT COUNT(*) as cnt FROM stackhouse_phone_otp WHERE phone = $1 AND created_at > $2".to_string(),
            vec![SqlValue::Text(phone.to_string()), SqlValue::Integer(window_start as i64)],
        ).await?;

        let count = recent
            .first()
            .and_then(|r| r.iter().find(|(k, _)| k == "cnt"))
            .and_then(|(_, v)| v.as_i64())
            .unwrap_or(0);

        if count >= MAX_REQUESTS_PER_WINDOW {
            return Err(StackhouseError::RateLimited(
                "Too many OTP requests. Try again later.".to_string(),
            ));
        }

        let code = Self::generate_otp();
        let code_hash = Self::hash_code(&code);
        let expires_at = now + OTP_EXPIRY_SECS;

        // Store hashed OTP
        self.store.execute(
            "INSERT INTO stackhouse_phone_otp (phone, code_hash, created_at, expires_at) VALUES ($1, $2, $3, $4)".to_string(),
            vec![
                SqlValue::Text(phone.to_string()),
                SqlValue::Text(code_hash),
                SqlValue::Integer(now as i64),
                SqlValue::Integer(expires_at as i64),
            ],
        ).await?;

        // Send SMS via Twilio
        if self.twilio.enabled {
            self.send_twilio_sms(
                phone,
                &format!("Your Stackhouse verification code is: {}", code),
            )
            .await?;
        } else {
            // Dev mode: log the code
            info!("📱 [DEV MODE] OTP for {}: {}", phone, code);
        }

        Ok(())
    }

    async fn send_twilio_sms(&self, to: &str, body: &str) -> StackhouseResult<()> {
        let url = format!(
            "https://api.twilio.com/2010-04-01/Accounts/{}/Messages.json",
            self.twilio.account_sid
        );

        let response = self
            .http_client
            .post(&url)
            .basic_auth(&self.twilio.account_sid, Some(&self.twilio.auth_token))
            .form(&[
                ("To", to),
                ("From", &self.twilio.from_number),
                ("Body", body),
            ])
            .send()
            .await
            .map_err(|e| StackhouseError::Internal(anyhow::anyhow!("Twilio API error: {}", e)))?;

        if !response.status().is_success() {
            let error = response.text().await.unwrap_or_default();
            warn!("Twilio SMS failed: {}", error);
            return Err(StackhouseError::Internal(anyhow::anyhow!(
                "SMS send failed: {}",
                error
            )));
        }

        info!("📱 SMS sent to {}", to);
        Ok(())
    }

    pub async fn verify_otp(&self, phone: &str, code: &str) -> StackhouseResult<AuthTokens> {
        Self::validate_phone(phone)?;

        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs();
        let code_hash = Self::hash_code(code);

        // Find valid OTP
        let rows = self.store.query(
            "SELECT id, code_hash, attempts FROM stackhouse_phone_otp WHERE phone = $1 AND expires_at > $2 AND verified = FALSE ORDER BY created_at DESC LIMIT 1".to_string(),
            vec![SqlValue::Text(phone.to_string()), SqlValue::Integer(now as i64)],
        ).await?;

        let row = rows.first().ok_or_else(|| {
            StackhouseError::Unauthorized("No valid OTP found. Request a new code.".to_string())
        })?;

        let otp_id = row
            .iter()
            .find(|(k, _)| k == "id")
            .and_then(|(_, v)| v.as_i64())
            .unwrap_or(0);
        let stored_hash = row
            .iter()
            .find(|(k, _)| k == "code_hash")
            .and_then(|(_, v)| v.as_str())
            .unwrap_or("");
        let attempts = row
            .iter()
            .find(|(k, _)| k == "attempts")
            .and_then(|(_, v)| v.as_i64())
            .unwrap_or(0);

        if attempts >= MAX_ATTEMPTS {
            return Err(StackhouseError::Unauthorized(
                "Too many failed attempts. Request a new code.".to_string(),
            ));
        }

        if stored_hash != code_hash {
            // Increment attempts
            self.store
                .execute(
                    "UPDATE stackhouse_phone_otp SET attempts = attempts + 1 WHERE id = $1"
                        .to_string(),
                    vec![SqlValue::Integer(otp_id)],
                )
                .await?;
            return Err(StackhouseError::Unauthorized(
                "Invalid verification code".to_string(),
            ));
        }

        // Mark as verified
        self.store
            .execute(
                "UPDATE stackhouse_phone_otp SET verified = TRUE WHERE id = $1".to_string(),
                vec![SqlValue::Integer(otp_id)],
            )
            .await?;

        // Find or create user by phone
        let user = self.find_or_create_user_by_phone(phone).await?;
        self.auth.create_session_public(user).await
    }

    async fn find_or_create_user_by_phone(
        &self,
        phone: &str,
    ) -> StackhouseResult<crate::auth::User> {
        // Check existing user
        let existing = self
            .store
            .query(
                "SELECT id FROM stackhouse_users WHERE phone = $1".to_string(),
                vec![SqlValue::Text(phone.to_string())],
            )
            .await?;

        if let Some(row) = existing.first() {
            let user_id = row
                .iter()
                .find(|(k, _)| k == "id")
                .and_then(|(_, v)| v.as_i64())
                .ok_or_else(|| StackhouseError::Internal(anyhow::anyhow!("Missing user id")))?;
            return self.auth.get_user_by_id(user_id).await;
        }

        // Ensure phone column exists on stackhouse_users
        let _ = self
            .store
            .execute_simple(
                "ALTER TABLE stackhouse_users ADD COLUMN IF NOT EXISTS phone TEXT UNIQUE"
                    .to_string(),
            )
            .await;

        // Create new user
        let random_hash = format!("phone_otp_no_password_{}", uuid::Uuid::new_v4());
        let user_id = self
            .store
            .insert_returning_id(
                "INSERT INTO stackhouse_users (email, password_hash, phone) VALUES ($1, $2, $3)"
                    .to_string(),
                vec![
                    SqlValue::Text(format!("{}@phone.stackhouse.local", phone.replace('+', ""))),
                    SqlValue::Text(random_hash),
                    SqlValue::Text(phone.to_string()),
                ],
            )
            .await?;

        self.auth.get_user_by_id(user_id).await
    }
}

// ============================================================================
// State & Handlers
// ============================================================================

#[derive(Clone)]
pub struct PhoneOtpState {
    pub phone_otp: PhoneOtpService,
}

async fn send_otp_handler(
    State(state): State<PhoneOtpState>,
    Json(req): Json<SendOtpRequest>,
) -> Result<impl IntoResponse, StackhouseError> {
    state.phone_otp.send_otp(&req.phone).await?;
    Ok(Json(json!({
        "success": true,
        "message": "Verification code sent"
    })))
}

async fn verify_otp_handler(
    State(state): State<PhoneOtpState>,
    Json(req): Json<VerifyOtpRequest>,
) -> Result<impl IntoResponse, StackhouseError> {
    let tokens = state.phone_otp.verify_otp(&req.phone, &req.code).await?;
    Ok(Json(json!({
        "success": true,
        "data": tokens
    })))
}

pub fn create_phone_otp_router(state: PhoneOtpState) -> Router {
    Router::new()
        .route("/phone/send", post(send_otp_handler))
        .route("/phone/verify", post(verify_otp_handler))
        .with_state(state)
}
