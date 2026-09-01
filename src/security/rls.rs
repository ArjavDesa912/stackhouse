//! # Row Level Security Module (Stackhouse-RLS)
//!
//! Provides PostgreSQL Row Level Security integration for Stackhouse.
//! Enables per-user data isolation using JWT claims injected into
//! the PostgreSQL session context.
//!
//! ## How It Works
//!
//! 1. Admin enables RLS on a table via API
//! 2. Admin creates policies (e.g., "users can only see their own rows")
//! 3. On every authenticated request, Stackhouse runs `SET LOCAL` to inject JWT claims
//! 4. PostgreSQL enforces the policies transparently
//!
//! ## Endpoints
//!
//! - `POST /v1/rls/:table/enable` - Enable RLS on a table
//! - `POST /v1/rls/:table/disable` - Disable RLS on a table
//! - `POST /v1/rls/:table/policies` - Create a policy
//! - `GET /v1/rls/:table/policies` - List policies
//! - `DELETE /v1/rls/:table/policies/:name` - Drop a policy

use crate::db::StackhouseStore;
use crate::error::{StackhouseError, StackhouseResult};
use crate::guard::SchemaGuard;

use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::IntoResponse,
    routing::{delete, get, post},
    Json, Router,
};
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::sync::Arc;
use tracing::{debug, info};

// ============================================================================
// Core Types
// ============================================================================

/// RLS policy operations
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "UPPERCASE")]
pub enum PolicyOperation {
    All,
    Select,
    Insert,
    Update,
    Delete,
}

impl PolicyOperation {
    pub fn as_sql(&self) -> &'static str {
        match self {
            PolicyOperation::All => "ALL",
            PolicyOperation::Select => "SELECT",
            PolicyOperation::Insert => "INSERT",
            PolicyOperation::Update => "UPDATE",
            PolicyOperation::Delete => "DELETE",
        }
    }
}

/// RLS policy definition
#[derive(Debug, Serialize)]
pub struct RlsPolicy {
    pub name: String,
    pub table: String,
    pub operation: String,
    pub permissive: bool,
    pub roles: Vec<String>,
    pub using_expression: Option<String>,
    pub check_expression: Option<String>,
}

/// Table RLS status
#[derive(Debug, Serialize)]
pub struct RlsStatus {
    pub table: String,
    pub rls_enabled: bool,
    pub force_rls: bool,
    pub policies: Vec<RlsPolicy>,
}

// ============================================================================
// Request DTOs
// ============================================================================

/// Request to create an RLS policy
#[derive(Debug, Deserialize)]
pub struct CreatePolicyRequest {
    /// Policy name (must be unique per table)
    pub name: String,
    /// Operation this policy applies to
    #[serde(default = "default_operation")]
    pub operation: PolicyOperation,
    /// USING expression — controls which rows are visible
    /// Use `current_setting('request.jwt.claims', true)::json->>'user_id'` to reference the JWT
    pub using_expr: Option<String>,
    /// WITH CHECK expression — controls which rows can be inserted/updated
    pub check_expr: Option<String>,
    /// Whether this is a permissive (default) or restrictive policy
    #[serde(default = "default_permissive")]
    pub permissive: bool,
}

fn default_operation() -> PolicyOperation {
    PolicyOperation::All
}
fn default_permissive() -> bool {
    true
}

// ============================================================================
// RLS Service Implementation
// ============================================================================

/// Row Level Security service
#[derive(Clone)]
pub struct RlsService {
    store: Arc<StackhouseStore>,
}

/// Shared state for RLS routes
#[derive(Clone)]
pub struct RlsState {
    pub rls: Arc<RlsService>,
}

impl RlsService {
    /// Creates a new RlsService
    pub async fn new(store: Arc<StackhouseStore>) -> StackhouseResult<Self> {
        let service = Self { store };
        service.init().await?;
        Ok(service)
    }

    /// Initialize — ensure we can set custom GUC variables for JWT context
    async fn init(&self) -> StackhouseResult<()> {
        info!("🔒 Initializing Row Level Security engine...");
        // Create the custom GUC variable namespace for JWT claims
        // This allows SET LOCAL request.jwt.claims = '...'
        // Note: In PostgreSQL 9.2+, this works without explicit registration
        // as long as the variable name contains a dot
        Ok(())
    }

