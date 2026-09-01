//! # Multi-Tenancy: Isolation, White-Label, Sub-Orgs
//!
//! Schema-level and row-level tenant isolation, white-label domain mapping,
//! and hierarchical sub-organization support.

use crate::db::{SqlValue, StackhouseStore};
use crate::error::{StackhouseError, StackhouseResult};

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::info;

// ============================================================================
// Types
// ============================================================================

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Tenant {
    pub id: i64,
    pub name: String,
    pub slug: String,
    pub parent_id: Option<i64>,
    pub isolation_level: IsolationLevel,
    pub custom_domain: Option<String>,
    pub branding: TenantBranding,
    pub settings: Value,
    pub status: TenantStatus,
    pub created_at: String,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum IsolationLevel {
    Shared,        // Row-level isolation (RLS)
    SchemaLevel,   // Separate schema per tenant
    DatabaseLevel, // Separate database per tenant
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum TenantStatus {
    Active,
    Suspended,
    Provisioning,
    Deleted,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct TenantBranding {
    pub logo_url: Option<String>,
    pub primary_color: Option<String>,
    pub secondary_color: Option<String>,
    pub favicon_url: Option<String>,
    pub company_name: Option<String>,
    pub support_email: Option<String>,
    pub custom_css: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SubOrganization {
    pub id: i64,
    pub parent_id: i64,
    pub name: String,
    pub path: String, // e.g., "acme/engineering/backend"
    pub depth: u32,
}

// ============================================================================
// Service
// ============================================================================

#[derive(Clone)]
pub struct MultiTenancyService {
    store: Arc<StackhouseStore>,
    domain_cache: Arc<RwLock<HashMap<String, i64>>>, // domain -> tenant_id
}

impl MultiTenancyService {
    pub async fn new(store: Arc<StackhouseStore>) -> StackhouseResult<Self> {
        let service = Self {
            store,
            domain_cache: Arc::new(RwLock::new(HashMap::new())),
        };
        service.initialize_tables().await?;
        service.load_domain_cache().await?;
        info!("🏢 Multi-tenancy service initialized");
        Ok(service)
    }

    async fn initialize_tables(&self) -> StackhouseResult<()> {
        self.store
            .execute_batch(
                r#"
            CREATE TABLE IF NOT EXISTS stackhouse_tenants (
                id BIGSERIAL PRIMARY KEY,
                name TEXT NOT NULL,
                slug TEXT NOT NULL UNIQUE,
                parent_id BIGINT REFERENCES stackhouse_tenants(id),
                isolation_level TEXT DEFAULT 'shared',
                custom_domain TEXT UNIQUE,
                branding JSONB DEFAULT '{}',
                settings JSONB DEFAULT '{}',
                status TEXT DEFAULT 'active',
                created_at TIMESTAMPTZ DEFAULT NOW()
            );
            CREATE TABLE IF NOT EXISTS stackhouse_tenant_domains (
                domain TEXT PRIMARY KEY,
                tenant_id BIGINT NOT NULL REFERENCES stackhouse_tenants(id),
                verified BOOLEAN DEFAULT FALSE,
                ssl_provisioned BOOLEAN DEFAULT FALSE,
                created_at TIMESTAMPTZ DEFAULT NOW()
            );
            CREATE INDEX IF NOT EXISTS idx_tenants_parent ON stackhouse_tenants(parent_id);
            CREATE INDEX IF NOT EXISTS idx_tenants_slug ON stackhouse_tenants(slug);
            CREATE INDEX IF NOT EXISTS idx_tenants_domain ON stackhouse_tenants(custom_domain);
        "#
                .to_string(),
            )
            .await?;
        Ok(())
    }

    async fn load_domain_cache(&self) -> StackhouseResult<()> {
        let rows = self
            .store
            .query(
                "SELECT domain, tenant_id FROM stackhouse_tenant_domains WHERE verified = true"
                    .to_string(),
                vec![],
            )
            .await?;
        let mut cache = self.domain_cache.write().await;
        for row in rows {
            let domain = row
                .iter()
                .find(|(k, _)| k == "domain")
                .and_then(|(_, v)| v.as_str())
                .unwrap_or("");
            let tid = row
                .iter()
                .find(|(k, _)| k == "tenant_id")
                .and_then(|(_, v)| v.as_i64())
                .unwrap_or(0);
            cache.insert(domain.to_string(), tid);
        }
        Ok(())
    }

    /// Create a new tenant
    pub async fn create_tenant(
        &self,
        name: &str,
        slug: &str,
        parent_id: Option<i64>,
        isolation_level: IsolationLevel,
    ) -> StackhouseResult<Tenant> {
        let iso_str = serde_json::to_string(&isolation_level)
            .unwrap_or_default()
            .trim_matches('"')
            .to_string();

        let rows = self.store.query(
            "INSERT INTO stackhouse_tenants (name, slug, parent_id, isolation_level) VALUES (?, ?, ?, ?) RETURNING id, created_at".to_string(),
            vec![
                SqlValue::Text(name.to_string()),
                SqlValue::Text(slug.to_string()),
                SqlValue::Integer(parent_id.unwrap_or(0)),
                SqlValue::Text(iso_str),
            ],
        ).await?;

        let id = rows
            .first()
            .and_then(|r| r.iter().find(|(k, _)| k == "id"))
            .and_then(|(_, v)| v.as_i64())
            .unwrap_or(0);

        // If schema-level isolation, create schema
        if matches!(isolation_level, IsolationLevel::SchemaLevel) {
            self.store
                .execute_batch(format!("CREATE SCHEMA IF NOT EXISTS tenant_{}", id))
                .await
                .ok();
        }

        info!("🏢 Tenant created: {} ({})", name, slug);

        Ok(Tenant {
            id,
            name: name.to_string(),
            slug: slug.to_string(),
            parent_id,
            isolation_level,
            custom_domain: None,
            branding: TenantBranding::default(),
            settings: json!({}),
            status: TenantStatus::Active,
            created_at: chrono::Utc::now().to_rfc3339(),
        })
    }

    /// Resolve tenant from custom domain
    pub async fn resolve_domain(&self, domain: &str) -> Option<i64> {
        let cache = self.domain_cache.read().await;
        cache.get(domain).copied()
    }

    /// Map a custom domain to a tenant (white-label)
    pub async fn add_custom_domain(&self, tenant_id: i64, domain: &str) -> StackhouseResult<()> {
        self.store.execute(
            "INSERT INTO stackhouse_tenant_domains (domain, tenant_id) VALUES (?, ?) ON CONFLICT (domain) DO UPDATE SET tenant_id = EXCLUDED.tenant_id".to_string(),
            vec![SqlValue::Text(domain.to_string()), SqlValue::Integer(tenant_id)],
        ).await?;
        self.store
            .execute(
                "UPDATE stackhouse_tenants SET custom_domain = ? WHERE id = ?".to_string(),
                vec![
                    SqlValue::Text(domain.to_string()),
                    SqlValue::Integer(tenant_id),
                ],
            )
            .await?;
        self.domain_cache
            .write()
            .await
            .insert(domain.to_string(), tenant_id);
        Ok(())
    }

    /// Set tenant branding
    pub async fn set_branding(
        &self,
        tenant_id: i64,
        branding: TenantBranding,
    ) -> StackhouseResult<()> {
        self.store
            .execute(
                "UPDATE stackhouse_tenants SET branding = ?::jsonb WHERE id = ?".to_string(),
                vec![
                    SqlValue::Text(serde_json::to_string(&branding).unwrap_or_default()),
                    SqlValue::Integer(tenant_id),
                ],
            )
            .await?;
        Ok(())
    }

    /// Get sub-organizations (recursive)
    pub async fn get_sub_orgs(&self, parent_id: i64) -> StackhouseResult<Vec<Value>> {
        let rows = self.store.query(
            r#"WITH RECURSIVE org_tree AS (
                SELECT id, name, slug, parent_id, 1 as depth FROM stackhouse_tenants WHERE parent_id = ?
                UNION ALL
                SELECT t.id, t.name, t.slug, t.parent_id, ot.depth + 1
                FROM stackhouse_tenants t JOIN org_tree ot ON t.parent_id = ot.id
                WHERE ot.depth < 5
            )
            SELECT * FROM org_tree ORDER BY depth, name"#.to_string(),
            vec![SqlValue::Integer(parent_id)],
        ).await?;
        Ok(rows
            .into_iter()
            .map(|r| json!(r.into_iter().collect::<std::collections::HashMap<_, _>>()))
            .collect())
    }

    /// Get tenant by ID
    pub async fn get_tenant(&self, tenant_id: i64) -> StackhouseResult<Value> {
        let rows = self.store.query(
            "SELECT id, name, slug, parent_id, isolation_level, custom_domain, branding, settings, status, created_at FROM stackhouse_tenants WHERE id = ?".to_string(),
            vec![SqlValue::Integer(tenant_id)],
        ).await?;
        if rows.is_empty() {
            return Err(StackhouseError::NotFound("Tenant not found".into()));
        }
        Ok(json!(rows[0]
            .iter()
            .cloned()
            .collect::<std::collections::HashMap<_, _>>()))
    }
}
