//! # Data Residency Configuration
//!
//! Per-tenant region configuration, data routing rules, and residency enforcement.
//! Supports US, EU, APAC regions as required for GDPR and sovereignty laws.

use crate::auth::{extract_auth_user, AuthState};
use crate::db::{SqlValue, StackhouseStore};
use crate::error::{StackhouseError, StackhouseResult};

use axum::{
    extract::State,
    http::HeaderMap,
    response::IntoResponse,
    routing::{get, post},
    Json, Router,
};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::sync::Arc;
use tracing::info;

// ============================================================================
// Types
// ============================================================================

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum DataRegion {
    UsEast1,
    UsWest2,
    EuWest1,
    EuCentral1,
    ApSoutheast1,
    ApNortheast1,
}

impl DataRegion {
    pub fn display_name(&self) -> &str {
        match self {
            Self::UsEast1 => "US East (Virginia)",
            Self::UsWest2 => "US West (Oregon)",
            Self::EuWest1 => "EU West (Ireland)",
            Self::EuCentral1 => "EU Central (Frankfurt)",
            Self::ApSoutheast1 => "Asia Pacific (Singapore)",
            Self::ApNortheast1 => "Asia Pacific (Tokyo)",
        }
    }

    pub fn jurisdiction(&self) -> &str {
        match self {
            Self::UsEast1 | Self::UsWest2 => "US",
            Self::EuWest1 | Self::EuCentral1 => "EU",
            Self::ApSoutheast1 | Self::ApNortheast1 => "APAC",
        }
    }

    pub fn from_str(s: &str) -> Option<Self> {
        match s {
            "us-east-1" | "us_east_1" => Some(Self::UsEast1),
            "us-west-2" | "us_west_2" => Some(Self::UsWest2),
            "eu-west-1" | "eu_west_1" => Some(Self::EuWest1),
            "eu-central-1" | "eu_central_1" => Some(Self::EuCentral1),
            "ap-southeast-1" | "ap_southeast_1" => Some(Self::ApSoutheast1),
            "ap-northeast-1" | "ap_northeast_1" => Some(Self::ApNortheast1),
            _ => None,
        }
    }

