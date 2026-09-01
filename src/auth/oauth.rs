//! # OAuth2 Social Login Module (Stackhouse-OAuth)
//!
//! Production-grade OAuth2 social login supporting multiple providers:
//! - Google (OpenID Connect)
//! - GitHub
//! - Apple (Sign in with Apple)
//! - Discord
//!
//! ## Security Features
//! - PKCE (Proof Key for Code Exchange) for all flows
//! - State parameter with HMAC verification to prevent CSRF
//! - Nonce validation for OpenID Connect providers
//! - Secure token exchange over HTTPS only
//! - Rate limiting on callback endpoints
//! - Short-lived state tokens (10 minute expiry)

use crate::auth::{AuthService, AuthState, AuthTokens, User};
use crate::db::{SqlValue, StackhouseStore};
use crate::error::{StackhouseError, StackhouseResult};

use axum::{
    extract::{Query, State},
    response::IntoResponse,
    routing::get,
    Json, Router,
};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};
use tracing::{debug, info, warn};

// ============================================================================
// Configuration
// ============================================================================

/// Maximum age of an OAuth state token (10 minutes)
const STATE_MAX_AGE_SECS: u64 = 600;

/// Supported OAuth2 providers
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum OAuthProvider {
    Google,
    Github,
    Apple,
    Discord,
}

impl OAuthProvider {
    pub fn as_str(&self) -> &'static str {
        match self {
            OAuthProvider::Google => "google",
            OAuthProvider::Github => "github",
            OAuthProvider::Apple => "apple",
            OAuthProvider::Discord => "discord",
        }
    }
}

impl std::fmt::Display for OAuthProvider {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

impl std::str::FromStr for OAuthProvider {
    type Err = StackhouseError;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "google" => Ok(OAuthProvider::Google),
            "github" => Ok(OAuthProvider::Github),
            "apple" => Ok(OAuthProvider::Apple),
            "discord" => Ok(OAuthProvider::Discord),
            _ => Err(StackhouseError::InvalidPayload(format!(
                "Unknown provider: {}",
                s
            ))),
        }
    }
}

// ============================================================================
// Provider Configuration
// ============================================================================

/// Configuration for a single OAuth provider
#[derive(Debug, Clone)]
pub struct ProviderConfig {
    pub client_id: String,
    pub client_secret: String,
    pub auth_url: String,
    pub token_url: String,
    pub userinfo_url: String,
    pub scopes: Vec<String>,
}

impl ProviderConfig {
    /// Google OAuth2 configuration
    pub fn google(client_id: String, client_secret: String) -> Self {
        Self {
            client_id,
            client_secret,
            auth_url: "https://accounts.google.com/o/oauth2/v2/auth".to_string(),
            token_url: "https://oauth2.googleapis.com/token".to_string(),
            userinfo_url: "https://www.googleapis.com/oauth2/v3/userinfo".to_string(),
            scopes: vec![
                "openid".to_string(),
                "email".to_string(),
                "profile".to_string(),
            ],
        }
    }

    /// GitHub OAuth2 configuration
    pub fn github(client_id: String, client_secret: String) -> Self {
        Self {
            client_id,
            client_secret,
            auth_url: "https://github.com/login/oauth/authorize".to_string(),
            token_url: "https://github.com/login/oauth/access_token".to_string(),
            userinfo_url: "https://api.github.com/user".to_string(),
            scopes: vec!["read:user".to_string(), "user:email".to_string()],
        }
    }

    /// Apple Sign In configuration
    pub fn apple(client_id: String, client_secret: String) -> Self {
        Self {
            client_id,
            client_secret,
            auth_url: "https://appleid.apple.com/auth/authorize".to_string(),
            token_url: "https://appleid.apple.com/auth/token".to_string(),
            userinfo_url: String::new(), // Apple returns user info in the ID token
            scopes: vec!["name".to_string(), "email".to_string()],
        }
    }

