//! # Network Restrictions Module (Stackhouse-Network)
//!
//! IP allowlisting and network security controls.
//! Provides middleware for IP-based access control.

use crate::api::admin::AdminAuditService;
use crate::auth::{extract_auth_user, AuthState, AuthUser};
use crate::authorization::AuthorizationService;
use axum::{
    body::{to_bytes, Body},
    extract::ConnectInfo,
    extract::State,
    http::{HeaderMap, Request},
    middleware::Next,
    response::{IntoResponse, Response},
    routing::{get, post},
    Json, Router,
};
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::net::{IpAddr, SocketAddr};
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{info, warn};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct NetworkRule {
    pub ip: String, // IP or CIDR
    pub description: String,
    pub enabled: bool,
}

#[derive(Clone)]
pub struct NetworkService {
    allowlist: Arc<RwLock<Vec<NetworkRule>>>,
    enabled: Arc<RwLock<bool>>,
}

impl NetworkService {
    pub fn new() -> Self {
        info!("🔒 Stackhouse-Network initialized");
        Self {
            allowlist: Arc::new(RwLock::new(Vec::new())),
            enabled: Arc::new(RwLock::new(false)),
        }
    }

    pub async fn enable(&self) {
        *self.enabled.write().await = true;
        info!("🔒 Network restrictions enabled");
    }

    pub async fn disable(&self) {
        *self.enabled.write().await = false;
        info!("🔓 Network restrictions disabled");
    }

    pub async fn add_rule(&self, rule: NetworkRule) {
        self.allowlist.write().await.push(rule);
    }

    pub async fn remove_rule(&self, ip: &str) {
        self.allowlist.write().await.retain(|r| r.ip != ip);
    }

    pub async fn list_rules(&self) -> Vec<NetworkRule> {
        self.allowlist.read().await.clone()
    }

    pub async fn is_allowed(&self, ip: &str) -> bool {
        let enabled = *self.enabled.read().await;
        if !enabled {
            return true;
        }

        let rules = self.allowlist.read().await;
        if rules.is_empty() {
            return false;
        }

        for rule in rules.iter() {
            if !rule.enabled {
                continue;
            }
            if self.matches_rule(&rule.ip, ip) {
                return true;
            }
        }

        false
    }

    fn matches_rule(&self, rule: &str, ip: &str) -> bool {
        // Exact match
        if rule == ip {
            return true;
        }

        // CIDR match (simple /24, /16, /8)
        if rule.contains('/') {
            let parts: Vec<&str> = rule.split('/').collect();
            if parts.len() == 2 {
                if let (Ok(rule_ip), Ok(prefix_len)) =
                    (parts[0].parse::<IpAddr>(), parts[1].parse::<u32>())
                {
                    if let Ok(check_ip) = ip.parse::<IpAddr>() {
                        match (rule_ip, check_ip) {
                            (IpAddr::V4(r), IpAddr::V4(c)) => {
                                let mask = if prefix_len == 0 {
                                    0
                                } else {
                                    !0u32 << (32 - prefix_len)
                                };
                                return (u32::from(r) & mask) == (u32::from(c) & mask);
                            }
                            _ => {}
                        }
                    }
                }
            }
        }

        // Wildcard match (e.g., "192.168.*")
        if rule.contains('*') {
            let pattern = rule.replace('.', "\\.").replace('*', ".*");
            if let Ok(re) = regex::Regex::new(&format!("^{}$", pattern)) {
                return re.is_match(ip);
            }
        }

        false
    }
}

/// Network restriction middleware
pub async fn network_middleware(
    State(network): State<Arc<NetworkService>>,
    request: Request<Body>,
    next: Next,
) -> Response<Body> {
    let client_ip = request
        .extensions()
        .get::<ConnectInfo<SocketAddr>>()
        .map(|ConnectInfo(addr)| addr.ip().to_string());

    if *network.enabled.read().await {
        let Some(ip) = client_ip.as_deref() else {
            warn!("🚫 Blocked request without connection info");
            return (
                axum::http::StatusCode::FORBIDDEN,
                Json(json!({"error": "Access denied by network policy"})),
            )
                .into_response();
        };

        if !network.is_allowed(ip).await {
            warn!("🚫 Blocked request from IP: {}", ip);
            return (
                axum::http::StatusCode::FORBIDDEN,
                Json(json!({"error": "Access denied by network policy"})),
            )
                .into_response();
        }
    }

    next.run(request).await
}

