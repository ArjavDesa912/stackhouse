//! # Security Headers Middleware
//!
//! Enforces security headers on all API responses:
//! HSTS, CSP, X-Frame-Options, X-Content-Type-Options, etc.

use axum::{
    body::Body,
    http::{header::HeaderName, HeaderValue, Request, Response},
    middleware::Next,
};

/// Security header configuration
#[derive(Clone, Debug)]
pub struct SecurityHeadersConfig {
    pub hsts_max_age: u64,
    pub hsts_include_subdomains: bool,
    pub hsts_preload: bool,
    pub csp_policy: String,
    pub frame_options: String,
    pub content_type_options: bool,
    pub xss_protection: bool,
    pub referrer_policy: String,
    pub permissions_policy: String,
    pub cross_origin_embedder_policy: String,
    pub cross_origin_opener_policy: String,
    pub cross_origin_resource_policy: String,
}

impl Default for SecurityHeadersConfig {
    fn default() -> Self {
        Self {
            hsts_max_age: 31536000, // 1 year
            hsts_include_subdomains: true,
            hsts_preload: true,
            csp_policy: "default-src 'self'; script-src 'self' 'unsafe-inline'; style-src 'self' 'unsafe-inline'; img-src 'self' data: https:; font-src 'self' data:; connect-src 'self' https:; frame-ancestors 'none'".to_string(),
            frame_options: "DENY".to_string(),
            content_type_options: true,
            xss_protection: true,
            referrer_policy: "strict-origin-when-cross-origin".to_string(),
            permissions_policy: "camera=(), microphone=(), geolocation=(), payment=()".to_string(),
            cross_origin_embedder_policy: "require-corp".to_string(),
            cross_origin_opener_policy: "same-origin".to_string(),
            cross_origin_resource_policy: "same-origin".to_string(),
        }
    }
}

impl SecurityHeadersConfig {
    pub fn from_env() -> Self {
        let mut config = Self::default();

        if let Ok(max_age) = std::env::var("STACKHOUSE_HSTS_MAX_AGE") {
            config.hsts_max_age = max_age.parse().unwrap_or(config.hsts_max_age);
        }
        if let Ok(csp) = std::env::var("STACKHOUSE_CSP_POLICY") {
            config.csp_policy = csp;
        }
        if let Ok(frame) = std::env::var("STACKHOUSE_FRAME_OPTIONS") {
            config.frame_options = frame;
        }
        if let Ok(referrer) = std::env::var("STACKHOUSE_REFERRER_POLICY") {
            config.referrer_policy = referrer;
        }

        config
    }

    fn hsts_value(&self) -> String {
        let mut value = format!("max-age={}", self.hsts_max_age);
        if self.hsts_include_subdomains {
            value.push_str("; includeSubDomains");
        }
        if self.hsts_preload {
            value.push_str("; preload");
        }
        value
    }
}

/// Middleware that adds security headers to all responses
pub async fn security_headers_middleware(request: Request<Body>, next: Next) -> Response<Body> {
    let config = SecurityHeadersConfig::from_env();
    let mut response = next.run(request).await;
    let headers = response.headers_mut();

    // Strict-Transport-Security
    if let Ok(val) = HeaderValue::from_str(&config.hsts_value()) {
        headers.insert(HeaderName::from_static("strict-transport-security"), val);
    }

    // Content-Security-Policy
    if let Ok(val) = HeaderValue::from_str(&config.csp_policy) {
        headers.insert(HeaderName::from_static("content-security-policy"), val);
    }

    // X-Frame-Options
    if let Ok(val) = HeaderValue::from_str(&config.frame_options) {
        headers.insert(HeaderName::from_static("x-frame-options"), val);
    }

    // X-Content-Type-Options
    if config.content_type_options {
        headers.insert(
            HeaderName::from_static("x-content-type-options"),
            HeaderValue::from_static("nosniff"),
        );
    }

    // X-XSS-Protection
    if config.xss_protection {
        headers.insert(
            HeaderName::from_static("x-xss-protection"),
            HeaderValue::from_static("1; mode=block"),
        );
    }

    // Referrer-Policy
    if let Ok(val) = HeaderValue::from_str(&config.referrer_policy) {
        headers.insert(HeaderName::from_static("referrer-policy"), val);
    }

    // Permissions-Policy
    if let Ok(val) = HeaderValue::from_str(&config.permissions_policy) {
        headers.insert(HeaderName::from_static("permissions-policy"), val);
    }

    // Cross-Origin-Embedder-Policy
    if let Ok(val) = HeaderValue::from_str(&config.cross_origin_embedder_policy) {
        headers.insert(HeaderName::from_static("cross-origin-embedder-policy"), val);
    }

    // Cross-Origin-Opener-Policy
    if let Ok(val) = HeaderValue::from_str(&config.cross_origin_opener_policy) {
        headers.insert(HeaderName::from_static("cross-origin-opener-policy"), val);
    }

    // Cross-Origin-Resource-Policy
    if let Ok(val) = HeaderValue::from_str(&config.cross_origin_resource_policy) {
        headers.insert(HeaderName::from_static("cross-origin-resource-policy"), val);
    }

    // Remove Server header to avoid information disclosure
    headers.remove("server");

    response
}
