//! # Authentication Module (Stackhouse-Auth)
//!
//! Provides JWT-based authentication for Stackhouse, similar to Supabase Auth.
//!
//! ## Features
//! - User signup/login with email and password
//! - Argon2id password hashing
//! - JWT access tokens (short-lived) and refresh tokens (long-lived)
//! - Session management with token refresh
//!
//! ## System Tables
//! - `stackhouse_users` - Stores user credentials and metadata
//! - `stackhouse_sessions` - Tracks active refresh tokens

pub mod api_keys;
pub mod captcha;
pub mod device_trust;
pub mod impersonation;
pub mod jwt_sessions;
pub mod magic_link;
pub mod mfa;
pub mod oauth;
pub mod phone_otp;
pub mod rbac;
pub mod saml;
pub mod webauthn;

pub use api_keys::*;
pub use captcha::*;
pub use device_trust::*;
pub use impersonation::*;
pub use jwt_sessions::*;
pub use magic_link::*;
pub use mfa::*;
pub use oauth::*;
pub use phone_otp::*;
pub use rbac::*;
pub use saml::*;
pub use webauthn::*;

use crate::db::{SqlValue, StackhouseStore};
use crate::error::{StackhouseError, StackhouseResult};

use argon2::{
    password_hash::{rand_core::OsRng, PasswordHash, PasswordHasher, PasswordVerifier, SaltString},
    Argon2,
};
use axum::{
    extract::State,
    http::{header::AUTHORIZATION, StatusCode},
    response::IntoResponse,
    routing::{get, post, put},
    Json, Router,
};
use jsonwebtoken::{decode, encode, DecodingKey, EncodingKey, Header, Validation};
use rand::Rng;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use tracing::{debug, info};
use uuid::Uuid;

// ============================================================================
// Configuration Constants
// ============================================================================

/// Default access token expiry (1 hour)
const DEFAULT_ACCESS_TOKEN_DURATION: Duration = Duration::from_secs(3600);

/// Default refresh token expiry (7 days)
const DEFAULT_REFRESH_TOKEN_DURATION: Duration = Duration::from_secs(7 * 24 * 3600);

/// Minimum password length
const MIN_PASSWORD_LENGTH: usize = 8;

// ============================================================================
// Core Types
// ============================================================================

/// Authentication service managing users and sessions
#[derive(Clone)]
pub struct AuthService {
    store: Arc<StackhouseStore>,
    jwt_secret: Vec<u8>,
    access_token_duration: Duration,
    refresh_token_duration: Duration,
}

/// User data returned from authentication endpoints
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct User {
    pub id: i64,
    pub email: String,
    pub created_at: String,
    pub updated_at: String,
    #[serde(default)]
    pub metadata: Value,
}

/// Token pair returned after successful authentication
#[derive(Debug, Serialize)]
pub struct AuthTokens {
    pub access_token: String,
    pub refresh_token: String,
    pub expires_in: i64,
    pub token_type: String,
    pub user: User,
}

/// JWT Claims structure
#[derive(Debug, Serialize, Deserialize)]
pub struct Claims {
    /// Subject (user ID)
    pub sub: i64,
    /// User email
    pub email: String,
    /// JWT ID (unique token identifier for blacklisting)
    pub jti: String,
    /// Expiration time (Unix timestamp)
    pub exp: u64,
    /// Issued at time (Unix timestamp)
    pub iat: u64,
}

/// Authenticated user extracted from request headers
#[derive(Debug, Clone)]
pub struct AuthUser {
    pub id: i64,
    pub email: String,
}

// ============================================================================
// Request/Response DTOs
// ============================================================================

#[derive(Debug, Deserialize)]
pub struct SignupRequest {
    pub email: String,
    pub password: String,
    #[serde(default)]
    pub metadata: Option<Value>,
}

#[derive(Debug, Deserialize)]
pub struct LoginRequest {
    pub email: String,
    pub password: String,
}

#[derive(Debug, Deserialize)]
pub struct RefreshRequest {
    pub refresh_token: String,
}

#[derive(Debug, Deserialize)]
pub struct UpdateUserRequest {
    #[serde(default)]
    pub metadata: Option<Value>,
}

#[derive(Debug, Deserialize)]
pub struct ChangePasswordRequest {
    pub current_password: String,
    pub new_password: String,
}

fn strip_reserved_metadata_flags(metadata: Option<Value>) -> Option<Value> {
    metadata.map(|metadata| match metadata {
        Value::Object(mut map) => {
            map.remove("service_admin");
            Value::Object(map)
        }
        other => other,
    })
}