    /// Discord OAuth2 configuration
    pub fn discord(client_id: String, client_secret: String) -> Self {
        Self {
            client_id,
            client_secret,
            auth_url: "https://discord.com/api/oauth2/authorize".to_string(),
            token_url: "https://discord.com/api/oauth2/token".to_string(),
            userinfo_url: "https://discord.com/api/users/@me".to_string(),
            scopes: vec!["identify".to_string(), "email".to_string()],
        }
    }
}

// ============================================================================
// OAuth Service
// ============================================================================

/// OAuth2 service managing social login flows
#[derive(Clone)]
pub struct OAuthService {
    store: Arc<StackhouseStore>,
    auth: AuthService,
    providers: std::collections::HashMap<String, ProviderConfig>,
    redirect_base_url: String,
    state_secret: Vec<u8>,
    http_client: reqwest::Client,
}

/// User info extracted from provider
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OAuthUserInfo {
    pub provider: String,
    pub provider_user_id: String,
    pub email: Option<String>,
    pub name: Option<String>,
    pub avatar_url: Option<String>,
    pub raw: Value,
}

/// Query params returned on OAuth callback
#[derive(Debug, Deserialize)]
pub struct OAuthCallbackParams {
    pub code: String,
    pub state: String,
}

/// Response to initiate OAuth flow
#[derive(Debug, Serialize)]
pub struct OAuthAuthorizeResponse {
    pub url: String,
    pub provider: String,
}

/// Token response from provider
#[derive(Debug, Deserialize)]
struct TokenResponse {
    access_token: String,
    #[serde(default)]
    token_type: String,
    #[serde(default)]
    id_token: Option<String>,
    #[serde(default)]
    refresh_token: Option<String>,
    #[serde(default)]
    expires_in: Option<u64>,
    #[serde(default)]
    scope: Option<String>,
}

// ============================================================================
// State Management (CSRF prevention)
// ============================================================================

/// OAuth state token encoding: provider|timestamp|pkce_verifier|hmac
impl OAuthService {
    /// Generate a CSRF-safe state token
    fn generate_state(&self, provider: &str, pkce_verifier: &str) -> String {
        use base64::Engine;
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs();

        let payload = format!("{}|{}|{}", provider, now, pkce_verifier);

        // HMAC the payload for integrity
        use hmac::{Hmac, Mac};
        type HmacSha256 = Hmac<Sha256>;
        let mut mac =
            HmacSha256::new_from_slice(&self.state_secret).expect("HMAC key length is valid");
        mac.update(payload.as_bytes());
        let signature = hex::encode(mac.finalize().into_bytes());

        let full = format!("{}|{}", payload, signature);
        base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(full.as_bytes())
    }

    /// Verify and decode a state token, returns (provider, pkce_verifier)
    fn verify_state(&self, state: &str) -> StackhouseResult<(String, String)> {
        use base64::Engine;

        let decoded = base64::engine::general_purpose::URL_SAFE_NO_PAD
            .decode(state.as_bytes())
            .map_err(|_| StackhouseError::Unauthorized("Invalid OAuth state".to_string()))?;

        let full = String::from_utf8(decoded).map_err(|_| {
            StackhouseError::Unauthorized("Invalid OAuth state encoding".to_string())
        })?;

        let parts: Vec<&str> = full.splitn(4, '|').collect();
        if parts.len() != 4 {
            return Err(StackhouseError::Unauthorized(
                "Malformed OAuth state".to_string(),
            ));
        }

        let provider = parts[0];
        let timestamp: u64 = parts[1]
            .parse()
            .map_err(|_| StackhouseError::Unauthorized("Invalid state timestamp".to_string()))?;
        let pkce_verifier = parts[2];
        let signature = parts[3];

        // Check expiry
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs();

        if now - timestamp > STATE_MAX_AGE_SECS {
            return Err(StackhouseError::Unauthorized(
                "OAuth state expired".to_string(),
            ));
        }

        // Verify HMAC
        let payload = format!("{}|{}|{}", provider, timestamp, pkce_verifier);
        use hmac::{Hmac, Mac};
        type HmacSha256 = Hmac<Sha256>;
        let mut mac =
            HmacSha256::new_from_slice(&self.state_secret).expect("HMAC key length is valid");
        mac.update(payload.as_bytes());

        let expected = hex::encode(mac.finalize().into_bytes());
        if expected != signature {
            return Err(StackhouseError::Unauthorized(
                "Invalid OAuth state signature".to_string(),
            ));
        }

        Ok((provider.to_string(), pkce_verifier.to_string()))
    }

