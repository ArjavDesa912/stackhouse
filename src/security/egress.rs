//! # Network Egress Controls
//!
//! Allowlist which external domains serverless functions and agents can call.
//! Per-tenant and per-function domain-based filtering.

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
use dashmap::DashMap;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::sync::Arc;
use tracing::info;

// ============================================================================
// Types
// ============================================================================

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EgressRule {
    pub id: String,
    pub tenant_id: i64,
    pub name: String,
    pub rule_type: EgressRuleType,
    pub domains: Vec<String>,
    pub ports: Vec<u16>,
    pub protocol: String,
    pub enabled: bool,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EgressRuleType {
    Allow,
    Deny,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EgressPolicy {
    pub tenant_id: i64,
    pub default_action: EgressRuleType,
    pub rules: Vec<EgressRule>,
}

// ============================================================================
// Egress Service
// ============================================================================

#[derive(Clone)]
pub struct EgressService {
    store: Arc<StackhouseStore>,
    // In-memory cache for fast lookup
    policies: Arc<DashMap<i64, EgressPolicy>>,
}

impl EgressService {
    pub async fn new(store: Arc<StackhouseStore>) -> StackhouseResult<Self> {
        let service = Self {
            store,
            policies: Arc::new(DashMap::new()),
        };
        service.initialize_tables().await?;
        info!("🌐 Network egress controls initialized");
        Ok(service)
    }

    async fn initialize_tables(&self) -> StackhouseResult<()> {
        self.store.execute_batch(r#"
            CREATE TABLE IF NOT EXISTS stackhouse_egress_rules (
                id TEXT PRIMARY KEY,
                tenant_id BIGINT NOT NULL,
                name TEXT NOT NULL,
                rule_type TEXT NOT NULL DEFAULT 'allow',
                domains TEXT NOT NULL DEFAULT '[]',
                ports TEXT NOT NULL DEFAULT '[]',
                protocol TEXT NOT NULL DEFAULT 'https',
                enabled BOOLEAN DEFAULT TRUE,
                created_at TIMESTAMPTZ DEFAULT NOW()
            );
            CREATE INDEX IF NOT EXISTS idx_egress_rules_tenant ON stackhouse_egress_rules(tenant_id);

            CREATE TABLE IF NOT EXISTS stackhouse_egress_policies (
                tenant_id BIGINT PRIMARY KEY,
                default_action TEXT NOT NULL DEFAULT 'allow',
                updated_at TIMESTAMPTZ DEFAULT NOW()
            );
        "#.to_string()).await?;
        Ok(())
    }

    /// Check if a domain is allowed for egress
    pub fn check_egress(&self, tenant_id: i64, domain: &str, port: u16) -> bool {
        if let Some(policy) = self.policies.get(&tenant_id) {
            let domain_lower = domain.to_lowercase();

            for rule in &policy.rules {
                if !rule.enabled {
                    continue;
                }

                let domain_match = rule.domains.iter().any(|d| {
                    let d_lower = d.to_lowercase();
                    if d_lower.starts_with("*.") {
                        domain_lower.ends_with(&d_lower[1..]) || domain_lower == d_lower[2..]
                    } else {
                        domain_lower == d_lower
                    }
                });

                let port_match = rule.ports.is_empty() || rule.ports.contains(&port);

                if domain_match && port_match {
                    return matches!(rule.rule_type, EgressRuleType::Allow);
                }
            }

            // Fall through to default
            matches!(policy.default_action, EgressRuleType::Allow)
        } else {
            // No policy = allow all
            true
        }
    }

    /// Set the default egress policy for a tenant
    pub async fn set_default_policy(
        &self,
        tenant_id: i64,
        default_action: EgressRuleType,
    ) -> StackhouseResult<()> {
        let action_str = match &default_action {
            EgressRuleType::Allow => "allow",
            EgressRuleType::Deny => "deny",
        };

        self.store.execute(
            r#"INSERT INTO stackhouse_egress_policies (tenant_id, default_action, updated_at) VALUES (?, ?, NOW())
               ON CONFLICT (tenant_id) DO UPDATE SET default_action = EXCLUDED.default_action, updated_at = NOW()"#.to_string(),
            vec![
                SqlValue::Integer(tenant_id),
                SqlValue::Text(action_str.to_string()),
            ],
        ).await?;

        // Update cache
        self.reload_policy(tenant_id).await?;
        Ok(())
    }

    /// Add an egress rule
    pub async fn add_rule(
        &self,
        tenant_id: i64,
        name: &str,
        rule_type: EgressRuleType,
        domains: Vec<String>,
        ports: Vec<u16>,
    ) -> StackhouseResult<EgressRule> {
        let id = uuid::Uuid::new_v4().to_string();
        let type_str = match &rule_type {
            EgressRuleType::Allow => "allow",
            EgressRuleType::Deny => "deny",
        };

        self.store.execute(
            "INSERT INTO stackhouse_egress_rules (id, tenant_id, name, rule_type, domains, ports) VALUES (?, ?, ?, ?, ?, ?)".to_string(),
            vec![
                SqlValue::Text(id.clone()),
                SqlValue::Integer(tenant_id),
                SqlValue::Text(name.to_string()),
                SqlValue::Text(type_str.to_string()),
                SqlValue::Text(serde_json::to_string(&domains).unwrap_or_default()),
                SqlValue::Text(serde_json::to_string(&ports).unwrap_or_default()),
            ],
        ).await?;

        let rule = EgressRule {
            id,
            tenant_id,
            name: name.to_string(),
            rule_type,
            domains,
            ports,
            protocol: "https".to_string(),
            enabled: true,
            created_at: chrono::Utc::now().to_rfc3339(),
        };

        self.reload_policy(tenant_id).await?;
        Ok(rule)
    }

    /// Delete an egress rule
    pub async fn delete_rule(&self, rule_id: &str, tenant_id: i64) -> StackhouseResult<()> {
        self.store
            .execute(
                "DELETE FROM stackhouse_egress_rules WHERE id = ? AND tenant_id = ?".to_string(),
                vec![
                    SqlValue::Text(rule_id.to_string()),
                    SqlValue::Integer(tenant_id),
                ],
            )
            .await?;
        self.reload_policy(tenant_id).await?;
        Ok(())
    }

    /// List egress rules for a tenant
    pub async fn list_rules(&self, tenant_id: i64) -> StackhouseResult<Vec<Value>> {
        let rows = self.store.query(
            "SELECT id, name, rule_type, domains, ports, protocol, enabled, created_at FROM stackhouse_egress_rules WHERE tenant_id = ? ORDER BY created_at".to_string(),
            vec![SqlValue::Integer(tenant_id)],
        ).await?;
        Ok(rows
            .into_iter()
            .map(|r| json!(r.into_iter().collect::<std::collections::HashMap<_, _>>()))
            .collect())
    }

    async fn reload_policy(&self, tenant_id: i64) -> StackhouseResult<()> {
        let policy_rows = self
            .store
            .query(
                "SELECT default_action FROM stackhouse_egress_policies WHERE tenant_id = ?"
                    .to_string(),
                vec![SqlValue::Integer(tenant_id)],
            )
            .await?;

        let default_action = policy_rows
            .first()
            .and_then(|r| r.iter().find(|(k, _)| k == "default_action"))
            .and_then(|(_, v)| v.as_str())
            .map(|s| {
                if s == "deny" {
                    EgressRuleType::Deny
                } else {
                    EgressRuleType::Allow
                }
            })
            .unwrap_or(EgressRuleType::Allow);

        let rule_rows = self.store.query(
            "SELECT id, name, rule_type, domains, ports, protocol, enabled, created_at FROM stackhouse_egress_rules WHERE tenant_id = ? AND enabled = true".to_string(),
            vec![SqlValue::Integer(tenant_id)],
        ).await?;

        let rules: Vec<EgressRule> = rule_rows
            .into_iter()
            .map(|r| {
                let get = |key: &str| r.iter().find(|(k, _)| k == key).map(|(_, v)| v.clone());
                EgressRule {
                    id: get("id")
                        .and_then(|v| v.as_str().map(String::from))
                        .unwrap_or_default(),
                    tenant_id,
                    name: get("name")
                        .and_then(|v| v.as_str().map(String::from))
                        .unwrap_or_default(),
                    rule_type: if get("rule_type")
                        .and_then(|v| v.as_str().map(String::from))
                        .unwrap_or_default()
                        == "deny"
                    {
                        EgressRuleType::Deny
                    } else {
                        EgressRuleType::Allow
                    },
                    domains: get("domains")
                        .and_then(|v| v.as_str().and_then(|s| serde_json::from_str(s).ok()))
                        .unwrap_or_default(),
                    ports: get("ports")
                        .and_then(|v| v.as_str().and_then(|s| serde_json::from_str(s).ok()))
                        .unwrap_or_default(),
                    protocol: get("protocol")
                        .and_then(|v| v.as_str().map(String::from))
                        .unwrap_or_else(|| "https".into()),
                    enabled: true,
                    created_at: get("created_at")
                        .and_then(|v| v.as_str().map(String::from))
                        .unwrap_or_default(),
                }
            })
            .collect();

        self.policies.insert(
            tenant_id,
            EgressPolicy {
                tenant_id,
                default_action,
                rules,
            },
        );
        Ok(())
    }
}

// ============================================================================
// Router
// ============================================================================

#[derive(Clone)]
pub struct EgressState {
    pub egress: Arc<EgressService>,
    pub auth: AuthState,
}

#[derive(Deserialize)]
struct AddRuleRequest {
    name: String,
    rule_type: String,
    domains: Vec<String>,
    #[serde(default)]
    ports: Vec<u16>,
}

#[derive(Deserialize)]
struct SetPolicyRequest {
    default_action: String,
}

async fn list_rules_handler(
    State(state): State<EgressState>,
    headers: HeaderMap,
) -> Result<impl IntoResponse, StackhouseError> {
    let user = extract_auth_user(&state.auth, &headers)?;
    let rules = state.egress.list_rules(user.id).await?;
    Ok(Json(json!({"success": true, "data": rules})))
}

async fn add_rule_handler(
    State(state): State<EgressState>,
    headers: HeaderMap,
    Json(req): Json<AddRuleRequest>,
) -> Result<impl IntoResponse, StackhouseError> {
    let user = extract_auth_user(&state.auth, &headers)?;
    let rule_type = if req.rule_type == "deny" {
        EgressRuleType::Deny
    } else {
        EgressRuleType::Allow
    };
    let rule = state
        .egress
        .add_rule(user.id, &req.name, rule_type, req.domains, req.ports)
        .await?;
    Ok(Json(json!({"success": true, "data": rule})))
}

async fn delete_rule_handler(
    State(state): State<EgressState>,
    headers: HeaderMap,
    axum::extract::Path(rule_id): axum::extract::Path<String>,
) -> Result<impl IntoResponse, StackhouseError> {
    let user = extract_auth_user(&state.auth, &headers)?;
    state.egress.delete_rule(&rule_id, user.id).await?;
    Ok(Json(json!({"success": true, "message": "Rule deleted"})))
}

async fn set_policy_handler(
    State(state): State<EgressState>,
    headers: HeaderMap,
    Json(req): Json<SetPolicyRequest>,
) -> Result<impl IntoResponse, StackhouseError> {
    let user = extract_auth_user(&state.auth, &headers)?;
    let action = if req.default_action == "deny" {
        EgressRuleType::Deny
    } else {
        EgressRuleType::Allow
    };
    state.egress.set_default_policy(user.id, action).await?;
    Ok(Json(json!({"success": true, "message": "Policy updated"})))
}

pub fn create_egress_router(state: EgressState) -> Router {
    Router::new()
        .route("/rules", get(list_rules_handler))
        .route("/rules", post(add_rule_handler))
        .route("/rules/:id", delete(delete_rule_handler))
        .route("/policy", post(set_policy_handler))
        .with_state(state)
}