// ============================================================================
// AuthService Implementation
// ============================================================================

impl AuthService {
    /// Creates a new AuthService with the given store and JWT secret
    pub async fn new(store: Arc<StackhouseStore>, jwt_secret: Vec<u8>) -> StackhouseResult<Self> {
        let service = Self {
            store,
            jwt_secret,
            access_token_duration: DEFAULT_ACCESS_TOKEN_DURATION,
            refresh_token_duration: DEFAULT_REFRESH_TOKEN_DURATION,
        };

        // Initialize auth tables
        service.initialize_tables().await?;

        info!("🔐 Stackhouse-Auth initialized");
        Ok(service)
    }

    /// Initialize authentication tables
    async fn initialize_tables(&self) -> StackhouseResult<()> {
        // Create users table
        self.store
            .execute_batch(
                r#"
            CREATE TABLE IF NOT EXISTS stackhouse_users (
                id SERIAL PRIMARY KEY,
                email TEXT UNIQUE NOT NULL,
                password_hash TEXT NOT NULL,
                metadata TEXT DEFAULT '{}',
                created_at TIMESTAMP WITH TIME ZONE DEFAULT CURRENT_TIMESTAMP,
                updated_at TIMESTAMP WITH TIME ZONE DEFAULT CURRENT_TIMESTAMP
            );
            CREATE INDEX IF NOT EXISTS idx_stackhouse_users_email ON stackhouse_users(email);
            "#
                .to_string(),
            )
            .await?;

        // Create sessions table for refresh tokens
        self.store.execute_batch(
            r#"
            CREATE TABLE IF NOT EXISTS stackhouse_sessions (
                id SERIAL PRIMARY KEY,
                user_id INTEGER NOT NULL,
                refresh_token TEXT UNIQUE NOT NULL,
                expires_at TIMESTAMP WITH TIME ZONE NOT NULL,
                created_at TIMESTAMP WITH TIME ZONE DEFAULT CURRENT_TIMESTAMP,
                FOREIGN KEY (user_id) REFERENCES stackhouse_users(id) ON DELETE CASCADE
            );
            CREATE INDEX IF NOT EXISTS idx_stackhouse_sessions_token ON stackhouse_sessions(refresh_token);
            CREATE INDEX IF NOT EXISTS idx_stackhouse_sessions_user ON stackhouse_sessions(user_id);
            "#
            .to_string(),
        ).await?;

        // Create token blacklist table for revoked access tokens
        self.store.execute_batch(
            r#"
            CREATE TABLE IF NOT EXISTS stackhouse_token_blacklist (
                jti TEXT PRIMARY KEY,
                user_id INTEGER NOT NULL,
                expires_at TIMESTAMP WITH TIME ZONE NOT NULL,
                created_at TIMESTAMP WITH TIME ZONE DEFAULT CURRENT_TIMESTAMP
            );
            CREATE INDEX IF NOT EXISTS idx_stackhouse_token_blacklist_jti ON stackhouse_token_blacklist(jti);
            CREATE INDEX IF NOT EXISTS idx_stackhouse_token_blacklist_expires ON stackhouse_token_blacklist(expires_at);
            "#
            .to_string(),
        ).await?;

        debug!("Auth tables initialized");
        Ok(())
    }

    /// Generate a secure random JWT secret
    pub fn generate_secret() -> Vec<u8> {
        let mut secret = vec![0u8; 64];
        rand::thread_rng().fill(&mut secret[..]);
        secret
    }

    /// Hash a password using Argon2id
    pub(crate) fn hash_password(&self, password: &str) -> StackhouseResult<String> {
        let salt = SaltString::generate(&mut OsRng);
        let argon2 = Argon2::default();

        argon2
            .hash_password(password.as_bytes(), &salt)
            .map(|hash| hash.to_string())
            .map_err(|e| {
                StackhouseError::Internal(anyhow::anyhow!("Password hashing failed: {}", e))
            })
    }

    /// Verify a password against its hash
    fn verify_password(&self, password: &str, hash: &str) -> StackhouseResult<bool> {
        let parsed_hash = PasswordHash::new(hash).map_err(|e| {
            StackhouseError::Internal(anyhow::anyhow!("Invalid password hash: {}", e))
        })?;

        Ok(Argon2::default()
            .verify_password(password.as_bytes(), &parsed_hash)
            .is_ok())
    }

