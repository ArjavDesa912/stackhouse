//! # Database Branching Module (Stackhouse-Branching)
//!
//! Preview database branches for development/staging.
//! Creates isolated schema copies for testing changes.

pub mod db_branches;
pub use db_branches::*;

use crate::api::admin::AdminAuditService;
use crate::auth::{extract_auth_user, AuthState, AuthUser};
use crate::authorization::AuthorizationService;
use crate::db::StackhouseStore;
use crate::error::{StackhouseError, StackhouseResult};
use crate::guard::SchemaGuard;

use async_trait::async_trait;
use axum::{
    extract::{FromRequestParts, State},
    http::request::Parts,
    response::IntoResponse,
    routing::get,
    Json, Router,
};
use serde::Deserialize;
use serde_json::{json, Value};
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};
use tracing::info;

#[derive(Clone)]
pub struct BranchingService {
    store: Arc<StackhouseStore>,
}

#[derive(Deserialize)]
pub struct CreateBranchRequest {
    pub name: String,
    pub source: Option<String>, // Source branch/schema (defaults to "public")
}

#[derive(Deserialize)]
pub struct MergeBranchRequest {
    pub source_branch: String,
    pub target_branch: Option<String>,
}

impl BranchingService {
    pub fn new(store: Arc<StackhouseStore>) -> Self {
        info!("🌿 Stackhouse-Branching initialized");
        Self { store }
    }

    pub async fn create_branch(&self, name: &str, source: Option<&str>) -> StackhouseResult<()> {
        SchemaGuard::validate_identifier(name)?;
        let source_schema = source.unwrap_or("public");
        // Validate source schema to prevent SQL injection
        SchemaGuard::validate_identifier(source_schema)?;

        // Create new schema
        self.store
            .execute_simple(format!("CREATE SCHEMA IF NOT EXISTS \"branch_{}\"", name))
            .await?;

        // Copy all tables from source schema
        let tables = self
            .store
            .query(
                "SELECT tablename FROM pg_tables WHERE schemaname = $1".to_string(),
                vec![crate::db::SqlValue::Text(source_schema.to_string())],
            )
            .await?;

        for row in tables {
            if let Some(table_name) = row
                .iter()
                .find(|(k, _)| k == "tablename")
                .and_then(|(_, v)| v.as_str())
            {
                if table_name.starts_with("pg_") || table_name.starts_with("sql_") {
                    continue;
                }
                // Use LIKE INCLUDING ALL to copy schema (constraints, indexes, defaults, storage, comments)
                let _ = self
                    .store
                    .execute_simple(format!(
                        "CREATE TABLE \"branch_{}\".\"{}\" (LIKE \"{}\".\"{}\" INCLUDING ALL)",
                        name, table_name, source_schema, table_name
                    ))
                    .await;
                // Copy data
                let _ = self
                    .store
                    .execute_simple(format!(
                        "INSERT INTO \"branch_{}\".\"{}\" SELECT * FROM \"{}\".\"{}\"",
                        name, table_name, source_schema, table_name
                    ))
                    .await;
            }
        }

        // Copy sequences from source schema
        let sequences = self
            .store
            .query(
                "SELECT sequencename FROM pg_sequences WHERE schemaname = $1".to_string(),
                vec![crate::db::SqlValue::Text(source_schema.to_string())],
            )
            .await
            .unwrap_or_default();

        for row in sequences {
            if let Some(seq_name) = row
                .iter()
                .find(|(k, _)| k == "sequencename")
                .and_then(|(_, v)| v.as_str())
            {
                let _ = self.store.execute_simple(format!(
                    "CREATE SEQUENCE \"branch_{}\".\"{}\" AS (SELECT last_value FROM \"{}\".\"{}\")",
                    name, seq_name, source_schema, seq_name
                )).await;
            }
        }

        // Track branch metadata
        self.store.execute_simple(
            "CREATE TABLE IF NOT EXISTS stackhouse_branches (name TEXT PRIMARY KEY, source TEXT, created_at BIGINT)".to_string(),
        ).await?;

        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs();
        self.store.execute(
            "INSERT INTO stackhouse_branches (name, source, created_at) VALUES ($1, $2, $3) ON CONFLICT (name) DO UPDATE SET created_at = $3".to_string(),
            vec![
                crate::db::SqlValue::Text(name.to_string()),
                crate::db::SqlValue::Text(source_schema.to_string()),
                crate::db::SqlValue::Integer(now as i64),
            ],
        ).await?;

        info!("🌿 Created branch '{}' from '{}'", name, source_schema);
        Ok(())
    }

