//! # Attribute-Based Access Control (ABAC) Policy Engine
//!
//! Cedar-style policy DSL with attribute evaluation and policy CRUD API.
//! Supports conditions on user attributes, resource attributes, and environment.

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
use std::collections::HashMap;
use std::sync::Arc;
use tracing::info;

// ============================================================================
// Policy Types
// ============================================================================

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Policy {
    pub id: String,
    pub name: String,
    pub description: String,
    pub effect: PolicyEffect,
    pub principals: Vec<PrincipalMatcher>,
    pub actions: Vec<String>,
    pub resources: Vec<ResourceMatcher>,
    pub conditions: Vec<Condition>,
    pub priority: i32,
    pub enabled: bool,
    pub tenant_id: i64,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PolicyEffect {
    Allow,
    Deny,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PrincipalMatcher {
    pub principal_type: String, // "user", "role", "group", "any"
    pub value: String,          // user_id, role name, group name, or "*"
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResourceMatcher {
    pub resource_type: String, // "table", "bucket", "function", "any"
    pub resource_id: String,   // specific id, prefix pattern, or "*"
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Condition {
    pub attribute: String, // "user.department", "resource.owner", "env.time", "request.ip"
    pub operator: ConditionOp,
    pub value: Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ConditionOp {
    Equals,
    NotEquals,
    Contains,
    In,
    NotIn,
    GreaterThan,
    LessThan,
    StartsWith,
    EndsWith,
    Matches, // regex
    IpInCidr,
    TimeBetween,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuthorizationRequest {
    pub principal: PrincipalContext,
    pub action: String,
    pub resource: ResourceContext,
    pub environment: HashMap<String, Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PrincipalContext {
    pub id: i64,
    pub roles: Vec<String>,
    pub groups: Vec<String>,
    pub attributes: HashMap<String, Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResourceContext {
    pub resource_type: String,
    pub resource_id: String,
    pub attributes: HashMap<String, Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuthorizationDecision {
    pub allowed: bool,
    pub matching_policy: Option<String>,
    pub effect: PolicyEffect,
    pub reason: String,
}

// ============================================================================
// ABAC Engine
// ============================================================================

#[derive(Clone)]
pub struct AbacEngine {
    store: Arc<StackhouseStore>,
}

impl AbacEngine {
    pub async fn new(store: Arc<StackhouseStore>) -> StackhouseResult<Self> {
        let engine = Self { store };
        engine.initialize_tables().await?;
        info!("🛡️ ABAC policy engine initialized");
        Ok(engine)
    }

    async fn initialize_tables(&self) -> StackhouseResult<()> {
        self.store.execute_batch(r#"
            CREATE TABLE IF NOT EXISTS stackhouse_abac_policies (
                id TEXT PRIMARY KEY,
                name TEXT NOT NULL,
                description TEXT DEFAULT '',
                effect TEXT NOT NULL DEFAULT 'deny',
                principals TEXT NOT NULL DEFAULT '[]',
                actions TEXT NOT NULL DEFAULT '[]',
                resources TEXT NOT NULL DEFAULT '[]',
                conditions TEXT NOT NULL DEFAULT '[]',
                priority INTEGER NOT NULL DEFAULT 0,
                enabled BOOLEAN DEFAULT TRUE,
                tenant_id BIGINT NOT NULL,
                created_at TIMESTAMPTZ DEFAULT NOW(),
                updated_at TIMESTAMPTZ DEFAULT NOW()
            );
            CREATE INDEX IF NOT EXISTS idx_abac_policies_tenant ON stackhouse_abac_policies(tenant_id);
            CREATE INDEX IF NOT EXISTS idx_abac_policies_enabled ON stackhouse_abac_policies(enabled);
        "#.to_string()).await?;
        Ok(())
    }

    /// Evaluate an authorization request against all policies
    pub async fn evaluate(
        &self,
        tenant_id: i64,
        request: &AuthorizationRequest,
    ) -> StackhouseResult<AuthorizationDecision> {
        let policies = self.get_active_policies(tenant_id).await?;

        // Sort by priority (higher = evaluated first)
        let mut sorted_policies = policies;
        sorted_policies.sort_by(|a, b| b.priority.cmp(&a.priority));

        // Evaluate each policy — first explicit DENY wins, otherwise first ALLOW wins
        let mut first_allow: Option<&Policy> = None;

        for policy in &sorted_policies {
            if self.matches_policy(policy, request) {
                match policy.effect {
                    PolicyEffect::Deny => {
                        return Ok(AuthorizationDecision {
                            allowed: false,
                            matching_policy: Some(policy.id.clone()),
                            effect: PolicyEffect::Deny,
                            reason: format!("Denied by policy: {}", policy.name),
                        });
                    }
                    PolicyEffect::Allow => {
                        if first_allow.is_none() {
                            first_allow = Some(policy);
                        }
                    }
                }
            }
        }

        if let Some(policy) = first_allow {
            Ok(AuthorizationDecision {
                allowed: true,
                matching_policy: Some(policy.id.clone()),
                effect: PolicyEffect::Allow,
                reason: format!("Allowed by policy: {}", policy.name),
            })
        } else {
            // Default deny
            Ok(AuthorizationDecision {
                allowed: false,
                matching_policy: None,
                effect: PolicyEffect::Deny,
                reason: "No matching allow policy found (implicit deny)".into(),
            })
        }
    }

    fn matches_policy(&self, policy: &Policy, request: &AuthorizationRequest) -> bool {
        // Check principal match
        let principal_match = policy.principals.is_empty()
            || policy
                .principals
                .iter()
                .any(|p| match p.principal_type.as_str() {
                    "any" | "*" => true,
                    "user" => p.value == request.principal.id.to_string() || p.value == "*",
                    "role" => request.principal.roles.contains(&p.value) || p.value == "*",
                    "group" => request.principal.groups.contains(&p.value) || p.value == "*",
                    _ => false,
                });
        if !principal_match {
            return false;
        }

        // Check action match
        let action_match = policy.actions.is_empty()
            || policy.actions.iter().any(|a| {
                a == "*" || a == &request.action || {
                    if let Some(prefix) = a.strip_suffix("*") {
                        request.action.starts_with(prefix)
                    } else {
                        false
                    }
                }
            });
        if !action_match {
            return false;
        }

        // Check resource match
        let resource_match = policy.resources.is_empty()
            || policy.resources.iter().any(|r| {
                let type_match = r.resource_type == "any"
                    || r.resource_type == "*"
                    || r.resource_type == request.resource.resource_type;
                let id_match =
                    r.resource_id == "*" || r.resource_id == request.resource.resource_id || {
                        if let Some(prefix) = r.resource_id.strip_suffix("*") {
                            request.resource.resource_id.starts_with(prefix)
                        } else {
                            false
                        }
                    };
                type_match && id_match
            });
        if !resource_match {
            return false;
        }

        // Check conditions
        let conditions_match = policy
            .conditions
            .iter()
            .all(|c| self.evaluate_condition(c, request));
        conditions_match
    }

    fn evaluate_condition(&self, condition: &Condition, request: &AuthorizationRequest) -> bool {
        let actual_value = self.resolve_attribute(&condition.attribute, request);

        match &condition.operator {
            ConditionOp::Equals => actual_value == Some(condition.value.clone()),
            ConditionOp::NotEquals => actual_value != Some(condition.value.clone()),
            ConditionOp::Contains => {
                if let (Some(Value::String(actual)), Value::String(expected)) =
                    (&actual_value, &condition.value)
                {
                    actual.contains(expected.as_str())
                } else {
                    false
                }
            }
            ConditionOp::In => {
                if let (Some(actual), Value::Array(arr)) = (&actual_value, &condition.value) {
                    arr.contains(actual)
                } else {
                    false
                }
            }
            ConditionOp::NotIn => {
                if let (Some(actual), Value::Array(arr)) = (&actual_value, &condition.value) {
                    !arr.contains(actual)
                } else {
                    true
                }
            }
            ConditionOp::GreaterThan => match (&actual_value, &condition.value) {
                (Some(Value::Number(a)), Value::Number(b)) => {
                    a.as_f64().unwrap_or(0.0) > b.as_f64().unwrap_or(0.0)
                }
                _ => false,
            },
            ConditionOp::LessThan => match (&actual_value, &condition.value) {
                (Some(Value::Number(a)), Value::Number(b)) => {
                    a.as_f64().unwrap_or(0.0) < b.as_f64().unwrap_or(0.0)
                }
                _ => false,
            },
            ConditionOp::StartsWith => {
                if let (Some(Value::String(actual)), Value::String(prefix)) =
                    (&actual_value, &condition.value)
                {
                    actual.starts_with(prefix.as_str())
                } else {
                    false
                }
            }
            ConditionOp::EndsWith => {
                if let (Some(Value::String(actual)), Value::String(suffix)) =
                    (&actual_value, &condition.value)
                {
                    actual.ends_with(suffix.as_str())
                } else {
                    false
                }
            }
            ConditionOp::Matches => {
                if let (Some(Value::String(actual)), Value::String(pattern)) =
                    (&actual_value, &condition.value)
                {
                    regex::Regex::new(pattern)
                        .map(|re| re.is_match(actual))
                        .unwrap_or(false)
                } else {
                    false
                }
            }
            ConditionOp::IpInCidr => {
                if let (Some(Value::String(ip)), Value::String(cidr)) =
                    (&actual_value, &condition.value)
                {
                    self.ip_in_cidr(ip, cidr)
                } else {
                    false
                }
            }
            ConditionOp::TimeBetween => {
                // value should be {"start": "09:00", "end": "17:00"}
                if let Value::Object(range) = &condition.value {
                    let now = chrono::Utc::now().format("%H:%M").to_string();
                    let start = range
                        .get("start")
                        .and_then(|v| v.as_str())
                        .unwrap_or("00:00");
                    let end = range.get("end").and_then(|v| v.as_str()).unwrap_or("23:59");
                    now >= start.to_string() && now <= end.to_string()
                } else {
                    false
                }
            }
        }
    }

    fn resolve_attribute(&self, attribute: &str, request: &AuthorizationRequest) -> Option<Value> {
        let parts: Vec<&str> = attribute.splitn(2, '.').collect();
        if parts.len() != 2 {
            return None;
        }

        match parts[0] {
            "user" | "principal" => request.principal.attributes.get(parts[1]).cloned(),
            "resource" => request.resource.attributes.get(parts[1]).cloned(),
            "env" | "environment" => request.environment.get(parts[1]).cloned(),
            _ => None,
        }
    }

    fn ip_in_cidr(&self, ip: &str, cidr: &str) -> bool {
        // Simple CIDR check for IPv4
        let parts: Vec<&str> = cidr.split('/').collect();
        if parts.len() != 2 {
            return false;
        }

        let cidr_ip: Vec<u8> = parts[0].split('.').filter_map(|s| s.parse().ok()).collect();
        let mask_bits: u32 = parts[1].parse().unwrap_or(32);
        let check_ip: Vec<u8> = ip.split('.').filter_map(|s| s.parse().ok()).collect();

        if cidr_ip.len() != 4 || check_ip.len() != 4 {
            return false;
        }

        let cidr_u32 = u32::from_be_bytes([cidr_ip[0], cidr_ip[1], cidr_ip[2], cidr_ip[3]]);
        let check_u32 = u32::from_be_bytes([check_ip[0], check_ip[1], check_ip[2], check_ip[3]]);
        let mask = if mask_bits == 0 {
            0
        } else {
            !0u32 << (32 - mask_bits)
        };

        (cidr_u32 & mask) == (check_u32 & mask)
    }

    /// Create a policy
    pub async fn create_policy(&self, tenant_id: i64, policy: Policy) -> StackhouseResult<Policy> {
        let id = if policy.id.is_empty() {
            uuid::Uuid::new_v4().to_string()
        } else {
            policy.id.clone()
        };
        let effect_str = match &policy.effect {
            PolicyEffect::Allow => "allow",
            PolicyEffect::Deny => "deny",
        };

        self.store.execute(
            "INSERT INTO stackhouse_abac_policies (id, name, description, effect, principals, actions, resources, conditions, priority, enabled, tenant_id) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)".to_string(),
            vec![
                SqlValue::Text(id.clone()),
                SqlValue::Text(policy.name.clone()),
                SqlValue::Text(policy.description.clone()),
                SqlValue::Text(effect_str.to_string()),
                SqlValue::Text(serde_json::to_string(&policy.principals).unwrap_or_default()),
                SqlValue::Text(serde_json::to_string(&policy.actions).unwrap_or_default()),
                SqlValue::Text(serde_json::to_string(&policy.resources).unwrap_or_default()),
                SqlValue::Text(serde_json::to_string(&policy.conditions).unwrap_or_default()),
                SqlValue::Integer(policy.priority as i64),
                SqlValue::Text(policy.enabled.to_string()),
                SqlValue::Integer(tenant_id),
            ],
        ).await?;

        Ok(Policy {
            id,
            tenant_id,
            created_at: chrono::Utc::now().to_rfc3339(),
            updated_at: chrono::Utc::now().to_rfc3339(),
            ..policy
        })
    }

    /// List policies for a tenant
    pub async fn list_policies(&self, tenant_id: i64) -> StackhouseResult<Vec<Value>> {
        let rows = self.store.query(
            "SELECT id, name, description, effect, principals, actions, resources, conditions, priority, enabled, created_at FROM stackhouse_abac_policies WHERE tenant_id = ? ORDER BY priority DESC".to_string(),
            vec![SqlValue::Integer(tenant_id)],
        ).await?;
        Ok(rows
            .into_iter()
            .map(|r| json!(r.into_iter().collect::<std::collections::HashMap<_, _>>()))
            .collect())
    }

    /// Delete a policy
    pub async fn delete_policy(&self, policy_id: &str, tenant_id: i64) -> StackhouseResult<()> {
        self.store
            .execute(
                "DELETE FROM stackhouse_abac_policies WHERE id = ? AND tenant_id = ?".to_string(),
                vec![
                    SqlValue::Text(policy_id.to_string()),
                    SqlValue::Integer(tenant_id),
                ],
            )
            .await?;
        Ok(())
    }

    /// Update a policy
    pub async fn update_policy(
        &self,
        policy_id: &str,
        tenant_id: i64,
        updates: Value,
    ) -> StackhouseResult<()> {
        if let Some(name) = updates.get("name").and_then(|v| v.as_str()) {
            self.store.execute(
                "UPDATE stackhouse_abac_policies SET name = ?, updated_at = NOW() WHERE id = ? AND tenant_id = ?".to_string(),
                vec![SqlValue::Text(name.to_string()), SqlValue::Text(policy_id.to_string()), SqlValue::Integer(tenant_id)],
            ).await?;
        }
        if let Some(enabled) = updates.get("enabled").and_then(|v| v.as_bool()) {
            self.store.execute(
                "UPDATE stackhouse_abac_policies SET enabled = ?, updated_at = NOW() WHERE id = ? AND tenant_id = ?".to_string(),
                vec![SqlValue::Text(enabled.to_string()), SqlValue::Text(policy_id.to_string()), SqlValue::Integer(tenant_id)],
            ).await?;
        }
        Ok(())
    }

    async fn get_active_policies(&self, tenant_id: i64) -> StackhouseResult<Vec<Policy>> {
        let rows = self.store.query(
            "SELECT id, name, description, effect, principals, actions, resources, conditions, priority, enabled, created_at, updated_at FROM stackhouse_abac_policies WHERE tenant_id = ? AND enabled = true ORDER BY priority DESC".to_string(),
            vec![SqlValue::Integer(tenant_id)],
        ).await?;

        let policies: Vec<Policy> = rows
            .into_iter()
            .filter_map(|r| {
                let get = |key: &str| r.iter().find(|(k, _)| k == key).map(|(_, v)| v.clone());
                Some(Policy {
                    id: get("id")?.as_str()?.to_string(),
                    name: get("name")?.as_str()?.to_string(),
                    description: get("description")
                        .and_then(|v| v.as_str().map(String::from))
                        .unwrap_or_default(),
                    effect: if get("effect")?.as_str()? == "allow" {
                        PolicyEffect::Allow
                    } else {
                        PolicyEffect::Deny
                    },
                    principals: get("principals")
                        .and_then(|v| v.as_str().and_then(|s| serde_json::from_str(s).ok()))
                        .unwrap_or_default(),
                    actions: get("actions")
                        .and_then(|v| v.as_str().and_then(|s| serde_json::from_str(s).ok()))
                        .unwrap_or_default(),
                    resources: get("resources")
                        .and_then(|v| v.as_str().and_then(|s| serde_json::from_str(s).ok()))
                        .unwrap_or_default(),
                    conditions: get("conditions")
                        .and_then(|v| v.as_str().and_then(|s| serde_json::from_str(s).ok()))
                        .unwrap_or_default(),
                    priority: get("priority").and_then(|v| v.as_i64()).unwrap_or(0) as i32,
                    enabled: true,
                    tenant_id,
                    created_at: get("created_at")
                        .and_then(|v| v.as_str().map(String::from))
                        .unwrap_or_default(),
                    updated_at: get("updated_at")
                        .and_then(|v| v.as_str().map(String::from))
                        .unwrap_or_default(),
                })
            })
            .collect();

        Ok(policies)
    }
}

// ============================================================================
// Router
// ============================================================================

#[derive(Clone)]
pub struct AbacState {
    pub abac: Arc<AbacEngine>,
    pub auth: AuthState,
}

#[derive(Deserialize)]
struct EvaluateRequest {
    action: String,
    resource_type: String,
    resource_id: String,
    #[serde(default)]
    resource_attributes: HashMap<String, Value>,
    #[serde(default)]
    environment: HashMap<String, Value>,
}

async fn evaluate_handler(
    State(state): State<AbacState>,
    headers: HeaderMap,
    Json(req): Json<EvaluateRequest>,
) -> Result<impl IntoResponse, StackhouseError> {
    let user = extract_auth_user(&state.auth, &headers)?;
    let auth_request = AuthorizationRequest {
        principal: PrincipalContext {
            id: user.id,
            roles: vec![],
            groups: vec![],
            attributes: HashMap::new(),
        },
        action: req.action,
        resource: ResourceContext {
            resource_type: req.resource_type,
            resource_id: req.resource_id,
            attributes: req.resource_attributes,
        },
        environment: req.environment,
    };
    let decision = state.abac.evaluate(user.id, &auth_request).await?;
    Ok(Json(json!({"success": true, "data": decision})))
}

async fn create_policy_handler(
    State(state): State<AbacState>,
    headers: HeaderMap,
    Json(policy): Json<Policy>,
) -> Result<impl IntoResponse, StackhouseError> {
    let user = extract_auth_user(&state.auth, &headers)?;
    let result = state.abac.create_policy(user.id, policy).await?;
    Ok(Json(json!({"success": true, "data": result})))
}

async fn list_policies_handler(
    State(state): State<AbacState>,
    headers: HeaderMap,
) -> Result<impl IntoResponse, StackhouseError> {
    let user = extract_auth_user(&state.auth, &headers)?;
    let policies = state.abac.list_policies(user.id).await?;
    Ok(Json(json!({"success": true, "data": policies})))
}

async fn delete_policy_handler(
    State(state): State<AbacState>,
    headers: HeaderMap,
    axum::extract::Path(policy_id): axum::extract::Path<String>,
) -> Result<impl IntoResponse, StackhouseError> {
    let user = extract_auth_user(&state.auth, &headers)?;
    state.abac.delete_policy(&policy_id, user.id).await?;
    Ok(Json(json!({"success": true, "message": "Policy deleted"})))
}

pub fn create_abac_router(state: AbacState) -> Router {
    Router::new()
        .route("/policies", post(create_policy_handler))
        .route("/policies", get(list_policies_handler))
        .route("/policies/:id", delete(delete_policy_handler))
        .route("/evaluate", post(evaluate_handler))
        .with_state(state)
}
