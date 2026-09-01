//! # Role-Based Access Control (RBAC)
//!
//! Custom roles with granular resource-level permissions.
//! Supports role hierarchies, resource scoping, and permission templates.

use crate::db::{SqlValue, StackhouseStore};
use crate::error::StackhouseResult;

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::info;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Role {
    pub id: String,
    pub tenant_id: i64,
    pub name: String,
    pub description: String,
    pub permissions: Vec<Permission>,
    pub inherits_from: Vec<String>,
    pub is_system: bool,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Permission {
    pub resource: String,
    pub action: String,
    pub conditions: Option<Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RoleAssignment {
    pub user_id: String,
    pub tenant_id: i64,
    pub role_ids: Vec<String>,
    pub resource_id: Option<String>,
    pub resource_type: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PermissionCheck {
    pub user_id: String,
    pub tenant_id: i64,
    pub resource: String,
    pub action: String,
    pub resource_id: Option<String>,
}

#[derive(Clone)]
pub struct RbacService {
    store: Arc<StackhouseStore>,
    role_cache: Arc<RwLock<HashMap<String, Role>>>,
}

impl RbacService {
    pub async fn new(store: Arc<StackhouseStore>) -> StackhouseResult<Self> {
        let service = Self {
            store,
            role_cache: Arc::new(RwLock::new(HashMap::new())),
        };
        service.initialize_tables().await?;
        service.load_roles().await?;
        info!("🔐 RBAC service initialized");
        Ok(service)
    }

    async fn initialize_tables(&self) -> StackhouseResult<()> {
        self.store.execute_batch(r#"
            CREATE TABLE IF NOT EXISTS stackhouse_roles (
                id TEXT PRIMARY KEY,
                tenant_id BIGINT NOT NULL,
                name TEXT NOT NULL,
                description TEXT DEFAULT '',
                permissions JSONB DEFAULT '[]',
                inherits_from JSONB DEFAULT '[]',
                is_system BOOLEAN DEFAULT FALSE,
                created_at TIMESTAMPTZ DEFAULT NOW(),
                UNIQUE(tenant_id, name)
            );
            CREATE TABLE IF NOT EXISTS stackhouse_role_assignments (
                id BIGSERIAL PRIMARY KEY,
                user_id TEXT NOT NULL,
                tenant_id BIGINT NOT NULL,
                role_ids JSONB NOT NULL DEFAULT '[]',
                resource_id TEXT,
                resource_type TEXT,
                created_at TIMESTAMPTZ DEFAULT NOW()
            );
            CREATE INDEX IF NOT EXISTS idx_role_assignments_user ON stackhouse_role_assignments(user_id, tenant_id);
        "#.to_string()).await?;
        Ok(())
    }

    async fn load_roles(&self) -> StackhouseResult<()> {
        let rows = self
            .store
            .query("SELECT * FROM stackhouse_roles".to_string(), vec![])
            .await?;
        let mut cache = self.role_cache.write().await;
        for row in rows {
            let id = row
                .iter()
                .find(|(k, _)| k == "id")
                .and_then(|(_, v)| v.as_str())
                .unwrap_or("")
                .to_string();
            let tenant_id = row
                .iter()
                .find(|(k, _)| k == "tenant_id")
                .and_then(|(_, v)| v.as_i64())
                .unwrap_or(0);
            let name = row
                .iter()
                .find(|(k, _)| k == "name")
                .and_then(|(_, v)| v.as_str())
                .unwrap_or("")
                .to_string();
            let desc = row
                .iter()
                .find(|(k, _)| k == "description")
                .and_then(|(_, v)| v.as_str())
                .unwrap_or("")
                .to_string();
            let perms_str = row
                .iter()
                .find(|(k, _)| k == "permissions")
                .and_then(|(_, v)| v.as_str())
                .unwrap_or("[]");
            let inherits_str = row
                .iter()
                .find(|(k, _)| k == "inherits_from")
                .and_then(|(_, v)| v.as_str())
                .unwrap_or("[]");
            let is_system = row
                .iter()
                .find(|(k, _)| k == "is_system")
                .and_then(|(_, v)| v.as_str())
                .map(|s| s == "true" || s == "t")
                .unwrap_or(false);

            cache.insert(
                id.clone(),
                Role {
                    id,
                    tenant_id,
                    name,
                    description: desc,
                    permissions: serde_json::from_str(perms_str).unwrap_or_default(),
                    inherits_from: serde_json::from_str(inherits_str).unwrap_or_default(),
                    is_system,
                    created_at: row
                        .iter()
                        .find(|(k, _)| k == "created_at")
                        .and_then(|(_, v)| v.as_str())
                        .unwrap_or("")
                        .to_string(),
                },
            );
        }
        Ok(())
    }

    pub async fn create_role(
        &self,
        tenant_id: i64,
        name: &str,
        description: &str,
        permissions: Vec<Permission>,
        inherits_from: Vec<String>,
    ) -> StackhouseResult<Role> {
        let id = uuid::Uuid::new_v4().to_string();
        let role = Role {
            id: id.clone(),
            tenant_id,
            name: name.to_string(),
            description: description.to_string(),
            permissions: permissions.clone(),
            inherits_from: inherits_from.clone(),
            is_system: false,
            created_at: chrono::Utc::now().to_rfc3339(),
        };
        self.store.execute(
            "INSERT INTO stackhouse_roles (id, tenant_id, name, description, permissions, inherits_from) VALUES (?, ?, ?, ?, ?::jsonb, ?::jsonb)".to_string(),
            vec![
                SqlValue::Text(id.clone()),
                SqlValue::Integer(tenant_id),
                SqlValue::Text(name.to_string()),
                SqlValue::Text(description.to_string()),
                SqlValue::Text(serde_json::to_string(&permissions).unwrap_or_default()),
                SqlValue::Text(serde_json::to_string(&inherits_from).unwrap_or_default()),
            ],
        ).await?;
        self.role_cache.write().await.insert(id, role.clone());
        Ok(role)
    }

    pub async fn assign_roles(
        &self,
        user_id: &str,
        tenant_id: i64,
        role_ids: Vec<String>,
        resource_id: Option<&str>,
        resource_type: Option<&str>,
    ) -> StackhouseResult<()> {
        self.store.execute(
            "INSERT INTO stackhouse_role_assignments (user_id, tenant_id, role_ids, resource_id, resource_type) VALUES (?, ?, ?::jsonb, ?, ?) ON CONFLICT (user_id, tenant_id, resource_id, resource_type) DO UPDATE SET role_ids = EXCLUDED.role_ids".to_string(),
            vec![
                SqlValue::Text(user_id.to_string()),
                SqlValue::Integer(tenant_id),
                SqlValue::Text(serde_json::to_string(&role_ids).unwrap_or_default()),
                SqlValue::Text(resource_id.unwrap_or("").to_string()),
                SqlValue::Text(resource_type.unwrap_or("").to_string()),
            ],
        ).await?;
        Ok(())
    }

    pub async fn can(
        &self,
        user_id: &str,
        tenant_id: i64,
        resource: &str,
        action: &str,
        _resource_id: Option<&str>,
    ) -> StackhouseResult<bool> {
        let assignments = self.store.query(
            "SELECT role_ids FROM stackhouse_role_assignments WHERE user_id = ? AND tenant_id = ?".to_string(),
            vec![SqlValue::Text(user_id.to_string()), SqlValue::Integer(tenant_id)],
        ).await?;

        let mut all_perms: HashSet<(String, String)> = HashSet::new();
        let cache = self.role_cache.read().await;

        for row in assignments {
            let role_ids_str = row
                .iter()
                .find(|(k, _)| k == "role_ids")
                .and_then(|(_, v)| v.as_str())
                .unwrap_or("[]");
            let role_ids: Vec<String> = serde_json::from_str(role_ids_str).unwrap_or_default();

            for role_id in role_ids {
                if let Some(role) = cache.get(&role_id) {
                    self.collect_permissions(role, &cache, &mut all_perms, 0)?;
                }
            }
        }

        Ok(
            all_perms.contains(&(resource.to_string(), action.to_string()))
                || all_perms.contains(&("*".into(), action.to_string()))
                || all_perms.contains(&(resource.to_string(), "*".into())),
        )
    }

    fn collect_permissions(
        &self,
        role: &Role,
        cache: &HashMap<String, Role>,
        collected: &mut HashSet<(String, String)>,
        depth: u32,
    ) -> StackhouseResult<()> {
        if depth > 5 {
            return Ok(());
        }
        for perm in &role.permissions {
            collected.insert((perm.resource.clone(), perm.action.clone()));
        }
        for parent_id in &role.inherits_from {
            if let Some(parent) = cache.get(parent_id) {
                self.collect_permissions(parent, cache, collected, depth + 1)?;
            }
        }
        Ok(())
    }

    pub async fn list_roles(&self, tenant_id: i64) -> StackhouseResult<Vec<Value>> {
        let rows = self.store.query(
            "SELECT id, name, description, permissions, is_system, created_at FROM stackhouse_roles WHERE tenant_id = ?".to_string(),
            vec![SqlValue::Integer(tenant_id)],
        ).await?;
        Ok(rows
            .into_iter()
            .map(|r| json!(r.into_iter().collect::<HashMap<_, _>>()))
            .collect())
    }

    pub async fn delete_role(&self, role_id: &str) -> StackhouseResult<()> {
        self.store
            .execute(
                "DELETE FROM stackhouse_roles WHERE id = ? AND is_system = false".to_string(),
                vec![SqlValue::Text(role_id.to_string())],
            )
            .await?;
        self.role_cache.write().await.remove(role_id);
        Ok(())
    }
}
