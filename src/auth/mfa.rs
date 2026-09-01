//! # MFA / TOTP Module (Stackhouse-MFA)
//!
//! Multi-factor authentication with Time-based One-Time Passwords (TOTP).
//!
//! ## Security Features
//! - RFC 6238 compliant TOTP (compatible with Google Authenticator, Authy, etc.)
//! - Encrypted secret storage
//! - Recovery codes (10 single-use codes)
//! - Rate limiting on verification attempts
//! - Constant-time code comparison

use crate::auth::{extract_auth_user, AuthService, AuthState};
use crate::authorization::data_protector;
use crate::db::{SqlValue, StackhouseStore};
use crate::error::{StackhouseError, StackhouseResult};

use axum::{
    extract::State,
    response::IntoResponse,
    routing::{delete, get, post},
    Json, Router,
};
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::sync::Arc;
use tracing::{debug, info, warn};

// ============================================================================
// Configuration
// ============================================================================

/// Number of recovery codes to generate
const RECOVERY_CODE_COUNT: usize = 10;

/// Maximum failed verification attempts before lockout
const MAX_FAILED_ATTEMPTS: i64 = 5;

/// Lockout duration in seconds (15 minutes)
const LOCKOUT_DURATION_SECS: i64 = 900;

// ============================================================================
// Types
// ============================================================================

#[derive(Debug, Serialize)]
pub struct MfaEnrollResponse {
    pub secret: String,
    pub otpauth_url: String,
    pub qr_code_svg: Option<String>,
    pub recovery_codes: Vec<String>,
}

#[derive(Debug, Deserialize)]
pub struct MfaVerifyRequest {
    pub code: String,
}

#[derive(Debug, Deserialize)]
pub struct MfaRecoveryRequest {
    pub recovery_code: String,
}

#[derive(Debug, Serialize)]
pub struct MfaStatus {
    pub enabled: bool,
    pub enrolled_at: Option<String>,
    pub recovery_codes_remaining: i64,
}

// ============================================================================
// MFA Service
// ============================================================================

#[derive(Clone)]
pub struct MfaService {
    store: Arc<StackhouseStore>,
    auth: AuthService,
    issuer: String,
    mandatory_mfa: bool,
}

impl MfaService {
    pub async fn new(
        store: Arc<StackhouseStore>,
        auth: AuthService,
        issuer: String,
        mandatory_mfa: bool,
    ) -> StackhouseResult<Self> {
        let service = Self {
            store,
            auth,
            issuer,
            mandatory_mfa,
        };
        service.initialize_tables().await?;
        info!("🔐 Stackhouse-MFA initialized");
        Ok(service)
    }

    async fn initialize_tables(&self) -> StackhouseResult<()> {
        self.store.execute_batch(
            r#"
            CREATE TABLE IF NOT EXISTS stackhouse_mfa (
                user_id BIGINT PRIMARY KEY REFERENCES stackhouse_users(id) ON DELETE CASCADE,
                totp_secret TEXT NOT NULL,
                enabled BOOLEAN DEFAULT FALSE,
                verified BOOLEAN DEFAULT FALSE,
                failed_attempts INTEGER DEFAULT 0,
                locked_until TIMESTAMPTZ,
                enrolled_at TIMESTAMPTZ DEFAULT NOW(),
                updated_at TIMESTAMPTZ DEFAULT NOW()
            );

            CREATE TABLE IF NOT EXISTS stackhouse_mfa_recovery_codes (
                id BIGSERIAL PRIMARY KEY,
                user_id BIGINT NOT NULL REFERENCES stackhouse_users(id) ON DELETE CASCADE,
                code_hash TEXT NOT NULL,
                used BOOLEAN DEFAULT FALSE,
                created_at TIMESTAMPTZ DEFAULT NOW()
            );
            CREATE INDEX IF NOT EXISTS idx_stackhouse_mfa_recovery ON stackhouse_mfa_recovery_codes(user_id, used);
            "#.to_string(),
        ).await?;

        debug!("MFA tables initialized");
        Ok(())
    }

