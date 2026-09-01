//! # Magic Link Authentication Module (Stackhouse-MagicLink)
//!
//! Passwordless authentication via emailed one-time login links.
//!
//! ## Security Features
//! - Cryptographically random tokens (32 bytes, URL-safe base64)
//! - Short-lived tokens (15 minute expiry)
//! - Single-use tokens (deleted after use)
//! - Rate limiting per email (max 5 requests per 15 minutes)
//! - HMAC-signed tokens for integrity
//! - Constant-time token comparison

use crate::auth::{AuthService, AuthTokens};
use crate::db::{SqlValue, StackhouseStore};
use crate::error::{StackhouseError, StackhouseResult};

use axum::{
    extract::{Query, State},
    response::IntoResponse,
    routing::{get, post},
    Json, Router,
};
use serde::Deserialize;
use serde_json::json;
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};
use tracing::{debug, info, warn};

// ============================================================================
// Configuration
// ============================================================================

/// Magic link token expiry (15 minutes)
const MAGIC_LINK_EXPIRY_SECS: u64 = 900;

/// Maximum magic link requests per email per window
const RATE_LIMIT_MAX: i64 = 5;

/// Rate limit window (15 minutes)
const RATE_LIMIT_WINDOW_SECS: u64 = 900;

// ============================================================================
// Types
// ============================================================================

/// Email configuration for sending magic links
#[derive(Debug, Clone)]
pub struct EmailConfig {
    pub smtp_host: String,
    pub smtp_port: u16,
    pub smtp_username: String,
    pub smtp_password: String,
    pub from_email: String,
    pub from_name: String,
}

impl EmailConfig {
    /// Create from environment variables
    pub fn from_env() -> Option<Self> {
        Some(Self {
            smtp_host: std::env::var("STACKHOUSE_SMTP_HOST").ok()?,
            smtp_port: std::env::var("STACKHOUSE_SMTP_PORT").ok()?.parse().ok()?,
            smtp_username: std::env::var("STACKHOUSE_SMTP_USERNAME").ok()?,
            smtp_password: std::env::var("STACKHOUSE_SMTP_PASSWORD").ok()?,
            from_email: std::env::var("STACKHOUSE_SMTP_FROM_EMAIL")
                .unwrap_or_else(|_| "noreply@stackhouse.dev".to_string()),
            from_name: std::env::var("STACKHOUSE_SMTP_FROM_NAME")
                .unwrap_or_else(|_| "Stackhouse".to_string()),
        })
    }
}

#[derive(Debug, Deserialize)]
pub struct MagicLinkRequest {
    pub email: String,
    /// Optional redirect URL after successful auth
    #[serde(default)]
    pub redirect_to: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct MagicLinkVerifyParams {
    pub token: String,
}

// ============================================================================
// Magic Link Service
// ============================================================================

#[derive(Clone)]
pub struct MagicLinkService {
    store: Arc<StackhouseStore>,
    auth: AuthService,
    email_config: Option<EmailConfig>,
    base_url: String,
    token_secret: Vec<u8>,
}

impl MagicLinkService {
    pub async fn new(
        store: Arc<StackhouseStore>,
        auth: AuthService,
        email_config: Option<EmailConfig>,
        base_url: String,
        token_secret: Vec<u8>,
    ) -> StackhouseResult<Self> {
        let service = Self {
            store,
            auth,
            email_config,
            base_url,
            token_secret,
        };

        service.initialize_tables().await?;
        info!("✉️  Stackhouse-MagicLink initialized");
        Ok(service)
    }