    /// Generate a JWT access token
    fn generate_access_token(&self, user: &User) -> StackhouseResult<String> {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(|e| StackhouseError::Internal(anyhow::anyhow!("Time error: {}", e)))?;

        let claims = Claims {
            sub: user.id,
            email: user.email.clone(),
            jti: Uuid::new_v4().to_string(),
            iat: now.as_secs(),
            exp: (now + self.access_token_duration).as_secs(),
        };

        encode(
            &Header::default(),
            &claims,
            &EncodingKey::from_secret(&self.jwt_secret),
        )
        .map_err(|e| StackhouseError::Internal(anyhow::anyhow!("JWT encoding failed: {}", e)))
    }

    /// Generate a secure refresh token
    fn generate_refresh_token(&self) -> String {
        use base64::Engine;
        let mut bytes = [0u8; 32];
        rand::thread_rng().fill(&mut bytes);
        base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(bytes)
    }

    /// Check if a token's jti is in the blacklist
    pub async fn is_token_blacklisted(&self, jti: &str) -> StackhouseResult<bool> {
        let rows = self
            .store
            .query(
                "SELECT 1 FROM stackhouse_token_blacklist WHERE jti = ?".to_string(),
                vec![SqlValue::Text(jti.to_string())],
            )
            .await?;
        Ok(!rows.is_empty())
    }

    /// Validate a JWT access token and return claims
    pub fn validate_token(&self, token: &str) -> StackhouseResult<Claims> {
        decode::<Claims>(
            token,
            &DecodingKey::from_secret(&self.jwt_secret),
            &Validation::default(),
        )
        .map(|data| data.claims)
        .map_err(|e| StackhouseError::Unauthorized(format!("Invalid token: {}", e)))
    }

    /// Validate email format
    fn validate_email(&self, email: &str) -> StackhouseResult<()> {
        if !email.contains('@') || email.len() < 5 {
            return Err(StackhouseError::InvalidPayload(
                "Invalid email format".to_string(),
            ));
        }
        Ok(())
    }

    /// Validate password requirements
    fn validate_password(&self, password: &str) -> StackhouseResult<()> {
        if password.len() < MIN_PASSWORD_LENGTH {
            return Err(StackhouseError::InvalidPayload(format!(
                "Password must be at least {} characters",
                MIN_PASSWORD_LENGTH
            )));
        }
        Ok(())
    }

    // ========================================================================
    // User Operations
    // ========================================================================

    /// Register a new user
    pub async fn signup(&self, req: SignupRequest) -> StackhouseResult<AuthTokens> {
        // Validate input
        self.validate_email(&req.email)?;
        self.validate_password(&req.password)?;

        // Check if user already exists
        let existing = self
            .store
            .query(
                "SELECT id FROM stackhouse_users WHERE email = ?".to_string(),
                vec![SqlValue::Text(req.email.clone())],
            )
            .await?;

        if !existing.is_empty() {
            return Err(StackhouseError::Conflict("User already exists".to_string()));
        }

        // Hash password
        let password_hash = self.hash_password(&req.password)?;
        let metadata = req.metadata.unwrap_or(json!({}));

        // Insert user
        let user_id = self
            .store
            .insert_returning_id(
                "INSERT INTO stackhouse_users (email, password_hash, metadata) VALUES (?, ?, ?)"
                    .to_string(),
                vec![
                    SqlValue::Text(req.email.clone()),
                    SqlValue::Text(password_hash),
                    SqlValue::Text(metadata.to_string()),
                ],
            )
            .await?;

        info!("New user registered: {}", req.email);

        // Get the created user
        let user = self.get_user_by_id(user_id).await?;

        // Generate tokens
        self.create_session(user).await
    }

    /// Authenticate a user and return tokens
    pub async fn login(&self, req: LoginRequest) -> StackhouseResult<AuthTokens> {
        // Find user by email
        let rows = self.store.query(
            "SELECT id, email, password_hash, metadata, created_at, updated_at FROM stackhouse_users WHERE email = ?"
                .to_string(),
            vec![SqlValue::Text(req.email.clone())],
        ).await?;

        if rows.is_empty() {
            return Err(StackhouseError::Unauthorized(
                "Invalid credentials".to_string(),
            ));
        }

        let row = &rows[0];
        let password_hash = row
            .iter()
            .find(|(k, _)| k == "password_hash")
            .and_then(|(_, v)| v.as_str())
            .ok_or_else(|| StackhouseError::Internal(anyhow::anyhow!("Missing password_hash")))?;

        // Verify password
        if !self.verify_password(&req.password, password_hash)? {
            return Err(StackhouseError::Unauthorized(
                "Invalid credentials".to_string(),
            ));
        }

        let user = self.row_to_user(row)?;
        info!("User logged in: {}", user.email);

        // Generate tokens
        self.create_session(user).await
    }

