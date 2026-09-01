//! # Bot Detection & Brute-Force Protection
//!
//! Rate limiting with progressive lockout, bot fingerprinting,
//! IP reputation scoring, and automatic blocking after suspicious activity.

use crate::db::{SqlValue, StackhouseStore};
use crate::error::StackhouseResult;

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::RwLock;
use tracing::{info, warn};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BotProtectionConfig {
    pub max_login_attempts: u32,
    pub lockout_duration_secs: u64,
    pub progressive_lockout: bool,
    pub lockout_multiplier: f64,
    pub suspicious_threshold: u32,
    pub block_known_bots: bool,
    pub require_captcha_after: u32,
    pub ip_rate_limit_per_minute: u32,
}

impl Default for BotProtectionConfig {
    fn default() -> Self {
        Self {
            max_login_attempts: 5,
            lockout_duration_secs: 900, // 15 minutes
            progressive_lockout: true,
            lockout_multiplier: 2.0,
            suspicious_threshold: 10,
            block_known_bots: true,
            require_captcha_after: 3,
            ip_rate_limit_per_minute: 100,
        }
    }
}

#[derive(Debug, Clone)]
struct IpTracker {
    attempts: u32,
    lockout_count: u32,
    first_attempt: Instant,
    last_attempt: Instant,
    locked_until: Option<Instant>,
    fingerprints: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProtectionResult {
    pub allowed: bool,
    pub reason: Option<String>,
    pub require_captcha: bool,
    pub remaining_attempts: u32,
    pub lockout_until: Option<String>,
    pub risk_score: f64,
}

#[derive(Clone)]
pub struct BotProtectionService {
    store: Arc<StackhouseStore>,
    config: BotProtectionConfig,
    trackers: Arc<RwLock<HashMap<String, IpTracker>>>,
    blocked_ips: Arc<RwLock<std::collections::HashSet<String>>>,
}

impl BotProtectionService {
    pub async fn new(
        store: Arc<StackhouseStore>,
        config: BotProtectionConfig,
    ) -> StackhouseResult<Self> {
        let service = Self {
            store,
            config,
            trackers: Arc::new(RwLock::new(HashMap::new())),
            blocked_ips: Arc::new(RwLock::new(std::collections::HashSet::new())),
        };
        service.initialize_tables().await?;
        service.start_cleanup_worker();
        info!("🤖 Bot protection service initialized");
        Ok(service)
    }