// ============================================================================
// Handlers & Router
// ============================================================================

#[derive(Clone)]
pub struct NetworkState {
    pub network: Arc<NetworkService>,
    pub auth: AuthState,
    pub authorization: AuthorizationService,
    pub admin_audit: Arc<AdminAuditService>,
}

#[derive(Deserialize)]
struct AddRuleRequest {
    ip: String,
    #[serde(default)]
    description: String,
}

async fn list_rules_handler(
    State(state): State<NetworkState>,
    headers: HeaderMap,
) -> Result<impl IntoResponse, crate::error::StackhouseError> {
    let auth_user = require_service_admin(&state, &headers).await?;
    let rules = state.network.list_rules().await;
    let enabled = *state.network.enabled.read().await;
    state
        .admin_audit
        .record(
            auth_user.id,
            "network.list_rules",
            "network_rule",
            None,
            "success",
            json!({"route": "/v1/admin/network/rules", "enabled": enabled, "count": rules.len()}),
        )
        .await?;
    Ok(Json(
        json!({"success": true, "enabled": enabled, "data": rules}),
    ))
}

async fn add_rule_handler(
    State(state): State<NetworkState>,
    request: Request<Body>,
) -> Result<impl IntoResponse, crate::error::StackhouseError> {
    let auth_user = require_service_admin(&state, request.headers()).await?;
    let body = to_bytes(request.into_body(), usize::MAX)
        .await
        .map_err(|e| {
            crate::error::StackhouseError::InvalidPayload(format!("Invalid request body: {}", e))
        })?;
    let req: AddRuleRequest = serde_json::from_slice(&body)?;
    state
        .network
        .add_rule(NetworkRule {
            ip: req.ip.clone(),
            description: req.description,
            enabled: true,
        })
        .await;
    state
        .admin_audit
        .record(
            auth_user.id,
            "network.add_rule",
            "network_rule",
            Some(req.ip.clone()),
            "success",
            json!({"route": "/v1/admin/network/rules"}),
        )
        .await?;
    Ok(Json(
        json!({"success": true, "message": format!("Added rule for {}", req.ip)}),
    ))
}

async fn enable_handler(
    State(state): State<NetworkState>,
    headers: HeaderMap,
) -> Result<impl IntoResponse, crate::error::StackhouseError> {
    let auth_user = require_service_admin(&state, &headers).await?;
    state.network.enable().await;
    state
        .admin_audit
        .record(
            auth_user.id,
            "network.enable",
            "network_policy",
            None,
            "success",
            json!({"route": "/v1/admin/network/enable"}),
        )
        .await?;
    Ok(Json(
        json!({"success": true, "message": "Network restrictions enabled"}),
    ))
}

async fn disable_handler(
    State(state): State<NetworkState>,
    headers: HeaderMap,
) -> Result<impl IntoResponse, crate::error::StackhouseError> {
    let auth_user = require_service_admin(&state, &headers).await?;
    state.network.disable().await;
    state
        .admin_audit
        .record(
            auth_user.id,
            "network.disable",
            "network_policy",
            None,
            "success",
            json!({"route": "/v1/admin/network/disable"}),
        )
        .await?;
    Ok(Json(
        json!({"success": true, "message": "Network restrictions disabled"}),
    ))
}

pub fn create_network_router(state: NetworkState) -> Router {
    Router::new()
        .route(
            "/network/rules",
            get(list_rules_handler).post(add_rule_handler),
        )
        .route("/network/enable", post(enable_handler))
        .route("/network/disable", post(disable_handler))
        .with_state(state)
}

async fn require_service_admin(
    state: &NetworkState,
    headers: &HeaderMap,
) -> Result<AuthUser, crate::error::StackhouseError> {
    let auth_user = extract_auth_user(&state.auth, headers)?;
    let user = state.auth.auth.get_user_by_id(auth_user.id).await?;
    state
        .authorization
        .require_service_admin_unconditional(&user)?;
    Ok(auth_user)
}