    /// Create a new session with tokens
    async fn create_session(&self, user: User) -> StackhouseResult<AuthTokens> {
        let access_token = self.generate_access_token(&user)?;
        let refresh_token = self.generate_refresh_token();

        // Calculate expiry
        let expires_at = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(|e| StackhouseError::Internal(anyhow::anyhow!("Time error: {}", e)))?
            + self.refresh_token_duration;

        let expires_at_str = chrono::DateTime::from_timestamp(expires_at.as_secs() as i64, 0)
            .map(|dt| dt.format("%Y-%m-%d %H:%M:%S").to_string())
            .unwrap_or_default();

        // Store refresh token
        self.store.execute(
            "INSERT INTO stackhouse_sessions (user_id, refresh_token, expires_at) VALUES (?, ?, ?::timestamptz)"
                .to_string(),
            vec![
                SqlValue::Integer(user.id),
                SqlValue::Text(refresh_token.clone()),
                SqlValue::Text(expires_at_str),
            ],
        ).await?;

        Ok(AuthTokens {
            access_token,
            refresh_token,
            expires_in: self.access_token_duration.as_secs() as i64,
            token_type: "Bearer".to_string(),
            user,
        })
    }

    /// Public wrapper for create_session (used by OAuth and magic link modules)
    pub async fn create_session_public(&self, user: User) -> StackhouseResult<AuthTokens> {
        self.create_session(user).await
    }

    /// Refresh access token using refresh token
    pub async fn refresh(&self, req: RefreshRequest) -> StackhouseResult<AuthTokens> {
        // Find session by refresh token
        let rows = self
            .store
            .query(
                "SELECT user_id, expires_at FROM stackhouse_sessions WHERE refresh_token = ?"
                    .to_string(),
                vec![SqlValue::Text(req.refresh_token.clone())],
            )
            .await?;

        if rows.is_empty() {
            return Err(StackhouseError::Unauthorized(
                "Invalid refresh token".to_string(),
            ));
        }

        let row = &rows[0];
        let user_id = row
            .iter()
            .find(|(k, _)| k == "user_id")
            .and_then(|(_, v)| v.as_i64())
            .ok_or_else(|| StackhouseError::Internal(anyhow::anyhow!("Missing user_id")))?;

        // Delete old session
        self.store
            .execute(
                "DELETE FROM stackhouse_sessions WHERE refresh_token = ?".to_string(),
                vec![SqlValue::Text(req.refresh_token)],
            )
            .await?;

        // Get user and create new session
        let user = self.get_user_by_id(user_id).await?;
        self.create_session(user).await
    }

    /// Logout - invalidate refresh token and blacklist access token
    pub async fn logout(
        &self,
        refresh_token: &str,
        access_token_jti: Option<&str>,
        user_id: i64,
    ) -> StackhouseResult<()> {
        // Delete the refresh token session
        self.store
            .execute(
                "DELETE FROM stackhouse_sessions WHERE refresh_token = ?".to_string(),
                vec![SqlValue::Text(refresh_token.to_string())],
            )
            .await?;

        // Blacklist the access token so it can't be reused
        if let Some(jti) = access_token_jti {
            let expires_at = (SystemTime::now() + self.access_token_duration)
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default();
            self.store.execute(
                "INSERT INTO stackhouse_token_blacklist (jti, user_id, expires_at) VALUES (?, ?, to_timestamp(?)) ON CONFLICT (jti) DO NOTHING".to_string(),
                vec![
                    SqlValue::Text(jti.to_string()),
                    SqlValue::Integer(user_id),
                    SqlValue::Integer(expires_at.as_secs() as i64),
                ],
            ).await?;
        }

        Ok(())
    }

    /// List active sessions for a user
    pub async fn list_sessions(&self, user_id: i64) -> StackhouseResult<Vec<Value>> {
        let rows = self.store.query(
            "SELECT id, refresh_token, created_at, expires_at FROM stackhouse_sessions WHERE user_id = ? ORDER BY created_at DESC"
                .to_string(),
            vec![SqlValue::Integer(user_id)],
        ).await?;

        let sessions: Vec<Value> = rows.into_iter().map(|row| {
            let get = |key: &str| row.iter().find(|(k, _)| k == key).map(|(_, v)| v);
            json!({
                "id": get("id").and_then(|v| v.as_i64()).unwrap_or(0),
                "token_hash": format!("{:.8}", get("refresh_token").and_then(|v| v.as_str()).unwrap_or("")),
                "created_at": get("created_at").and_then(|v| v.as_str()).unwrap_or(""),
                "expires_at": get("expires_at").and_then(|v| v.as_str()).unwrap_or(""),
                "device": "Unknown device",
                "ip": "—",
            })
        }).collect();
        Ok(sessions)
    }

