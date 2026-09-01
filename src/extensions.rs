//! # Postgres Extensions Module (Stackhouse-Extensions)
//!
//! Manage Postgres extensions: install, list, and remove.
//! Provides a safe API for extension management with allowlisting.

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
    routing::{get, post},
    Json, Router,
};
use serde::Deserialize;
use serde_json::{json, Value};
use std::sync::Arc;
use tracing::info;

// Allowed extensions (safe to enable)
const ALLOWED_EXTENSIONS: &[&str] = &[
    "pgcrypto",
    "uuid-ossp",
    "hstore",
    "pg_trgm",
    "citext",
    "postgis",
    "btree_gist",
    "btree_gin",
    "pg_stat_statements",
    "unaccent",
    "fuzzystrmatch",
    "tablefunc",
    "intarray",
    "cube",
    "earthdistance",
    "ltree",
    "isn",
    "lo",
    "plpgsql",
    "pg_jsonschema",
    "moddatetime",
    "pg_graphql",
    "pgjwt",
    "pg_net",
];

#[derive(Clone)]
pub struct ExtensionsService {
    store: Arc<StackhouseStore>,
}

impl ExtensionsService {
    pub fn new(store: Arc<StackhouseStore>) -> Self {
        info!("🧩 Stackhouse-Extensions initialized");
        Self { store }
    }

    pub async fn list_installed(&self) -> StackhouseResult<Vec<Value>> {
        let rows = self
            .store
            .query_simple(
                "SELECT extname, extversion, extrelocatable FROM pg_extension ORDER BY extname"
                    .to_string(),
            )
            .await?;

        let extensions: Vec<Value> = rows
            .into_iter()
            .map(|row| {
                let mut obj = serde_json::Map::new();
                for (k, v) in row {
                    obj.insert(k, v);
                }
                Value::Object(obj)
            })
            .collect();

        Ok(extensions)
    }

    pub async fn list_available(&self) -> StackhouseResult<Vec<Value>> {
        let rows = self.store.query_simple(
            "SELECT name, default_version, comment FROM pg_available_extensions WHERE installed_version IS NULL ORDER BY name".to_string(),
        ).await?;

        let extensions: Vec<Value> = rows
            .into_iter()
            .map(|row| {
                let mut obj = serde_json::Map::new();
                for (k, v) in row {
                    obj.insert(k, v);
                }
                Value::Object(obj)
            })
            .collect();

        Ok(extensions)
    }

    pub async fn install(&self, name: &str) -> StackhouseResult<()> {
        SchemaGuard::validate_identifier(name)?;

        if !ALLOWED_EXTENSIONS.contains(&name) {
            return Err(StackhouseError::InvalidPayload(format!(
                "Extension '{}' is not in the allowlist. Allowed: {:?}",
                name, ALLOWED_EXTENSIONS
            )));
        }

        self.store
            .execute_simple(format!("CREATE EXTENSION IF NOT EXISTS \"{}\"", name))
            .await?;

        info!("🧩 Installed extension: {}", name);
        Ok(())
    }

    pub async fn uninstall(&self, name: &str) -> StackhouseResult<()> {
        SchemaGuard::validate_identifier(name)?;

        // Prevent removing critical extensions
        if name == "plpgsql" {
            return Err(StackhouseError::InvalidPayload(
                "Cannot remove plpgsql".to_string(),
            ));
        }

        self.store
            .execute_simple(format!("DROP EXTENSION IF EXISTS \"{}\" CASCADE", name))
            .await?;

        info!("🧩 Uninstalled extension: {}", name);
        Ok(())
    }
}

// ============================================================================
// Handlers & Router
// ============================================================================

#[derive(Clone)]
pub struct ExtensionsState {
    pub extensions: Arc<ExtensionsService>,
    pub auth: AuthState,
    pub authorization: AuthorizationService,
    pub admin_audit: Arc<AdminAuditService>,
}

#[derive(Deserialize)]
struct ExtensionRequest {
    name: String,
}

struct ExtensionsAdminAuth(AuthUser);

async fn list_installed_handler(
    State(state): State<ExtensionsState>,
    ExtensionsAdminAuth(auth_user): ExtensionsAdminAuth,
) -> Result<impl IntoResponse, StackhouseError> {
    let exts = state.extensions.list_installed().await?;
    state
        .admin_audit
        .record(
            auth_user.id,
            "extensions.list_installed",
            "extensions",
            None,
            "success",
            json!({"route": "/v1/admin/extensions"}),
        )
        .await?;
    Ok(Json(json!({"success": true, "data": exts})))
}

async fn list_available_handler(
    State(state): State<ExtensionsState>,
    ExtensionsAdminAuth(auth_user): ExtensionsAdminAuth,
) -> Result<impl IntoResponse, StackhouseError> {
    let exts = state.extensions.list_available().await?;
    state
        .admin_audit
        .record(
            auth_user.id,
            "extensions.list_available",
            "extensions",
            None,
            "success",
            json!({"route": "/v1/admin/extensions/available"}),
        )
        .await?;
    Ok(Json(json!({"success": true, "data": exts})))
}

async fn install_handler(
    State(state): State<ExtensionsState>,
    ExtensionsAdminAuth(auth_user): ExtensionsAdminAuth,
    Json(req): Json<ExtensionRequest>,
) -> Result<impl IntoResponse, StackhouseError> {
    state.extensions.install(&req.name).await?;
    state
        .admin_audit
        .record(
            auth_user.id,
            "extensions.install",
            "extension",
            Some(req.name.clone()),
            "success",
            json!({
                "route": "/v1/admin/extensions/install",
                "name": req.name,
            }),
        )
        .await?;
    Ok(Json(
        json!({"success": true, "message": format!("Extension '{}' installed", req.name)}),
    ))
}

async fn uninstall_handler(
    State(state): State<ExtensionsState>,
    ExtensionsAdminAuth(auth_user): ExtensionsAdminAuth,
    Json(req): Json<ExtensionRequest>,
) -> Result<impl IntoResponse, StackhouseError> {
    state.extensions.uninstall(&req.name).await?;
    state
        .admin_audit
        .record(
            auth_user.id,
            "extensions.uninstall",
            "extension",
            Some(req.name.clone()),
            "success",
            json!({
                "route": "/v1/admin/extensions/uninstall",
                "name": req.name,
            }),
        )
        .await?;
    Ok(Json(
        json!({"success": true, "message": format!("Extension '{}' removed", req.name)}),
    ))
}

pub fn create_extensions_router(state: ExtensionsState) -> Router {
    Router::new()
        .route("/extensions", get(list_installed_handler))
        .route("/extensions/available", get(list_available_handler))
        .route("/extensions/install", post(install_handler))
        .route("/extensions/uninstall", post(uninstall_handler))
        .with_state(state)
}

#[async_trait]
impl FromRequestParts<ExtensionsState> for ExtensionsAdminAuth {
    type Rejection = StackhouseError;

    async fn from_request_parts(
        parts: &mut Parts,
        state: &ExtensionsState,
    ) -> Result<Self, Self::Rejection> {
        let auth_user = extract_auth_user(&state.auth, &parts.headers)?;
        let user = state.auth.auth.get_user_by_id(auth_user.id).await?;
        state
            .authorization
            .require_service_admin_unconditional(&user)?;
        Ok(Self(auth_user))
    }
}
