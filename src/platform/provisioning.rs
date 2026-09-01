//! # Management / Provisioning API
//!
//! Programmatic project/tenant creation for partner integrations (Bolt, Lovable,
//! v0, Replit, etc.). One `POST /v1/platform/projects` call provisions an
//! isolated tenant, schema, service account, API key, and storage bucket.

use crate::auth::{ApiKeyCreated, ApiKeyService, AuthService};
use crate::db::{SqlValue, StackhouseStore};
use crate::error::{StackhouseError, StackhouseResult};
use crate::platform::{IsolationLevel, MultiTenancyService};
use crate::storage::{CreateBucketRequest, StorageService};

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::path::PathBuf;
use std::sync::Arc;
use tracing::info;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProvisionProjectRequest {
    pub name: String,
    pub slug: Option<String>,
    pub isolation: Option<String>,
    pub region: Option<String>,
    pub partner_metadata: Option<Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProvisionProjectResponse {
    pub project: ProjectSummary,
    pub service_account: ServiceAccount,
    pub api_key: ApiKeyCreated,
    pub bucket: String,
    pub database_url: String,
    pub api_base_url: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProjectSummary {
    pub id: i64,
    pub name: String,
    pub slug: String,
    pub isolation: String,
    pub schema: String,
    pub status: String,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServiceAccount {
    pub id: i64,
    pub email: String,
}

#[derive(Clone)]
pub struct ProvisioningService {
    store: Arc<StackhouseStore>,
    multi_tenancy: MultiTenancyService,
    api_keys: ApiKeyService,
    auth: AuthService,
    storage: StorageService,
    partner_keys: Vec<String>,
    base_url: String,
}

impl ProvisioningService {
    pub async fn new(
        store: Arc<StackhouseStore>,
        storage_path: PathBuf,
        partner_keys: Vec<String>,
        base_url: String,
    ) -> StackhouseResult<Self> {
        let multi_tenancy = MultiTenancyService::new(Arc::clone(&store)).await?;
        let api_keys = ApiKeyService::new(Arc::clone(&store)).await?;
        let auth = AuthService::new(Arc::clone(&store), vec![]).await?;
        let storage = StorageService::new(Arc::clone(&store), Some(storage_path)).await?;

        Ok(Self {
            store,
            multi_tenancy,
            api_keys,
            auth,
            storage,
            partner_keys,
            base_url,
        })
    }

    /// Validate a partner key. Accepts a `vdb_`-prefixed key or raw token.
    pub fn validate_partner_key(&self, key: &str) -> bool {
        let key = key.strip_prefix("vdb_").unwrap_or(key);
        self.partner_keys.iter().any(|k| {
            let k = k.strip_prefix("vdb_").unwrap_or(k);
            crate::security::constant_time_compare(k, key)
        })
    }

    /// Provision a new project (tenant + schema + service account + API key + bucket)
    pub async fn provision_project(
        &self,
        req: ProvisionProjectRequest,
        db_url: &str,
    ) -> StackhouseResult<ProvisionProjectResponse> {
        let name = req.name.trim().to_string();
        if name.is_empty() {
            return Err(StackhouseError::InvalidPayload(
                "Project name is required".into(),
            ));
        }

        let slug = req
            .slug
            .as_ref()
            .map(|s| s.to_lowercase())
            .unwrap_or_else(|| Self::slugify(&name));

        if slug.is_empty()
            || !slug
                .chars()
                .all(|c| c.is_ascii_lowercase() || c == '-' || c.is_ascii_digit())
        {
            return Err(StackhouseError::InvalidPayload(
                "Project slug must be lowercase letters, numbers, and hyphens".into(),
            ));
        }

        let isolation = match req.isolation.as_deref() {
            Some("schema") => IsolationLevel::SchemaLevel,
            Some("database") => IsolationLevel::DatabaseLevel,
            _ => IsolationLevel::Shared,
        };

        // 1. Create tenant
        let tenant = self
            .multi_tenancy
            .create_tenant(&name, &slug, None, isolation)
            .await?;

        // 2. Create schema-level isolation if requested
        let schema = format!("tenant_{}", tenant.id);
        if matches!(isolation, IsolationLevel::SchemaLevel) {
            self.store
                .execute_batch(format!("CREATE SCHEMA IF NOT EXISTS {}", schema))
                .await
                .ok();
        }

        // 3. Create service account user
        let email = format!("admin-{}-{}@stackhouse.internal", slug, tenant.id);
        let password = Self::generate_secret(32);
        let user_id = self.create_service_user(&email, &password).await?;

        // 4. Create API key with broad project scopes
        let key_name = format!("{}-project-key", slug);
        let scopes = vec![
            "data:read".into(),
            "data:write".into(),
            "data:delete".into(),
            "storage:read".into(),
            "storage:write".into(),
            "storage:delete".into(),
            "auth:read".into(),
            "auth:manage".into(),
            "teams:read".into(),
            "teams:manage".into(),
            "functions:invoke".into(),
            "functions:manage".into(),
            "realtime:subscribe".into(),
            "brain:query".into(),
            "brain:manage".into(),
            "mcp:write".into(),
        ];
        let api_key = self
            .api_keys
            .create_key(user_id, &key_name, scopes, None)
            .await?;

        // 5. Create default storage bucket
        let bucket_name = format!("{}-default", slug).replace('_', "-");
        let bucket = self
            .storage
            .create_bucket(
                CreateBucketRequest {
                    name: bucket_name.clone(),
                    public: false,
                },
                Some(user_id),
            )
            .await?;

        // 6. Record provisioning metadata
        self.store
            .execute(
                "UPDATE stackhouse_tenants SET settings = ?::jsonb WHERE id = ?".to_string(),
                vec![
                    SqlValue::Text(
                        json!({
                            "provisioned_by": "partner_api",
                            "project_bucket": bucket.name,
                            "service_user_id": user_id,
                            "api_key_id": api_key.id,
                            "region": req.region,
                            "partner_metadata": req.partner_metadata,
                        })
                        .to_string(),
                    ),
                    SqlValue::Integer(tenant.id),
                ],
            )
            .await?;

        info!(
            "✅ Provisioned project '{}' (tenant {}, slug {}, user {})",
            name, tenant.id, slug, user_id
        );

        Ok(ProvisionProjectResponse {
            project: ProjectSummary {
                id: tenant.id,
                name: tenant.name,
                slug: tenant.slug,
                isolation: serde_json::to_string(&isolation)
                    .unwrap_or_default()
                    .trim_matches('"')
                    .to_string(),
                schema,
                status: "active".to_string(),
                created_at: tenant.created_at,
            },
            service_account: ServiceAccount { id: user_id, email },
            api_key,
            bucket: bucket.name,
            database_url: db_url.to_string(),
            api_base_url: self.base_url.clone(),
        })
    }

    async fn create_service_user(&self, email: &str, password: &str) -> StackhouseResult<i64> {
        // Use the same password hashing as AuthService
        let password_hash = self.auth.hash_password(password)?;

        let rows = self
            .store
            .query(
                "SELECT id FROM stackhouse_users WHERE email = ?".to_string(),
                vec![SqlValue::Text(email.to_string())],
            )
            .await?;

        if !rows.is_empty() {
            return Err(StackhouseError::Conflict(
                "Service user email already exists".into(),
            ));
        }

        self.store.insert_returning_id(
            "INSERT INTO stackhouse_users (email, password_hash, metadata) VALUES (?, ?, ?::jsonb)".to_string(),
            vec![
                SqlValue::Text(email.to_string()),
                SqlValue::Text(password_hash),
                SqlValue::Text(json!({"type": "service_account", "auto_provisioned": true}).to_string()),
            ],
        ).await
    }

    fn slugify(name: &str) -> String {
        name.to_lowercase()
            .chars()
            .map(|c| if c.is_ascii_alphanumeric() { c } else { '-' })
            .collect::<String>()
            .trim_matches('-')
            .split('-')
            .filter(|s| !s.is_empty())
            .collect::<Vec<_>>()
            .join("-")
    }

    fn generate_secret(len: usize) -> String {
        use rand::Rng;
        const ALPHANUM: &[u8] = b"abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789";
        let mut rng = rand::thread_rng();
        (0..len)
            .map(|_| ALPHANUM[rng.gen_range(0..ALPHANUM.len())] as char)
            .collect()
    }
}

/// Constant-time string comparison helper used for partner-key checks.
pub use crate::security::constant_time_compare;
