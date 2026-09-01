//! # Web Application Firewall (WAF)
//!
//! Provides DDoS protection, rate limiting, IP reputation scoring,
//! and request pattern detection for all public endpoints.

use axum::{
    body::Body,
    extract::ConnectInfo,
    http::{Request, StatusCode},
    middleware::Next,
    response::Response,
};
use dashmap::DashMap;
use serde::{Deserialize, Serialize};
use std::net::SocketAddr;
use std::time::{Duration, Instant};
use tracing::warn;

// ============================================================================
// Configuration
// ============================================================================

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct WafConfig {
    pub enabled: bool,
    pub rate_limit_requests_per_second: u32,
    pub rate_limit_burst: u32,
    pub rate_limit_window_secs: u64,
    pub block_duration_secs: u64,
    pub max_request_body_bytes: usize,
    pub max_uri_length: usize,
    pub blocked_user_agents: Vec<String>,
    pub blocked_patterns: Vec<String>,
    pub geo_block_countries: Vec<String>,
    pub ip_reputation_threshold: f64,
}

impl Default for WafConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            rate_limit_requests_per_second: 100,
            rate_limit_burst: 200,
            rate_limit_window_secs: 60,
            block_duration_secs: 300,
            max_request_body_bytes: 10 * 1024 * 1024, // 10MB
            max_uri_length: 2048,
            blocked_user_agents: vec![
                "sqlmap".to_string(),
                "nikto".to_string(),
                "nmap".to_string(),
                "masscan".to_string(),
            ],
            blocked_patterns: vec![
                "/../".to_string(),
                "/etc/passwd".to_string(),
                "/proc/self".to_string(),
                "<script>".to_string(),
                "UNION SELECT".to_string(),
                "DROP TABLE".to_string(),
                "'; --".to_string(),
            ],
            geo_block_countries: Vec::new(),
            ip_reputation_threshold: 0.3,
        }
    }
}

impl WafConfig {
    pub fn from_env() -> Self {
        let mut config = Self::default();
        if let Ok(val) = std::env::var("STACKHOUSE_WAF_ENABLED") {
            config.enabled = val == "1" || val == "true";
        }
        if let Ok(val) = std::env::var("STACKHOUSE_WAF_RATE_LIMIT_RPS") {
            config.rate_limit_requests_per_second = val.parse().unwrap_or(100);
        }
        if let Ok(val) = std::env::var("STACKHOUSE_WAF_MAX_BODY_BYTES") {
            config.max_request_body_bytes = val.parse().unwrap_or(10 * 1024 * 1024);
        }
        if let Ok(val) = std::env::var("STACKHOUSE_WAF_BLOCKED_COUNTRIES") {
            config.geo_block_countries = val.split(',').map(|s| s.trim().to_uppercase()).collect();
        }
        config
    }
}

// ============================================================================
// Rate Limiter (Token Bucket)
// ============================================================================

#[derive(Debug, Clone)]
struct TokenBucket {
    tokens: f64,
    max_tokens: f64,
    refill_rate: f64, // tokens per second
    last_refill: Instant,
}

impl TokenBucket {
    fn new(max_tokens: f64, refill_rate: f64) -> Self {
        Self {
            tokens: max_tokens,
            max_tokens,
            refill_rate,
            last_refill: Instant::now(),
        }
    }

    fn try_consume(&mut self) -> bool {
        let now = Instant::now();
        let elapsed = now.duration_since(self.last_refill).as_secs_f64();
        self.tokens = (self.tokens + elapsed * self.refill_rate).min(self.max_tokens);
        self.last_refill = now;

        if self.tokens >= 1.0 {
            self.tokens -= 1.0;
            true
        } else {
            false
        }
    }
}

// ============================================================================
// IP Reputation
// ============================================================================

#[derive(Debug, Clone)]
struct IpReputation {
    score: f64, // 0.0 = malicious, 1.0 = trusted
    violations: u32,
    last_violation: Option<Instant>,
    blocked_until: Option<Instant>,
}

impl Default for IpReputation {
    fn default() -> Self {
        Self {
            score: 1.0,
            violations: 0,
            last_violation: None,
            blocked_until: None,
        }
    }
}

impl IpReputation {
    fn record_violation(&mut self, block_duration: Duration) {
        self.violations += 1;
        self.last_violation = Some(Instant::now());
        self.score = (self.score - 0.1).max(0.0);

        if self.violations >= 5 {
            self.blocked_until = Some(Instant::now() + block_duration);
        }
    }

    fn is_blocked(&self) -> bool {
        if let Some(until) = self.blocked_until {
            Instant::now() < until
        } else {
            false
        }
    }

    fn decay(&mut self) {
        if let Some(last) = self.last_violation {
            let elapsed = Instant::now().duration_since(last).as_secs();
            if elapsed > 3600 {
                self.score = (self.score + 0.01).min(1.0);
                if elapsed > 7200 && self.violations > 0 {
                    self.violations -= 1;
                }
            }
        }
    }
}

// ============================================================================
// WAF Service
// ============================================================================

pub struct WafService {
    config: WafConfig,
    rate_limiters: DashMap<String, TokenBucket>,
    ip_reputations: DashMap<String, IpReputation>,
}

