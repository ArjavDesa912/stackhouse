//! # Per-Object & Per-Bucket Access Control Lists
//!
//! ACL policies tied to the auth layer. Bucket-level defaults with object-level overrides.

use crate::auth::User;
use crate::db::{SqlValue, StackhouseStore};
use crate::error::StackhouseResult;

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::sync::Arc;
use tracing::info;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StorageAcl {
    pub id: String,
    pub bucket: String,
    pub object_key: Option<String>, // None = bucket-level ACL
    pub principal: AclPrincipal,
    pub permissions: Vec<AclPermission>,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AclPrincipal {
    Public,
    Authenticated,
    UserId(i64),
    RoleId(String),
    TeamId(i64),
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum AclPermission {
    Read,
    Write,
    Delete,
    Admin,
}

#[derive(Clone)]
pub struct StorageAclService {
    store: Arc<StackhouseStore>,
}

impl StorageAclService {
    pub async fn new(store: Arc<StackhouseStore>) -> StackhouseResult<Self> {
        let service = Self { store };
        service.initialize_tables().await?;
        info!("🔒 Storage ACL service initialized");
        Ok(service)
    }

    async fn initialize_tables(&self) -> StackhouseResult<()> {
        self.store.execute_batch(r#"
            CREATE TABLE IF NOT EXISTS stackhouse_storage_acls (
                id TEXT PRIMARY KEY,
                bucket TEXT NOT NULL,
                object_key TEXT,
                principal_type TEXT NOT NULL,
                principal_value TEXT NOT NULL,
                permissions TEXT NOT NULL DEFAULT '["read"]',
                created_at TIMESTAMPTZ DEFAULT NOW()
            );
            CREATE INDEX IF NOT EXISTS idx_storage_acls_bucket ON stackhouse_storage_acls(bucket);
            CREATE INDEX IF NOT EXISTS idx_storage_acls_object ON stackhouse_storage_acls(bucket, object_key);
        "#.to_string()).await?;
        Ok(())
    }

    /// Check if a user has permission for an operation
    pub async fn check_permission(
        &self,
        bucket: &str,
        key: Option<&str>,
        user: &User,
        permission: AclPermission,
    ) -> StackhouseResult<bool> {
        // Check object-level ACL first if key provided
        if let Some(k) = key {
            let rows = self.store.query(
                "SELECT principal_type, principal_value, permissions FROM stackhouse_storage_acls WHERE bucket = ? AND object_key = ?".to_string(),
                vec![SqlValue::Text(bucket.to_string()), SqlValue::Text(k.to_string())],
            ).await?;
            if !rows.is_empty() {
                return Ok(self.evaluate_acls(&rows, user, &permission));
            }
        }

        // Fall back to bucket-level ACL
        let rows = self.store.query(
            "SELECT principal_type, principal_value, permissions FROM stackhouse_storage_acls WHERE bucket = ? AND object_key IS NULL".to_string(),
            vec![SqlValue::Text(bucket.to_string())],
        ).await?;

        if rows.is_empty() {
            // No ACL = deny (except bucket owner which should be checked separately)
            return Ok(false);
        }

        Ok(self.evaluate_acls(&rows, user, &permission))
    }

    fn evaluate_acls(
        &self,
        rows: &[Vec<(String, Value)>],
        user: &User,
        permission: &AclPermission,
    ) -> bool {
        for row in rows {
            let principal_type = row
                .iter()
                .find(|(k, _)| k == "principal_type")
                .and_then(|(_, v)| v.as_str())
                .unwrap_or("");
            let principal_value = row
                .iter()
                .find(|(k, _)| k == "principal_value")
                .and_then(|(_, v)| v.as_str())
                .unwrap_or("");
            let perms_str = row
                .iter()
                .find(|(k, _)| k == "permissions")
                .and_then(|(_, v)| v.as_str())
                .unwrap_or("[]");
            let perms: Vec<String> = serde_json::from_str(perms_str).unwrap_or_default();

            let perm_str = match permission {
                AclPermission::Read => "read",
                AclPermission::Write => "write",
                AclPermission::Delete => "delete",
                AclPermission::Admin => "admin",
            };

            let has_perm = perms.iter().any(|p| p == perm_str || p == "admin");
            if !has_perm {
                continue;
            }

            match principal_type {
                "public" => return true,
                "authenticated" => return true,
                "user_id" => {
                    if principal_value == user.id.to_string() {
                        return true;
                    }
                }
                _ => {}
            }
        }
        false
    }

    /// Set ACL for a bucket or object
    pub async fn set_acl(
        &self,
        bucket: &str,
        object_key: Option<&str>,
        principal: AclPrincipal,
        permissions: Vec<AclPermission>,
    ) -> StackhouseResult<StorageAcl> {
        let id = uuid::Uuid::new_v4().to_string();
        let (ptype, pvalue) = match &principal {
            AclPrincipal::Public => ("public", "*".to_string()),
            AclPrincipal::Authenticated => ("authenticated", "*".to_string()),
            AclPrincipal::UserId(uid) => ("user_id", uid.to_string()),
            AclPrincipal::RoleId(r) => ("role_id", r.clone()),
            AclPrincipal::TeamId(tid) => ("team_id", tid.to_string()),
        };

        let perms_json = serde_json::to_string(&permissions).unwrap_or_default();

        self.store.execute(
            "INSERT INTO stackhouse_storage_acls (id, bucket, object_key, principal_type, principal_value, permissions) VALUES (?, ?, ?, ?, ?, ?)".to_string(),
            vec![
                SqlValue::Text(id.clone()),
                SqlValue::Text(bucket.to_string()),
                SqlValue::Text(object_key.unwrap_or("").to_string()),
                SqlValue::Text(ptype.to_string()),
                SqlValue::Text(pvalue),
                SqlValue::Text(perms_json),
            ],
        ).await?;

        Ok(StorageAcl {
            id,
            bucket: bucket.to_string(),
            object_key: object_key.map(String::from),
            principal,
            permissions,
            created_at: chrono::Utc::now().to_rfc3339(),
        })
    }

    /// List ACLs for a bucket/object
    pub async fn list_acls(
        &self,
        bucket: &str,
        object_key: Option<&str>,
    ) -> StackhouseResult<Vec<Value>> {
        let query = if let Some(key) = object_key {
            self.store.query(
                "SELECT id, principal_type, principal_value, permissions, created_at FROM stackhouse_storage_acls WHERE bucket = ? AND object_key = ?".to_string(),
                vec![SqlValue::Text(bucket.to_string()), SqlValue::Text(key.to_string())],
            ).await?
        } else {
            self.store.query(
                "SELECT id, principal_type, principal_value, permissions, created_at FROM stackhouse_storage_acls WHERE bucket = ? AND object_key IS NULL".to_string(),
                vec![SqlValue::Text(bucket.to_string())],
            ).await?
        };
        Ok(query
            .into_iter()
            .map(|r| json!(r.into_iter().collect::<std::collections::HashMap<_, _>>()))
            .collect())
    }

    /// Remove an ACL
    pub async fn remove_acl(&self, acl_id: &str) -> StackhouseResult<()> {
        self.store
            .execute(
                "DELETE FROM stackhouse_storage_acls WHERE id = ?".to_string(),
                vec![SqlValue::Text(acl_id.to_string())],
            )
            .await?;
        Ok(())
    }
}