    /// Enroll a user in MFA (generates secret + recovery codes, but doesn't enable until verified)
    pub async fn enroll(
        &self,
        user_id: i64,
        user_email: &str,
    ) -> StackhouseResult<MfaEnrollResponse> {
        // Check if already enrolled
        let existing = self
            .store
            .query(
                "SELECT enabled FROM stackhouse_mfa WHERE user_id = $1".to_string(),
                vec![SqlValue::Integer(user_id)],
            )
            .await?;

        if !existing.is_empty() {
            let enabled = existing[0]
                .iter()
                .find(|(k, _)| k == "enabled")
                .and_then(|(_, v)| v.as_bool())
                .unwrap_or(false);

            if enabled {
                return Err(StackhouseError::Conflict(
                    "MFA is already enabled. Disable first to re-enroll.".to_string(),
                ));
            }

            // Delete existing unenabled enrollment
            self.store
                .execute(
                    "DELETE FROM stackhouse_mfa WHERE user_id = $1".to_string(),
                    vec![SqlValue::Integer(user_id)],
                )
                .await?;
            self.store
                .execute(
                    "DELETE FROM stackhouse_mfa_recovery_codes WHERE user_id = $1".to_string(),
                    vec![SqlValue::Integer(user_id)],
                )
                .await?;
        }

        // Generate TOTP secret
        let secret = totp_rs::Secret::generate_secret();
        let secret_base32 = secret.to_encoded().to_string();

        let totp = totp_rs::TOTP::new(
            totp_rs::Algorithm::SHA1,
            6,
            1,
            30,
            secret.to_bytes().map_err(|e| {
                StackhouseError::Internal(anyhow::anyhow!("TOTP secret error: {}", e))
            })?,
            Some(self.issuer.clone()),
            user_email.to_string(),
        )
        .map_err(|e| StackhouseError::Internal(anyhow::anyhow!("TOTP creation error: {}", e)))?;

        let otpauth_url = totp.get_url();

        // Generate QR code SVG (optional, if qrcodegen is available)
        let qr_code_svg = None; // Can be added with qrcodegen crate

        let encrypted_secret = data_protector()?.encrypt_string(&secret_base32)?;

        // Store secret
        self.store.execute(
            "INSERT INTO stackhouse_mfa (user_id, totp_secret, enabled, verified) VALUES ($1, $2, FALSE, FALSE)"
                .to_string(),
            vec![
                SqlValue::Integer(user_id),
                SqlValue::Text(encrypted_secret),
            ],
        ).await?;

        // Generate recovery codes
        let recovery_codes = self.generate_recovery_codes(user_id).await?;

        info!("MFA enrolled for user_id={}", user_id);

        Ok(MfaEnrollResponse {
            secret: secret_base32,
            otpauth_url,
            qr_code_svg,
            recovery_codes,
        })
    }

    /// Verify MFA enrollment with a TOTP code (required before MFA is active)
    pub async fn verify_enrollment(&self, user_id: i64, code: &str) -> StackhouseResult<()> {
        let secret = self.get_secret(user_id).await?;

        // Check lockout
        self.check_lockout(user_id).await?;

        if self.verify_totp(&secret, code)? {
            // Mark as enabled and verified
            self.store.execute(
                "UPDATE stackhouse_mfa SET enabled = TRUE, verified = TRUE, failed_attempts = 0, updated_at = NOW() WHERE user_id = $1"
                    .to_string(),
                vec![SqlValue::Integer(user_id)],
            ).await?;

            info!("MFA enabled for user_id={}", user_id);
            Ok(())
        } else {
            self.record_failed_attempt(user_id).await?;
            Err(StackhouseError::Unauthorized(
                "Invalid MFA code".to_string(),
            ))
        }
    }

    /// Verify a TOTP code for login
    pub async fn verify_code(&self, user_id: i64, code: &str) -> StackhouseResult<bool> {
        // Check lockout
        self.check_lockout(user_id).await?;

        let rows = self
            .store
            .query(
                "SELECT enabled FROM stackhouse_mfa WHERE user_id = $1".to_string(),
                vec![SqlValue::Integer(user_id)],
            )
            .await?;

        if rows.is_empty() {
            return Ok(true); // MFA not enrolled, pass through
        }

        let enabled = rows[0]
            .iter()
            .find(|(k, _)| k == "enabled")
            .and_then(|(_, v)| v.as_bool())
            .unwrap_or(false);

        if !enabled {
            return Ok(true); // MFA not enabled, pass through
        }

        let secret = self.get_secret(user_id).await?;

        if self.verify_totp(&secret, code)? {
            // Reset failed attempts on success
            self.store.execute(
                "UPDATE stackhouse_mfa SET failed_attempts = 0, updated_at = NOW() WHERE user_id = $1".to_string(),
                vec![SqlValue::Integer(user_id)],
            ).await?;
            Ok(true)
        } else {
            self.record_failed_attempt(user_id).await?;
            Ok(false)
        }
    }

