//! # Database Branching (Neon-style)
//!
//! Creates isolated database branches for dev, test, staging environments.
//! Each branch is a copy-on-write clone of the parent with independent schema and data.

use crate::db::{SqlValue, StackhouseStore};
use crate::error::{StackhouseError, StackhouseResult};

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::HashMap;
use std::sync::Arc;
use tracing::info;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DbBranch {
    pub id: String,
    pub tenant_id: i64,
    pub name: String,
    pub parent_branch: Option<String>,
    pub schema_name: String,
    pub status: BranchStatus,
    pub created_at: String,
    pub last_accessed: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum BranchStatus {
    Creating,
    Ready,
    Archiving,
    Archived,
    Deleting,
}

#[derive(Clone)]
pub struct DbBranchService {
    store: Arc<StackhouseStore>,
}

impl DbBranchService {
    pub async fn new(store: Arc<StackhouseStore>) -> StackhouseResult<Self> {
        let service = Self { store };
        service.initialize_tables().await?;
        info!("🌿 Database branching service initialized");
        Ok(service)
    }

    async fn initialize_tables(&self) -> StackhouseResult<()> {
        self.store
            .execute_batch(
                r#"
            CREATE TABLE IF NOT EXISTS stackhouse_db_branches (
                id TEXT PRIMARY KEY,
                tenant_id BIGINT NOT NULL,
                name TEXT NOT NULL,
                parent_branch TEXT,
                schema_name TEXT NOT NULL UNIQUE,
                status TEXT DEFAULT 'creating',
                created_at TIMESTAMPTZ DEFAULT NOW(),
                last_accessed TIMESTAMPTZ DEFAULT NOW()
            );
            CREATE INDEX IF NOT EXISTS idx_branches_tenant ON stackhouse_db_branches(tenant_id);
        "#
                .to_string(),
            )
            .await?;
        Ok(())
    }

    /// Create a new branch from the current main schema
    pub async fn create_branch(
        &self,
        tenant_id: i64,
        name: &str,
        parent: Option<&str>,
    ) -> StackhouseResult<DbBranch> {
        let id = uuid::Uuid::new_v4().to_string();
        let schema_name = format!("stackhouse_branch_{}", id[..8].to_string());

        // Create schema
        self.store
            .execute(
                format!("CREATE SCHEMA IF NOT EXISTS {}", schema_name),
                vec![],
            )
            .await?;

        // Clone tables from parent (or main)
        let parent_schema = parent.unwrap_or("public");
        let tables = self.store.query(
            format!("SELECT table_name FROM information_schema.tables WHERE table_schema = '{}' AND table_type = 'BASE TABLE'", parent_schema),
            vec![],
        ).await?;

        for row in tables {
            let table = row
                .iter()
                .find(|(k, _)| k == "table_name")
                .and_then(|(_, v)| v.as_str())
                .unwrap_or("");
            if table.starts_with("stackhouse_") {
                // Clone table structure including constraints, defaults, and indexes
                self.store
                    .execute(
                        format!(
                            "CREATE TABLE IF NOT EXISTS {}.{} (LIKE {}.{} INCLUDING ALL)",
                            schema_name, table, parent_schema, table
                        ),
                        vec![],
                    )
                    .await
                    .ok();

                // Copy all data from parent table
                self.store
                    .execute(
                        format!(
                            "INSERT INTO {}.{} SELECT * FROM {}.{}",
                            schema_name, table, parent_schema, table
                        ),
                        vec![],
                    )
                    .await
                    .ok();

                // Copy sequences (set branch sequence values to match parent)
                let seq_rows = self.store.query(
                    format!(
                        "SELECT sequence_name FROM information_schema.sequences WHERE sequence_schema = '{}'",
                        parent_schema
                    ),
                    vec![],
                ).await.unwrap_or_default();

                for seq_row in seq_rows {
                    if let Some(seq_name) = seq_row
                        .iter()
                        .find(|(k, _)| k == "sequence_name")
                        .and_then(|(_, v)| v.as_str())
                    {
                        if seq_name.starts_with("stackhouse_") {
                            // Get current value from parent sequence
                            let curr_val = self
                                .store
                                .query(
                                    format!(
                                        "SELECT last_value FROM {}.{}",
                                        parent_schema, seq_name
                                    ),
                                    vec![],
                                )
                                .await
                                .unwrap_or_default();

                            if let Some(val) = curr_val
                                .first()
                                .and_then(|r| r.first())
                                .and_then(|(_, v)| v.as_i64())
                            {
                                // Set the branch sequence to the same value
                                self.store
                                    .execute(
                                        format!(
                                            "SELECT setval('{}.{}', {}, true)",
                                            schema_name, seq_name, val
                                        ),
                                        vec![],
                                    )
                                    .await
                                    .ok();
                            }
                        }
                    }
                }
            }
        }

        self.store.execute(
            "INSERT INTO stackhouse_db_branches (id, tenant_id, name, parent_branch, schema_name, status) VALUES (?, ?, ?, ?, ?, 'ready')".to_string(),
            vec![
                SqlValue::Text(id.clone()), SqlValue::Integer(tenant_id),
                SqlValue::Text(name.to_string()), SqlValue::Text(parent.unwrap_or("").to_string()),
                SqlValue::Text(schema_name.clone()),
            ],
        ).await?;

        Ok(DbBranch {
            id,
            tenant_id,
            name: name.to_string(),
            parent_branch: parent.map(|s| s.to_string()),
            schema_name,
            status: BranchStatus::Ready,
            created_at: chrono::Utc::now().to_rfc3339(),
            last_accessed: chrono::Utc::now().to_rfc3339(),
        })
    }

    /// List branches for a tenant
    pub async fn list_branches(&self, tenant_id: i64) -> StackhouseResult<Vec<Value>> {
        let rows = self.store.query(
            "SELECT id, name, parent_branch, schema_name, status, created_at FROM stackhouse_db_branches WHERE tenant_id = ? ORDER BY created_at DESC".to_string(),
            vec![SqlValue::Integer(tenant_id)],
        ).await?;
        Ok(rows
            .into_iter()
            .map(|r| json!(r.iter().cloned().collect::<HashMap<_, _>>()))
            .collect())
    }

    /// Switch to a branch (returns the schema name to use)
    pub async fn switch_branch(&self, tenant_id: i64, branch_id: &str) -> StackhouseResult<String> {
        let rows = self.store.query(
            "SELECT schema_name FROM stackhouse_db_branches WHERE id = ? AND tenant_id = ? AND status = 'ready'".to_string(),
            vec![SqlValue::Text(branch_id.to_string()), SqlValue::Integer(tenant_id)],
        ).await?;
        if rows.is_empty() {
            return Err(StackhouseError::NotFound(
                "Branch not found or not ready".into(),
            ));
        }
        let schema = rows[0]
            .iter()
            .find(|(k, _)| k == "schema_name")
            .and_then(|(_, v)| v.as_str())
            .unwrap_or("public")
            .to_string();
        self.store
            .execute(
                "UPDATE stackhouse_db_branches SET last_accessed = NOW() WHERE id = ?".to_string(),
                vec![SqlValue::Text(branch_id.to_string())],
            )
            .await?;
        Ok(schema)
    }

    /// Delete a branch
    pub async fn delete_branch(&self, tenant_id: i64, branch_id: &str) -> StackhouseResult<()> {
        let rows = self
            .store
            .query(
                "SELECT schema_name FROM stackhouse_db_branches WHERE id = ? AND tenant_id = ?"
                    .to_string(),
                vec![
                    SqlValue::Text(branch_id.to_string()),
                    SqlValue::Integer(tenant_id),
                ],
            )
            .await?;

        if let Some(row) = rows.first() {
            let schema = row
                .iter()
                .find(|(k, _)| k == "schema_name")
                .and_then(|(_, v)| v.as_str())
                .unwrap_or("");
            self.store
                .execute(format!("DROP SCHEMA IF EXISTS {} CASCADE", schema), vec![])
                .await
                .ok();
        }

        self.store
            .execute(
                "DELETE FROM stackhouse_db_branches WHERE id = ? AND tenant_id = ?".to_string(),
                vec![
                    SqlValue::Text(branch_id.to_string()),
                    SqlValue::Integer(tenant_id),
                ],
            )
            .await?;
        Ok(())
    }

    /// Reset a branch to its parent state
    pub async fn reset_branch(&self, tenant_id: i64, branch_id: &str) -> StackhouseResult<()> {
        let rows = self.store.query(
            "SELECT schema_name, parent_branch FROM stackhouse_db_branches WHERE id = ? AND tenant_id = ?".to_string(),
            vec![SqlValue::Text(branch_id.to_string()), SqlValue::Integer(tenant_id)],
        ).await?;

        if let Some(row) = rows.first() {
            let schema = row
                .iter()
                .find(|(k, _)| k == "schema_name")
                .and_then(|(_, v)| v.as_str())
                .unwrap_or("");
            let parent = row
                .iter()
                .find(|(k, _)| k == "parent_branch")
                .and_then(|(_, v)| v.as_str())
                .unwrap_or("public");
            // Drop and recreate the branch schema
            self.store
                .execute(format!("DROP SCHEMA IF EXISTS {} CASCADE", schema), vec![])
                .await
                .ok();
            self.store
                .execute(format!("CREATE SCHEMA IF NOT EXISTS {}", schema), vec![])
                .await?;

            // Re-clone tables and data from parent
            let parent_schema = if parent.is_empty() { "public" } else { parent };
            let tables = self.store.query(
                format!("SELECT table_name FROM information_schema.tables WHERE table_schema = '{}' AND table_type = 'BASE TABLE'", parent_schema),
                vec![],
            ).await?;

            for table_row in tables {
                let table = table_row
                    .iter()
                    .find(|(k, _)| k == "table_name")
                    .and_then(|(_, v)| v.as_str())
                    .unwrap_or("");
                if table.starts_with("stackhouse_") {
                    self.store
                        .execute(
                            format!(
                                "CREATE TABLE IF NOT EXISTS {}.{} (LIKE {}.{} INCLUDING ALL)",
                                schema, table, parent_schema, table
                            ),
                            vec![],
                        )
                        .await
                        .ok();
                    self.store
                        .execute(
                            format!(
                                "INSERT INTO {}.{} SELECT * FROM {}.{}",
                                schema, table, parent_schema, table
                            ),
                            vec![],
                        )
                        .await
                        .ok();
                }
            }
        }
        Ok(())
    }
}