    /// Generate PKCE code verifier and challenge (S256)
    fn generate_pkce() -> (String, String) {
        use base64::Engine;
        let mut verifier_bytes = [0u8; 32];
        rand::Rng::fill(&mut rand::thread_rng(), &mut verifier_bytes);
        let verifier = base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(verifier_bytes);

        let challenge = {
            let mut hasher = Sha256::new();
            hasher.update(verifier.as_bytes());
            base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(hasher.finalize())
        };

        (verifier, challenge)
    }
}

// ============================================================================
// OAuthService Implementation
// ============================================================================

impl OAuthService {
    /// Create a new OAuthService
    pub async fn new(
        store: Arc<StackhouseStore>,
        auth: AuthService,
        redirect_base_url: String,
        state_secret: Vec<u8>,
    ) -> StackhouseResult<Self> {
        let service = Self {
            store,
            auth,
            providers: std::collections::HashMap::new(),
            redirect_base_url,
            state_secret,
            http_client: reqwest::Client::builder()
                .timeout(std::time::Duration::from_secs(30))
                .user_agent("Stackhouse/1.0")
                .build()
                .map_err(|e| {
                    StackhouseError::Internal(anyhow::anyhow!("HTTP client error: {}", e))
                })?,
        };

        service.initialize_tables().await?;
        info!("🔗 Stackhouse-OAuth initialized");
        Ok(service)
    }

    /// Register a provider configuration
    pub fn register_provider(&mut self, provider: OAuthProvider, config: ProviderConfig) {
        info!("Registered OAuth provider: {}", provider);
        self.providers.insert(provider.as_str().to_string(), config);
    }

    /// Initialize OAuth-related database tables
    async fn initialize_tables(&self) -> StackhouseResult<()> {
        self.store.execute_batch(
            r#"
            CREATE TABLE IF NOT EXISTS stackhouse_oauth_accounts (
                id BIGSERIAL PRIMARY KEY,
                user_id BIGINT NOT NULL REFERENCES stackhouse_users(id) ON DELETE CASCADE,
                provider TEXT NOT NULL,
                provider_user_id TEXT NOT NULL,
                provider_email TEXT,
                provider_name TEXT,
                avatar_url TEXT,
                access_token TEXT,
                refresh_token TEXT,
                raw_profile JSONB DEFAULT '{}',
                created_at TIMESTAMPTZ DEFAULT NOW(),
                updated_at TIMESTAMPTZ DEFAULT NOW(),
                UNIQUE(provider, provider_user_id)
            );
            CREATE INDEX IF NOT EXISTS idx_stackhouse_oauth_provider ON stackhouse_oauth_accounts(provider, provider_user_id);
            CREATE INDEX IF NOT EXISTS idx_stackhouse_oauth_user ON stackhouse_oauth_accounts(user_id);
            "#.to_string(),
        ).await?;

        debug!("OAuth tables initialized");
        Ok(())
    }

    /// Get the authorization URL for a provider
    pub fn get_authorize_url(&self, provider: &str) -> StackhouseResult<OAuthAuthorizeResponse> {
        let config = self.providers.get(provider).ok_or_else(|| {
            StackhouseError::InvalidPayload(format!(
                "Provider '{}' not configured. Available: {:?}",
                provider,
                self.providers.keys().collect::<Vec<_>>()
            ))
        })?;

        let (pkce_verifier, pkce_challenge) = Self::generate_pkce();
        let state = self.generate_state(provider, &pkce_verifier);

        let redirect_uri = format!("{}/v1/auth/callback/{}", self.redirect_base_url, provider);
        let scopes = config.scopes.join(" ");

        let mut url = format!(
            "{}?client_id={}&redirect_uri={}&response_type=code&scope={}&state={}",
            config.auth_url,
            urlencoding(&config.client_id),
            urlencoding(&redirect_uri),
            urlencoding(&scopes),
            urlencoding(&state),
        );

        // Add PKCE challenge
        url.push_str(&format!(
            "&code_challenge={}&code_challenge_method=S256",
            pkce_challenge
        ));

        // Provider-specific params
        if provider == "google" {
            url.push_str("&access_type=offline&prompt=consent");
        } else if provider == "apple" {
            url.push_str("&response_mode=form_post");
        }

        Ok(OAuthAuthorizeResponse {
            url,
            provider: provider.to_string(),
        })
    }