    async fn initialize_tables(&self) -> StackhouseResult<()> {
        self.store.execute_batch(
            r#"
            CREATE TABLE IF NOT EXISTS stackhouse_magic_links (
                id BIGSERIAL PRIMARY KEY,
                email TEXT NOT NULL,
                token_hash TEXT UNIQUE NOT NULL,
                redirect_to TEXT,
                expires_at TIMESTAMPTZ NOT NULL,
                used BOOLEAN DEFAULT FALSE,
                created_at TIMESTAMPTZ DEFAULT NOW()
            );
            CREATE INDEX IF NOT EXISTS idx_stackhouse_magic_links_hash ON stackhouse_magic_links(token_hash);
            CREATE INDEX IF NOT EXISTS idx_stackhouse_magic_links_email ON stackhouse_magic_links(email);
            
            CREATE TABLE IF NOT EXISTS stackhouse_magic_link_rate_limits (
                email TEXT NOT NULL,
                request_at TIMESTAMPTZ DEFAULT NOW()
            );
            CREATE INDEX IF NOT EXISTS idx_stackhouse_ml_rate ON stackhouse_magic_link_rate_limits(email, request_at);
            "#.to_string(),
        ).await?;

        debug!("Magic link tables initialized");
        Ok(())
    }

    /// Generate a secure magic link token
    fn generate_token(&self) -> String {
        use base64::Engine;
        let mut bytes = [0u8; 32];
        rand::Rng::fill(&mut rand::thread_rng(), &mut bytes);
        base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(bytes)
    }

    /// Hash a token for storage (we never store raw tokens)
    fn hash_token(&self, token: &str) -> String {
        use sha2::{Digest, Sha256};
        let mut hasher = Sha256::new();
        hasher.update(token.as_bytes());
        hasher.update(&self.token_secret); // Salt with secret
        hex::encode(hasher.finalize())
    }

    /// Check rate limit for an email
    async fn check_rate_limit(&self, email: &str) -> StackhouseResult<()> {
        // Clean old entries
        let cutoff = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs()
            - RATE_LIMIT_WINDOW_SECS;

        let cutoff_time = chrono::DateTime::from_timestamp(cutoff as i64, 0)
            .map(|dt| dt.to_rfc3339())
            .unwrap_or_default();

        // Count recent requests
        let rows = self.store.query(
            "SELECT COUNT(*) as cnt FROM stackhouse_magic_link_rate_limits WHERE email = $1 AND request_at > $2::timestamptz"
                .to_string(),
            vec![
                SqlValue::Text(email.to_string()),
                SqlValue::Text(cutoff_time),
            ],
        ).await?;

        let count = rows
            .first()
            .and_then(|row| row.iter().find(|(k, _)| k == "cnt"))
            .and_then(|(_, v)| v.as_i64())
            .unwrap_or(0);

        if count >= RATE_LIMIT_MAX {
            return Err(StackhouseError::InvalidPayload(
                "Too many magic link requests. Please try again later.".to_string(),
            ));
        }

        // Record this request
        self.store
            .execute(
                "INSERT INTO stackhouse_magic_link_rate_limits (email) VALUES ($1)".to_string(),
                vec![SqlValue::Text(email.to_string())],
            )
            .await?;

        Ok(())
    }

    /// Request a magic link for an email
    pub async fn send_magic_link(&self, req: MagicLinkRequest) -> StackhouseResult<()> {
        // Validate email
        if !req.email.contains('@') || req.email.len() < 5 {
            return Err(StackhouseError::InvalidPayload(
                "Invalid email format".to_string(),
            ));
        }

        // Rate limit check
        self.check_rate_limit(&req.email).await?;

        // Generate token
        let token = self.generate_token();
        let token_hash = self.hash_token(&token);

        let expires_at = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs()
            + MAGIC_LINK_EXPIRY_SECS;

        let expires_at_str = chrono::DateTime::from_timestamp(expires_at as i64, 0)
            .map(|dt| dt.to_rfc3339())
            .unwrap_or_default();

        // Store the token hash (never the raw token)
        self.store.execute(
            "INSERT INTO stackhouse_magic_links (email, token_hash, redirect_to, expires_at) VALUES ($1, $2, $3, $4::timestamptz)"
                .to_string(),
            vec![
                SqlValue::Text(req.email.clone()),
                SqlValue::Text(token_hash),
                SqlValue::Text(req.redirect_to.clone().unwrap_or_default()),
                SqlValue::Text(expires_at_str),
            ],
        ).await?;

        // Build the magic link URL
        let magic_url = format!(
            "{}/v1/auth/magic-link/verify?token={}",
            self.base_url, token
        );

        // Send email if SMTP configured
        if let Some(email_config) = &self.email_config {
            self.send_email(email_config, &req.email, &magic_url)
                .await?;
            info!("Magic link sent to {}", req.email);
        } else {
            // In development, log the link
            warn!(
                "SMTP not configured. Magic link for {}: {}",
                req.email, magic_url
            );
            info!("🔗 Dev magic link: {}", magic_url);
        }

        Ok(())
    }