    pub fn as_str(&self) -> &str {
        match self {
            Self::UsEast1 => "us-east-1",
            Self::UsWest2 => "us-west-2",
            Self::EuWest1 => "eu-west-1",
            Self::EuCentral1 => "eu-central-1",
            Self::ApSoutheast1 => "ap-southeast-1",
            Self::ApNortheast1 => "ap-northeast-1",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResidencyConfig {
    pub tenant_id: i64,
    pub primary_region: DataRegion,
    pub allowed_regions: Vec<DataRegion>,
    pub backup_region: Option<DataRegion>,
    pub cross_border_allowed: bool,
    pub data_classification: DataClassification,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DataClassification {
    Public,
    Internal,
    Confidential,
    Restricted,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResidencyViolation {
    pub id: String,
    pub tenant_id: i64,
    pub violation_type: String,
    pub source_region: String,
    pub target_region: String,
    pub resource_type: String,
    pub resource_id: String,
    pub detected_at: String,
    pub resolved: bool,
}

// ============================================================================
// Data Residency Service
// ============================================================================

#[derive(Clone)]
pub struct DataResidencyService {
    store: Arc<StackhouseStore>,
}

impl DataResidencyService {
    pub async fn new(store: Arc<StackhouseStore>) -> StackhouseResult<Self> {
        let service = Self { store };
        service.initialize_tables().await?;
        info!("🌍 Data residency service initialized");
        Ok(service)
    }

    async fn initialize_tables(&self) -> StackhouseResult<()> {
        self.store.execute_batch(r#"
            CREATE TABLE IF NOT EXISTS stackhouse_residency_configs (
                tenant_id BIGINT PRIMARY KEY,
                primary_region TEXT NOT NULL,
                allowed_regions TEXT NOT NULL DEFAULT '[]',
                backup_region TEXT,
                cross_border_allowed BOOLEAN DEFAULT FALSE,
                data_classification TEXT NOT NULL DEFAULT 'internal',
                created_at TIMESTAMPTZ DEFAULT NOW(),
                updated_at TIMESTAMPTZ DEFAULT NOW()
            );
            CREATE TABLE IF NOT EXISTS stackhouse_residency_violations (
                id TEXT PRIMARY KEY,
                tenant_id BIGINT NOT NULL,
                violation_type TEXT NOT NULL,
                source_region TEXT NOT NULL,
                target_region TEXT NOT NULL,
                resource_type TEXT NOT NULL,
                resource_id TEXT NOT NULL,
                detected_at TIMESTAMPTZ DEFAULT NOW(),
                resolved BOOLEAN DEFAULT FALSE,
                resolved_at TIMESTAMPTZ
            );
            CREATE INDEX IF NOT EXISTS idx_residency_violations_tenant ON stackhouse_residency_violations(tenant_id);
        "#.to_string()).await?;
        Ok(())
    }

    /// Configure data residency for a tenant
    pub async fn configure(
        &self,
        tenant_id: i64,
        primary_region: DataRegion,
        allowed_regions: Vec<DataRegion>,
        cross_border: bool,
        classification: DataClassification,
    ) -> StackhouseResult<ResidencyConfig> {
        let allowed_str = serde_json::to_string(&allowed_regions).unwrap_or_default();
        let class_str = serde_json::to_string(&classification)
            .unwrap_or_default()
            .trim_matches('"')
            .to_string();

        self.store.execute(
            r#"INSERT INTO stackhouse_residency_configs (tenant_id, primary_region, allowed_regions, cross_border_allowed, data_classification, updated_at)
               VALUES (?, ?, ?, ?, ?, NOW())
               ON CONFLICT (tenant_id) DO UPDATE SET primary_region = EXCLUDED.primary_region, allowed_regions = EXCLUDED.allowed_regions,
               cross_border_allowed = EXCLUDED.cross_border_allowed, data_classification = EXCLUDED.data_classification, updated_at = NOW()"#.to_string(),
            vec![
                SqlValue::Integer(tenant_id),
                SqlValue::Text(primary_region.as_str().to_string()),
                SqlValue::Text(allowed_str),
                SqlValue::Text(cross_border.to_string()),
                SqlValue::Text(class_str),
            ],
        ).await?;

        Ok(ResidencyConfig {
            tenant_id,
            primary_region,
            allowed_regions,
            backup_region: None,
            cross_border_allowed: cross_border,
            data_classification: classification,
            created_at: chrono::Utc::now().to_rfc3339(),
            updated_at: chrono::Utc::now().to_rfc3339(),
        })
    }

    /// Check if a data operation is allowed for the given region
    pub async fn check_allowed(
        &self,
        tenant_id: i64,
        target_region: &DataRegion,
    ) -> StackhouseResult<bool> {
        let rows = self.store.query(
            "SELECT primary_region, allowed_regions, cross_border_allowed FROM stackhouse_residency_configs WHERE tenant_id = ?".to_string(),
            vec![SqlValue::Integer(tenant_id)],
        ).await?;

        if rows.is_empty() {
            return Ok(true); // No policy = allow
        }

        let row = &rows[0];
        let primary = row
            .iter()
            .find(|(k, _)| k == "primary_region")
            .and_then(|(_, v)| v.as_str())
            .unwrap_or("");
        let allowed_str = row
            .iter()
            .find(|(k, _)| k == "allowed_regions")
            .and_then(|(_, v)| v.as_str())
            .unwrap_or("[]");
        let cross_border = row
            .iter()
            .find(|(k, _)| k == "cross_border_allowed")
            .and_then(|(_, v)| v.as_str())
            .unwrap_or("false")
            == "true";

        if target_region.as_str() == primary {
            return Ok(true);
        }

        let allowed: Vec<String> = serde_json::from_str(allowed_str).unwrap_or_default();
        if allowed.iter().any(|r| r == target_region.as_str()) {
            return Ok(true);
        }

        if cross_border {
            return Ok(true);
        }

        // Record violation
        self.record_violation(
            tenant_id,
            "cross_border_transfer",
            primary,
            target_region.as_str(),
            "data",
            "",
        )
        .await?;
        Ok(false)
    }

    /// Record a residency violation
    async fn record_violation(
        &self,
        tenant_id: i64,
        violation_type: &str,
        source: &str,
        target: &str,
        resource_type: &str,
        resource_id: &str,
    ) -> StackhouseResult<()> {
        let id = uuid::Uuid::new_v4().to_string();
        self.store.execute(
            "INSERT INTO stackhouse_residency_violations (id, tenant_id, violation_type, source_region, target_region, resource_type, resource_id) VALUES (?, ?, ?, ?, ?, ?, ?)".to_string(),
            vec![
                SqlValue::Text(id),
                SqlValue::Integer(tenant_id),
                SqlValue::Text(violation_type.to_string()),
                SqlValue::Text(source.to_string()),
                SqlValue::Text(target.to_string()),
                SqlValue::Text(resource_type.to_string()),
                SqlValue::Text(resource_id.to_string()),
            ],
        ).await?;
        Ok(())
    }

    /// Get residency config for a tenant
    pub async fn get_config(&self, tenant_id: i64) -> StackhouseResult<Value> {
        let rows = self.store.query(
            "SELECT primary_region, allowed_regions, backup_region, cross_border_allowed, data_classification, created_at, updated_at FROM stackhouse_residency_configs WHERE tenant_id = ?".to_string(),
            vec![SqlValue::Integer(tenant_id)],
        ).await?;
        if rows.is_empty() {
            return Err(StackhouseError::NotFound(
                "No residency config for tenant".into(),
            ));
        }
        Ok(json!(rows[0]
            .iter()
            .cloned()
            .collect::<std::collections::HashMap<_, _>>()))
    }

    /// List violations for a tenant
    pub async fn list_violations(&self, tenant_id: i64) -> StackhouseResult<Vec<Value>> {
        let rows = self.store.query(
            "SELECT id, violation_type, source_region, target_region, resource_type, resource_id, detected_at, resolved FROM stackhouse_residency_violations WHERE tenant_id = ? ORDER BY detected_at DESC LIMIT 100".to_string(),
            vec![SqlValue::Integer(tenant_id)],
        ).await?;
        Ok(rows
            .into_iter()
            .map(|r| json!(r.into_iter().collect::<std::collections::HashMap<_, _>>()))
            .collect())
    }

    /// Get available regions
    pub fn available_regions() -> Vec<Value> {
        vec![
            json!({"id": "us-east-1", "name": "US East (Virginia)", "jurisdiction": "US"}),
            json!({"id": "us-west-2", "name": "US West (Oregon)", "jurisdiction": "US"}),
            json!({"id": "eu-west-1", "name": "EU West (Ireland)", "jurisdiction": "EU"}),
            json!({"id": "eu-central-1", "name": "EU Central (Frankfurt)", "jurisdiction": "EU"}),
            json!({"id": "ap-southeast-1", "name": "Asia Pacific (Singapore)", "jurisdiction": "APAC"}),
            json!({"id": "ap-northeast-1", "name": "Asia Pacific (Tokyo)", "jurisdiction": "APAC"}),
        ]
    }
}

// ============================================================================
// Router
// ============================================================================

#[derive(Clone)]
pub struct DataResidencyState {
    pub residency: Arc<DataResidencyService>,
    pub auth: AuthState,
}

#[derive(Deserialize)]
struct ConfigureRequest {
    primary_region: String,
    #[serde(default)]
    allowed_regions: Vec<String>,
    #[serde(default)]
    cross_border_allowed: bool,
    #[serde(default = "default_classification")]
    data_classification: String,
}
fn default_classification() -> String {
    "internal".to_string()
}

async fn get_config_handler(
    State(state): State<DataResidencyState>,
    headers: HeaderMap,
) -> Result<impl IntoResponse, StackhouseError> {
    let user = extract_auth_user(&state.auth, &headers)?;
    let config = state.residency.get_config(user.id).await?;
    Ok(Json(json!({"success": true, "data": config})))
}

async fn configure_handler(
    State(state): State<DataResidencyState>,
    headers: HeaderMap,
    Json(req): Json<ConfigureRequest>,
) -> Result<impl IntoResponse, StackhouseError> {
    let user = extract_auth_user(&state.auth, &headers)?;
    let primary = DataRegion::from_str(&req.primary_region)
        .ok_or_else(|| StackhouseError::InvalidPayload("Invalid primary region".into()))?;
    let allowed: Vec<DataRegion> = req
        .allowed_regions
        .iter()
        .filter_map(|r| DataRegion::from_str(r))
        .collect();
    let classification = match req.data_classification.as_str() {
        "public" => DataClassification::Public,
        "confidential" => DataClassification::Confidential,
        "restricted" => DataClassification::Restricted,
        _ => DataClassification::Internal,
    };
    let config = state
        .residency
        .configure(
            user.id,
            primary,
            allowed,
            req.cross_border_allowed,
            classification,
        )
        .await?;
    Ok(Json(json!({"success": true, "data": config})))
}

async fn list_violations_handler(
    State(state): State<DataResidencyState>,
    headers: HeaderMap,
) -> Result<impl IntoResponse, StackhouseError> {
    let user = extract_auth_user(&state.auth, &headers)?;
    let violations = state.residency.list_violations(user.id).await?;
    Ok(Json(json!({"success": true, "data": violations})))
}

async fn regions_handler() -> impl IntoResponse {
    Json(json!({"success": true, "data": DataResidencyService::available_regions()}))
}

pub fn create_data_residency_router(state: DataResidencyState) -> Router {
    Router::new()
        .route("/config", get(get_config_handler))
        .route("/config", post(configure_handler))
        .route("/violations", get(list_violations_handler))
        .route("/regions", get(regions_handler))
        .with_state(state)
}