    /// Revoke a session by its internal id
    pub async fn revoke_session(&self, session_id: i64) -> StackhouseResult<()> {
        self.store
            .execute(
                "DELETE FROM stackhouse_sessions WHERE id = ?".to_string(),
                vec![SqlValue::Integer(session_id)],
            )
            .await?;
        Ok(())
    }

    /// Get user by ID
    pub async fn get_user_by_id(&self, id: i64) -> StackhouseResult<User> {
        let rows = self.store.query(
            "SELECT id, email, metadata, created_at, updated_at FROM stackhouse_users WHERE id = ?"
                .to_string(),
            vec![SqlValue::Integer(id)],
        ).await?;

        if rows.is_empty() {
            return Err(StackhouseError::NotFound("User not found".to_string()));
        }

        self.row_to_user(&rows[0])
    }

    /// Update user metadata
    pub async fn update_user(
        &self,
        user_id: i64,
        req: UpdateUserRequest,
    ) -> StackhouseResult<User> {
        if let Some(metadata) = req.metadata {
            self.store.execute(
                "UPDATE stackhouse_users SET metadata = ?, updated_at = CURRENT_TIMESTAMP WHERE id = ?"
                    .to_string(),
                vec![SqlValue::Text(metadata.to_string()), SqlValue::Integer(user_id)],
            ).await?;
        }

        self.get_user_by_id(user_id).await
    }

    /// Change user password
    pub async fn change_password(
        &self,
        user_id: i64,
        req: ChangePasswordRequest,
    ) -> StackhouseResult<()> {
        // Validate new password
        self.validate_password(&req.new_password)?;

        // Get current password hash
        let rows = self
            .store
            .query(
                "SELECT password_hash FROM stackhouse_users WHERE id = ?".to_string(),
                vec![SqlValue::Integer(user_id)],
            )
            .await?;

        if rows.is_empty() {
            return Err(StackhouseError::NotFound("User not found".to_string()));
        }

        let current_hash = rows[0]
            .iter()
            .find(|(k, _)| k == "password_hash")
            .and_then(|(_, v)| v.as_str())
            .ok_or_else(|| StackhouseError::Internal(anyhow::anyhow!("Missing password_hash")))?;

        // Verify current password
        if !self.verify_password(&req.current_password, current_hash)? {
            return Err(StackhouseError::Unauthorized(
                "Current password is incorrect".to_string(),
            ));
        }

        // Hash and save new password
        let new_hash = self.hash_password(&req.new_password)?;
        self.store.execute(
            "UPDATE stackhouse_users SET password_hash = ?, updated_at = CURRENT_TIMESTAMP WHERE id = ?"
                .to_string(),
            vec![SqlValue::Text(new_hash), SqlValue::Integer(user_id)],
        ).await?;

        info!("Password changed for user_id={}", user_id);
        Ok(())
    }

    /// Convert database row to User struct
    fn row_to_user(&self, row: &[(String, Value)]) -> StackhouseResult<User> {
        let get_str = |key: &str| -> StackhouseResult<String> {
            row.iter()
                .find(|(k, _)| k == key)
                .and_then(|(_, v)| v.as_str().map(String::from))
                .ok_or_else(|| StackhouseError::Internal(anyhow::anyhow!("Missing field: {}", key)))
        };

        let get_i64 = |key: &str| -> StackhouseResult<i64> {
            row.iter()
                .find(|(k, _)| k == key)
                .and_then(|(_, v)| v.as_i64())
                .ok_or_else(|| StackhouseError::Internal(anyhow::anyhow!("Missing field: {}", key)))
        };

        let metadata = row
            .iter()
            .find(|(k, _)| k == "metadata")
            .map(|(_, v)| match v {
                Value::String(raw) => serde_json::from_str(raw).unwrap_or_else(|_| json!({})),
                Value::Object(_) => v.clone(),
                Value::Null => json!({}),
                other => other.clone(),
            })
            .unwrap_or_else(|| json!({}));

        Ok(User {
            id: get_i64("id")?,
            email: get_str("email")?,
            created_at: get_str("created_at")?,
            updated_at: get_str("updated_at")?,
            metadata,
        })
    }
}