    pub async fn delete_branch(&self, name: &str) -> StackhouseResult<()> {
        SchemaGuard::validate_identifier(name)?;
        if name == "public" || name == "main" {
            return Err(StackhouseError::InvalidPayload(
                "Cannot delete the main branch".to_string(),
            ));
        }

        self.store
            .execute_simple(format!("DROP SCHEMA IF EXISTS \"branch_{}\" CASCADE", name))
            .await?;

        self.store
            .execute(
                "DELETE FROM stackhouse_branches WHERE name = $1".to_string(),
                vec![crate::db::SqlValue::Text(name.to_string())],
            )
            .await?;

        info!("🌿 Deleted branch '{}'", name);
        Ok(())
    }

    pub async fn list_branches(&self) -> StackhouseResult<Vec<Value>> {
        let _ = self.store.execute_simple(
            "CREATE TABLE IF NOT EXISTS stackhouse_branches (name TEXT PRIMARY KEY, source TEXT, created_at BIGINT)".to_string(),
        ).await;

        let rows = self
            .store
            .query_simple(
                "SELECT name, source, created_at FROM stackhouse_branches ORDER BY created_at DESC"
                    .to_string(),
            )
            .await?;

        let branches: Vec<Value> = rows
            .into_iter()
            .map(|row| {
                let mut obj = serde_json::Map::new();
                for (k, v) in row {
                    obj.insert(k, v);
                }
                Value::Object(obj)
            })
            .collect();

        Ok(branches)
    }