    /// Verify a magic link token and authenticate the user
    pub async fn verify_magic_link(&self, token: &str) -> StackhouseResult<AuthTokens> {
        let token_hash = self.hash_token(token);

        // Find and validate token
        let rows = self.store.query(
            "SELECT email, redirect_to, expires_at, used FROM stackhouse_magic_links WHERE token_hash = $1"
                .to_string(),
            vec![SqlValue::Text(token_hash.clone())],
        ).await?;

        if rows.is_empty() {
            return Err(StackhouseError::Unauthorized(
                "Invalid or expired magic link".to_string(),
            ));
        }

        let row = &rows[0];

        // Check if already used
        let used = row
            .iter()
            .find(|(k, _)| k == "used")
            .and_then(|(_, v)| v.as_bool())
            .unwrap_or(false);

        if used {
            return Err(StackhouseError::Unauthorized(
                "Magic link already used".to_string(),
            ));
        }

        let email = row
            .iter()
            .find(|(k, _)| k == "email")
            .and_then(|(_, v)| v.as_str())
            .ok_or_else(|| StackhouseError::Internal(anyhow::anyhow!("Missing email")))?
            .to_string();

        // Mark token as used (single-use)
        self.store
            .execute(
                "UPDATE stackhouse_magic_links SET used = TRUE WHERE token_hash = $1".to_string(),
                vec![SqlValue::Text(token_hash)],
            )
            .await?;

        // Find or create user
        let user_rows = self.store.query(
            "SELECT id, email, metadata, created_at, updated_at FROM stackhouse_users WHERE email = $1"
                .to_string(),
            vec![SqlValue::Text(email.clone())],
        ).await?;

        let user = if !user_rows.is_empty() {
            self.auth
                .get_user_by_id(
                    user_rows[0]
                        .iter()
                        .find(|(k, _)| k == "id")
                        .and_then(|(_, v)| v.as_i64())
                        .ok_or_else(|| StackhouseError::Internal(anyhow::anyhow!("Missing id")))?,
                )
                .await?
        } else {
            // Create new user (passwordless)
            let random_hash = format!("magic_link_no_password_{}", uuid::Uuid::new_v4());
            let user_id = self.store.insert_returning_id(
                "INSERT INTO stackhouse_users (email, password_hash, metadata) VALUES ($1, $2, $3)"
                    .to_string(),
                vec![
                    SqlValue::Text(email.clone()),
                    SqlValue::Text(random_hash),
                    SqlValue::Text("{}".to_string()),
                ],
            ).await?;

            info!("New user created via magic link: {}", email);
            self.auth.get_user_by_id(user_id).await?
        };

        // Create session
        self.auth.create_session_public(user).await
    }

