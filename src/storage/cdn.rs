//! # CDN Integration & Signed URLs
//!
//! Generate time-limited signed URLs for private objects.
//! CDN origin configuration for edge caching.

use crate::error::{StackhouseError, StackhouseResult};
use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine as _};
use hmac::{Hmac, Mac};
use serde::{Deserialize, Serialize};
use sha2::Sha256;
use std::time::{SystemTime, UNIX_EPOCH};

type HmacSha256 = Hmac<Sha256>;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SignedUrl {
    pub url: String,
    pub expires_at: u64,
    pub bucket: String,
    pub key: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CdnConfig {
    pub origin_url: String,
    pub cdn_domain: Option<String>,
    pub cache_ttl_secs: u64,
    pub signing_key: String,
}

impl CdnConfig {
    pub fn from_env() -> Self {
        Self {
            origin_url: std::env::var("STACKHOUSE_STORAGE_ORIGIN")
                .unwrap_or_else(|_| "http://localhost:8080".into()),
            cdn_domain: std::env::var("STACKHOUSE_CDN_DOMAIN").ok(),
            cache_ttl_secs: std::env::var("STACKHOUSE_CDN_CACHE_TTL")
                .ok()
                .and_then(|v| v.parse().ok())
                .unwrap_or(3600),
            signing_key: std::env::var("STACKHOUSE_SIGNED_URL_KEY")
                .unwrap_or_else(|_| "default-signing-key-change-me".into()),
        }
    }
}

#[derive(Clone)]
pub struct CdnService {
    config: CdnConfig,
}

impl CdnService {
    pub fn new(config: CdnConfig) -> Self {
        Self { config }
    }

    pub fn from_env() -> Self {
        Self::new(CdnConfig::from_env())
    }

    /// Generate a signed URL for a private object
    pub fn generate_signed_url(
        &self,
        bucket: &str,
        key: &str,
        expires_in_secs: u64,
    ) -> StackhouseResult<SignedUrl> {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(|e| StackhouseError::Internal(anyhow::anyhow!("Time error: {}", e)))?;
        let expires_at = now.as_secs() + expires_in_secs;

        let base_url = self
            .config
            .cdn_domain
            .as_deref()
            .map(|d| format!("https://{}", d))
            .unwrap_or_else(|| self.config.origin_url.clone());

        let path = format!("/v1/storage/{}/{}", bucket, key);
        let string_to_sign = format!("{}:{}:{}", path, expires_at, bucket);

        let mut mac = HmacSha256::new_from_slice(self.config.signing_key.as_bytes())
            .map_err(|e| StackhouseError::Internal(anyhow::anyhow!("HMAC init failed: {}", e)))?;
        mac.update(string_to_sign.as_bytes());
        let signature = URL_SAFE_NO_PAD.encode(mac.finalize().into_bytes());

        let url = format!(
            "{}{}?expires={}&signature={}",
            base_url, path, expires_at, signature
        );

        Ok(SignedUrl {
            url,
            expires_at,
            bucket: bucket.to_string(),
            key: key.to_string(),
        })
    }

    /// Verify a signed URL signature
    pub fn verify_signature(
        &self,
        path: &str,
        expires: u64,
        bucket: &str,
        signature: &str,
    ) -> bool {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
        if now > expires {
            return false; // Expired
        }

        let string_to_sign = format!("{}:{}:{}", path, expires, bucket);
        let mut mac = match HmacSha256::new_from_slice(self.config.signing_key.as_bytes()) {
            Ok(m) => m,
            Err(_) => return false,
        };
        mac.update(string_to_sign.as_bytes());
        let expected = URL_SAFE_NO_PAD.encode(mac.finalize().into_bytes());

        expected == signature
    }

    /// Get CDN cache headers for a response
    pub fn cache_headers(&self, is_public: bool) -> Vec<(String, String)> {
        let mut headers = Vec::new();
        if is_public {
            headers.push((
                "Cache-Control".into(),
                format!("public, max-age={}", self.config.cache_ttl_secs),
            ));
        } else {
            headers.push(("Cache-Control".into(), "private, no-cache".into()));
        }
        headers.push(("X-CDN-Origin".into(), self.config.origin_url.clone()));
        if let Some(ref domain) = self.config.cdn_domain {
            headers.push(("X-CDN-Domain".into(), domain.clone()));
        }
        headers
    }
}
