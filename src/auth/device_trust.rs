//! # Device Trust Policies
//!
//! Device fingerprint tracking, trust policies, and suspicious login detection.
//! Tracks known devices per user and alerts on unknown device access.

use crate::auth::{extract_auth_user, AuthState};
use crate::db::{SqlValue, StackhouseStore};
use crate::error::{StackhouseError, StackhouseResult};

use axum::{
    extract::State,
    http::HeaderMap,
    response::IntoResponse,
    routing::{delete, get, post},
    Json, Router,
};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::sync::Arc;
use tracing::{info, warn};

// ============================================================================
// Types
// ============================================================================

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TrustedDevice {
    pub id: String,
    pub user_id: i64,
    pub fingerprint: String,
    pub name: String,
    pub device_type: String, // desktop, mobile, tablet
    pub browser: String,
    pub os: String,
    pub ip_address: String,
    pub trusted: bool,
    pub last_seen_at: String,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DevicePolicy {
    pub id: String,
    pub tenant_id: i64,
    pub require_device_approval: bool,
    pub max_devices_per_user: u32,
    pub block_unknown_devices: bool,
    pub notify_on_new_device: bool,
    pub auto_expire_days: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeviceLoginEvent {
    pub id: String,
    pub user_id: i64,
    pub fingerprint: String,
    pub ip_address: String,
    pub user_agent: String,
    pub is_known_device: bool,
    pub risk_score: f64,
    pub timestamp: String,
}

// ============================================================================
// Device Trust Service
// ============================================================================

#[derive(Clone)]
pub struct DeviceTrustService {
    store: Arc<StackhouseStore>,
}

impl DeviceTrustService {
    pub async fn new(store: Arc<StackhouseStore>) -> StackhouseResult<Self> {
        let service = Self { store };
        service.initialize_tables().await?;
        info!("📱 Device trust service initialized");
        Ok(service)
    }

    async fn initialize_tables(&self) -> StackhouseResult<()> {
        self.store.execute_batch(r#"
            CREATE TABLE IF NOT EXISTS stackhouse_trusted_devices (
                id TEXT PRIMARY KEY,
                user_id BIGINT NOT NULL REFERENCES stackhouse_users(id) ON DELETE CASCADE,
                fingerprint TEXT NOT NULL,
                name TEXT NOT NULL DEFAULT 'Unknown Device',
                device_type TEXT NOT NULL DEFAULT 'desktop',
                browser TEXT,
                os TEXT,
                ip_address TEXT,
                trusted BOOLEAN DEFAULT TRUE,
                last_seen_at TIMESTAMPTZ DEFAULT NOW(),
                created_at TIMESTAMPTZ DEFAULT NOW(),
                UNIQUE(user_id, fingerprint)
            );
            CREATE TABLE IF NOT EXISTS stackhouse_device_policies (
                id TEXT PRIMARY KEY,
                tenant_id BIGINT NOT NULL UNIQUE,
                require_device_approval BOOLEAN DEFAULT FALSE,
                max_devices_per_user INTEGER DEFAULT 10,
                block_unknown_devices BOOLEAN DEFAULT FALSE,
                notify_on_new_device BOOLEAN DEFAULT TRUE,
                auto_expire_days INTEGER DEFAULT 90
            );
            CREATE TABLE IF NOT EXISTS stackhouse_device_login_events (
                id TEXT PRIMARY KEY,
                user_id BIGINT NOT NULL,
                fingerprint TEXT NOT NULL,
                ip_address TEXT,
                user_agent TEXT,
                is_known_device BOOLEAN DEFAULT FALSE,
                risk_score FLOAT DEFAULT 0.0,
                timestamp TIMESTAMPTZ DEFAULT NOW()
            );
            CREATE INDEX IF NOT EXISTS idx_trusted_devices_user ON stackhouse_trusted_devices(user_id);
            CREATE INDEX IF NOT EXISTS idx_device_events_user ON stackhouse_device_login_events(user_id);
            CREATE INDEX IF NOT EXISTS idx_device_events_time ON stackhouse_device_login_events(timestamp);
        "#.to_string()).await?;
        Ok(())
    }

    /// Generate a device fingerprint from request headers
    pub fn generate_fingerprint(ip: &str, user_agent: &str, accept_lang: &str) -> String {
        let input = format!(
            "{}|{}|{}",
            user_agent,
            accept_lang,
            ip.split('.').take(3).collect::<Vec<_>>().join(".")
        );
        let hash = Sha256::digest(input.as_bytes());
        hex::encode(&hash[..16])
    }

    /// Record a login event and check device trust
    pub async fn check_device_login(
        &self,
        user_id: i64,
        ip: &str,
        user_agent: &str,
        accept_lang: &str,
    ) -> StackhouseResult<DeviceLoginEvent> {
        let fingerprint = Self::generate_fingerprint(ip, user_agent, accept_lang);
        let event_id = uuid::Uuid::new_v4().to_string();

        // Check if device is known
        let existing = self.store.query(
            "SELECT id, trusted FROM stackhouse_trusted_devices WHERE user_id = ? AND fingerprint = ?".to_string(),
            vec![SqlValue::Integer(user_id), SqlValue::Text(fingerprint.clone())],
        ).await?;

        let is_known = !existing.is_empty();
        let _is_trusted = existing
            .first()
            .and_then(|r| r.iter().find(|(k, _)| k == "trusted"))
            .and_then(|(_, v)| v.as_str())
            .map(|s| s == "true")
            .unwrap_or(false);

        // Calculate risk score
        let risk_score = self
            .calculate_risk_score(user_id, &fingerprint, ip, is_known)
            .await;

        // Record event
        self.store.execute(
            "INSERT INTO stackhouse_device_login_events (id, user_id, fingerprint, ip_address, user_agent, is_known_device, risk_score) VALUES (?, ?, ?, ?, ?, ?, ?)".to_string(),
            vec![
                SqlValue::Text(event_id.clone()),
                SqlValue::Integer(user_id),
                SqlValue::Text(fingerprint.clone()),
                SqlValue::Text(ip.to_string()),
                SqlValue::Text(user_agent.to_string()),
                SqlValue::Text(is_known.to_string()),
                SqlValue::Text(risk_score.to_string()),
            ],
        ).await?;

        // If known device, update last_seen
        if is_known {
            self.store.execute(
                "UPDATE stackhouse_trusted_devices SET last_seen_at = NOW(), ip_address = ? WHERE user_id = ? AND fingerprint = ?".to_string(),
                vec![SqlValue::Text(ip.to_string()), SqlValue::Integer(user_id), SqlValue::Text(fingerprint.clone())],
            ).await.ok();
        } else {
            // Register as new (untrusted) device
            let device_info = Self::parse_user_agent(user_agent);
            let device_id = uuid::Uuid::new_v4().to_string();
            self.store.execute(
                "INSERT INTO stackhouse_trusted_devices (id, user_id, fingerprint, name, device_type, browser, os, ip_address, trusted) VALUES (?, ?, ?, ?, ?, ?, ?, ?, FALSE) ON CONFLICT (user_id, fingerprint) DO NOTHING".to_string(),
                vec![
                    SqlValue::Text(device_id),
                    SqlValue::Integer(user_id),
                    SqlValue::Text(fingerprint.clone()),
                    SqlValue::Text(device_info.name),
                    SqlValue::Text(device_info.device_type),
                    SqlValue::Text(device_info.browser),
                    SqlValue::Text(device_info.os),
                    SqlValue::Text(ip.to_string()),
                ],
            ).await.ok();

            if risk_score > 0.7 {
                warn!(
                    "⚠️ High-risk login detected for user {}: risk={:.2}",
                    user_id, risk_score
                );
            }
        }

        Ok(DeviceLoginEvent {
            id: event_id,
            user_id,
            fingerprint,
            ip_address: ip.to_string(),
            user_agent: user_agent.to_string(),
            is_known_device: is_known,
            risk_score,
            timestamp: chrono::Utc::now().to_rfc3339(),
        })
    }

    /// Trust a device
    pub async fn trust_device(&self, user_id: i64, device_id: &str) -> StackhouseResult<()> {
        self.store
            .execute(
                "UPDATE stackhouse_trusted_devices SET trusted = TRUE WHERE id = ? AND user_id = ?"
                    .to_string(),
                vec![
                    SqlValue::Text(device_id.to_string()),
                    SqlValue::Integer(user_id),
                ],
            )
            .await?;
        Ok(())
    }

    /// Revoke device trust
    pub async fn revoke_device(&self, user_id: i64, device_id: &str) -> StackhouseResult<()> {
        self.store
            .execute(
                "DELETE FROM stackhouse_trusted_devices WHERE id = ? AND user_id = ?".to_string(),
                vec![
                    SqlValue::Text(device_id.to_string()),
                    SqlValue::Integer(user_id),
                ],
            )
            .await?;
        Ok(())
    }

    /// List user's devices
    pub async fn list_devices(&self, user_id: i64) -> StackhouseResult<Vec<Value>> {
        let rows = self.store.query(
            "SELECT id, name, device_type, browser, os, ip_address, trusted, last_seen_at, created_at FROM stackhouse_trusted_devices WHERE user_id = ? ORDER BY last_seen_at DESC".to_string(),
            vec![SqlValue::Integer(user_id)],
        ).await?;
        Ok(rows
            .into_iter()
            .map(|r| json!(r.into_iter().collect::<std::collections::HashMap<_, _>>()))
            .collect())
    }

    /// Get recent login events
    pub async fn get_login_events(
        &self,
        user_id: i64,
        limit: usize,
    ) -> StackhouseResult<Vec<Value>> {
        let rows = self.store.query(
            format!("SELECT id, fingerprint, ip_address, is_known_device, risk_score, timestamp FROM stackhouse_device_login_events WHERE user_id = ? ORDER BY timestamp DESC LIMIT {}", limit),
            vec![SqlValue::Integer(user_id)],
        ).await?;
        Ok(rows
            .into_iter()
            .map(|r| json!(r.into_iter().collect::<std::collections::HashMap<_, _>>()))
            .collect())
    }

    /// Set device policy for a tenant
    pub async fn set_policy(&self, tenant_id: i64, policy: DevicePolicy) -> StackhouseResult<()> {
        self.store.execute(
            r#"INSERT INTO stackhouse_device_policies (id, tenant_id, require_device_approval, max_devices_per_user, block_unknown_devices, notify_on_new_device, auto_expire_days)
               VALUES (?, ?, ?, ?, ?, ?, ?)
               ON CONFLICT (tenant_id) DO UPDATE SET
               require_device_approval = EXCLUDED.require_device_approval,
               max_devices_per_user = EXCLUDED.max_devices_per_user,
               block_unknown_devices = EXCLUDED.block_unknown_devices,
               notify_on_new_device = EXCLUDED.notify_on_new_device,
               auto_expire_days = EXCLUDED.auto_expire_days"#.to_string(),
            vec![
                SqlValue::Text(uuid::Uuid::new_v4().to_string()),
                SqlValue::Integer(tenant_id),
                SqlValue::Text(policy.require_device_approval.to_string()),
                SqlValue::Integer(policy.max_devices_per_user as i64),
                SqlValue::Text(policy.block_unknown_devices.to_string()),
                SqlValue::Text(policy.notify_on_new_device.to_string()),
                SqlValue::Integer(policy.auto_expire_days as i64),
            ],
        ).await?;
        Ok(())
    }

    async fn calculate_risk_score(
        &self,
        user_id: i64,
        _fingerprint: &str,
        ip: &str,
        is_known: bool,
    ) -> f64 {
        let mut score: f64 = 0.0;

        if !is_known {
            score += 0.4;
        }

        // Check if IP is new for this user
        let ip_rows = self.store.query(
            "SELECT COUNT(*) as cnt FROM stackhouse_device_login_events WHERE user_id = ? AND ip_address = ? AND timestamp > NOW() - INTERVAL '30 days'".to_string(),
            vec![SqlValue::Integer(user_id), SqlValue::Text(ip.to_string())],
        ).await.unwrap_or_default();

        let ip_count = ip_rows
            .first()
            .and_then(|r| r.iter().find(|(k, _)| k == "cnt"))
            .and_then(|(_, v)| v.as_i64())
            .unwrap_or(0);

        if ip_count == 0 {
            score += 0.3;
        }

        // Check login frequency (too many recent logins from different devices = suspicious)
        let recent_rows = self.store.query(
            "SELECT COUNT(DISTINCT fingerprint) as cnt FROM stackhouse_device_login_events WHERE user_id = ? AND timestamp > NOW() - INTERVAL '1 hour'".to_string(),
            vec![SqlValue::Integer(user_id)],
        ).await.unwrap_or_default();

        let recent_devices = recent_rows
            .first()
            .and_then(|r| r.iter().find(|(k, _)| k == "cnt"))
            .and_then(|(_, v)| v.as_i64())
            .unwrap_or(0);

        if recent_devices > 3 {
            score += 0.2;
        }

        score.min(1.0)
    }

    fn parse_user_agent(ua: &str) -> DeviceInfo {
        let ua_lower = ua.to_lowercase();
        let device_type = if ua_lower.contains("mobile")
            || ua_lower.contains("android")
            || ua_lower.contains("iphone")
        {
            "mobile"
        } else if ua_lower.contains("tablet") || ua_lower.contains("ipad") {
            "tablet"
        } else {
            "desktop"
        };

        let browser = if ua_lower.contains("chrome") && !ua_lower.contains("edge") {
            "Chrome"
        } else if ua_lower.contains("firefox") {
            "Firefox"
        } else if ua_lower.contains("safari") && !ua_lower.contains("chrome") {
            "Safari"
        } else if ua_lower.contains("edge") {
            "Edge"
        } else {
            "Unknown"
        };

        let os = if ua_lower.contains("windows") {
            "Windows"
        } else if ua_lower.contains("mac os") || ua_lower.contains("macos") {
            "macOS"
        } else if ua_lower.contains("linux") {
            "Linux"
        } else if ua_lower.contains("android") {
            "Android"
        } else if ua_lower.contains("ios") || ua_lower.contains("iphone") {
            "iOS"
        } else {
            "Unknown"
        };

        DeviceInfo {
            name: format!("{} on {}", browser, os),
            device_type: device_type.to_string(),
            browser: browser.to_string(),
            os: os.to_string(),
        }
    }
}

struct DeviceInfo {
    name: String,
    device_type: String,
    browser: String,
    os: String,
}

// ============================================================================
// Router
// ============================================================================

#[derive(Clone)]
pub struct DeviceTrustState {
    pub devices: Arc<DeviceTrustService>,
    pub auth: AuthState,
}

#[derive(Deserialize)]
struct TrustDeviceRequest {
    device_id: String,
}

async fn list_devices_handler(
    State(state): State<DeviceTrustState>,
    headers: HeaderMap,
) -> Result<impl IntoResponse, StackhouseError> {
    let user = extract_auth_user(&state.auth, &headers)?;
    let devices = state.devices.list_devices(user.id).await?;
    Ok(Json(json!({"success": true, "data": devices})))
}

async fn trust_device_handler(
    State(state): State<DeviceTrustState>,
    headers: HeaderMap,
    Json(req): Json<TrustDeviceRequest>,
) -> Result<impl IntoResponse, StackhouseError> {
    let user = extract_auth_user(&state.auth, &headers)?;
    state.devices.trust_device(user.id, &req.device_id).await?;
    Ok(Json(json!({"success": true, "message": "Device trusted"})))
}

async fn revoke_device_handler(
    State(state): State<DeviceTrustState>,
    headers: HeaderMap,
    axum::extract::Path(device_id): axum::extract::Path<String>,
) -> Result<impl IntoResponse, StackhouseError> {
    let user = extract_auth_user(&state.auth, &headers)?;
    state.devices.revoke_device(user.id, &device_id).await?;
    Ok(Json(json!({"success": true, "message": "Device removed"})))
}

async fn login_events_handler(
    State(state): State<DeviceTrustState>,
    headers: HeaderMap,
) -> Result<impl IntoResponse, StackhouseError> {
    let user = extract_auth_user(&state.auth, &headers)?;
    let events = state.devices.get_login_events(user.id, 50).await?;
    Ok(Json(json!({"success": true, "data": events})))
}

pub fn create_device_trust_router(state: DeviceTrustState) -> Router {
    Router::new()
        .route("/devices", get(list_devices_handler))
        .route("/devices/trust", post(trust_device_handler))
        .route("/devices/:id", delete(revoke_device_handler))
        .route("/devices/events", get(login_events_handler))
        .with_state(state)
}