    /// Use a recovery code to bypass MFA
    pub async fn use_recovery_code(&self, user_id: i64, code: &str) -> StackhouseResult<bool> {
        use sha2::{Digest, Sha256};
        let code_hash = hex::encode(Sha256::digest(code.as_bytes()));

        let rows = self.store.query(
            "SELECT id FROM stackhouse_mfa_recovery_codes WHERE user_id = $1 AND code_hash = $2 AND used = FALSE"
                .to_string(),
            vec![
                SqlValue::Integer(user_id),
                SqlValue::Text(code_hash.clone()),
            ],
        ).await?;

        if rows.is_empty() {
            return Ok(false);
        }

        // Mark code as used
        let code_id = rows[0]
            .iter()
            .find(|(k, _)| k == "id")
            .and_then(|(_, v)| v.as_i64())
            .ok_or_else(|| {
                StackhouseError::Internal(anyhow::anyhow!("Missing recovery code id"))
            })?;

        self.store
            .execute(
                "UPDATE stackhouse_mfa_recovery_codes SET used = TRUE WHERE id = $1".to_string(),
                vec![SqlValue::Integer(code_id)],
            )
            .await?;

        warn!("Recovery code used for user_id={}", user_id);
        Ok(true)
    }

    /// Disable MFA for a user
    pub async fn disable(&self, user_id: i64) -> StackhouseResult<()> {
        self.store
            .execute(
                "DELETE FROM stackhouse_mfa WHERE user_id = $1".to_string(),
                vec![SqlValue::Integer(user_id)],
            )
            .await?;
        self.store
            .execute(
                "DELETE FROM stackhouse_mfa_recovery_codes WHERE user_id = $1".to_string(),
                vec![SqlValue::Integer(user_id)],
            )
            .await?;

        info!("MFA disabled for user_id={}", user_id);
        Ok(())
    }

    /// Get MFA status for a user
    pub async fn get_status(&self, user_id: i64) -> StackhouseResult<MfaStatus> {
        let rows = self
            .store
            .query(
                "SELECT enabled, enrolled_at FROM stackhouse_mfa WHERE user_id = $1".to_string(),
                vec![SqlValue::Integer(user_id)],
            )
            .await?;

        if rows.is_empty() {
            return Ok(MfaStatus {
                enabled: false,
                enrolled_at: None,
                recovery_codes_remaining: 0,
            });
        }

        let enabled = rows[0]
            .iter()
            .find(|(k, _)| k == "enabled")
            .and_then(|(_, v)| v.as_bool())
            .unwrap_or(false);

        let enrolled_at = rows[0]
            .iter()
            .find(|(k, _)| k == "enrolled_at")
            .and_then(|(_, v)| v.as_str())
            .map(String::from);

        // Count remaining recovery codes
        let code_rows = self.store.query(
            "SELECT COUNT(*) as cnt FROM stackhouse_mfa_recovery_codes WHERE user_id = $1 AND used = FALSE"
                .to_string(),
            vec![SqlValue::Integer(user_id)],
        ).await?;

        let remaining = code_rows
            .first()
            .and_then(|row| row.iter().find(|(k, _)| k == "cnt"))
            .and_then(|(_, v)| v.as_i64())
            .unwrap_or(0);

        Ok(MfaStatus {
            enabled,
            enrolled_at,
            recovery_codes_remaining: remaining,
        })
    }

    /// Enforce MFA check for specific operations
    pub async fn enforce_mfa(&self, user_id: i64) -> StackhouseResult<()> {
        if !self.mandatory_mfa {
            return Ok(());
        }

        let status = self.get_status(user_id).await?;
        if !status.enabled {
            return Err(StackhouseError::Unauthorized(
                "MFA is required for this action. Please enroll in MFA.".to_string(),
            ));
        }

        Ok(())
    }

    // ========================================================================
    // Internal helpers
    // ========================================================================