impl WafService {
    pub fn new(config: WafConfig) -> Self {
        Self {
            config,
            rate_limiters: DashMap::new(),
            ip_reputations: DashMap::new(),
        }
    }

    pub fn from_env() -> Self {
        Self::new(WafConfig::from_env())
    }

    /// Check if a request should be allowed
    pub fn check_request(
        &self,
        ip: &str,
        uri: &str,
        user_agent: &str,
        body_size: usize,
    ) -> Result<(), WafDecision> {
        if !self.config.enabled {
            return Ok(());
        }

        // Check IP block
        if let Some(mut rep) = self.ip_reputations.get_mut(ip) {
            rep.decay();
            if rep.is_blocked() {
                return Err(WafDecision::Blocked("IP temporarily blocked".into()));
            }
            if rep.score < self.config.ip_reputation_threshold {
                return Err(WafDecision::Blocked("IP reputation too low".into()));
            }
        }

        // Check rate limit
        let mut bucket = self.rate_limiters.entry(ip.to_string()).or_insert_with(|| {
            TokenBucket::new(
                self.config.rate_limit_burst as f64,
                self.config.rate_limit_requests_per_second as f64,
            )
        });
        if !bucket.try_consume() {
            self.record_violation(ip);
            return Err(WafDecision::RateLimited);
        }

        // Check URI length
        if uri.len() > self.config.max_uri_length {
            self.record_violation(ip);
            return Err(WafDecision::Blocked("URI too long".into()));
        }

        // Check body size
        if body_size > self.config.max_request_body_bytes {
            return Err(WafDecision::Blocked("Request body too large".into()));
        }

        // Check blocked user agents
        let ua_lower = user_agent.to_lowercase();
        for blocked in &self.config.blocked_user_agents {
            if ua_lower.contains(&blocked.to_lowercase()) {
                self.record_violation(ip);
                return Err(WafDecision::Blocked(format!(
                    "Blocked user agent: {}",
                    blocked
                )));
            }
        }

        // Check blocked patterns in URI
        let uri_lower = uri.to_lowercase();
        for pattern in &self.config.blocked_patterns {
            if uri_lower.contains(&pattern.to_lowercase()) {
                self.record_violation(ip);
                return Err(WafDecision::Blocked(format!(
                    "Blocked pattern detected: {}",
                    pattern
                )));
            }
        }

        Ok(())
    }

    fn record_violation(&self, ip: &str) {
        let block_duration = Duration::from_secs(self.config.block_duration_secs);
        self.ip_reputations
            .entry(ip.to_string())
            .or_default()
            .record_violation(block_duration);
    }

    /// Get stats for monitoring
    pub fn stats(&self) -> WafStats {
        let total_ips = self.rate_limiters.len();
        let blocked_ips = self
            .ip_reputations
            .iter()
            .filter(|r| r.is_blocked())
            .count();
        let low_reputation_ips = self
            .ip_reputations
            .iter()
            .filter(|r| r.score < self.config.ip_reputation_threshold)
            .count();

        WafStats {
            total_tracked_ips: total_ips,
            blocked_ips,
            low_reputation_ips,
            config_enabled: self.config.enabled,
        }
    }

    /// Manually block an IP
    pub fn block_ip(&self, ip: &str, duration_secs: u64) {
        let mut rep = self.ip_reputations.entry(ip.to_string()).or_default();
        rep.blocked_until = Some(Instant::now() + Duration::from_secs(duration_secs));
        rep.score = 0.0;
    }

    /// Manually unblock an IP
    pub fn unblock_ip(&self, ip: &str) {
        if let Some(mut rep) = self.ip_reputations.get_mut(ip) {
            rep.blocked_until = None;
            rep.score = 1.0;
            rep.violations = 0;
        }
    }
}

#[derive(Debug)]
pub enum WafDecision {
    RateLimited,
    Blocked(String),
}

#[derive(Debug, Serialize)]
pub struct WafStats {
    pub total_tracked_ips: usize,
    pub blocked_ips: usize,
    pub low_reputation_ips: usize,
    pub config_enabled: bool,
}

/// Axum middleware for WAF
pub async fn waf_middleware(
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    request: Request<Body>,
    next: Next,
) -> Response<Body> {
    let waf = WafService::from_env();
    let ip = addr.ip().to_string();
    let uri = request.uri().path().to_string();
    let user_agent = request
        .headers()
        .get("user-agent")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");
    let content_length: usize = request
        .headers()
        .get("content-length")
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.parse().ok())
        .unwrap_or(0);

    match waf.check_request(&ip, &uri, user_agent, content_length) {
        Ok(()) => next.run(request).await,
        Err(WafDecision::RateLimited) => {
            warn!("WAF: Rate limited IP {}", ip);
            Response::builder()
                .status(StatusCode::TOO_MANY_REQUESTS)
                .header("Retry-After", "60")
                .header("X-RateLimit-Remaining", "0")
                .body(Body::from(
                    r#"{"error":"rate_limited","message":"Too many requests"}"#,
                ))
                .unwrap()
        }
        Err(WafDecision::Blocked(reason)) => {
            warn!("WAF: Blocked IP {} - {}", ip, reason);
            Response::builder()
                .status(StatusCode::FORBIDDEN)
                .body(Body::from(format!(
                    r#"{{"error":"blocked","message":"{}"}}"#,
                    reason
                )))
                .unwrap()
        }
    }
}