    pub async fn diff_branch(&self, branch_name: &str) -> StackhouseResult<Value> {
        SchemaGuard::validate_identifier(branch_name)?;
        let branch_schema = format!("branch_{}", branch_name);

        // Compare tables in branch vs public using parameterized query
        let branch_tables = self
            .store
            .query(
                "SELECT tablename FROM pg_tables WHERE schemaname = $1 ORDER BY tablename"
                    .to_string(),
                vec![crate::db::SqlValue::Text(branch_schema.clone())],
            )
            .await
            .unwrap_or_default();

        let public_tables = self
            .store
            .query(
                "SELECT tablename FROM pg_tables WHERE schemaname = $1 ORDER BY tablename"
                    .to_string(),
                vec![crate::db::SqlValue::Text("public".to_string())],
            )
            .await
            .unwrap_or_default();

        let branch_set: std::collections::HashSet<String> = branch_tables
            .iter()
            .filter_map(|r| {
                r.iter()
                    .find(|(k, _)| k == "tablename")
                    .and_then(|(_, v)| v.as_str())
                    .map(String::from)
            })
            .collect();

        let public_set: std::collections::HashSet<String> = public_tables
            .iter()
            .filter_map(|r| {
                r.iter()
                    .find(|(k, _)| k == "tablename")
                    .and_then(|(_, v)| v.as_str())
                    .map(String::from)
            })
            .collect();

        let added: Vec<&String> = branch_set.difference(&public_set).collect();
        let removed: Vec<&String> = public_set.difference(&branch_set).collect();

        // Compare columns for shared tables
        let mut column_diffs = Vec::new();
        for table in branch_set.intersection(&public_set) {
            let branch_cols = self.store.query(
                "SELECT column_name, data_type FROM information_schema.columns WHERE table_schema = $1 AND table_name = $2 ORDER BY column_name".to_string(),
                vec![
                    crate::db::SqlValue::Text(branch_schema.clone()),
                    crate::db::SqlValue::Text(table.clone()),
                ],
            ).await.unwrap_or_default();

            let public_cols = self.store.query(
                "SELECT column_name, data_type FROM information_schema.columns WHERE table_schema = $1 AND table_name = $2 ORDER BY column_name".to_string(),
                vec![
                    crate::db::SqlValue::Text("public".to_string()),
                    crate::db::SqlValue::Text(table.clone()),
                ],
            ).await.unwrap_or_default();

            let branch_col_map: std::collections::HashMap<String, String> = branch_cols
                .iter()
                .filter_map(|r| {
                    let col = r
                        .iter()
                        .find(|(k, _)| k == "column_name")
                        .and_then(|(_, v)| v.as_str())?;
                    let typ = r
                        .iter()
                        .find(|(k, _)| k == "data_type")
                        .and_then(|(_, v)| v.as_str())?;
                    Some((col.to_string(), typ.to_string()))
                })
                .collect();

            let public_col_map: std::collections::HashMap<String, String> = public_cols
                .iter()
                .filter_map(|r| {
                    let col = r
                        .iter()
                        .find(|(k, _)| k == "column_name")
                        .and_then(|(_, v)| v.as_str())?;
                    let typ = r
                        .iter()
                        .find(|(k, _)| k == "data_type")
                        .and_then(|(_, v)| v.as_str())?;
                    Some((col.to_string(), typ.to_string()))
                })
                .collect();

            let cols_added: Vec<String> = branch_col_map
                .keys()
                .filter(|k| !public_col_map.contains_key(*k))
                .cloned()
                .collect();
            let cols_removed: Vec<String> = public_col_map
                .keys()
                .filter(|k| !branch_col_map.contains_key(*k))
                .cloned()
                .collect();
            let cols_changed: Vec<String> = branch_col_map
                .iter()
                .filter(|(k, v)| public_col_map.get(*k).map(|pv| pv != *v).unwrap_or(false))
                .map(|(k, _)| k.clone())
                .collect();

            if !cols_added.is_empty() || !cols_removed.is_empty() || !cols_changed.is_empty() {
                column_diffs.push(json!({
                    "table": table,
                    "columns_added": cols_added,
                    "columns_removed": cols_removed,
                    "columns_changed": cols_changed,
                }));
            }
        }

        Ok(json!({
            "branch": branch_name,
            "tables_added": added,
            "tables_removed": removed,
            "tables_shared": branch_set.intersection(&public_set).collect::<Vec<_>>(),
            "column_diffs": column_diffs,
        }))
    }