// ============================================================================
// Auth Middleware Extractor
// ============================================================================

/// App state that includes AuthService
#[derive(Clone)]
pub struct AuthState {
    pub auth: AuthService,
}

/// Extract and validate JWT token from Authorization header
pub fn extract_auth_user(
    auth_state: &AuthState,
    headers: &axum::http::HeaderMap,
) -> Result<AuthUser, StackhouseError> {
    let auth_header = headers
        .get(AUTHORIZATION)
        .and_then(|h| h.to_str().ok())
        .ok_or_else(|| StackhouseError::Unauthorized("Missing authorization header".to_string()))?;

    let token = auth_header
        .strip_prefix("Bearer ")
        .ok_or_else(|| StackhouseError::Unauthorized("Invalid authorization format".to_string()))?;

    let claims = auth_state.auth.validate_token(token)?;

    Ok(AuthUser {
        id: claims.sub,
        email: claims.email,
    })
}

// ============================================================================
// API Handlers
// ============================================================================

/// POST /v1/auth/signup
async fn signup_handler(
    State(state): State<AuthState>,
    Json(req): Json<SignupRequest>,
) -> Result<impl IntoResponse, StackhouseError> {
    let req = SignupRequest {
        metadata: strip_reserved_metadata_flags(req.metadata),
        ..req
    };

    let tokens = state.auth.signup(req).await?;
    Ok((
        StatusCode::CREATED,
        Json(json!({
            "success": true,
            "data": tokens
        })),
    ))
}

/// POST /v1/auth/login
async fn login_handler(
    State(state): State<AuthState>,
    Json(req): Json<LoginRequest>,
) -> Result<impl IntoResponse, StackhouseError> {
    let tokens = state.auth.login(req).await?;
    Ok(Json(json!({
        "success": true,
        "data": tokens
    })))
}

/// POST /v1/auth/refresh
async fn refresh_handler(
    State(state): State<AuthState>,
    Json(req): Json<RefreshRequest>,
) -> Result<impl IntoResponse, StackhouseError> {
    let tokens = state.auth.refresh(req).await?;
    Ok(Json(json!({
        "success": true,
        "data": tokens
    })))
}

/// POST /v1/auth/logout
async fn logout_handler(
    State(state): State<AuthState>,
    headers: axum::http::HeaderMap,
    Json(req): Json<RefreshRequest>,
) -> Result<impl IntoResponse, StackhouseError> {
    // Extract access token jti from Authorization header for blacklisting
    let (access_jti, user_id) = headers
        .get(AUTHORIZATION)
        .and_then(|h| h.to_str().ok())
        .and_then(|h| h.strip_prefix("Bearer "))
        .and_then(|token| state.auth.validate_token(token).ok())
        .map(|c| (Some(c.jti), c.sub))
        .unwrap_or((None, 0));

    state
        .auth
        .logout(&req.refresh_token, access_jti.as_deref(), user_id)
        .await?;
    Ok(Json(json!({
        "success": true,
        "message": "Logged out successfully"
    })))
}

/// GET /v1/auth/me
async fn me_handler(
    State(state): State<AuthState>,
    headers: axum::http::HeaderMap,
) -> Result<impl IntoResponse, StackhouseError> {
    let auth_user = extract_auth_user(&state, &headers)?;
    let user = state.auth.get_user_by_id(auth_user.id).await?;
    Ok(Json(json!({
        "success": true,
        "data": user
    })))
}

/// PUT /v1/auth/user
async fn update_user_handler(
    State(state): State<AuthState>,
    headers: axum::http::HeaderMap,
    Json(req): Json<UpdateUserRequest>,
) -> Result<impl IntoResponse, StackhouseError> {
    let auth_user = extract_auth_user(&state, &headers)?;
    let req = UpdateUserRequest {
        metadata: strip_reserved_metadata_flags(req.metadata),
    };
    let user = state.auth.update_user(auth_user.id, req).await?;
    Ok(Json(json!({
        "success": true,
        "data": user
    })))
}

/// POST /v1/auth/change-password
async fn change_password_handler(
    State(state): State<AuthState>,
    headers: axum::http::HeaderMap,
    Json(req): Json<ChangePasswordRequest>,
) -> Result<impl IntoResponse, StackhouseError> {
    let auth_user = extract_auth_user(&state, &headers)?;
    state.auth.change_password(auth_user.id, req).await?;
    Ok(Json(json!({
        "success": true,
        "message": "Password changed successfully"
    })))
}