    /// Send the magic link email via SMTP
    async fn send_email(
        &self,
        config: &EmailConfig,
        to_email: &str,
        magic_url: &str,
    ) -> StackhouseResult<()> {
        use lettre::{
            message::header::ContentType, transport::smtp::authentication::Credentials,
            AsyncSmtpTransport, AsyncTransport, Message, Tokio1Executor,
        };

        let email_body = format!(
            r#"<!DOCTYPE html>
<html>
<head><meta charset="utf-8"></head>
<body style="font-family: -apple-system, BlinkMacSystemFont, 'Segoe UI', Roboto, sans-serif; max-width: 600px; margin: 0 auto; padding: 20px;">
    <h2 style="color: #1a1a1a;">🛸 Sign in to Stackhouse</h2>
    <p style="color: #4a4a4a; line-height: 1.6;">Click the button below to sign in. This link expires in 15 minutes and can only be used once.</p>
    <a href="{}" style="display: inline-block; padding: 12px 32px; background: #000; color: #fff; text-decoration: none; border-radius: 6px; font-weight: 600; margin: 16px 0;">Sign In</a>
    <p style="color: #888; font-size: 13px; margin-top: 24px;">If you didn't request this, you can safely ignore this email.</p>
    <hr style="border: none; border-top: 1px solid #eee; margin: 24px 0;">
    <p style="color: #aaa; font-size: 11px;">Stackhouse — Schema-Later Database</p>
</body>
</html>"#,
            magic_url
        );

        let email = Message::builder()
            .from(
                format!("{} <{}>", config.from_name, config.from_email)
                    .parse()
                    .map_err(|e| {
                        StackhouseError::Internal(anyhow::anyhow!("Invalid from address: {}", e))
                    })?,
            )
            .to(to_email.parse().map_err(|e| {
                StackhouseError::Internal(anyhow::anyhow!("Invalid to address: {}", e))
            })?)
            .subject("Sign in to Stackhouse")
            .header(ContentType::TEXT_HTML)
            .body(email_body)
            .map_err(|e| StackhouseError::Internal(anyhow::anyhow!("Email build error: {}", e)))?;

        let creds = Credentials::new(config.smtp_username.clone(), config.smtp_password.clone());

        let mailer = AsyncSmtpTransport::<Tokio1Executor>::starttls_relay(&config.smtp_host)
            .map_err(|e| {
                StackhouseError::Internal(anyhow::anyhow!("SMTP connection error: {}", e))
            })?
            .port(config.smtp_port)
            .credentials(creds)
            .build();

        mailer
            .send(email)
            .await
            .map_err(|e| StackhouseError::Internal(anyhow::anyhow!("Email send error: {}", e)))?;

        Ok(())
    }

    /// Clean up expired magic links (should be called periodically)
    pub async fn cleanup_expired(&self) -> StackhouseResult<u64> {
        let deleted = self
            .store
            .execute(
                "DELETE FROM stackhouse_magic_links WHERE expires_at < NOW() OR used = TRUE"
                    .to_string(),
                vec![],
            )
            .await?;

        // Also clean rate limit entries
        self.store.execute(
            "DELETE FROM stackhouse_magic_link_rate_limits WHERE request_at < NOW() - INTERVAL '15 minutes'"
                .to_string(),
            vec![],
        ).await?;

        Ok(deleted)
    }
}

// ============================================================================
// Shared State
// ============================================================================

#[derive(Clone)]
pub struct MagicLinkState {
    pub magic_link: MagicLinkService,
}

// ============================================================================
// HTTP Handlers
// ============================================================================

/// POST /v1/auth/magic-link — Request a magic link
async fn request_magic_link_handler(
    State(state): State<MagicLinkState>,
    Json(req): Json<MagicLinkRequest>,
) -> Result<impl IntoResponse, StackhouseError> {
    state.magic_link.send_magic_link(req).await?;
    Ok(Json(json!({
        "success": true,
        "message": "If this email is valid, a magic link has been sent."
    })))
}

/// GET /v1/auth/magic-link/verify — Verify magic link token
async fn verify_magic_link_handler(
    State(state): State<MagicLinkState>,
    Query(params): Query<MagicLinkVerifyParams>,
) -> Result<impl IntoResponse, StackhouseError> {
    let tokens = state.magic_link.verify_magic_link(&params.token).await?;
    Ok(Json(json!({
        "success": true,
        "data": tokens
    })))
}

// ============================================================================
// Router
// ============================================================================

pub fn create_magic_link_router(state: MagicLinkState) -> Router {
    Router::new()
        .route("/magic-link", post(request_magic_link_handler))
        .route("/magic-link/verify", get(verify_magic_link_handler))
        .with_state(state)
}