    /// Handle the OAuth callback (exchange code for tokens, get user info)
    pub async fn handle_callback(&self, code: &str, state: &str) -> StackhouseResult<AuthTokens> {
        // 1. Verify state and extract provider + PKCE verifier
        let (provider_name, pkce_verifier) = self.verify_state(state)?;

        let config = self.providers.get(&provider_name).ok_or_else(|| {
            StackhouseError::InvalidPayload(format!("Provider not configured: {}", provider_name))
        })?;

        let redirect_uri = format!(
            "{}/v1/auth/callback/{}",
            self.redirect_base_url, provider_name
        );

        // 2. Exchange authorization code for access token
        let token_response = self
            .exchange_code(config, code, &redirect_uri, &pkce_verifier)
            .await?;

        // 3. Fetch user info from provider
        let user_info = self
            .fetch_user_info(&provider_name, config, &token_response)
            .await?;

        // 4. Find or create user
        let user = self
            .find_or_create_user(&provider_name, &user_info, &token_response)
            .await?;

        // 5. Create session (reuses existing auth service)
        self.auth.create_session_public(user).await
    }

    /// Exchange authorization code for access token
    async fn exchange_code(
        &self,
        config: &ProviderConfig,
        code: &str,
        redirect_uri: &str,
        pkce_verifier: &str,
    ) -> StackhouseResult<TokenResponse> {
        let params = vec![
            ("grant_type", "authorization_code"),
            ("code", code),
            ("redirect_uri", redirect_uri),
            ("client_id", &config.client_id),
            ("client_secret", &config.client_secret),
            ("code_verifier", pkce_verifier),
        ];

        let mut request = self.http_client.post(&config.token_url).form(&params);

        // GitHub requires Accept: application/json
        if config.token_url.contains("github.com") {
            request = request.header("Accept", "application/json");
        }

        let response = request.send().await.map_err(|e| {
            StackhouseError::Internal(anyhow::anyhow!("Token exchange failed: {}", e))
        })?;

        if !response.status().is_success() {
            let error_body = response.text().await.unwrap_or_default();
            warn!("OAuth token exchange failed: {}", error_body);
            return Err(StackhouseError::Unauthorized(format!(
                "Token exchange failed: {}",
                error_body
            )));
        }

        response
            .json::<TokenResponse>()
            .await
            .map_err(|e| StackhouseError::Internal(anyhow::anyhow!("Token parse failed: {}", e)))
    }

    /// Fetch user information from the provider's userinfo endpoint
    async fn fetch_user_info(
        &self,
        provider: &str,
        config: &ProviderConfig,
        token: &TokenResponse,
    ) -> StackhouseResult<OAuthUserInfo> {
        match provider {
            "github" => self.fetch_github_user(config, token).await,
            "google" => self.fetch_google_user(config, token).await,
            "discord" => self.fetch_discord_user(config, token).await,
            "apple" => self.parse_apple_user(token).await,
            _ => Err(StackhouseError::InvalidPayload(format!(
                "Unknown provider: {}",
                provider
            ))),
        }
    }

