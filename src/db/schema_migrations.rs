//! # Automatic Schema Migrations with Version History and Rollback
//!
//! Tracks schema changes, applies them idempotently, supports rollback,
//! and maintains a full audit trail of all migrations.

use crate::db::{SqlValue, StackhouseStore};
use crate::error::{StackhouseError, StackhouseResult};

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sha2::Digest;
use std::collections::HashMap;
use std::sync::Arc;
use tracing::{info, warn};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Migration {
    pub id: String,
    pub version: u64,
    pub name: String,
    pub up_sql: String,
    pub down_sql: String,
    pub checksum: String,
    pub applied_at: Option<String>,
    pub execution_time_ms: Option<u64>,
    pub status: MigrationStatus,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum MigrationStatus {
    Pending,
    Applied,
    Failed,
    RolledBack,
}

#[derive(Clone)]
pub struct SchemaMigrationService {
    store: Arc<StackhouseStore>,
}

impl SchemaMigrationService {
    pub async fn new(store: Arc<StackhouseStore>) -> StackhouseResult<Self> {
        let service = Self { store };
        service.initialize_migration_table().await?;
        info!("🗄️ Schema migration service initialized");
        Ok(service)
    }

    async fn initialize_migration_table(&self) -> StackhouseResult<()> {
        self.store.execute_batch(r#"
            CREATE TABLE IF NOT EXISTS stackhouse_schema_migrations (
                id TEXT PRIMARY KEY,
                version BIGINT NOT NULL UNIQUE,
                name TEXT NOT NULL,
                up_sql TEXT NOT NULL,
                down_sql TEXT NOT NULL,
                checksum TEXT NOT NULL,
                applied_at TIMESTAMPTZ,
                execution_time_ms BIGINT,
                status TEXT DEFAULT 'pending'
            );
            CREATE INDEX IF NOT EXISTS idx_migrations_version ON stackhouse_schema_migrations(version);
        "#.to_string()).await?;
        Ok(())
    }

    /// Register a migration without running it
    pub async fn register(&self, migration: &Migration) -> StackhouseResult<()> {
        let status_str = serde_json::to_string(&migration.status)
            .unwrap_or_default()
            .trim_matches('"')
            .to_string();
        self.store.execute(
            "INSERT INTO stackhouse_schema_migrations (id, version, name, up_sql, down_sql, checksum, status) VALUES (?, ?, ?, ?, ?, ?, ?) ON CONFLICT (version) DO NOTHING".to_string(),
            vec![
                SqlValue::Text(migration.id.clone()),
                SqlValue::Integer(migration.version as i64),
                SqlValue::Text(migration.name.clone()),
                SqlValue::Text(migration.up_sql.clone()),
                SqlValue::Text(migration.down_sql.clone()),
                SqlValue::Text(migration.checksum.clone()),
                SqlValue::Text(status_str),
            ],
        ).await?;
        Ok(())
    }

    /// Apply all pending migrations in order
    pub async fn migrate(&self) -> StackhouseResult<Vec<String>> {
        let pending = self.store.query(
            "SELECT id, version, name, up_sql, checksum FROM stackhouse_schema_migrations WHERE status = 'pending' ORDER BY version".to_string(),
            vec![],
        ).await?;

        let mut applied = Vec::new();
        for row in pending {
            let id = row
                .iter()
                .find(|(k, _)| k == "id")
                .and_then(|(_, v)| v.as_str())
                .unwrap_or("")
                .to_string();
            let version = row
                .iter()
                .find(|(k, _)| k == "version")
                .and_then(|(_, v)| v.as_i64())
                .unwrap_or(0) as u64;
            let name = row
                .iter()
                .find(|(k, _)| k == "name")
                .and_then(|(_, v)| v.as_str())
                .unwrap_or("")
                .to_string();
            let sql = row
                .iter()
                .find(|(k, _)| k == "up_sql")
                .and_then(|(_, v)| v.as_str())
                .unwrap_or("")
                .to_string();

            info!("Applying migration {}: {}", version, name);
            let start = std::time::Instant::now();

            match self.store.execute_batch(sql).await {
                Ok(_) => {
                    let elapsed = start.elapsed().as_millis() as u64;
                    self.store.execute(
                        "UPDATE stackhouse_schema_migrations SET status = 'applied', applied_at = NOW(), execution_time_ms = ? WHERE id = ?".to_string(),
                        vec![SqlValue::Integer(elapsed as i64), SqlValue::Text(id.clone())],
                    ).await?;
                    applied.push(format!("{}: {}", version, name));
                }
                Err(e) => {
                    warn!("Migration {} failed: {}", version, e);
                    self.store.execute(
                        "UPDATE stackhouse_schema_migrations SET status = 'failed' WHERE id = ?".to_string(),
                        vec![SqlValue::Text(id)],
                    ).await?;
                    return Err(e);
                }
            }
        }
        Ok(applied)
    }

    /// Rollback the last applied migration
    pub async fn rollback_one(&self) -> StackhouseResult<String> {
        let rows = self.store.query(
            "SELECT id, version, name, down_sql FROM stackhouse_schema_migrations WHERE status = 'applied' ORDER BY version DESC LIMIT 1".to_string(),
            vec![],
        ).await?;

        if rows.is_empty() {
            return Err(StackhouseError::NotFound(
                "No applied migrations to rollback".into(),
            ));
        }

        let row = &rows[0];
        let id = row
            .iter()
            .find(|(k, _)| k == "id")
            .and_then(|(_, v)| v.as_str())
            .unwrap_or("")
            .to_string();
        let version = row
            .iter()
            .find(|(k, _)| k == "version")
            .and_then(|(_, v)| v.as_i64())
            .unwrap_or(0) as u64;
        let name = row
            .iter()
            .find(|(k, _)| k == "name")
            .and_then(|(_, v)| v.as_str())
            .unwrap_or("")
            .to_string();
        let down_sql = row
            .iter()
            .find(|(k, _)| k == "down_sql")
            .and_then(|(_, v)| v.as_str())
            .unwrap_or("")
            .to_string();

        info!("Rolling back migration {}: {}", version, name);
        self.store.execute_batch(down_sql).await?;

        self.store.execute(
            "UPDATE stackhouse_schema_migrations SET status = 'rolled_back', applied_at = NULL WHERE id = ?".to_string(),
            vec![SqlValue::Text(id)],
        ).await?;

        Ok(format!("Rolled back {}: {}", version, name))
    }

    /// Rollback to a specific version
    pub async fn rollback_to(&self, target_version: u64) -> StackhouseResult<Vec<String>> {
        let rows = self.store.query(
            "SELECT id, version, name, down_sql FROM stackhouse_schema_migrations WHERE status = 'applied' AND version > ? ORDER BY version DESC".to_string(),
            vec![SqlValue::Integer(target_version as i64)],
        ).await?;

        let mut rolled_back = Vec::new();
        for row in rows {
            let id = row
                .iter()
                .find(|(k, _)| k == "id")
                .and_then(|(_, v)| v.as_str())
                .unwrap_or("")
                .to_string();
            let version = row
                .iter()
                .find(|(k, _)| k == "version")
                .and_then(|(_, v)| v.as_i64())
                .unwrap_or(0) as u64;
            let name = row
                .iter()
                .find(|(k, _)| k == "name")
                .and_then(|(_, v)| v.as_str())
                .unwrap_or("")
                .to_string();
            let down_sql = row
                .iter()
                .find(|(k, _)| k == "down_sql")
                .and_then(|(_, v)| v.as_str())
                .unwrap_or("")
                .to_string();

            self.store.execute_batch(down_sql).await?;
            self.store.execute(
                "UPDATE stackhouse_schema_migrations SET status = 'rolled_back', applied_at = NULL WHERE id = ?".to_string(),
                vec![SqlValue::Text(id)],
            ).await?;
            rolled_back.push(format!("Rolled back {}: {}", version, name));
        }
        Ok(rolled_back)
    }

    /// Get migration history
    pub async fn history(&self) -> StackhouseResult<Vec<Value>> {
        let rows = self.store.query(
            "SELECT version, name, status, applied_at, execution_time_ms FROM stackhouse_schema_migrations ORDER BY version".to_string(),
            vec![],
        ).await?;
        Ok(rows
            .into_iter()
            .map(|r| json!(r.iter().cloned().collect::<HashMap<_, _>>()))
            .collect())
    }

    /// Verify migration checksums haven't been tampered with
    pub async fn verify_checksums(&self) -> StackhouseResult<Vec<String>> {
        let rows = self
            .store
            .query(
                "SELECT version, name, up_sql, checksum FROM stackhouse_schema_migrations"
                    .to_string(),
                vec![],
            )
            .await?;

        let mut issues = Vec::new();
        for row in rows {
            let version = row
                .iter()
                .find(|(k, _)| k == "version")
                .and_then(|(_, v)| v.as_i64())
                .unwrap_or(0) as u64;
            let sql = row
                .iter()
                .find(|(k, _)| k == "up_sql")
                .and_then(|(_, v)| v.as_str())
                .unwrap_or("");
            let expected = row
                .iter()
                .find(|(k, _)| k == "checksum")
                .and_then(|(_, v)| v.as_str())
                .unwrap_or("");
            let actual = Self::compute_checksum(sql);
            if actual != expected {
                issues.push(format!("Migration {} checksum mismatch", version));
            }
        }
        Ok(issues)
    }

    fn compute_checksum(sql: &str) -> String {
        let mut hasher = sha2::Sha256::new();
        hasher.update(sql.as_bytes());
        hex::encode(hasher.finalize())[..16].to_string()
    }
}