/// GET /v1/auth/sessions
async fn list_sessions_handler(
    State(state): State<AuthState>,
    headers: axum::http::HeaderMap,
) -> Result<impl IntoResponse, StackhouseError> {
    let auth_user = extract_auth_user(&state, &headers)?;
    let sessions = state.auth.list_sessions(auth_user.id).await?;
    Ok(Json(json!({
        "success": true,
        "data": sessions
    })))
}

/// DELETE /v1/auth/sessions/:id
async fn revoke_session_handler(
    State(state): State<AuthState>,
    headers: axum::http::HeaderMap,
    axum::extract::Path(session_id): axum::extract::Path<i64>,
) -> Result<impl IntoResponse, StackhouseError> {
    let _auth_user = extract_auth_user(&state, &headers)?;
    state.auth.revoke_session(session_id).await?;
    Ok(Json(json!({
        "success": true,
        "message": "Session revoked"
    })))
}

// ============================================================================
// Token Blacklist Middleware
// ============================================================================

/// Middleware that rejects requests with blacklisted access tokens.
/// This should be applied to protected routes that require authentication.
pub async fn token_blacklist_middleware(
    State(state): State<AuthState>,
    req: axum::extract::Request,
    next: axum::middleware::Next,
) -> Result<impl IntoResponse, StackhouseError> {
    if let Some(auth_header) = req.headers().get(AUTHORIZATION) {
        if let Ok(header_str) = auth_header.to_str() {
            if let Some(token) = header_str.strip_prefix("Bearer ") {
                if let Ok(claims) = state.auth.validate_token(token) {
                    if state.auth.is_token_blacklisted(&claims.jti).await? {
                        return Err(StackhouseError::Unauthorized(
                            "Token has been revoked".to_string(),
                        ));
                    }
                }
            }
        }
    }
    Ok(next.run(req).await)
}

// ============================================================================
// Rate Limiter (Token Bucket)
// ============================================================================

use std::collections::HashMap;
use std::sync::Mutex;

/// Token bucket for rate limiting
struct TokenBucket {
    tokens: f64,
    last_updated: std::time::Instant,
}

/// In-memory token bucket rate limiter per client IP
#[derive(Clone)]
pub struct RateLimiter {
    buckets: std::sync::Arc<Mutex<HashMap<String, TokenBucket>>>,
    capacity: f64,
    refill_per_second: f64,
}

impl RateLimiter {
    pub fn new(capacity: f64, refill_per_second: f64) -> Self {
        Self {
            buckets: std::sync::Arc::new(Mutex::new(HashMap::new())),
            capacity,
            refill_per_second,
        }
    }

    /// Check if the request is allowed. Returns true if within rate limit.
    pub fn check(&self, key: &str) -> bool {
        let mut buckets = self.buckets.lock().unwrap();
        let now = std::time::Instant::now();
        let bucket = buckets.entry(key.to_string()).or_insert(TokenBucket {
            tokens: self.capacity,
            last_updated: now,
        });

        let elapsed = now.duration_since(bucket.last_updated).as_secs_f64();
        bucket.tokens = (bucket.tokens + elapsed * self.refill_per_second).min(self.capacity);
        bucket.last_updated = now;

        if bucket.tokens >= 1.0 {
            bucket.tokens -= 1.0;
            true
        } else {
            false
        }
    }
}

/// Global auth endpoint rate limiter: 10 burst, 1 token/sec refill
lazy_static::lazy_static! {
    static ref AUTH_RATE_LIMITER: RateLimiter = RateLimiter::new(10.0, 1.0);
}

/// Middleware that applies token bucket rate limiting to auth endpoints
pub async fn auth_rate_limit_middleware(
    req: axum::extract::Request,
    next: axum::middleware::Next,
) -> Result<impl IntoResponse, StackhouseError> {
    let client_ip = req
        .headers()
        .get("x-forwarded-for")
        .and_then(|h| h.to_str().ok())
        .and_then(|h| h.split(',').next())
        .or_else(|| req.headers().get("x-real-ip").and_then(|h| h.to_str().ok()))
        .unwrap_or("unknown")
        .trim()
        .to_string();

    if !AUTH_RATE_LIMITER.check(&client_ip) {
        return Err(StackhouseError::RateLimited(
            "Too many requests. Please try again later.".to_string(),
        ));
    }

    Ok(next.run(req).await)
}

// ============================================================================
// Router
// ============================================================================