    /// Enable Row Level Security on a table
    pub async fn enable_rls(&self, table: &str) -> StackhouseResult<()> {
        SchemaGuard::validate_identifier(table)?;

        let sql = format!("ALTER TABLE {} ENABLE ROW LEVEL SECURITY", table);
        self.store.execute_simple(sql).await?;

        // Also force RLS for table owners (important for security)
        let force_sql = format!("ALTER TABLE {} FORCE ROW LEVEL SECURITY", table);
        self.store.execute_simple(force_sql).await?;

        info!("🔒 RLS enabled on table '{}'", table);
        Ok(())
    }

    /// Disable Row Level Security on a table
    pub async fn disable_rls(&self, table: &str) -> StackhouseResult<()> {
        SchemaGuard::validate_identifier(table)?;

        let sql = format!("ALTER TABLE {} DISABLE ROW LEVEL SECURITY", table);
        self.store.execute_simple(sql).await?;

        info!("🔓 RLS disabled on table '{}'", table);
        Ok(())
    }

    /// Create a Row Level Security policy
    pub async fn create_policy(
        &self,
        table: &str,
        req: &CreatePolicyRequest,
    ) -> StackhouseResult<()> {
        SchemaGuard::validate_identifier(table)?;
        SchemaGuard::validate_identifier(&req.name)?;

        // Validate USING and WITH CHECK expressions to prevent SQL injection
        if let Some(using) = &req.using_expr {
            SchemaGuard::validate_sql_expression(using)?;
        }
        if let Some(check) = &req.check_expr {
            SchemaGuard::validate_sql_expression(check)?;
        }

        let permissive_str = if req.permissive {
            "PERMISSIVE"
        } else {
            "RESTRICTIVE"
        };

        let mut sql = format!(
            "CREATE POLICY {} ON {} AS {} FOR {}",
            req.name,
            table,
            permissive_str,
            req.operation.as_sql()
        );

        // Add TO clause (all roles by default)
        sql.push_str(" TO PUBLIC");

        // Add USING clause
        if let Some(using) = &req.using_expr {
            sql.push_str(&format!(" USING ({})", using));
        }

        // Add WITH CHECK clause
        if let Some(check) = &req.check_expr {
            sql.push_str(&format!(" WITH CHECK ({})", check));
        }

        self.store.execute_simple(sql).await?;
        info!("📜 Created RLS policy '{}' on table '{}'", req.name, table);
        Ok(())
    }

    /// Drop an RLS policy
    pub async fn drop_policy(&self, table: &str, policy_name: &str) -> StackhouseResult<()> {
        SchemaGuard::validate_identifier(table)?;
        SchemaGuard::validate_identifier(policy_name)?;

        let sql = format!("DROP POLICY IF EXISTS {} ON {}", policy_name, table);
        self.store.execute_simple(sql).await?;
        info!(
            "🗑️ Dropped RLS policy '{}' from table '{}'",
            policy_name, table
        );
        Ok(())
    }

    /// List all RLS policies for a table
    pub async fn list_policies(&self, table: &str) -> StackhouseResult<Vec<RlsPolicy>> {
        SchemaGuard::validate_identifier(table)?;

        let sql = format!(
            "SELECT polname, polcmd, polpermissive, polroles::text, \
                    pg_get_expr(polqual, polrelid) as using_expr, \
                    pg_get_expr(polwithcheck, polrelid) as check_expr \
             FROM pg_policy \
             WHERE polrelid = '{}'::regclass",
            table
        );

        let rows = self.store.query_simple(sql).await?;

        let policies: Vec<RlsPolicy> = rows
            .into_iter()
            .map(|row| {
                let mut policy = RlsPolicy {
                    name: String::new(),
                    table: table.to_string(),
                    operation: String::new(),
                    permissive: true,
                    roles: vec![],
                    using_expression: None,
                    check_expression: None,
                };

                for (key, value) in row {
                    match key.as_str() {
                        "polname" => policy.name = value.as_str().unwrap_or("").to_string(),
                        "polcmd" => {
                            policy.operation = match value.as_str().unwrap_or("") {
                                "r" => "SELECT".to_string(),
                                "a" => "INSERT".to_string(),
                                "w" => "UPDATE".to_string(),
                                "d" => "DELETE".to_string(),
                                "*" => "ALL".to_string(),
                                other => other.to_string(),
                            };
                        }
                        "polpermissive" => {
                            policy.permissive = value.as_bool().unwrap_or(true);
                        }
                        "polroles" => {
                            policy.roles = value
                                .as_str()
                                .unwrap_or("")
                                .split(',')
                                .map(|s| s.trim().to_string())
                                .collect();
                        }
                        "using_expr" => {
                            policy.using_expression = value.as_str().map(|s| s.to_string());
                        }
                        "check_expr" => {
                            policy.check_expression = value.as_str().map(|s| s.to_string());
                        }
                        _ => {}
                    }
                }

                policy
            })
            .collect();

        Ok(policies)
    }