    /// Merge a branch into a target schema.
    /// Generates and executes ALTER TABLE statements to reconcile differences.
    pub async fn merge_branch(
        &self,
        source_branch: &str,
        target_branch: Option<&str>,
    ) -> StackhouseResult<Vec<String>> {
        SchemaGuard::validate_identifier(source_branch)?;
        let source_schema = format!("branch_{}", source_branch);
        let target_schema = target_branch.unwrap_or("public").to_string();
        if target_schema != "public" {
            SchemaGuard::validate_identifier(&target_schema)?;
        }

        let mut statements = Vec::new();

        // Get tables in both schemas
        let source_tables = self
            .store
            .query(
                "SELECT tablename FROM pg_tables WHERE schemaname = $1 ORDER BY tablename"
                    .to_string(),
                vec![crate::db::SqlValue::Text(source_schema.clone())],
            )
            .await
            .unwrap_or_default();

        let target_tables = self
            .store
            .query(
                "SELECT tablename FROM pg_tables WHERE schemaname = $1 ORDER BY tablename"
                    .to_string(),
                vec![crate::db::SqlValue::Text(target_schema.clone())],
            )
            .await
            .unwrap_or_default();

        let source_set: std::collections::HashSet<String> = source_tables
            .iter()
            .filter_map(|r| {
                r.iter()
                    .find(|(k, _)| k == "tablename")
                    .and_then(|(_, v)| v.as_str())
                    .map(String::from)
            })
            .collect();

        let target_set: std::collections::HashSet<String> = target_tables
            .iter()
            .filter_map(|r| {
                r.iter()
                    .find(|(k, _)| k == "tablename")
                    .and_then(|(_, v)| v.as_str())
                    .map(String::from)
            })
            .collect();

        // For shared tables, compare and reconcile columns
        for table in source_set.intersection(&target_set) {
            if table.starts_with("pg_")
                || table.starts_with("sql_")
                || table.starts_with("stackhouse_")
            {
                continue;
            }

            let source_cols = self.store.query(
                "SELECT column_name, data_type, is_nullable FROM information_schema.columns WHERE table_schema = $1 AND table_name = $2 ORDER BY ordinal_position".to_string(),
                vec![
                    crate::db::SqlValue::Text(source_schema.clone()),
                    crate::db::SqlValue::Text(table.clone()),
                ],
            ).await.unwrap_or_default();

            let target_cols = self.store.query(
                "SELECT column_name, data_type, is_nullable FROM information_schema.columns WHERE table_schema = $1 AND table_name = $2 ORDER BY ordinal_position".to_string(),
                vec![
                    crate::db::SqlValue::Text(target_schema.clone()),
                    crate::db::SqlValue::Text(table.clone()),
                ],
            ).await.unwrap_or_default();

            let source_col_map: std::collections::HashMap<String, (String, String)> = source_cols
                .iter()
                .filter_map(|r| {
                    let col = r
                        .iter()
                        .find(|(k, _)| k == "column_name")
                        .and_then(|(_, v)| v.as_str())?;
                    let typ = r
                        .iter()
                        .find(|(k, _)| k == "data_type")
                        .and_then(|(_, v)| v.as_str())?;
                    let null = r
                        .iter()
                        .find(|(k, _)| k == "is_nullable")
                        .and_then(|(_, v)| v.as_str())?;
                    Some((col.to_string(), (typ.to_string(), null.to_string())))
                })
                .collect();

            let target_col_map: std::collections::HashMap<String, (String, String)> = target_cols
                .iter()
                .filter_map(|r| {
                    let col = r
                        .iter()
                        .find(|(k, _)| k == "column_name")
                        .and_then(|(_, v)| v.as_str())?;
                    let typ = r
                        .iter()
                        .find(|(k, _)| k == "data_type")
                        .and_then(|(_, v)| v.as_str())?;
                    let null = r
                        .iter()
                        .find(|(k, _)| k == "is_nullable")
                        .and_then(|(_, v)| v.as_str())?;
                    Some((col.to_string(), (typ.to_string(), null.to_string())))
                })
                .collect();

            // Add missing columns to target
            for (col_name, (data_type, is_nullable)) in &source_col_map {
                if !target_col_map.contains_key(col_name) {
                    let null_clause = if is_nullable == "YES" { "" } else { "NOT NULL" };
                    let sql = format!(
                        "ALTER TABLE \"{}\".\"{}\" ADD COLUMN \"{}\" {} {}",
                        target_schema, table, col_name, data_type, null_clause
                    );
                    let _ = self.store.execute_simple(sql.clone()).await;
                    statements.push(sql);
                }
            }
        }

        info!(
            "🌿 Merged branch '{}' into '{}' with {} statements",
            source_branch,
            target_schema,
            statements.len()
        );
        Ok(statements)
    }
}

// ============================================================================
// Handlers & Router
// ============================================================================

#[derive(Clone)]
pub struct BranchingState {
    pub branching: Arc<BranchingService>,
    pub auth: AuthState,
    pub authorization: AuthorizationService,
    pub admin_audit: Arc<AdminAuditService>,
}

struct BranchingAdminAuth(AuthUser);

async fn create_branch_handler(
    State(state): State<BranchingState>,
    BranchingAdminAuth(auth_user): BranchingAdminAuth,
    Json(req): Json<CreateBranchRequest>,
) -> Result<impl IntoResponse, StackhouseError> {
    state
        .branching
        .create_branch(&req.name, req.source.as_deref())
        .await?;
    state
        .admin_audit
        .record(
            auth_user.id,
            "branching.create",
            "branch",
            Some(req.name.clone()),
            "success",
            json!({
                "route": "/v1/admin/branches",
                "source": req.source,
            }),
        )
        .await?;
    Ok(Json(
        json!({"success": true, "message": format!("Branch '{}' created", req.name)}),
    ))
}