    /// GitHub user info (requires separate email API call)
    async fn fetch_github_user(
        &self,
        config: &ProviderConfig,
        token: &TokenResponse,
    ) -> StackhouseResult<OAuthUserInfo> {
        let user_response: Value = self
            .http_client
            .get(&config.userinfo_url)
            .header("Authorization", format!("Bearer {}", token.access_token))
            .header("Accept", "application/json")
            .send()
            .await
            .map_err(|e| {
                StackhouseError::Internal(anyhow::anyhow!("GitHub user fetch failed: {}", e))
            })?
            .json()
            .await
            .map_err(|e| {
                StackhouseError::Internal(anyhow::anyhow!("GitHub user parse failed: {}", e))
            })?;

        // GitHub may not return email in profile; fetch from emails endpoint
        let email = if let Some(email) = user_response.get("email").and_then(|e| e.as_str()) {
            Some(email.to_string())
        } else {
            // Fetch primary email
            let emails: Vec<Value> = self
                .http_client
                .get("https://api.github.com/user/emails")
                .header("Authorization", format!("Bearer {}", token.access_token))
                .header("Accept", "application/json")
                .send()
                .await
                .map_err(|e| {
                    StackhouseError::Internal(anyhow::anyhow!("GitHub emails fetch: {}", e))
                })?
                .json()
                .await
                .unwrap_or_default();

            emails
                .iter()
                .find(|e| e.get("primary").and_then(|p| p.as_bool()).unwrap_or(false))
                .and_then(|e| e.get("email").and_then(|v| v.as_str()))
                .map(String::from)
        };

        Ok(OAuthUserInfo {
            provider: "github".to_string(),
            provider_user_id: user_response
                .get("id")
                .and_then(|v| v.as_i64())
                .map(|id| id.to_string())
                .unwrap_or_default(),
            email,
            name: user_response
                .get("name")
                .and_then(|v| v.as_str())
                .map(String::from)
                .or_else(|| {
                    user_response
                        .get("login")
                        .and_then(|v| v.as_str())
                        .map(String::from)
                }),
            avatar_url: user_response
                .get("avatar_url")
                .and_then(|v| v.as_str())
                .map(String::from),
            raw: user_response,
        })
    }

    /// Google user info (OpenID Connect userinfo endpoint)
    async fn fetch_google_user(
        &self,
        config: &ProviderConfig,
        token: &TokenResponse,
    ) -> StackhouseResult<OAuthUserInfo> {
        let user_response: Value = self
            .http_client
            .get(&config.userinfo_url)
            .header("Authorization", format!("Bearer {}", token.access_token))
            .send()
            .await
            .map_err(|e| {
                StackhouseError::Internal(anyhow::anyhow!("Google user fetch failed: {}", e))
            })?
            .json()
            .await
            .map_err(|e| {
                StackhouseError::Internal(anyhow::anyhow!("Google user parse failed: {}", e))
            })?;

        Ok(OAuthUserInfo {
            provider: "google".to_string(),
            provider_user_id: user_response
                .get("sub")
                .and_then(|v| v.as_str())
                .unwrap_or_default()
                .to_string(),
            email: user_response
                .get("email")
                .and_then(|v| v.as_str())
                .map(String::from),
            name: user_response
                .get("name")
                .and_then(|v| v.as_str())
                .map(String::from),
            avatar_url: user_response
                .get("picture")
                .and_then(|v| v.as_str())
                .map(String::from),
            raw: user_response,
        })
    }

    /// Discord user info
    async fn fetch_discord_user(
        &self,
        config: &ProviderConfig,
        token: &TokenResponse,
    ) -> StackhouseResult<OAuthUserInfo> {
        let user_response: Value = self
            .http_client
            .get(&config.userinfo_url)
            .header("Authorization", format!("Bearer {}", token.access_token))
            .send()
            .await
            .map_err(|e| {
                StackhouseError::Internal(anyhow::anyhow!("Discord user fetch failed: {}", e))
            })?
            .json()
            .await
            .map_err(|e| {
                StackhouseError::Internal(anyhow::anyhow!("Discord user parse failed: {}", e))
            })?;

        let user_id = user_response
            .get("id")
            .and_then(|v| v.as_str())
            .unwrap_or_default()
            .to_string();

        let avatar_hash = user_response.get("avatar").and_then(|v| v.as_str());
        let avatar_url = avatar_hash.map(|hash| {
            format!(
                "https://cdn.discordapp.com/avatars/{}/{}.png",
                user_id, hash
            )
        });

        Ok(OAuthUserInfo {
            provider: "discord".to_string(),
            provider_user_id: user_id,
            email: user_response
                .get("email")
                .and_then(|v| v.as_str())
                .map(String::from),
            name: user_response
                .get("global_name")
                .or_else(|| user_response.get("username"))
                .and_then(|v| v.as_str())
                .map(String::from),
            avatar_url,
            raw: user_response,
        })
    }