    /// Get RLS status for a table
    pub async fn get_status(&self, table: &str) -> StackhouseResult<RlsStatus> {
        SchemaGuard::validate_identifier(table)?;

        let sql = format!(
            "SELECT relrowsecurity, relforcerowsecurity \
             FROM pg_class WHERE relname = '{}'",
            table
        );
        let rows = self.store.query_simple(sql).await?;

        let (rls_enabled, force_rls) = if let Some(row) = rows.first() {
            let enabled = row
                .iter()
                .find(|(k, _)| k == "relrowsecurity")
                .map(|(_, v)| v.as_bool().unwrap_or(false))
                .unwrap_or(false);
            let force = row
                .iter()
                .find(|(k, _)| k == "relforcerowsecurity")
                .map(|(_, v)| v.as_bool().unwrap_or(false))
                .unwrap_or(false);
            (enabled, force)
        } else {
            return Err(StackhouseError::TableNotFound(table.to_string()));
        };

        let policies = self.list_policies(table).await?;

        Ok(RlsStatus {
            table: table.to_string(),
            rls_enabled,
            force_rls,
            policies,
        })
    }

    /// Inject JWT claims into the PostgreSQL session for RLS
    /// This should be called before every authenticated query
    pub async fn inject_jwt_context(&self, claims_json: &str) -> StackhouseResult<()> {
        let sql = format!(
            "SET LOCAL request.jwt.claims = '{}'",
            claims_json.replace('\'', "''")
        );
        self.store.execute_simple(sql).await?;
        debug!("🔑 Injected JWT context for RLS");
        Ok(())
    }