/// Creates the auth router with all authentication endpoints
pub fn create_auth_router(auth_state: AuthState) -> Router {
    let rate_limit_layer = axum::middleware::from_fn(auth_rate_limit_middleware);

    Router::new()
        .route("/signup", post(signup_handler))
        .route("/login", post(login_handler))
        .route("/refresh", post(refresh_handler))
        .route("/logout", post(logout_handler))
        .route_layer(rate_limit_layer)
        .route("/me", get(me_handler))
        .route("/user", put(update_user_handler))
        .route("/change-password", post(change_password_handler))
        .route("/sessions", get(list_sessions_handler))
        .route(
            "/sessions/:id",
            axum::routing::delete(revoke_session_handler),
        )
        .with_state(auth_state)
}

// ============================================================================
// Additional Error Types
// ============================================================================

impl StackhouseError {
    /// Create an unauthorized error
    pub fn unauthorized(msg: impl Into<String>) -> Self {
        StackhouseError::Unauthorized(msg.into())
    }

    /// Create a conflict error
    pub fn conflict(msg: impl Into<String>) -> Self {
        StackhouseError::Conflict(msg.into())
    }

    /// Create a not found error  
    pub fn not_found(msg: impl Into<String>) -> Self {
        StackhouseError::NotFound(msg.into())
    }
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    async fn create_test_service() -> AuthService {
        let store = Arc::new(StackhouseStore::in_memory().await.unwrap());
        let secret = AuthService::generate_secret();
        AuthService::new(store, secret).await.unwrap()
    }

    #[tokio::test]
    async fn test_password_hashing() {
        let service = create_test_service().await;
        let password = "supersecret123";

        let hash = service.hash_password(password).unwrap();
        assert!(service.verify_password(password, &hash).unwrap());
        assert!(!service.verify_password("wrongpassword", &hash).unwrap());
    }

    #[tokio::test]
    async fn test_signup_flow() {
        let service = create_test_service().await;

        let tokens = service
            .signup(SignupRequest {
                email: "test@stackhouse.dev".to_string(),
                password: "password123".to_string(),
                metadata: None,
            })
            .await
            .unwrap();

        assert!(!tokens.access_token.is_empty());
        assert!(!tokens.refresh_token.is_empty());
        assert_eq!(tokens.user.email, "test@stackhouse.dev");
    }

    #[tokio::test]
    async fn test_login_flow() {
        let service = create_test_service().await;

        // First signup
        service
            .signup(SignupRequest {
                email: "test@stackhouse.dev".to_string(),
                password: "password123".to_string(),
                metadata: None,
            })
            .await
            .unwrap();

        // Then login
        let tokens = service
            .login(LoginRequest {
                email: "test@stackhouse.dev".to_string(),
                password: "password123".to_string(),
            })
            .await
            .unwrap();

        assert!(!tokens.access_token.is_empty());
    }

    #[tokio::test]
    async fn test_token_validation() {
        let service = create_test_service().await;

        let tokens = service
            .signup(SignupRequest {
                email: "test@stackhouse.dev".to_string(),
                password: "password123".to_string(),
                metadata: None,
            })
            .await
            .unwrap();

        let claims = service.validate_token(&tokens.access_token).unwrap();
        assert_eq!(claims.email, "test@stackhouse.dev");
    }

    #[tokio::test]
    async fn test_refresh_flow() {
        let service = create_test_service().await;

        let tokens = service
            .signup(SignupRequest {
                email: "test@stackhouse.dev".to_string(),
                password: "password123".to_string(),
                metadata: None,
            })
            .await
            .unwrap();

        // Wait for 1 second to ensure new token has different timestamp (iat is in seconds)
        tokio::time::sleep(std::time::Duration::from_secs(2)).await;

        let new_tokens = service
            .refresh(RefreshRequest {
                refresh_token: tokens.refresh_token,
            })
            .await
            .unwrap();

        assert!(!new_tokens.access_token.is_empty());
        assert_ne!(new_tokens.access_token, tokens.access_token);
    }

    #[tokio::test]
    async fn test_invalid_email() {
        let service = create_test_service().await;

        let result = service
            .signup(SignupRequest {
                email: "invalid".to_string(),
                password: "password123".to_string(),
                metadata: None,
            })
            .await;

        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_short_password() {
        let service = create_test_service().await;

        let result = service
            .signup(SignupRequest {
                email: "test@stackhouse.dev".to_string(),
                password: "short".to_string(),
                metadata: None,
            })
            .await;

        assert!(result.is_err());
    }
}