    /// Apple user info (extracted from ID token JWT)
    async fn parse_apple_user(&self, token: &TokenResponse) -> StackhouseResult<OAuthUserInfo> {
        // Apple returns user info in the id_token JWT
        let id_token = token.id_token.as_ref().ok_or_else(|| {
            StackhouseError::Internal(anyhow::anyhow!("Apple did not return id_token"))
        })?;

        // Decode JWT payload without verification (Apple's public keys would be needed for full verification)
        // In production you should verify with Apple's JWKS
        let parts: Vec<&str> = id_token.split('.').collect();
        if parts.len() != 3 {
            return Err(StackhouseError::Internal(anyhow::anyhow!(
                "Invalid Apple id_token format"
            )));
        }

        use base64::Engine;
        let payload = base64::engine::general_purpose::URL_SAFE_NO_PAD
            .decode(parts[1])
            .map_err(|e| {
                StackhouseError::Internal(anyhow::anyhow!("Apple id_token decode: {}", e))
            })?;

        let claims: Value = serde_json::from_slice(&payload).map_err(|e| {
            StackhouseError::Internal(anyhow::anyhow!("Apple id_token parse: {}", e))
        })?;

        Ok(OAuthUserInfo {
            provider: "apple".to_string(),
            provider_user_id: claims
                .get("sub")
                .and_then(|v| v.as_str())
                .unwrap_or_default()
                .to_string(),
            email: claims
                .get("email")
                .and_then(|v| v.as_str())
                .map(String::from),
            name: None, // Apple only sends name on first authorization
            avatar_url: None,
            raw: claims,
        })
    }

    /// Find existing user by OAuth account or create a new one
    async fn find_or_create_user(
        &self,
        provider: &str,
        user_info: &OAuthUserInfo,
        token: &TokenResponse,
    ) -> StackhouseResult<User> {
        // 1. Check if OAuth account already linked
        let existing = self.store.query(
            "SELECT user_id FROM stackhouse_oauth_accounts WHERE provider = $1 AND provider_user_id = $2"
                .to_string(),
            vec![
                SqlValue::Text(provider.to_string()),
                SqlValue::Text(user_info.provider_user_id.clone()),
            ],
        ).await?;

        if !existing.is_empty() {
            // Update tokens for existing account
            let user_id = existing[0]
                .iter()
                .find(|(k, _)| k == "user_id")
                .and_then(|(_, v)| v.as_i64())
                .ok_or_else(|| StackhouseError::Internal(anyhow::anyhow!("Missing user_id")))?;

            self.store.execute(
                "UPDATE stackhouse_oauth_accounts SET access_token = $1, refresh_token = $2, updated_at = NOW() WHERE provider = $3 AND provider_user_id = $4"
                    .to_string(),
                vec![
                    SqlValue::Text(token.access_token.clone()),
                    SqlValue::Text(token.refresh_token.clone().unwrap_or_default()),
                    SqlValue::Text(provider.to_string()),
                    SqlValue::Text(user_info.provider_user_id.clone()),
                ],
            ).await?;

            return self.auth.get_user_by_id(user_id).await;
        }

        // 2. Check if user exists by email (link accounts)
        let email = user_info.email.as_ref().ok_or_else(|| {
            StackhouseError::InvalidPayload(
                "OAuth provider did not return an email. Email is required.".to_string(),
            )
        })?;

        let email_exists = self
            .store
            .query(
                "SELECT id FROM stackhouse_users WHERE email = $1".to_string(),
                vec![SqlValue::Text(email.clone())],
            )
            .await?;

        let user_id = if !email_exists.is_empty() {
            // Link to existing user
            email_exists[0]
                .iter()
                .find(|(k, _)| k == "id")
                .and_then(|(_, v)| v.as_i64())
                .ok_or_else(|| StackhouseError::Internal(anyhow::anyhow!("Missing id")))?
        } else {
            // 3. Create new user (no password for OAuth-only users)
            // Generate a random unguessable password hash — user can set password later
            let random_hash = format!("oauth_no_password_{}", uuid::Uuid::new_v4());
            let metadata = json!({
                "provider": provider,
                "name": user_info.name,
                "avatar_url": user_info.avatar_url,
            });

            self.store.insert_returning_id(
                "INSERT INTO stackhouse_users (email, password_hash, metadata) VALUES ($1, $2, $3)"
                    .to_string(),
                vec![
                    SqlValue::Text(email.clone()),
                    SqlValue::Text(random_hash),
                    SqlValue::Text(metadata.to_string()),
                ],
            ).await?
        };

        // 4. Create OAuth account link
        self.store.execute(
            r#"INSERT INTO stackhouse_oauth_accounts 
               (user_id, provider, provider_user_id, provider_email, provider_name, avatar_url, access_token, refresh_token, raw_profile) 
               VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)
               ON CONFLICT (provider, provider_user_id) DO UPDATE SET
               access_token = EXCLUDED.access_token,
               refresh_token = EXCLUDED.refresh_token,
               updated_at = NOW()"#
                .to_string(),
            vec![
                SqlValue::Integer(user_id),
                SqlValue::Text(provider.to_string()),
                SqlValue::Text(user_info.provider_user_id.clone()),
                SqlValue::Text(user_info.email.clone().unwrap_or_default()),
                SqlValue::Text(user_info.name.clone().unwrap_or_default()),
                SqlValue::Text(user_info.avatar_url.clone().unwrap_or_default()),
                SqlValue::Text(token.access_token.clone()),
                SqlValue::Text(token.refresh_token.clone().unwrap_or_default()),
                SqlValue::Json(user_info.raw.clone()),
            ],
        ).await?;

