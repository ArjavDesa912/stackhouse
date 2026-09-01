//! # SAML 2.0 & OIDC Single Sign-On
//!
//! SP-initiated SAML flow with metadata endpoint, assertion parsing,
//! and org-level Identity Provider configuration.

use crate::auth::{extract_auth_user, AuthService, AuthState, User};
use crate::db::{SqlValue, StackhouseStore};
use crate::error::{StackhouseError, StackhouseResult};

use axum::{
    extract::{Query, State},
    http::HeaderMap,
    response::IntoResponse,
    routing::{get, post},
    Json, Router,
};
use base64::{engine::general_purpose::STANDARD, Engine as _};
use rsa::pkcs1::DecodeRsaPublicKey;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::sync::Arc;
use tracing::info;

// ============================================================================
// Types
// ============================================================================

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SamlIdpConfig {
    pub id: String,
    pub org_id: i64,
    pub name: String,
    pub entity_id: String,
    pub sso_url: String,
    pub slo_url: Option<String>,
    pub certificate: String,
    pub name_id_format: String,
    pub sign_requests: bool,
    pub enabled: bool,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OidcIdpConfig {
    pub id: String,
    pub org_id: i64,
    pub name: String,
    pub issuer: String,
    pub client_id: String,
    pub client_secret: String,
    pub authorization_endpoint: String,
    pub token_endpoint: String,
    pub userinfo_endpoint: String,
    pub jwks_uri: String,
    pub scopes: Vec<String>,
    pub enabled: bool,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SsoSession {
    pub request_id: String,
    pub org_id: i64,
    pub provider_type: String, // "saml" or "oidc"
    pub provider_id: String,
    pub relay_state: Option<String>,
    pub created_at: String,
}

// ============================================================================
// SAML Service
// ============================================================================

#[derive(Clone)]
pub struct SamlService {
    store: Arc<StackhouseStore>,
    auth: AuthService,
    base_url: String,
    sp_entity_id: String,
}

impl SamlService {
    pub async fn new(
        store: Arc<StackhouseStore>,
        auth: AuthService,
        base_url: String,
    ) -> StackhouseResult<Self> {
        let sp_entity_id = std::env::var("STACKHOUSE_SAML_ENTITY_ID")
            .unwrap_or_else(|_| format!("{}/saml/metadata", base_url));

        let service = Self {
            store,
            auth,
            base_url,
            sp_entity_id,
        };
        service.initialize_tables().await?;
        info!("🔐 SAML/OIDC SSO service initialized");
        Ok(service)
    }

    async fn initialize_tables(&self) -> StackhouseResult<()> {
        self.store.execute_batch(r#"
            CREATE TABLE IF NOT EXISTS stackhouse_saml_idps (
                id TEXT PRIMARY KEY,
                org_id BIGINT NOT NULL,
                name TEXT NOT NULL,
                entity_id TEXT NOT NULL,
                sso_url TEXT NOT NULL,
                slo_url TEXT,
                certificate TEXT NOT NULL,
                name_id_format TEXT NOT NULL DEFAULT 'urn:oasis:names:tc:SAML:1.1:nameid-format:emailAddress',
                sign_requests BOOLEAN DEFAULT FALSE,
                enabled BOOLEAN DEFAULT TRUE,
                created_at TIMESTAMPTZ DEFAULT NOW()
            );
            CREATE TABLE IF NOT EXISTS stackhouse_oidc_idps (
                id TEXT PRIMARY KEY,
                org_id BIGINT NOT NULL,
                name TEXT NOT NULL,
                issuer TEXT NOT NULL,
                client_id TEXT NOT NULL,
                client_secret TEXT NOT NULL,
                authorization_endpoint TEXT NOT NULL,
                token_endpoint TEXT NOT NULL,
                userinfo_endpoint TEXT NOT NULL,
                jwks_uri TEXT NOT NULL,
                scopes TEXT NOT NULL DEFAULT '["openid","email","profile"]',
                enabled BOOLEAN DEFAULT TRUE,
                created_at TIMESTAMPTZ DEFAULT NOW()
            );
            CREATE TABLE IF NOT EXISTS stackhouse_sso_sessions (
                request_id TEXT PRIMARY KEY,
                org_id BIGINT NOT NULL,
                provider_type TEXT NOT NULL,
                provider_id TEXT NOT NULL,
                relay_state TEXT,
                created_at TIMESTAMPTZ DEFAULT NOW(),
                expires_at TIMESTAMPTZ DEFAULT (NOW() + INTERVAL '10 minutes')
            );
            CREATE INDEX IF NOT EXISTS idx_saml_idps_org ON stackhouse_saml_idps(org_id);
            CREATE INDEX IF NOT EXISTS idx_oidc_idps_org ON stackhouse_oidc_idps(org_id);
        "#.to_string()).await?;
        Ok(())
    }

    /// Register a SAML Identity Provider for an org
    pub async fn register_saml_idp(
        &self,
        org_id: i64,
        config: SamlIdpConfig,
    ) -> StackhouseResult<SamlIdpConfig> {
        let id = if config.id.is_empty() {
            uuid::Uuid::new_v4().to_string()
        } else {
            config.id.clone()
        };

        self.store.execute(
            "INSERT INTO stackhouse_saml_idps (id, org_id, name, entity_id, sso_url, slo_url, certificate, name_id_format, sign_requests) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)".to_string(),
            vec![
                SqlValue::Text(id.clone()),
                SqlValue::Integer(org_id),
                SqlValue::Text(config.name.clone()),
                SqlValue::Text(config.entity_id.clone()),
                SqlValue::Text(config.sso_url.clone()),
                SqlValue::Text(config.slo_url.clone().unwrap_or_default()),
                SqlValue::Text(config.certificate.clone()),
                SqlValue::Text(config.name_id_format.clone()),
                SqlValue::Text(config.sign_requests.to_string()),
            ],
        ).await?;

        info!("🔐 SAML IdP registered for org {}: {}", org_id, config.name);

        Ok(SamlIdpConfig {
            id,
            org_id,
            created_at: chrono::Utc::now().to_rfc3339(),
            ..config
        })
    }

    /// Register an OIDC Identity Provider
    pub async fn register_oidc_idp(
        &self,
        org_id: i64,
        config: OidcIdpConfig,
    ) -> StackhouseResult<OidcIdpConfig> {
        let id = if config.id.is_empty() {
            uuid::Uuid::new_v4().to_string()
        } else {
            config.id.clone()
        };

        self.store.execute(
            "INSERT INTO stackhouse_oidc_idps (id, org_id, name, issuer, client_id, client_secret, authorization_endpoint, token_endpoint, userinfo_endpoint, jwks_uri, scopes) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)".to_string(),
            vec![
                SqlValue::Text(id.clone()),
                SqlValue::Integer(org_id),
                SqlValue::Text(config.name.clone()),
                SqlValue::Text(config.issuer.clone()),
                SqlValue::Text(config.client_id.clone()),
                SqlValue::Text(config.client_secret.clone()),
                SqlValue::Text(config.authorization_endpoint.clone()),
                SqlValue::Text(config.token_endpoint.clone()),
                SqlValue::Text(config.userinfo_endpoint.clone()),
                SqlValue::Text(config.jwks_uri.clone()),
                SqlValue::Text(serde_json::to_string(&config.scopes).unwrap_or_default()),
            ],
        ).await?;

        info!("🔐 OIDC IdP registered for org {}: {}", org_id, config.name);

        Ok(OidcIdpConfig {
            id,
            org_id,
            created_at: chrono::Utc::now().to_rfc3339(),
            ..config
        })
    }

    /// Generate SP metadata XML
    pub fn sp_metadata(&self) -> String {
        format!(
            r#"<?xml version="1.0" encoding="UTF-8"?>
<md:EntityDescriptor xmlns:md="urn:oasis:names:tc:SAML:2.0:metadata"
    entityID="{entity_id}">
    <md:SPSSODescriptor
        AuthnRequestsSigned="false"
        WantAssertionsSigned="true"
        protocolSupportEnumeration="urn:oasis:names:tc:SAML:2.0:protocol">
        <md:NameIDFormat>urn:oasis:names:tc:SAML:1.1:nameid-format:emailAddress</md:NameIDFormat>
        <md:AssertionConsumerService
            Binding="urn:oasis:names:tc:SAML:2.0:bindings:HTTP-POST"
            Location="{base_url}/v1/auth/saml/acs"
            index="0"
            isDefault="true"/>
        <md:SingleLogoutService
            Binding="urn:oasis:names:tc:SAML:2.0:bindings:HTTP-Redirect"
            Location="{base_url}/v1/auth/saml/slo"/>
    </md:SPSSODescriptor>
</md:EntityDescriptor>"#,
            entity_id = self.sp_entity_id,
            base_url = self.base_url,
        )
    }

    /// Initiate SAML SSO login for an org
    pub async fn initiate_saml_login(
        &self,
        org_id: i64,
        relay_state: Option<String>,
    ) -> StackhouseResult<String> {
        let rows = self.store.query(
            "SELECT id, sso_url, entity_id FROM stackhouse_saml_idps WHERE org_id = ? AND enabled = true LIMIT 1".to_string(),
            vec![SqlValue::Integer(org_id)],
        ).await?;

        if rows.is_empty() {
            return Err(StackhouseError::NotFound(
                "No SAML IdP configured for this organization".into(),
            ));
        }

        let row = &rows[0];
        let idp_id = row
            .iter()
            .find(|(k, _)| k == "id")
            .and_then(|(_, v)| v.as_str())
            .unwrap_or("");
        let sso_url = row
            .iter()
            .find(|(k, _)| k == "sso_url")
            .and_then(|(_, v)| v.as_str())
            .unwrap_or("");

        let request_id = format!("_stackhouse_{}", uuid::Uuid::new_v4());

        // Store session
        self.store.execute(
            "INSERT INTO stackhouse_sso_sessions (request_id, org_id, provider_type, provider_id, relay_state) VALUES (?, ?, 'saml', ?, ?)".to_string(),
            vec![
                SqlValue::Text(request_id.clone()),
                SqlValue::Integer(org_id),
                SqlValue::Text(idp_id.to_string()),
                SqlValue::Text(relay_state.clone().unwrap_or_default()),
            ],
        ).await?;

        // Build AuthnRequest
        let authn_request = format!(
            r#"<samlp:AuthnRequest xmlns:samlp="urn:oasis:names:tc:SAML:2.0:protocol"
    xmlns:saml="urn:oasis:names:tc:SAML:2.0:assertion"
    ID="{request_id}"
    Version="2.0"
    IssueInstant="{now}"
    AssertionConsumerServiceURL="{acs_url}"
    Destination="{sso_url}">
    <saml:Issuer>{entity_id}</saml:Issuer>
    <samlp:NameIDPolicy Format="urn:oasis:names:tc:SAML:1.1:nameid-format:emailAddress" AllowCreate="true"/>
</samlp:AuthnRequest>"#,
            request_id = request_id,
            now = chrono::Utc::now().to_rfc3339(),
            acs_url = format!("{}/v1/auth/saml/acs", self.base_url),
            sso_url = sso_url,
            entity_id = self.sp_entity_id,
        );

        let encoded = STANDARD.encode(authn_request.as_bytes());
        let redirect_url = format!(
            "{}?SAMLRequest={}&RelayState={}",
            sso_url,
            urlencoding::encode(&encoded),
            urlencoding::encode(&relay_state.unwrap_or_default()),
        );

        Ok(redirect_url)
    }

    /// Process SAML assertion callback (ACS endpoint)
    pub async fn process_saml_response(
        &self,
        saml_response: &str,
        relay_state: Option<&str>,
    ) -> StackhouseResult<Value> {
        let decoded = STANDARD.decode(saml_response).map_err(|_| {
            StackhouseError::InvalidPayload("Invalid SAML response encoding".into())
        })?;
        let xml = String::from_utf8(decoded)
            .map_err(|_| StackhouseError::InvalidPayload("Invalid SAML response UTF-8".into()))?;

        // Extract the issuer from the response to find the IdP config
        let issuer = Self::extract_issuer(&xml).ok_or_else(|| {
            StackhouseError::InvalidPayload("Could not extract Issuer from SAML response".into())
        })?;

        // Look up the IdP config by entity_id (issuer)
        let idp_rows = self.store.query(
            "SELECT id, org_id, certificate FROM stackhouse_saml_idps WHERE entity_id = ? AND enabled = true LIMIT 1".to_string(),
            vec![SqlValue::Text(issuer.clone())],
        ).await?;

        if idp_rows.is_empty() {
            return Err(StackhouseError::Unauthorized(
                "No SAML IdP found for issuer".into(),
            ));
        }

        let idp_row = &idp_rows[0];
        let _idp_id = idp_row
            .iter()
            .find(|(k, _)| k == "id")
            .and_then(|(_, v)| v.as_str())
            .unwrap_or("")
            .to_string();
        let _org_id = idp_row
            .iter()
            .find(|(k, _)| k == "org_id")
            .and_then(|(_, v)| v.as_i64())
            .unwrap_or(0);
        let certificate = idp_row
            .iter()
            .find(|(k, _)| k == "certificate")
            .and_then(|(_, v)| v.as_str())
            .unwrap_or("")
            .to_string();

        // Verify the SAML response signature
        if !Self::verify_saml_signature(&xml, &certificate)? {
            return Err(StackhouseError::Unauthorized(
                "SAML response signature verification failed".into(),
            ));
        }

        // Extract email from NameID
        let email = Self::extract_name_id(&xml)?;

        // Find or create user
        let user = self.find_or_create_user(&email).await?;

        // Generate tokens
        let tokens = self.auth.create_session_public(user).await?;

        // Clean up SSO session if relay_state matches a pending session
        if let Some(rs) = relay_state {
            self.store.execute(
                "DELETE FROM stackhouse_sso_sessions WHERE request_id = ? AND provider_type = 'saml'".to_string(),
                vec![SqlValue::Text(rs.to_string())],
            ).await.ok();
        }

        Ok(json!({
            "success": true,
            "data": tokens,
            "relay_state": relay_state,
        }))
    }

    /// Initiate OIDC login for an org
    pub async fn initiate_oidc_login(
        &self,
        org_id: i64,
        redirect_uri: &str,
    ) -> StackhouseResult<String> {
        let rows = self.store.query(
            "SELECT id, client_id, authorization_endpoint, scopes FROM stackhouse_oidc_idps WHERE org_id = ? AND enabled = true LIMIT 1".to_string(),
            vec![SqlValue::Integer(org_id)],
        ).await?;

        if rows.is_empty() {
            return Err(StackhouseError::NotFound(
                "No OIDC IdP configured for this organization".into(),
            ));
        }

        let row = &rows[0];
        let idp_id = row
            .iter()
            .find(|(k, _)| k == "id")
            .and_then(|(_, v)| v.as_str())
            .unwrap_or("");
        let client_id = row
            .iter()
            .find(|(k, _)| k == "client_id")
            .and_then(|(_, v)| v.as_str())
            .unwrap_or("");
        let auth_endpoint = row
            .iter()
            .find(|(k, _)| k == "authorization_endpoint")
            .and_then(|(_, v)| v.as_str())
            .unwrap_or("");
        let scopes_str = row
            .iter()
            .find(|(k, _)| k == "scopes")
            .and_then(|(_, v)| v.as_str())
            .unwrap_or(r#"["openid","email","profile"]"#);

        let scopes: Vec<String> = serde_json::from_str(scopes_str)
            .unwrap_or_else(|_| vec!["openid".into(), "email".into()]);
        let state = uuid::Uuid::new_v4().to_string();
        let nonce = uuid::Uuid::new_v4().to_string();

        // Store session
        self.store.execute(
            "INSERT INTO stackhouse_sso_sessions (request_id, org_id, provider_type, provider_id, relay_state) VALUES (?, ?, 'oidc', ?, ?)".to_string(),
            vec![
                SqlValue::Text(state.clone()),
                SqlValue::Integer(org_id),
                SqlValue::Text(idp_id.to_string()),
                SqlValue::Text(redirect_uri.to_string()),
            ],
        ).await?;

        let url = format!(
            "{}?response_type=code&client_id={}&redirect_uri={}&scope={}&state={}&nonce={}",
            auth_endpoint,
            urlencoding::encode(client_id),
            urlencoding::encode(redirect_uri),
            urlencoding::encode(&scopes.join(" ")),
            urlencoding::encode(&state),
            urlencoding::encode(&nonce),
        );

        Ok(url)
    }

    /// Exchange OIDC authorization code for tokens
    pub async fn exchange_oidc_code(
        &self,
        code: &str,
        state: &str,
        redirect_uri: &str,
    ) -> StackhouseResult<Value> {
        // Verify state
        let rows = self.store.query(
            "SELECT org_id, provider_id FROM stackhouse_sso_sessions WHERE request_id = ? AND provider_type = 'oidc'".to_string(),
            vec![SqlValue::Text(state.to_string())],
        ).await?;

        if rows.is_empty() {
            return Err(StackhouseError::Unauthorized("Invalid SSO state".into()));
        }

        let row = &rows[0];
        let _org_id = row
            .iter()
            .find(|(k, _)| k == "org_id")
            .and_then(|(_, v)| v.as_i64())
            .unwrap_or(0);
        let provider_id = row
            .iter()
            .find(|(k, _)| k == "provider_id")
            .and_then(|(_, v)| v.as_str())
            .unwrap_or("")
            .to_string();

        // Get IdP config
        let idp_rows = self.store.query(
            "SELECT client_id, client_secret, token_endpoint, userinfo_endpoint FROM stackhouse_oidc_idps WHERE id = ?".to_string(),
            vec![SqlValue::Text(provider_id)],
        ).await?;

        if idp_rows.is_empty() {
            return Err(StackhouseError::Internal(anyhow::anyhow!(
                "OIDC IdP not found"
            )));
        }

        let idp = &idp_rows[0];
        let client_id = idp
            .iter()
            .find(|(k, _)| k == "client_id")
            .and_then(|(_, v)| v.as_str())
            .unwrap_or("");
        let client_secret = idp
            .iter()
            .find(|(k, _)| k == "client_secret")
            .and_then(|(_, v)| v.as_str())
            .unwrap_or("");
        let token_endpoint = idp
            .iter()
            .find(|(k, _)| k == "token_endpoint")
            .and_then(|(_, v)| v.as_str())
            .unwrap_or("");
        let userinfo_endpoint = idp
            .iter()
            .find(|(k, _)| k == "userinfo_endpoint")
            .and_then(|(_, v)| v.as_str())
            .unwrap_or("");

        // Exchange code for token
        let http = reqwest::Client::new();
        let token_resp = http
            .post(token_endpoint)
            .form(&[
                ("grant_type", "authorization_code"),
                ("code", code),
                ("redirect_uri", redirect_uri),
                ("client_id", client_id),
                ("client_secret", client_secret),
            ])
            .send()
            .await
            .map_err(|e| {
                StackhouseError::Internal(anyhow::anyhow!("OIDC token exchange failed: {}", e))
            })?;

        let token_data: Value = token_resp.json().await.map_err(|e| {
            StackhouseError::Internal(anyhow::anyhow!("OIDC token parse error: {}", e))
        })?;

        let access_token = token_data["access_token"].as_str().ok_or_else(|| {
            StackhouseError::Internal(anyhow::anyhow!("No access_token in OIDC response"))
        })?;

        // Get user info
        let userinfo_resp = http
            .get(userinfo_endpoint)
            .bearer_auth(access_token)
            .send()
            .await
            .map_err(|e| {
                StackhouseError::Internal(anyhow::anyhow!("OIDC userinfo failed: {}", e))
            })?;

        let userinfo: Value = userinfo_resp.json().await.map_err(|e| {
            StackhouseError::Internal(anyhow::anyhow!("OIDC userinfo parse error: {}", e))
        })?;

        let email = userinfo["email"].as_str().ok_or_else(|| {
            StackhouseError::Internal(anyhow::anyhow!("No email in OIDC userinfo"))
        })?;

        // Find or create user
        let user = self.find_or_create_user(email).await?;
        let tokens = self.auth.create_session_public(user).await?;

        // Clean up session
        self.store
            .execute(
                "DELETE FROM stackhouse_sso_sessions WHERE request_id = ?".to_string(),
                vec![SqlValue::Text(state.to_string())],
            )
            .await
            .ok();

        Ok(json!({"success": true, "data": tokens}))
    }

    fn extract_name_id(xml: &str) -> StackhouseResult<String> {
        // Namespace-aware extraction of NameID element
        // Handles both saml:NameID and NameID (no prefix)
        for tag in &["saml:NameID", "NameID"] {
            if let Some(start) = xml.find(&format!("<{}", tag)) {
                if let Some(content_start) = xml[start..].find('>') {
                    let after = &xml[start + content_start + 1..];
                    let close_tag = format!("</{}>", tag);
                    if let Some(end) = after.find(&close_tag) {
                        let value = after[..end].trim();
                        if !value.is_empty() {
                            return Ok(value.to_string());
                        }
                    }
                }
            }
        }
        Err(StackhouseError::InvalidPayload(
            "Could not extract NameID from SAML response".into(),
        ))
    }

    fn extract_issuer(xml: &str) -> Option<String> {
        for tag in &["saml:Issuer", "Issuer"] {
            if let Some(start) = xml.find(&format!("<{}", tag)) {
                if let Some(content_start) = xml[start..].find('>') {
                    let after = &xml[start + content_start + 1..];
                    let close_tag = format!("</{}>", tag);
                    if let Some(end) = after.find(&close_tag) {
                        let value = after[..end].trim();
                        if !value.is_empty() {
                            return Some(value.to_string());
                        }
                    }
                }
            }
        }
        None
    }

    fn verify_saml_signature(xml: &str, certificate_pem: &str) -> StackhouseResult<bool> {
        // Find the Signature element in the SAML response
        let _sig_start = xml
            .find("<ds:Signature")
            .or_else(|| xml.find("<Signature"))
            .ok_or_else(|| {
                StackhouseError::Unauthorized("SAML response has no signature element".into())
            })?;

        // Extract the signature value
        let sig_value = Self::extract_element_text(xml, "ds:SignatureValue")
            .or_else(|| Self::extract_element_text(xml, "SignatureValue"))
            .ok_or_else(|| {
                StackhouseError::Unauthorized("SAML response has no signature value".into())
            })?;

        // Extract the digest value
        let digest_value = Self::extract_element_text(xml, "ds:DigestValue")
            .or_else(|| Self::extract_element_text(xml, "DigestValue"))
            .ok_or_else(|| {
                StackhouseError::Unauthorized("SAML response has no digest value".into())
            })?;

        // Extract the signed content (the assertion XML that was signed)
        // In a SAML response, the signature covers the <saml:Assertion> element
        let assertion_start = xml
            .find("<saml:Assertion")
            .or_else(|| xml.find("<Assertion"))
            .ok_or_else(|| {
                StackhouseError::Unauthorized("SAML response has no assertion element".into())
            })?;

        let assertion_end = xml
            .find("</saml:Assertion>")
            .or_else(|| xml.find("</Assertion>"))
            .ok_or_else(|| {
                StackhouseError::Unauthorized("SAML assertion not properly closed".into())
            })?;

        // The signed content is the assertion XML, minus the signature element itself
        let assertion_xml = &xml[assertion_start..assertion_end];
        // Remove the Signature element from the assertion for digest verification
        let signed_content = if let Some(sig_pos) = assertion_xml
            .find("<ds:Signature")
            .or_else(|| assertion_xml.find("<Signature"))
        {
            &assertion_xml[..sig_pos]
        } else {
            assertion_xml
        };

        // Compute SHA-256 digest of the signed content
        use sha2::{Digest, Sha256};
        let mut hasher = Sha256::new();
        hasher.update(signed_content.as_bytes());
        let computed_digest = hasher.finalize();
        let computed_digest_b64 = STANDARD.encode(&computed_digest);

        // Compare digests
        if computed_digest_b64 != digest_value {
            return Ok(false);
        }

        // Decode the signature value
        let signature_bytes = STANDARD
            .decode(sig_value.trim())
            .map_err(|_| StackhouseError::Unauthorized("Invalid signature encoding".into()))?;

        // Parse the X.509 certificate and verify the signature
        let pem_cleaned = certificate_pem
            .replace("-----BEGIN CERTIFICATE-----", "")
            .replace("-----END CERTIFICATE-----", "")
            .replace("-----BEGIN PUBLIC KEY-----", "")
            .replace("-----END PUBLIC KEY-----", "")
            .replace('\n', "")
            .replace('\r', "")
            .trim()
            .to_string();

        let cert_der = STANDARD.decode(&pem_cleaned).map_err(|e| {
            StackhouseError::Unauthorized(format!("Invalid certificate encoding: {}", e))
        })?;

        // Parse the X.509 certificate to extract the public key
        let (_, cert) = x509_parser::parse_x509_certificate(&cert_der)
            .map_err(|e| StackhouseError::Unauthorized(format!("X509 parse error: {}", e)))?;

        let spki = cert.public_key();
        let spki_bytes: &[u8] = spki.subject_public_key.data.as_ref();

        // Try RSA verification first (most common for SAML)
        if let Ok(rsa_pub) = rsa::RsaPublicKey::from_pkcs1_der(spki_bytes) {
            use sha2::{Digest, Sha256};
            let mut hasher = Sha256::new();
            hasher.update(signed_content.as_bytes());
            let hash = hasher.finalize();
            let scheme = rsa::Pkcs1v15Sign::new::<Sha256>();
            if rsa_pub.verify(scheme, &hash, &signature_bytes).is_ok() {
                return Ok(true);
            }
        }

        // Try ECDSA P-256 verification
        use p256::ecdsa::VerifyingKey;
        if let Ok(vk) = VerifyingKey::from_sec1_bytes(spki_bytes) {
            use p256::ecdsa::signature::Verifier;
            let sig = p256::ecdsa::Signature::from_der(&signature_bytes).map_err(|_| {
                StackhouseError::Unauthorized("Invalid ECDSA signature format".into())
            })?;
            return Ok(vk.verify(signed_content.as_bytes(), &sig).is_ok());
        }

        // If we can't determine the key type, reject
        Err(StackhouseError::Unauthorized(
            "Unsupported public key type in SAML certificate".into(),
        ))
    }

    fn extract_element_text(xml: &str, tag: &str) -> Option<String> {
        if let Some(start) = xml.find(&format!("<{}", tag)) {
            if let Some(content_start) = xml[start..].find('>') {
                let after = &xml[start + content_start + 1..];
                let close_tag = format!("</{}>", tag);
                if let Some(end) = after.find(&close_tag) {
                    return Some(after[..end].trim().to_string());
                }
            }
        }
        None
    }

    async fn find_or_create_user(&self, email: &str) -> StackhouseResult<User> {
        let rows = self.store.query(
            "SELECT id, email, metadata, created_at, updated_at FROM stackhouse_users WHERE email = ?".to_string(),
            vec![SqlValue::Text(email.to_string())],
        ).await?;

        if !rows.is_empty() {
            let row = &rows[0];
            return Ok(User {
                id: row
                    .iter()
                    .find(|(k, _)| k == "id")
                    .and_then(|(_, v)| v.as_i64())
                    .unwrap_or(0),
                email: email.to_string(),
                created_at: row
                    .iter()
                    .find(|(k, _)| k == "created_at")
                    .and_then(|(_, v)| v.as_str().map(String::from))
                    .unwrap_or_default(),
                updated_at: row
                    .iter()
                    .find(|(k, _)| k == "updated_at")
                    .and_then(|(_, v)| v.as_str().map(String::from))
                    .unwrap_or_default(),
                metadata: json!({"sso": true}),
            });
        }

        // Create SSO user (no password)
        let user_id = self.store.insert_returning_id(
            "INSERT INTO stackhouse_users (email, password_hash, metadata) VALUES (?, 'SSO_USER', ?)".to_string(),
            vec![
                SqlValue::Text(email.to_string()),
                SqlValue::Text(json!({"sso": true, "provider": "saml"}).to_string()),
            ],
        ).await?;

        Ok(User {
            id: user_id,
            email: email.to_string(),
            created_at: chrono::Utc::now().to_rfc3339(),
            updated_at: chrono::Utc::now().to_rfc3339(),
            metadata: json!({"sso": true}),
        })
    }

    /// List IdPs for an org
    pub async fn list_idps(&self, org_id: i64) -> StackhouseResult<Value> {
        let saml_rows = self.store.query(
            "SELECT id, name, entity_id, sso_url, enabled, created_at FROM stackhouse_saml_idps WHERE org_id = ?".to_string(),
            vec![SqlValue::Integer(org_id)],
        ).await?;

        let oidc_rows = self.store.query(
            "SELECT id, name, issuer, enabled, created_at FROM stackhouse_oidc_idps WHERE org_id = ?".to_string(),
            vec![SqlValue::Integer(org_id)],
        ).await?;

        Ok(json!({
            "saml": saml_rows,
            "oidc": oidc_rows,
        }))
    }
}

// ============================================================================
// Router
// ============================================================================

#[derive(Clone)]
pub struct SamlState {
    pub saml: Arc<SamlService>,
    pub auth: AuthState,
}

#[derive(Deserialize)]
struct SamlLoginQuery {
    org_id: i64,
    #[serde(default)]
    relay_state: Option<String>,
}

#[derive(Deserialize)]
struct SamlAcsForm {
    #[serde(rename = "SAMLResponse")]
    saml_response: String,
    #[serde(rename = "RelayState")]
    relay_state: Option<String>,
}

#[derive(Deserialize)]
struct OidcLoginQuery {
    org_id: i64,
    redirect_uri: String,
}

#[derive(Deserialize)]
struct OidcCallbackQuery {
    code: String,
    state: String,
    #[serde(default)]
    redirect_uri: Option<String>,
}

#[derive(Deserialize)]
struct RegisterSamlIdpRequest {
    name: String,
    entity_id: String,
    sso_url: String,
    #[serde(default)]
    slo_url: Option<String>,
    certificate: String,
    #[serde(default = "default_name_id_format")]
    name_id_format: String,
    #[serde(default)]
    sign_requests: bool,
}
fn default_name_id_format() -> String {
    "urn:oasis:names:tc:SAML:1.1:nameid-format:emailAddress".to_string()
}

#[derive(Deserialize)]
struct RegisterOidcIdpRequest {
    name: String,
    issuer: String,
    client_id: String,
    client_secret: String,
    authorization_endpoint: String,
    token_endpoint: String,
    userinfo_endpoint: String,
    jwks_uri: String,
    #[serde(default)]
    scopes: Vec<String>,
}

async fn metadata_handler(State(state): State<SamlState>) -> impl IntoResponse {
    let xml = state.saml.sp_metadata();
    ([(axum::http::header::CONTENT_TYPE, "application/xml")], xml)
}

async fn saml_login_handler(
    State(state): State<SamlState>,
    Query(params): Query<SamlLoginQuery>,
) -> Result<impl IntoResponse, StackhouseError> {
    let url = state
        .saml
        .initiate_saml_login(params.org_id, params.relay_state)
        .await?;
    Ok(Json(json!({"success": true, "redirect_url": url})))
}

async fn saml_acs_handler(
    State(state): State<SamlState>,
    axum::extract::Form(form): axum::extract::Form<SamlAcsForm>,
) -> Result<impl IntoResponse, StackhouseError> {
    let result = state
        .saml
        .process_saml_response(&form.saml_response, form.relay_state.as_deref())
        .await?;
    Ok(Json(result))
}

async fn oidc_login_handler(
    State(state): State<SamlState>,
    Query(params): Query<OidcLoginQuery>,
) -> Result<impl IntoResponse, StackhouseError> {
    let url = state
        .saml
        .initiate_oidc_login(params.org_id, &params.redirect_uri)
        .await?;
    Ok(Json(json!({"success": true, "redirect_url": url})))
}

async fn oidc_callback_handler(
    State(state): State<SamlState>,
    Query(params): Query<OidcCallbackQuery>,
) -> Result<impl IntoResponse, StackhouseError> {
    let redirect_uri = params
        .redirect_uri
        .unwrap_or_else(|| format!("{}/v1/auth/sso/oidc/callback", state.saml.base_url));
    let result = state
        .saml
        .exchange_oidc_code(&params.code, &params.state, &redirect_uri)
        .await?;
    Ok(Json(result))
}

async fn register_saml_handler(
    State(state): State<SamlState>,
    headers: HeaderMap,
    Json(req): Json<RegisterSamlIdpRequest>,
) -> Result<impl IntoResponse, StackhouseError> {
    let user = extract_auth_user(&state.auth, &headers)?;
    let config = SamlIdpConfig {
        id: String::new(),
        org_id: user.id,
        name: req.name,
        entity_id: req.entity_id,
        sso_url: req.sso_url,
        slo_url: req.slo_url,
        certificate: req.certificate,
        name_id_format: req.name_id_format,
        sign_requests: req.sign_requests,
        enabled: true,
        created_at: String::new(),
    };
    let result = state.saml.register_saml_idp(user.id, config).await?;
    Ok(Json(json!({"success": true, "data": result})))
}

async fn register_oidc_handler(
    State(state): State<SamlState>,
    headers: HeaderMap,
    Json(req): Json<RegisterOidcIdpRequest>,
) -> Result<impl IntoResponse, StackhouseError> {
    let user = extract_auth_user(&state.auth, &headers)?;
    let scopes = if req.scopes.is_empty() {
        vec!["openid".into(), "email".into(), "profile".into()]
    } else {
        req.scopes
    };
    let config = OidcIdpConfig {
        id: String::new(),
        org_id: user.id,
        name: req.name,
        issuer: req.issuer,
        client_id: req.client_id,
        client_secret: req.client_secret,
        authorization_endpoint: req.authorization_endpoint,
        token_endpoint: req.token_endpoint,
        userinfo_endpoint: req.userinfo_endpoint,
        jwks_uri: req.jwks_uri,
        scopes,
        enabled: true,
        created_at: String::new(),
    };
    let result = state.saml.register_oidc_idp(user.id, config).await?;
    Ok(Json(json!({"success": true, "data": result})))
}

async fn list_idps_handler(
    State(state): State<SamlState>,
    headers: HeaderMap,
) -> Result<impl IntoResponse, StackhouseError> {
    let user = extract_auth_user(&state.auth, &headers)?;
    let idps = state.saml.list_idps(user.id).await?;
    Ok(Json(json!({"success": true, "data": idps})))
}

pub fn create_saml_router(state: SamlState) -> Router {
    Router::new()
        .route("/saml/metadata", get(metadata_handler))
        .route("/saml/login", get(saml_login_handler))
        .route("/saml/acs", post(saml_acs_handler))
        .route("/sso/oidc/login", get(oidc_login_handler))
        .route("/sso/oidc/callback", get(oidc_callback_handler))
        .route("/sso/saml/idp", post(register_saml_handler))
        .route("/sso/oidc/idp", post(register_oidc_handler))
        .route("/sso/idps", get(list_idps_handler))
        .with_state(state)
}
