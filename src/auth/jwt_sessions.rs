//! # JWT Session Management with Refresh Rotation & Revocation
//!
//! Manages JWT access tokens with short expiry, refresh tokens with rotation
//! (new refresh token on every use), and secure revocation lists.

use crate::db::{SqlValue, StackhouseStore};
use crate::error::{StackhouseError, StackhouseResult};

use chrono::{Duration, Utc};
use jsonwebtoken::{decode, encode, DecodingKey, EncodingKey, Header, Validation};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::info;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AccessToken {
    pub jti: String,
    pub sub: String,
    pub tenant_id: i64,
    pub roles: Vec<String>,
    pub email: String,
    pub exp: i64,
    pub iat: i64,
    #[serde(rename = "type")]
    pub type_field: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RefreshToken {
    pub jti: String,
    pub sub: String,
    pub tenant_id: i64,
    pub access_jti: String,
    pub device_id: Option<String>,
    pub exp: i64,
    pub iat: i64,
    #[serde(rename = "type")]
    pub type_field: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TokenPair {
    pub access_token: String,
    pub refresh_token: String,
    pub access_expires_in: u64,
    pub refresh_expires_in: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RevokedToken {
    pub jti: String,
    pub reason: String,
    pub revoked_at: String,
}

#[derive(Clone)]
pub struct JwtSessionService {
    store: Arc<StackhouseStore>,
    access_secret: Vec<u8>,
    refresh_secret: Vec<u8>,
    revoked_cache: Arc<RwLock<std::collections::HashSet<String>>>,
    access_ttl_minutes: i64,
    refresh_ttl_days: i64,
}

impl JwtSessionService {
    pub async fn new(store: Arc<StackhouseStore>) -> StackhouseResult<Self> {
        let access_secret = std::env::var("JWT_ACCESS_SECRET")
            .unwrap_or_else(|_| "stackhouse-access-secret-change-me".into())
            .into_bytes();
        let refresh_secret = std::env::var("JWT_REFRESH_SECRET")
            .unwrap_or_else(|_| "stackhouse-refresh-secret-change-me".into())
            .into_bytes();

        let service = Self {
            store,
            access_secret,
            refresh_secret,
            revoked_cache: Arc::new(RwLock::new(std::collections::HashSet::new())),
            access_ttl_minutes: std::env::var("JWT_ACCESS_TTL_MIN")
                .ok()
                .and_then(|s| s.parse().ok())
                .unwrap_or(15),
            refresh_ttl_days: std::env::var("JWT_REFRESH_TTL_DAYS")
                .ok()
                .and_then(|s| s.parse().ok())
                .unwrap_or(30),
        };
        service.initialize_tables().await?;
        info!("🔑 JWT session service initialized");
        Ok(service)
    }

    async fn initialize_tables(&self) -> StackhouseResult<()> {
        self.store
            .execute_batch(
                r#"
            CREATE TABLE IF NOT EXISTS stackhouse_revoked_tokens (
                jti TEXT PRIMARY KEY,
                reason TEXT NOT NULL,
                revoked_at TIMESTAMPTZ DEFAULT NOW()
            );
            CREATE TABLE IF NOT EXISTS stackhouse_sessions (
                id TEXT PRIMARY KEY,
                user_id TEXT NOT NULL,
                tenant_id BIGINT NOT NULL,
                device_id TEXT,
                ip_address TEXT,
                user_agent TEXT,
                access_jti TEXT,
                refresh_jti TEXT,
                expires_at TIMESTAMPTZ,
                last_used TIMESTAMPTZ DEFAULT NOW()
            );
            CREATE INDEX IF NOT EXISTS idx_sessions_user ON stackhouse_sessions(user_id, tenant_id);
            CREATE INDEX IF NOT EXISTS idx_revoked_jti ON stackhouse_revoked_tokens(jti);
        "#
                .to_string(),
            )
            .await?;
        Ok(())
    }

    fn encode_access(&self, claims: &AccessToken) -> StackhouseResult<String> {
        encode(
            &Header::default(),
            claims,
            &EncodingKey::from_secret(&self.access_secret),
        )
        .map_err(|e| StackhouseError::Internal(anyhow::anyhow!("JWT encode error: {}", e)))
    }

    fn decode_access(&self, token: &str) -> StackhouseResult<AccessToken> {
        let mut validation = Validation::default();
        validation.validate_exp = false;
        decode::<AccessToken>(
            token,
            &DecodingKey::from_secret(&self.access_secret),
            &validation,
        )
        .map(|data| data.claims)
        .map_err(|e| StackhouseError::Unauthorized(format!("Invalid access token: {}", e)))
    }

    fn encode_refresh(&self, claims: &RefreshToken) -> StackhouseResult<String> {
        encode(
            &Header::default(),
            claims,
            &EncodingKey::from_secret(&self.refresh_secret),
        )
        .map_err(|e| StackhouseError::Internal(anyhow::anyhow!("JWT encode error: {}", e)))
    }

    fn decode_refresh(&self, token: &str) -> StackhouseResult<RefreshToken> {
        let mut validation = Validation::default();
        validation.validate_exp = false;
        decode::<RefreshToken>(
            token,
            &DecodingKey::from_secret(&self.refresh_secret),
            &validation,
        )
        .map(|data| data.claims)
        .map_err(|e| StackhouseError::Unauthorized(format!("Invalid refresh token: {}", e)))
    }

    /// Create a new token pair for a user
    pub async fn create_tokens(
        &self,
        user_id: &str,
        tenant_id: i64,
        email: &str,
        roles: Vec<String>,
        device_id: Option<&str>,
        ip: Option<&str>,
        ua: Option<&str>,
    ) -> StackhouseResult<TokenPair> {
        let access_jti = uuid::Uuid::new_v4().to_string();
        let refresh_jti = uuid::Uuid::new_v4().to_string();
        let now = Utc::now();

        let access_claims = AccessToken {
            jti: access_jti.clone(),
            sub: user_id.to_string(),
            tenant_id,
            roles,
            email: email.to_string(),
            exp: (now + Duration::minutes(self.access_ttl_minutes)).timestamp(),
            iat: now.timestamp(),
            type_field: "access".to_string(),
        };

        let refresh_claims = RefreshToken {
            jti: refresh_jti.clone(),
            sub: user_id.to_string(),
            tenant_id,
            access_jti: access_jti.clone(),
            device_id: device_id.map(|s| s.to_string()),
            exp: (now + Duration::days(self.refresh_ttl_days)).timestamp(),
            iat: now.timestamp(),
            type_field: "refresh".to_string(),
        };

        let access_token = self.encode_access(&access_claims)?;
        let refresh_token = self.encode_refresh(&refresh_claims)?;

        // Store session
        let session_id = uuid::Uuid::new_v4().to_string();
        self.store.execute(
            "INSERT INTO stackhouse_sessions (id, user_id, tenant_id, device_id, ip_address, user_agent, access_jti, refresh_jti, expires_at) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?::timestamptz)".to_string(),
            vec![
                SqlValue::Text(session_id),
                SqlValue::Text(user_id.to_string()),
                SqlValue::Integer(tenant_id),
                SqlValue::Text(device_id.unwrap_or("").to_string()),
                SqlValue::Text(ip.unwrap_or("").to_string()),
                SqlValue::Text(ua.unwrap_or("").to_string()),
                SqlValue::Text(access_jti.clone()),
                SqlValue::Text(refresh_jti.clone()),
                SqlValue::Text((now + Duration::days(self.refresh_ttl_days)).to_rfc3339()),
            ],
        ).await?;

        Ok(TokenPair {
            access_token,
            refresh_token,
            access_expires_in: (self.access_ttl_minutes * 60) as u64,
            refresh_expires_in: (self.refresh_ttl_days * 86400) as u64,
        })
    }

    /// Verify an access token
    pub async fn verify_access(&self, token: &str) -> StackhouseResult<AccessToken> {
        let claims = self.decode_access(token)?;
        if Utc::now().timestamp() > claims.exp {
            return Err(StackhouseError::Unauthorized("Access token expired".into()));
        }
        if self.is_revoked(&claims.jti).await {
            return Err(StackhouseError::Unauthorized("Token revoked".into()));
        }
        Ok(claims)
    }

    /// Refresh token rotation — returns a new pair, invalidates old refresh
    pub async fn refresh(
        &self,
        refresh_token: &str,
        ip: Option<&str>,
        ua: Option<&str>,
    ) -> StackhouseResult<TokenPair> {
        let claims = self.decode_refresh(refresh_token)?;

        if Utc::now().timestamp() > claims.exp {
            return Err(StackhouseError::Unauthorized(
                "Refresh token expired".into(),
            ));
        }

        if self.is_revoked(&claims.jti).await {
            return Err(StackhouseError::Unauthorized(
                "Refresh token revoked".into(),
            ));
        }

        // Revoke the used refresh token (rotation)
        self.revoke(&claims.jti, "token rotation").await?;

        // Also revoke the associated access token
        self.revoke(&claims.access_jti, "refresh rotation").await?;

        // Get user details to create new pair
        let rows = self
            .store
            .query(
                "SELECT email, roles FROM stackhouse_users WHERE id = ?".to_string(),
                vec![SqlValue::Text(claims.sub.clone())],
            )
            .await?;

        let email = rows
            .first()
            .and_then(|r| r.iter().find(|(k, _)| k == "email"))
            .and_then(|(_, v)| v.as_str())
            .unwrap_or("");
        let roles_str = rows
            .first()
            .and_then(|r| r.iter().find(|(k, _)| k == "roles"))
            .and_then(|(_, v)| v.as_str())
            .unwrap_or("[]");
        let roles: Vec<String> = serde_json::from_str(roles_str).unwrap_or_default();

        self.create_tokens(
            &claims.sub,
            claims.tenant_id,
            email,
            roles,
            claims.device_id.as_deref(),
            ip,
            ua,
        )
        .await
    }

    /// Revoke a token by JTI
    pub async fn revoke(&self, jti: &str, reason: &str) -> StackhouseResult<()> {
        self.store.execute(
            "INSERT INTO stackhouse_revoked_tokens (jti, reason) VALUES (?, ?) ON CONFLICT (jti) DO NOTHING".to_string(),
            vec![SqlValue::Text(jti.to_string()), SqlValue::Text(reason.to_string())],
        ).await?;
        self.revoked_cache.write().await.insert(jti.to_string());
        Ok(())
    }

    /// Revoke all sessions for a user
    pub async fn revoke_all_for_user(
        &self,
        user_id: &str,
        tenant_id: i64,
        reason: &str,
    ) -> StackhouseResult<u32> {
        let rows = self.store.query(
            "SELECT access_jti, refresh_jti FROM stackhouse_sessions WHERE user_id = ? AND tenant_id = ?".to_string(),
            vec![SqlValue::Text(user_id.to_string()), SqlValue::Integer(tenant_id)],
        ).await?;

        let mut count = 0u32;
        for row in rows {
            if let Some(jti) = row
                .iter()
                .find(|(k, _)| k == "access_jti")
                .and_then(|(_, v)| v.as_str())
            {
                self.revoke(jti, reason).await?;
                count += 1;
            }
            if let Some(jti) = row
                .iter()
                .find(|(k, _)| k == "refresh_jti")
                .and_then(|(_, v)| v.as_str())
            {
                self.revoke(jti, reason).await?;
                count += 1;
            }
        }

        self.store
            .execute(
                "DELETE FROM stackhouse_sessions WHERE user_id = ? AND tenant_id = ?".to_string(),
                vec![
                    SqlValue::Text(user_id.to_string()),
                    SqlValue::Integer(tenant_id),
                ],
            )
            .await?;

        Ok(count)
    }

    async fn is_revoked(&self, jti: &str) -> bool {
        if self.revoked_cache.read().await.contains(jti) {
            return true;
        }
        match self
            .store
            .query(
                "SELECT 1 FROM stackhouse_revoked_tokens WHERE jti = ?".to_string(),
                vec![SqlValue::Text(jti.to_string())],
            )
            .await
        {
            Ok(rows) if !rows.is_empty() => true,
            _ => false,
        }
    }

    /// List active sessions for a user
    pub async fn list_sessions(
        &self,
        user_id: &str,
        tenant_id: i64,
    ) -> StackhouseResult<Vec<Value>> {
        let rows = self.store.query(
            "SELECT id, device_id, ip_address, last_used, expires_at FROM stackhouse_sessions WHERE user_id = ? AND tenant_id = ? ORDER BY last_used DESC".to_string(),
            vec![SqlValue::Text(user_id.to_string()), SqlValue::Integer(tenant_id)],
        ).await?;
        Ok(rows
            .into_iter()
            .map(|r| json!(r.into_iter().collect::<HashMap<_, _>>()))
            .collect())
    }

    /// Cleanup expired revoked tokens
    pub async fn cleanup(&self) -> StackhouseResult<u64> {
        let result1 = self.store.execute(
            "DELETE FROM stackhouse_revoked_tokens WHERE revoked_at < NOW() - INTERVAL '90 days'".to_string(),
            vec![],
        ).await?;
        let result2 = self
            .store
            .execute(
                "DELETE FROM stackhouse_sessions WHERE expires_at < NOW()".to_string(),
                vec![],
            )
            .await?;
        Ok(result1 + result2)
    }
}