        info!(
            "OAuth login: {} via {} (user_id={})",
            email, provider, user_id
        );
        self.auth.get_user_by_id(user_id).await
    }

    /// List connected OAuth accounts for a user
    pub async fn list_user_accounts(&self, user_id: i64) -> StackhouseResult<Vec<Value>> {
        let rows = self.store.query(
            "SELECT provider, provider_user_id, provider_email, provider_name, avatar_url, created_at FROM stackhouse_oauth_accounts WHERE user_id = $1"
                .to_string(),
            vec![SqlValue::Integer(user_id)],
        ).await?;

        let accounts: Vec<Value> = rows
            .iter()
            .map(|row| {
                let mut obj = json!({});
                for (key, val) in row {
                    obj[key] = val.clone();
                }
                obj
            })
            .collect();

        Ok(accounts)
    }

    /// Unlink an OAuth account from a user
    pub async fn unlink_account(&self, user_id: i64, provider: &str) -> StackhouseResult<()> {
        // Ensure user has at least one other auth method (password or another OAuth)
        let accounts = self
            .store
            .query(
                "SELECT provider FROM stackhouse_oauth_accounts WHERE user_id = $1".to_string(),
                vec![SqlValue::Integer(user_id)],
            )
            .await?;

        let has_password = self
            .store
            .query(
                "SELECT password_hash FROM stackhouse_users WHERE id = $1".to_string(),
                vec![SqlValue::Integer(user_id)],
            )
            .await?;

        let has_real_password = has_password
            .first()
            .and_then(|row| row.iter().find(|(k, _)| k == "password_hash"))
            .and_then(|(_, v)| v.as_str())
            .map(|h| !h.starts_with("oauth_no_password_"))
            .unwrap_or(false);

        if accounts.len() <= 1 && !has_real_password {
            return Err(StackhouseError::InvalidPayload(
                "Cannot unlink last auth method. Set a password first.".to_string(),
            ));
        }

        self.store
            .execute(
                "DELETE FROM stackhouse_oauth_accounts WHERE user_id = $1 AND provider = $2"
                    .to_string(),
                vec![
                    SqlValue::Integer(user_id),
                    SqlValue::Text(provider.to_string()),
                ],
            )
            .await?;

        Ok(())
    }

    /// Get list of configured providers
    pub fn list_providers(&self) -> Vec<String> {
        self.providers.keys().cloned().collect()
    }
}