    async fn initialize_tables(&self) -> StackhouseResult<()> {
        self.store.execute_batch(r#"
            CREATE TABLE IF NOT EXISTS stackhouse_blocked_ips (
                ip_address TEXT PRIMARY KEY,
                reason TEXT NOT NULL,
                blocked_at TIMESTAMPTZ DEFAULT NOW(),
                expires_at TIMESTAMPTZ,
                manual BOOLEAN DEFAULT FALSE
            );
            CREATE TABLE IF NOT EXISTS stackhouse_login_attempts (
                id BIGSERIAL PRIMARY KEY,
                ip_address TEXT NOT NULL,
                email TEXT,
                success BOOLEAN NOT NULL,
                user_agent TEXT,
                fingerprint TEXT,
                timestamp TIMESTAMPTZ DEFAULT NOW()
            );
            CREATE INDEX IF NOT EXISTS idx_login_attempts_ip ON stackhouse_login_attempts(ip_address, timestamp);
            CREATE INDEX IF NOT EXISTS idx_login_attempts_email ON stackhouse_login_attempts(email, timestamp);
        "#.to_string()).await?;
        Ok(())
    }

    fn start_cleanup_worker(&self) {
        let trackers = Arc::clone(&self.trackers);
        tokio::spawn(async move {
            let mut interval = tokio::time::interval(Duration::from_secs(300));
            loop {
                interval.tick().await;
                let mut t = trackers.write().await;
                t.retain(|_, v| v.last_attempt.elapsed() < Duration::from_secs(3600));
            }
        });
    }

    /// Check if a request should be allowed (called before login attempts)
    pub async fn check_request(
        &self,
        ip: &str,
        _email: Option<&str>,
        user_agent: Option<&str>,
    ) -> StackhouseResult<ProtectionResult> {
        // Check if IP is permanently blocked
        if self.blocked_ips.read().await.contains(ip) {
            return Ok(ProtectionResult {
                allowed: false,
                reason: Some("IP address is blocked".into()),
                require_captcha: false,
                remaining_attempts: 0,
                lockout_until: None,
                risk_score: 1.0,
            });
        }

        let mut trackers = self.trackers.write().await;
        let tracker = trackers.entry(ip.to_string()).or_insert(IpTracker {
            attempts: 0,
            lockout_count: 0,
            first_attempt: Instant::now(),
            last_attempt: Instant::now(),
            locked_until: None,
            fingerprints: Vec::new(),
        });

        // Check if currently locked out
        if let Some(locked_until) = tracker.locked_until {
            if Instant::now() < locked_until {
                let remaining = locked_until.duration_since(Instant::now()).as_secs();
                return Ok(ProtectionResult {
                    allowed: false,
                    reason: Some(format!(
                        "Account locked. Try again in {} seconds",
                        remaining
                    )),
                    require_captcha: false,
                    remaining_attempts: 0,
                    lockout_until: Some(format!("{}s", remaining)),
                    risk_score: 0.9,
                });
            } else {
                // Lockout expired
                tracker.locked_until = None;
                tracker.attempts = 0;
            }
        }

        tracker.attempts += 1;
        tracker.last_attempt = Instant::now();

        if let Some(ua) = user_agent {
            let fp = format!("{:x}", md5::compute(ua.as_bytes()));
            if !tracker.fingerprints.contains(&fp) {
                tracker.fingerprints.push(fp);
            }
        }

        // Risk scoring
        let risk_score = self.calculate_risk(tracker);

        let require_captcha = tracker.attempts >= self.config.require_captcha_after;
        let remaining = self
            .config
            .max_login_attempts
            .saturating_sub(tracker.attempts);

        if tracker.attempts >= self.config.max_login_attempts {
            // Lock the account
            let lockout_secs = if self.config.progressive_lockout {
                (self.config.lockout_duration_secs as f64
                    * self
                        .config
                        .lockout_multiplier
                        .powi(tracker.lockout_count as i32)) as u64
            } else {
                self.config.lockout_duration_secs
            };

            tracker.locked_until = Some(Instant::now() + Duration::from_secs(lockout_secs));
            tracker.lockout_count += 1;
            tracker.attempts = 0;

            warn!(
                "🔒 IP {} locked out for {}s (lockout #{})",
                ip, lockout_secs, tracker.lockout_count
            );

            // Block IP if too many lockouts
            if tracker.lockout_count >= 5 {
                drop(trackers);
                self.block_ip(ip, "Excessive lockouts").await?;
            }

            return Ok(ProtectionResult {
                allowed: false,
                reason: Some(format!(
                    "Too many attempts. Locked for {} seconds",
                    lockout_secs
                )),
                require_captcha: false,
                remaining_attempts: 0,
                lockout_until: Some(format!("{}s", lockout_secs)),
                risk_score,
            });
        }

        Ok(ProtectionResult {
            allowed: true,
            reason: None,
            require_captcha,
            remaining_attempts: remaining,
            lockout_until: None,
            risk_score,
        })
    }

    /// Record a login attempt (call after check_request)
    pub async fn record_attempt(
        &self,
        ip: &str,
        email: Option<&str>,
        success: bool,
        user_agent: Option<&str>,
    ) {
        if success {
            // Reset tracker on successful login
            self.trackers.write().await.remove(ip);
        }

        self.store.execute(
            "INSERT INTO stackhouse_login_attempts (ip_address, email, success, user_agent) VALUES (?, ?, ?, ?)".to_string(),
            vec![
                SqlValue::Text(ip.to_string()),
                SqlValue::Text(email.unwrap_or("").to_string()),
                SqlValue::Text(success.to_string()),
                SqlValue::Text(user_agent.unwrap_or("").to_string()),
            ],
        ).await.ok();
    }

    /// Block an IP address
    pub async fn block_ip(&self, ip: &str, reason: &str) -> StackhouseResult<()> {
        self.blocked_ips.write().await.insert(ip.to_string());
        self.store.execute(
            "INSERT INTO stackhouse_blocked_ips (ip_address, reason) VALUES (?, ?) ON CONFLICT (ip_address) DO NOTHING".to_string(),
            vec![SqlValue::Text(ip.to_string()), SqlValue::Text(reason.to_string())],
        ).await?;
        Ok(())
    }

    /// Unblock an IP
    pub async fn unblock_ip(&self, ip: &str) -> StackhouseResult<()> {
        self.blocked_ips.write().await.remove(ip);
        self.store
            .execute(
                "DELETE FROM stackhouse_blocked_ips WHERE ip_address = ?".to_string(),
                vec![SqlValue::Text(ip.to_string())],
            )
            .await?;
        Ok(())
    }

    fn calculate_risk(&self, tracker: &IpTracker) -> f64 {
        let mut score = 0.0;
        // High attempt count
        score += (tracker.attempts as f64 / self.config.max_login_attempts as f64).min(0.5);
        // Multiple lockouts
        score += (tracker.lockout_count as f64 * 0.15).min(0.3);
        // Multiple fingerprints from same IP
        if tracker.fingerprints.len() > 3 {
            score += 0.2;
        }
        score.min(1.0)
    }
}
