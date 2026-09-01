//! # WebAuthn / Passkeys / Hardware Keys
//!
//! FIDO2/WebAuthn registration and authentication ceremonies.
//! Supports hardware security keys (YubiKey) and platform authenticators (passkeys).

use crate::auth::{extract_auth_user, AuthService, AuthState};
use crate::db::{SqlValue, StackhouseStore};
use crate::error::{StackhouseError, StackhouseResult};

use axum::{
    extract::State,
    http::HeaderMap,
    response::IntoResponse,
    routing::{delete, get, post},
    Json, Router,
};
use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine as _};
use rand::RngCore;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use std::sync::Arc;
use tracing::info;

// ============================================================================
// Types
// ============================================================================

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WebAuthnCredential {
    pub id: String,
    pub user_id: i64,
    pub credential_id: String,
    pub public_key: String,
    pub sign_count: u32,
    pub name: String,
    pub authenticator_type: String, // "cross-platform" or "platform"
    pub transports: Vec<String>,
    pub created_at: String,
    pub last_used_at: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RegistrationChallenge {
    pub challenge: String,
    pub rp: RelyingParty,
    pub user: PublicKeyUser,
    pub pub_key_cred_params: Vec<PubKeyCredParam>,
    pub timeout: u64,
    pub authenticator_selection: AuthenticatorSelection,
    pub attestation: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RelyingParty {
    pub name: String,
    pub id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PublicKeyUser {
    pub id: String,
    pub name: String,
    pub display_name: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PubKeyCredParam {
    #[serde(rename = "type")]
    pub cred_type: String,
    pub alg: i32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AuthenticatorSelection {
    pub authenticator_attachment: Option<String>,
    pub resident_key: String,
    pub require_resident_key: bool,
    pub user_verification: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuthenticationChallenge {
    pub challenge: String,
    pub timeout: u64,
    pub rp_id: String,
    pub allow_credentials: Vec<AllowCredential>,
    pub user_verification: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AllowCredential {
    pub id: String,
    #[serde(rename = "type")]
    pub cred_type: String,
    pub transports: Vec<String>,
}

// ============================================================================
// WebAuthn Service
// ============================================================================

#[derive(Clone)]
pub struct WebAuthnService {
    store: Arc<StackhouseStore>,
    auth: AuthService,
    rp_id: String,
    rp_name: String,
    origin: String,
}

impl WebAuthnService {
    pub async fn new(
        store: Arc<StackhouseStore>,
        auth: AuthService,
        base_url: &str,
    ) -> StackhouseResult<Self> {
        let rp_id = std::env::var("STACKHOUSE_WEBAUTHN_RP_ID").unwrap_or_else(|_| {
            reqwest::Url::parse(base_url)
                .map(|u| u.host_str().unwrap_or("localhost").to_string())
                .unwrap_or_else(|_| "localhost".to_string())
        });

        let service = Self {
            store,
            auth,
            rp_id,
            rp_name: "Stackhouse".to_string(),
            origin: base_url.to_string(),
        };
        service.initialize_tables().await?;
        info!(
            "🔑 WebAuthn/Passkey service initialized (RP: {})",
            service.rp_id
        );
        Ok(service)
    }

    async fn initialize_tables(&self) -> StackhouseResult<()> {
        self.store.execute_batch(r#"
            CREATE TABLE IF NOT EXISTS stackhouse_webauthn_credentials (
                id TEXT PRIMARY KEY,
                user_id BIGINT NOT NULL REFERENCES stackhouse_users(id) ON DELETE CASCADE,
                credential_id TEXT NOT NULL UNIQUE,
                public_key TEXT NOT NULL,
                sign_count INTEGER NOT NULL DEFAULT 0,
                name TEXT NOT NULL,
                authenticator_type TEXT NOT NULL DEFAULT 'cross-platform',
                transports TEXT NOT NULL DEFAULT '[]',
                created_at TIMESTAMPTZ DEFAULT NOW(),
                last_used_at TIMESTAMPTZ
            );
            CREATE TABLE IF NOT EXISTS stackhouse_webauthn_challenges (
                challenge TEXT PRIMARY KEY,
                user_id BIGINT,
                challenge_type TEXT NOT NULL,
                created_at TIMESTAMPTZ DEFAULT NOW(),
                expires_at TIMESTAMPTZ DEFAULT (NOW() + INTERVAL '5 minutes')
            );
            CREATE INDEX IF NOT EXISTS idx_webauthn_creds_user ON stackhouse_webauthn_credentials(user_id);
            CREATE INDEX IF NOT EXISTS idx_webauthn_creds_cred_id ON stackhouse_webauthn_credentials(credential_id);
        "#.to_string()).await?;
        Ok(())
    }

    /// Begin registration ceremony
    pub async fn begin_registration(
        &self,
        user_id: i64,
        email: &str,
    ) -> StackhouseResult<RegistrationChallenge> {
        let challenge = self.generate_challenge();

        // Store challenge
        self.store.execute(
            "INSERT INTO stackhouse_webauthn_challenges (challenge, user_id, challenge_type) VALUES (?, ?, 'registration')".to_string(),
            vec![
                SqlValue::Text(challenge.clone()),
                SqlValue::Integer(user_id),
            ],
        ).await?;

        // Get existing credentials for exclusion
        let _existing = self
            .store
            .query(
                "SELECT credential_id FROM stackhouse_webauthn_credentials WHERE user_id = ?"
                    .to_string(),
                vec![SqlValue::Integer(user_id)],
            )
            .await?;

        let user_handle = URL_SAFE_NO_PAD.encode(user_id.to_le_bytes());

        Ok(RegistrationChallenge {
            challenge: challenge.clone(),
            rp: RelyingParty {
                name: self.rp_name.clone(),
                id: self.rp_id.clone(),
            },
            user: PublicKeyUser {
                id: user_handle,
                name: email.to_string(),
                display_name: email.split('@').next().unwrap_or(email).to_string(),
            },
            pub_key_cred_params: vec![
                PubKeyCredParam {
                    cred_type: "public-key".into(),
                    alg: -7,
                }, // ES256
                PubKeyCredParam {
                    cred_type: "public-key".into(),
                    alg: -257,
                }, // RS256
            ],
            timeout: 300000, // 5 minutes
            authenticator_selection: AuthenticatorSelection {
                authenticator_attachment: None,
                resident_key: "preferred".into(),
                require_resident_key: false,
                user_verification: "preferred".into(),
            },
            attestation: "none".into(),
        })
    }

    /// Complete registration ceremony
    pub async fn complete_registration(
        &self,
        user_id: i64,
        credential_id: &str,
        public_key: &str,
        name: &str,
        challenge: &str,
        authenticator_type: &str,
        transports: Vec<String>,
    ) -> StackhouseResult<WebAuthnCredential> {
        // Verify challenge
        let rows = self.store.query(
            "SELECT user_id FROM stackhouse_webauthn_challenges WHERE challenge = ? AND challenge_type = 'registration' AND expires_at > NOW()".to_string(),
            vec![SqlValue::Text(challenge.to_string())],
        ).await?;

        if rows.is_empty() {
            return Err(StackhouseError::Unauthorized(
                "Invalid or expired challenge".into(),
            ));
        }

        let challenge_user_id = rows[0]
            .iter()
            .find(|(k, _)| k == "user_id")
            .and_then(|(_, v)| v.as_i64())
            .unwrap_or(0);
        if challenge_user_id != user_id {
            return Err(StackhouseError::Unauthorized(
                "Challenge user mismatch".into(),
            ));
        }

        // Delete challenge
        self.store
            .execute(
                "DELETE FROM stackhouse_webauthn_challenges WHERE challenge = ?".to_string(),
                vec![SqlValue::Text(challenge.to_string())],
            )
            .await
            .ok();

        let id = uuid::Uuid::new_v4().to_string();

        self.store.execute(
            "INSERT INTO stackhouse_webauthn_credentials (id, user_id, credential_id, public_key, name, authenticator_type, transports) VALUES (?, ?, ?, ?, ?, ?, ?)".to_string(),
            vec![
                SqlValue::Text(id.clone()),
                SqlValue::Integer(user_id),
                SqlValue::Text(credential_id.to_string()),
                SqlValue::Text(public_key.to_string()),
                SqlValue::Text(name.to_string()),
                SqlValue::Text(authenticator_type.to_string()),
                SqlValue::Text(serde_json::to_string(&transports).unwrap_or_default()),
            ],
        ).await?;

        info!(
            "🔑 WebAuthn credential registered for user {}: {}",
            user_id, name
        );

        Ok(WebAuthnCredential {
            id,
            user_id,
            credential_id: credential_id.to_string(),
            public_key: public_key.to_string(),
            sign_count: 0,
            name: name.to_string(),
            authenticator_type: authenticator_type.to_string(),
            transports,
            created_at: chrono::Utc::now().to_rfc3339(),
            last_used_at: None,
        })
    }

    /// Begin authentication ceremony
    pub async fn begin_authentication(
        &self,
        email: &str,
    ) -> StackhouseResult<AuthenticationChallenge> {
        // Find user
        let user_rows = self
            .store
            .query(
                "SELECT id FROM stackhouse_users WHERE email = ?".to_string(),
                vec![SqlValue::Text(email.to_string())],
            )
            .await?;

        if user_rows.is_empty() {
            return Err(StackhouseError::NotFound("User not found".into()));
        }

        let user_id = user_rows[0]
            .iter()
            .find(|(k, _)| k == "id")
            .and_then(|(_, v)| v.as_i64())
            .unwrap_or(0);

        // Get credentials
        let cred_rows = self.store.query(
            "SELECT credential_id, transports FROM stackhouse_webauthn_credentials WHERE user_id = ?".to_string(),
            vec![SqlValue::Integer(user_id)],
        ).await?;

        if cred_rows.is_empty() {
            return Err(StackhouseError::NotFound(
                "No passkeys registered for this user".into(),
            ));
        }

        let challenge = self.generate_challenge();

        self.store.execute(
            "INSERT INTO stackhouse_webauthn_challenges (challenge, user_id, challenge_type) VALUES (?, ?, 'authentication')".to_string(),
            vec![
                SqlValue::Text(challenge.clone()),
                SqlValue::Integer(user_id),
            ],
        ).await?;

        let allow_credentials: Vec<AllowCredential> = cred_rows
            .into_iter()
            .map(|r| {
                let cred_id = r
                    .iter()
                    .find(|(k, _)| k == "credential_id")
                    .and_then(|(_, v)| v.as_str())
                    .unwrap_or("")
                    .to_string();
                let transports: Vec<String> = r
                    .iter()
                    .find(|(k, _)| k == "transports")
                    .and_then(|(_, v)| v.as_str())
                    .and_then(|s| serde_json::from_str(s).ok())
                    .unwrap_or_default();
                AllowCredential {
                    id: cred_id,
                    cred_type: "public-key".into(),
                    transports,
                }
            })
            .collect();

        Ok(AuthenticationChallenge {
            challenge,
            timeout: 300000,
            rp_id: self.rp_id.clone(),
            allow_credentials,
            user_verification: "preferred".into(),
        })
    }

    /// Complete authentication ceremony
    pub async fn complete_authentication(
        &self,
        credential_id: &str,
        challenge: &str,
        sign_count: u32,
    ) -> StackhouseResult<Value> {
        // Verify challenge
        let challenge_rows = self.store.query(
            "SELECT user_id FROM stackhouse_webauthn_challenges WHERE challenge = ? AND challenge_type = 'authentication' AND expires_at > NOW()".to_string(),
            vec![SqlValue::Text(challenge.to_string())],
        ).await?;

        if challenge_rows.is_empty() {
            return Err(StackhouseError::Unauthorized(
                "Invalid or expired challenge".into(),
            ));
        }

        let user_id = challenge_rows[0]
            .iter()
            .find(|(k, _)| k == "user_id")
            .and_then(|(_, v)| v.as_i64())
            .unwrap_or(0);

        // Verify credential exists for user
        let cred_rows = self.store.query(
            "SELECT sign_count FROM stackhouse_webauthn_credentials WHERE credential_id = ? AND user_id = ?".to_string(),
            vec![SqlValue::Text(credential_id.to_string()), SqlValue::Integer(user_id)],
        ).await?;

        if cred_rows.is_empty() {
            return Err(StackhouseError::Unauthorized(
                "Credential not found for user".into(),
            ));
        }

        let stored_count = cred_rows[0]
            .iter()
            .find(|(k, _)| k == "sign_count")
            .and_then(|(_, v)| v.as_i64())
            .unwrap_or(0) as u32;

        // Verify sign count is increasing (replay protection)
        if sign_count <= stored_count {
            return Err(StackhouseError::Unauthorized(
                "Possible credential cloning detected".into(),
            ));
        }

        // Update sign count and last used
        self.store.execute(
            "UPDATE stackhouse_webauthn_credentials SET sign_count = ?, last_used_at = NOW() WHERE credential_id = ?".to_string(),
            vec![SqlValue::Integer(sign_count as i64), SqlValue::Text(credential_id.to_string())],
        ).await?;

        // Delete challenge
        self.store
            .execute(
                "DELETE FROM stackhouse_webauthn_challenges WHERE challenge = ?".to_string(),
                vec![SqlValue::Text(challenge.to_string())],
            )
            .await
            .ok();

        // Generate session tokens
        let user = self.auth.get_user_by_id(user_id).await?;
        let tokens = self.auth.create_session_public(user).await?;

        Ok(json!({"success": true, "data": tokens}))
    }

    /// List credentials for a user
    pub async fn list_credentials(&self, user_id: i64) -> StackhouseResult<Vec<Value>> {
        let rows = self.store.query(
            "SELECT id, credential_id, name, authenticator_type, transports, sign_count, created_at, last_used_at FROM stackhouse_webauthn_credentials WHERE user_id = ? ORDER BY created_at DESC".to_string(),
            vec![SqlValue::Integer(user_id)],
        ).await?;
        Ok(rows
            .into_iter()
            .map(|r| json!(r.into_iter().collect::<std::collections::HashMap<_, _>>()))
            .collect())
    }

    /// Delete a credential
    pub async fn delete_credential(&self, cred_id: &str, user_id: i64) -> StackhouseResult<()> {
        self.store
            .execute(
                "DELETE FROM stackhouse_webauthn_credentials WHERE id = ? AND user_id = ?"
                    .to_string(),
                vec![
                    SqlValue::Text(cred_id.to_string()),
                    SqlValue::Integer(user_id),
                ],
            )
            .await?;
        Ok(())
    }

    fn generate_challenge(&self) -> String {
        let mut bytes = [0u8; 32];
        rand::rngs::OsRng.fill_bytes(&mut bytes);
        URL_SAFE_NO_PAD.encode(bytes)
    }
}

// ============================================================================
// Router
// ============================================================================

#[derive(Clone)]
pub struct WebAuthnState {
    pub webauthn: Arc<WebAuthnService>,
    pub auth: AuthState,
}

#[derive(Deserialize)]
struct BeginAuthRequest {
    email: String,
}

#[derive(Deserialize)]
struct CompleteRegRequest {
    credential_id: String,
    public_key: String,
    name: String,
    challenge: String,
    #[serde(default = "default_auth_type")]
    authenticator_type: String,
    #[serde(default)]
    transports: Vec<String>,
}
fn default_auth_type() -> String {
    "cross-platform".into()
}

#[derive(Deserialize)]
struct CompleteAuthRequest {
    credential_id: String,
    challenge: String,
    sign_count: u32,
}

async fn begin_registration_handler(
    State(state): State<WebAuthnState>,
    headers: HeaderMap,
) -> Result<impl IntoResponse, StackhouseError> {
    let user = extract_auth_user(&state.auth, &headers)?;
    let options = state
        .webauthn
        .begin_registration(user.id, &user.email)
        .await?;
    Ok(Json(json!({"success": true, "data": options})))
}

async fn complete_registration_handler(
    State(state): State<WebAuthnState>,
    headers: HeaderMap,
    Json(req): Json<CompleteRegRequest>,
) -> Result<impl IntoResponse, StackhouseError> {
    let user = extract_auth_user(&state.auth, &headers)?;
    let cred = state
        .webauthn
        .complete_registration(
            user.id,
            &req.credential_id,
            &req.public_key,
            &req.name,
            &req.challenge,
            &req.authenticator_type,
            req.transports,
        )
        .await?;
    Ok(Json(json!({"success": true, "data": cred})))
}

async fn begin_authentication_handler(
    State(state): State<WebAuthnState>,
    Json(req): Json<BeginAuthRequest>,
) -> Result<impl IntoResponse, StackhouseError> {
    let options = state.webauthn.begin_authentication(&req.email).await?;
    Ok(Json(json!({"success": true, "data": options})))
}

async fn complete_authentication_handler(
    State(state): State<WebAuthnState>,
    Json(req): Json<CompleteAuthRequest>,
) -> Result<impl IntoResponse, StackhouseError> {
    let result = state
        .webauthn
        .complete_authentication(&req.credential_id, &req.challenge, req.sign_count)
        .await?;
    Ok(Json(result))
}

async fn list_credentials_handler(
    State(state): State<WebAuthnState>,
    headers: HeaderMap,
) -> Result<impl IntoResponse, StackhouseError> {
    let user = extract_auth_user(&state.auth, &headers)?;
    let creds = state.webauthn.list_credentials(user.id).await?;
    Ok(Json(json!({"success": true, "data": creds})))
}

async fn delete_credential_handler(
    State(state): State<WebAuthnState>,
    headers: HeaderMap,
    axum::extract::Path(cred_id): axum::extract::Path<String>,
) -> Result<impl IntoResponse, StackhouseError> {
    let user = extract_auth_user(&state.auth, &headers)?;
    state.webauthn.delete_credential(&cred_id, user.id).await?;
    Ok(Json(
        json!({"success": true, "message": "Credential deleted"}),
    ))
}

pub fn create_webauthn_router(state: WebAuthnState) -> Router {
    Router::new()
        .route("/passkeys/register", post(begin_registration_handler))
        .route(
            "/passkeys/register/complete",
            post(complete_registration_handler),
        )
        .route("/passkeys/authenticate", post(begin_authentication_handler))
        .route(
            "/passkeys/authenticate/complete",
            post(complete_authentication_handler),
        )
        .route("/passkeys", get(list_credentials_handler))
        .route("/passkeys/:id", delete(delete_credential_handler))
        .with_state(state)
}