async fn delete_branch_handler(
    State(state): State<BranchingState>,
    BranchingAdminAuth(auth_user): BranchingAdminAuth,
    axum::extract::Path(name): axum::extract::Path<String>,
) -> Result<impl IntoResponse, StackhouseError> {
    state.branching.delete_branch(&name).await?;
    state
        .admin_audit
        .record(
            auth_user.id,
            "branching.delete",
            "branch",
            Some(name.clone()),
            "success",
            json!({"route": "/v1/admin/branches/:name"}),
        )
        .await?;
    Ok(Json(json!({"success": true})))
}

async fn list_branches_handler(
    State(state): State<BranchingState>,
    BranchingAdminAuth(auth_user): BranchingAdminAuth,
) -> Result<impl IntoResponse, StackhouseError> {
    let branches = state.branching.list_branches().await?;
    state
        .admin_audit
        .record(
            auth_user.id,
            "branching.list",
            "branch",
            None,
            "success",
            json!({"route": "/v1/admin/branches"}),
        )
        .await?;
    Ok(Json(json!({"success": true, "data": branches})))
}

async fn diff_branch_handler(
    State(state): State<BranchingState>,
    BranchingAdminAuth(auth_user): BranchingAdminAuth,
    axum::extract::Path(name): axum::extract::Path<String>,
) -> Result<impl IntoResponse, StackhouseError> {
    let diff = state.branching.diff_branch(&name).await?;
    state
        .admin_audit
        .record(
            auth_user.id,
            "branching.diff",
            "branch",
            Some(name.clone()),
            "success",
            json!({"route": "/v1/admin/branches/:name/diff"}),
        )
        .await?;
    Ok(Json(json!({"success": true, "data": diff})))
}

async fn merge_branch_handler(
    State(state): State<BranchingState>,
    BranchingAdminAuth(auth_user): BranchingAdminAuth,
    axum::extract::Path(name): axum::extract::Path<String>,
    Json(req): Json<MergeBranchRequest>,
) -> Result<impl IntoResponse, StackhouseError> {
    let statements = state
        .branching
        .merge_branch(&name, req.target_branch.as_deref())
        .await?;
    state
        .admin_audit
        .record(
            auth_user.id,
            "branching.merge",
            "branch",
            Some(name.clone()),
            "success",
            json!({
                "route": "/v1/admin/branches/:name/merge",
                "target": req.target_branch,
                "statements_executed": statements.len(),
            }),
        )
        .await?;
    Ok(Json(json!({
        "success": true,
        "message": format!("Merged branch '{}' into '{}'", name, req.target_branch.as_deref().unwrap_or("public")),
        "data": statements
    })))
}

pub fn create_branching_router(state: BranchingState) -> Router {
    Router::new()
        .route(
            "/branches",
            get(list_branches_handler).post(create_branch_handler),
        )
        .route(
            "/branches/:name",
            axum::routing::delete(delete_branch_handler),
        )
        .route("/branches/:name/diff", get(diff_branch_handler))
        .route(
            "/branches/:name/merge",
            axum::routing::post(merge_branch_handler),
        )
        .with_state(state)
}

#[async_trait]
impl FromRequestParts<BranchingState> for BranchingAdminAuth {
    type Rejection = StackhouseError;

    async fn from_request_parts(
        parts: &mut Parts,
        state: &BranchingState,
    ) -> Result<Self, Self::Rejection> {
        let auth_user = extract_auth_user(&state.auth, &parts.headers)?;
        let user = state.auth.auth.get_user_by_id(auth_user.id).await?;
        state
            .authorization
            .require_service_admin_unconditional(&user)?;
        Ok(Self(auth_user))
    }
}