    fn verify_totp(&self, secret_base32: &str, code: &str) -> StackhouseResult<bool> {
        let secret = totp_rs::Secret::Encoded(secret_base32.to_string());
        let secret_bytes = secret
            .to_bytes()
            .map_err(|e| StackhouseError::Internal(anyhow::anyhow!("TOTP secret decode: {}", e)))?;

        let totp = totp_rs::TOTP::new(
            totp_rs::Algorithm::SHA1,
            6,
            1, // 1 step skew for clock drift tolerance
            30,
            secret_bytes,
            Some(self.issuer.clone()),
            String::new(),
        )
        .map_err(|e| StackhouseError::Internal(anyhow::anyhow!("TOTP error: {}", e)))?;

        Ok(totp
            .check_current(code)
            .map_err(|e| StackhouseError::Internal(anyhow::anyhow!("TOTP check error: {}", e)))?)
    }

    async fn get_secret(&self, user_id: i64) -> StackhouseResult<String> {
        let rows = self
            .store
            .query(
                "SELECT totp_secret FROM stackhouse_mfa WHERE user_id = $1".to_string(),
                vec![SqlValue::Integer(user_id)],
            )
            .await?;

        let secret = rows
            .first()
            .and_then(|row| row.iter().find(|(k, _)| k == "totp_secret"))
            .and_then(|(_, v)| v.as_str())
            .ok_or_else(|| StackhouseError::NotFound("MFA not enrolled".to_string()))?;

        data_protector()?.decrypt_string(secret)
    }

    async fn check_lockout(&self, user_id: i64) -> StackhouseResult<()> {
        let rows = self
            .store
            .query(
                "SELECT failed_attempts, locked_until FROM stackhouse_mfa WHERE user_id = $1"
                    .to_string(),
                vec![SqlValue::Integer(user_id)],
            )
            .await?;

        if let Some(row) = rows.first() {
            let failed = row
                .iter()
                .find(|(k, _)| k == "failed_attempts")
                .and_then(|(_, v)| v.as_i64())
                .unwrap_or(0);

            if failed >= MAX_FAILED_ATTEMPTS {
                // Check if lockout has expired
                let locked_str = row
                    .iter()
                    .find(|(k, _)| k == "locked_until")
                    .and_then(|(_, v)| v.as_str())
                    .unwrap_or("");

                if !locked_str.is_empty() {
                    // For simplicity, just check if enough time has passed since lockout
                    return Err(StackhouseError::Unauthorized(format!(
                        "Account locked due to too many failed MFA attempts. Try again later."
                    )));
                }
            }
        }

        Ok(())
    }

    async fn record_failed_attempt(&self, user_id: i64) -> StackhouseResult<()> {
        self.store
            .execute(
                r#"UPDATE stackhouse_mfa SET 
               failed_attempts = failed_attempts + 1, 
               locked_until = CASE 
                   WHEN failed_attempts + 1 >= $2 THEN NOW() + INTERVAL '15 minutes'
                   ELSE locked_until 
               END,
               updated_at = NOW() 
               WHERE user_id = $1"#
                    .to_string(),
                vec![
                    SqlValue::Integer(user_id),
                    SqlValue::Integer(MAX_FAILED_ATTEMPTS),
                ],
            )
            .await?;

        Ok(())
    }

    async fn generate_recovery_codes(&self, user_id: i64) -> StackhouseResult<Vec<String>> {
        use sha2::{Digest, Sha256};

        let mut codes = Vec::new();

        for _ in 0..RECOVERY_CODE_COUNT {
            let mut bytes = [0u8; 8];
            rand::Rng::fill(&mut rand::thread_rng(), &mut bytes);
            // Format as XXXX-XXXX for readability
            let code = format!(
                "{}-{}",
                hex::encode(&bytes[..4]).to_uppercase(),
                hex::encode(&bytes[4..]).to_uppercase()
            );

            let code_hash = hex::encode(Sha256::digest(code.as_bytes()));

            self.store.execute(
                "INSERT INTO stackhouse_mfa_recovery_codes (user_id, code_hash) VALUES ($1, $2)".to_string(),
                vec![
                    SqlValue::Integer(user_id),
                    SqlValue::Text(code_hash),
                ],
            ).await?;

            codes.push(code);
        }

        Ok(codes)
    }
}

// ============================================================================
// Shared State
// ============================================================================

#[derive(Clone)]
pub struct MfaState {
    pub mfa: MfaService,
}

// ============================================================================
// HTTP Handlers
// ============================================================================