    /// Audit RLS status across all stackhouse_* tables.
    /// Returns per-table info: RLS enabled, force mode, policy count.
    /// Tables without RLS are flagged so admins can act.
    pub async fn audit_rls(&self) -> StackhouseResult<Vec<RlsAuditEntry>> {
        let rows = self
            .store
            .query_simple(
                "SELECT c.relname, c.relrowsecurity, c.relforcerowsecurity, \
                    (SELECT count(*) FROM pg_policy p WHERE p.polrelid = c.oid) as policy_count \
             FROM pg_class c \
             JOIN pg_namespace n ON c.relnamespace = n.oid \
             WHERE n.nspname = 'public' AND c.relkind = 'r' AND c.relname LIKE 'stackhouse_%' \
             ORDER BY c.relname"
                    .to_string(),
            )
            .await?;

        let mut entries = Vec::new();
        for row in rows {
            let table_name = row
                .iter()
                .find(|(k, _)| k == "relname")
                .and_then(|(_, v)| v.as_str())
                .unwrap_or("")
                .to_string();
            let rls_enabled = row
                .iter()
                .find(|(k, _)| k == "relrowsecurity")
                .and_then(|(_, v)| v.as_bool())
                .unwrap_or(false);
            let force_rls = row
                .iter()
                .find(|(k, _)| k == "relforcerowsecurity")
                .and_then(|(_, v)| v.as_bool())
                .unwrap_or(false);
            let policy_count = row
                .iter()
                .find(|(k, _)| k == "policy_count")
                .and_then(|(_, v)| v.as_i64())
                .unwrap_or(0) as usize;

            entries.push(RlsAuditEntry {
                table_name,
                rls_enabled,
                force_rls,
                policy_count,
                secure: rls_enabled && policy_count > 0,
            });
        }
        Ok(entries)
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct RlsAuditEntry {
    pub table_name: String,
    pub rls_enabled: bool,
    pub force_rls: bool,
    pub policy_count: usize,
    pub secure: bool,
}

// ============================================================================
// HTTP Handlers
// ============================================================================

/// POST /v1/rls/:table/enable — Enable RLS on a table
async fn rls_enable_handler(
    State(state): State<RlsState>,
    Path(table): Path<String>,
) -> Result<impl IntoResponse, StackhouseError> {
    state.rls.enable_rls(&table).await?;

    Ok(Json(json!({
        "success": true,
        "message": format!("RLS enabled on table '{}'", table),
        "table": table,
    })))
}

/// POST /v1/rls/:table/disable — Disable RLS on a table
async fn rls_disable_handler(
    State(state): State<RlsState>,
    Path(table): Path<String>,
) -> Result<impl IntoResponse, StackhouseError> {
    state.rls.disable_rls(&table).await?;

    Ok(Json(json!({
        "success": true,
        "message": format!("RLS disabled on table '{}'", table),
        "table": table,
    })))
}

/// POST /v1/rls/:table/policies — Create a policy
async fn rls_create_policy_handler(
    State(state): State<RlsState>,
    Path(table): Path<String>,
    Json(req): Json<CreatePolicyRequest>,
) -> Result<impl IntoResponse, StackhouseError> {
    state.rls.create_policy(&table, &req).await?;

    Ok((
        StatusCode::CREATED,
        Json(json!({
            "success": true,
            "message": format!("Policy '{}' created on table '{}'", req.name, table),
            "table": table,
            "policy": req.name,
        })),
    ))
}

/// GET /v1/rls/:table/policies — List policies
async fn rls_list_policies_handler(
    State(state): State<RlsState>,
    Path(table): Path<String>,
) -> Result<impl IntoResponse, StackhouseError> {
    let policies = state.rls.list_policies(&table).await?;

    Ok(Json(json!({
        "success": true,
        "data": policies,
        "count": policies.len(),
        "table": table,
    })))
}

/// DELETE /v1/rls/:table/policies/:name — Drop a policy
async fn rls_drop_policy_handler(
    State(state): State<RlsState>,
    Path((table, name)): Path<(String, String)>,
) -> Result<impl IntoResponse, StackhouseError> {
    state.rls.drop_policy(&table, &name).await?;

    Ok(Json(json!({
        "success": true,
        "message": format!("Policy '{}' dropped from table '{}'", name, table),
        "table": table,
    })))
}

/// GET /v1/rls/:table/status — Get RLS status
async fn rls_status_handler(
    State(state): State<RlsState>,
    Path(table): Path<String>,
) -> Result<impl IntoResponse, StackhouseError> {
    let status = state.rls.get_status(&table).await?;

    Ok(Json(json!({
        "success": true,
        "data": status,
    })))
}

/// GET /v1/rls/audit — Audit RLS across all tables
async fn rls_audit_handler(
    State(state): State<RlsState>,
) -> Result<impl IntoResponse, StackhouseError> {
    let audit = state.rls.audit_rls().await?;
    let total = audit.len();
    let secured = audit.iter().filter(|e| e.secure).count();
    let unprotected = audit.iter().filter(|e| !e.rls_enabled).count();

    Ok(Json(json!({
        "success": true,
        "total_tables": total,
        "secured_tables": secured,
        "unprotected_tables": unprotected,
        "data": audit,
    })))
}

// ============================================================================
// Router
// ============================================================================

/// Creates the RLS management router
pub fn create_rls_router(state: RlsState) -> Router {
    Router::new()
        .route("/audit", get(rls_audit_handler))
        .route("/:table/enable", post(rls_enable_handler))
        .route("/:table/disable", post(rls_disable_handler))
        .route("/:table/policies", post(rls_create_policy_handler))
        .route("/:table/policies", get(rls_list_policies_handler))
        .route("/:table/policies/:name", delete(rls_drop_policy_handler))
        .route("/:table/status", get(rls_status_handler))
        .with_state(state)
}
