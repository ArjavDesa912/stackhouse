//! # Management / Platform API
//!
//! Partner-facing endpoints for programmatic project provisioning,
//! analogous to Supabase's Management API.

use crate::api::AdminAuditService;
use crate::error::StackhouseError;
use crate::platform::{ProvisionProjectRequest, ProvisioningService};

use axum::{
    extract::State,
    http::{HeaderMap, StatusCode},
    response::IntoResponse,
    routing::post,
    Json, Router,
};
use serde_json::json;
use std::sync::Arc;
use tracing::info;

#[derive(Clone)]
pub struct PlatformState {
    pub provisioning: ProvisioningService,
    pub audit: Arc<AdminAuditService>,
    pub db_url: String,
    pub base_url: String,
}

/// POST /v1/platform/projects
async fn provision_project_handler(
    State(state): State<PlatformState>,
    headers: HeaderMap,
    Json(req): Json<ProvisionProjectRequest>,
) -> Result<impl IntoResponse, StackhouseError> {
    // Require partner key in X-Partner-Key header
    let partner_key = headers
        .get("x-partner-key")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("")
        .to_string();

    if !state.provisioning.validate_partner_key(&partner_key) {
        state
            .audit
            .record(
                0,
                "project.provision",
                "project",
                None,
                "denied",
                json!({"reason": "invalid_partner_key"}),
            )
            .await
            .ok();
        return Err(StackhouseError::Unauthorized("Invalid partner key".into()));
    }

    let response = state
        .provisioning
        .provision_project(req, &state.db_url)
        .await?;

    state
        .audit
        .record(
            response.service_account.id,
            "project.provision",
            "project",
            Some(response.project.slug.clone()),
            "success",
            json!({"tenant_id": response.project.id, "bucket": response.bucket }),
        )
        .await
        .ok();

    info!(
        "🚀 Partner provisioned project: {} (tenant {})",
        response.project.slug, response.project.id
    );

    Ok((
        StatusCode::CREATED,
        Json(json!({
            "success": true,
            "data": response
        })),
    ))
}

pub fn create_platform_router(state: PlatformState) -> Router {
    Router::new()
        .route("/projects", post(provision_project_handler))
        .with_state(state)
}