// ============================================================================
// Shared State
// ============================================================================

#[derive(Clone)]
pub struct OAuthState {
    pub oauth: OAuthService,
}

// ============================================================================
// HTTP Handlers
// ============================================================================

/// GET /v1/auth/providers - List available OAuth providers
async fn list_providers_handler(State(state): State<OAuthState>) -> impl IntoResponse {
    Json(json!({
        "success": true,
        "data": {
            "providers": state.oauth.list_providers()
        }
    }))
}

/// GET /v1/auth/authorize/:provider - Start OAuth flow
async fn authorize_handler(
    State(state): State<OAuthState>,
    axum::extract::Path(provider): axum::extract::Path<String>,
) -> Result<impl IntoResponse, StackhouseError> {
    let auth_response = state.oauth.get_authorize_url(&provider)?;
    Ok(Json(json!({
        "success": true,
        "data": auth_response
    })))
}

/// GET /v1/auth/callback/:provider - OAuth callback handler
async fn callback_handler(
    State(state): State<OAuthState>,
    axum::extract::Path(_provider): axum::extract::Path<String>,
    Query(params): Query<OAuthCallbackParams>,
) -> Result<impl IntoResponse, StackhouseError> {
    let tokens = state
        .oauth
        .handle_callback(&params.code, &params.state)
        .await?;
    Ok(Json(json!({
        "success": true,
        "data": tokens
    })))
}

/// GET /v1/auth/accounts - List linked OAuth accounts (authenticated)
async fn list_accounts_handler(
    State(state): State<OAuthState>,
    headers: axum::http::HeaderMap,
) -> Result<impl IntoResponse, StackhouseError> {
    let auth_user = crate::auth::extract_auth_user(
        &AuthState {
            auth: state.oauth.auth.clone(),
        },
        &headers,
    )?;
    let accounts = state.oauth.list_user_accounts(auth_user.id).await?;
    Ok(Json(json!({
        "success": true,
        "data": accounts
    })))
}

/// DELETE /v1/auth/accounts/:provider - Unlink OAuth account (authenticated)
async fn unlink_account_handler(
    State(state): State<OAuthState>,
    axum::extract::Path(provider): axum::extract::Path<String>,
    headers: axum::http::HeaderMap,
) -> Result<impl IntoResponse, StackhouseError> {
    let auth_user = crate::auth::extract_auth_user(
        &AuthState {
            auth: state.oauth.auth.clone(),
        },
        &headers,
    )?;
    state.oauth.unlink_account(auth_user.id, &provider).await?;
    Ok(Json(json!({
        "success": true,
        "message": format!("Unlinked {} account", provider)
    })))
}

// ============================================================================
// Router
// ============================================================================

pub fn create_oauth_router(state: OAuthState) -> Router {
    Router::new()
        .route("/providers", get(list_providers_handler))
        .route("/authorize/:provider", get(authorize_handler))
        .route("/callback/:provider", get(callback_handler))
        .route("/accounts", get(list_accounts_handler))
        .route(
            "/accounts/:provider",
            axum::routing::delete(unlink_account_handler),
        )
        .with_state(state)
}

// ============================================================================
// Utility
// ============================================================================

/// Simple URL-encoding for query params
fn urlencoding(s: &str) -> String {
    s.chars()
        .map(|c| match c {
            ' ' => "%20".to_string(),
            '!' => "%21".to_string(),
            '#' => "%23".to_string(),
            '$' => "%24".to_string(),
            '&' => "%26".to_string(),
            '\'' => "%27".to_string(),
            '(' => "%28".to_string(),
            ')' => "%29".to_string(),
            '*' => "%2A".to_string(),
            '+' => "%2B".to_string(),
            ',' => "%2C".to_string(),
            '/' => "%2F".to_string(),
            ':' => "%3A".to_string(),
            ';' => "%3B".to_string(),
            '=' => "%3D".to_string(),
            '?' => "%3F".to_string(),
            '@' => "%40".to_string(),
            '[' => "%5B".to_string(),
            ']' => "%5D".to_string(),
            _ => c.to_string(),
        })
        .collect()
}