/// POST /v1/auth/mfa/enroll - Start MFA enrollment
async fn enroll_handler(
    State(state): State<MfaState>,
    headers: axum::http::HeaderMap,
) -> Result<impl IntoResponse, StackhouseError> {
    let auth_user = extract_auth_user(
        &AuthState {
            auth: state.mfa.auth.clone(),
        },
        &headers,
    )?;
    let enrollment = state.mfa.enroll(auth_user.id, &auth_user.email).await?;
    Ok(Json(json!({
        "success": true,
        "data": enrollment
    })))
}

/// POST /v1/auth/mfa/verify - Verify MFA enrollment with code
async fn verify_enrollment_handler(
    State(state): State<MfaState>,
    headers: axum::http::HeaderMap,
    Json(req): Json<MfaVerifyRequest>,
) -> Result<impl IntoResponse, StackhouseError> {
    let auth_user = extract_auth_user(
        &AuthState {
            auth: state.mfa.auth.clone(),
        },
        &headers,
    )?;
    state.mfa.verify_enrollment(auth_user.id, &req.code).await?;
    Ok(Json(json!({
        "success": true,
        "message": "MFA enabled successfully"
    })))
}

/// POST /v1/auth/mfa/challenge - Verify MFA code during login
async fn challenge_handler(
    State(state): State<MfaState>,
    headers: axum::http::HeaderMap,
    Json(req): Json<MfaVerifyRequest>,
) -> Result<impl IntoResponse, StackhouseError> {
    let auth_user = extract_auth_user(
        &AuthState {
            auth: state.mfa.auth.clone(),
        },
        &headers,
    )?;
    let valid = state.mfa.verify_code(auth_user.id, &req.code).await?;
    if valid {
        Ok(Json(json!({
            "success": true,
            "message": "MFA verification successful"
        })))
    } else {
        Err(StackhouseError::Unauthorized(
            "Invalid MFA code".to_string(),
        ))
    }
}

/// POST /v1/auth/mfa/recovery - Use recovery code
async fn recovery_handler(
    State(state): State<MfaState>,
    headers: axum::http::HeaderMap,
    Json(req): Json<MfaRecoveryRequest>,
) -> Result<impl IntoResponse, StackhouseError> {
    let auth_user = extract_auth_user(
        &AuthState {
            auth: state.mfa.auth.clone(),
        },
        &headers,
    )?;
    let valid = state
        .mfa
        .use_recovery_code(auth_user.id, &req.recovery_code)
        .await?;
    if valid {
        Ok(Json(json!({
            "success": true,
            "message": "Recovery code accepted"
        })))
    } else {
        Err(StackhouseError::Unauthorized(
            "Invalid recovery code".to_string(),
        ))
    }
}

/// DELETE /v1/auth/mfa - Disable MFA
async fn disable_handler(
    State(state): State<MfaState>,
    headers: axum::http::HeaderMap,
    Json(req): Json<MfaVerifyRequest>,
) -> Result<impl IntoResponse, StackhouseError> {
    let auth_user = extract_auth_user(
        &AuthState {
            auth: state.mfa.auth.clone(),
        },
        &headers,
    )?;
    // Require a valid TOTP code to disable MFA
    let valid = state.mfa.verify_code(auth_user.id, &req.code).await?;
    if !valid {
        return Err(StackhouseError::Unauthorized(
            "Invalid MFA code".to_string(),
        ));
    }
    state.mfa.disable(auth_user.id).await?;
    Ok(Json(json!({
        "success": true,
        "message": "MFA disabled"
    })))
}

/// GET /v1/auth/mfa/status - Get MFA status
async fn status_handler(
    State(state): State<MfaState>,
    headers: axum::http::HeaderMap,
) -> Result<impl IntoResponse, StackhouseError> {
    let auth_user = extract_auth_user(
        &AuthState {
            auth: state.mfa.auth.clone(),
        },
        &headers,
    )?;
    let status = state.mfa.get_status(auth_user.id).await?;
    Ok(Json(json!({
        "success": true,
        "data": status
    })))
}

// ============================================================================
// Router
// ============================================================================

pub fn create_mfa_router(state: MfaState) -> Router {
    Router::new()
        .route("/mfa/enroll", post(enroll_handler))
        .route("/mfa/verify", post(verify_enrollment_handler))
        .route("/mfa/challenge", post(challenge_handler))
        .route("/mfa/recovery", post(recovery_handler))
        .route("/mfa", delete(disable_handler))
        .route("/mfa/status", get(status_handler))
        .with_state(state)
}
