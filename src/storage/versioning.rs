//! # Versioned Object Storage
//!
//! Object versioning with restore, version listing, and delete markers.

use crate::db::{SqlValue, StackhouseStore};
use crate::error::{StackhouseError, StackhouseResult};

use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::sync::Arc;
use tokio::fs;
use tracing::info;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ObjectVersion {
    pub version_id: String,
    pub bucket: String,
    pub key: String,
    pub size: u64,
    pub etag: String,
    pub is_latest: bool,
    pub is_delete_marker: bool,
    pub last_modified: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BucketVersioningConfig {
    pub bucket: String,
    pub enabled: bool,
    pub mfa_delete: bool,
}

#[derive(Clone)]
pub struct VersioningService {
    store: Arc<StackhouseStore>,
    storage_path: PathBuf,
}

impl VersioningService {
    pub async fn new(store: Arc<StackhouseStore>, storage_path: PathBuf) -> StackhouseResult<Self> {
        let service = Self {
            store,
            storage_path,
        };
        service.initialize_tables().await?;
        info!("📚 Object versioning service initialized");
        Ok(service)
    }

    async fn initialize_tables(&self) -> StackhouseResult<()> {
        self.store.execute_batch(r#"
            CREATE TABLE IF NOT EXISTS stackhouse_object_versions (
                id BIGSERIAL PRIMARY KEY,
                version_id TEXT NOT NULL UNIQUE,
                bucket TEXT NOT NULL,
                key TEXT NOT NULL,
                size BIGINT NOT NULL DEFAULT 0,
                etag TEXT NOT NULL,
                is_latest BOOLEAN DEFAULT TRUE,
                is_delete_marker BOOLEAN DEFAULT FALSE,
                last_modified TIMESTAMPTZ DEFAULT NOW()
            );
            CREATE TABLE IF NOT EXISTS stackhouse_bucket_versioning (
                bucket TEXT PRIMARY KEY,
                enabled BOOLEAN DEFAULT FALSE,
                mfa_delete BOOLEAN DEFAULT FALSE,
                updated_at TIMESTAMPTZ DEFAULT NOW()
            );
            CREATE INDEX IF NOT EXISTS idx_object_versions_bucket_key ON stackhouse_object_versions(bucket, key);
            CREATE INDEX IF NOT EXISTS idx_object_versions_latest ON stackhouse_object_versions(bucket, key, is_latest);
        "#.to_string()).await?;
        Ok(())
    }

    /// Enable versioning for a bucket
    pub async fn enable_versioning(&self, bucket: &str) -> StackhouseResult<()> {
        self.store
            .execute(
                r#"INSERT INTO stackhouse_bucket_versioning (bucket, enabled) VALUES (?, TRUE)
               ON CONFLICT (bucket) DO UPDATE SET enabled = TRUE, updated_at = NOW()"#
                    .to_string(),
                vec![SqlValue::Text(bucket.to_string())],
            )
            .await?;
        info!("📚 Versioning enabled for bucket: {}", bucket);
        Ok(())
    }

    /// Disable versioning
    pub async fn disable_versioning(&self, bucket: &str) -> StackhouseResult<()> {
        self.store.execute(
            "UPDATE stackhouse_bucket_versioning SET enabled = FALSE, updated_at = NOW() WHERE bucket = ?".to_string(),
            vec![SqlValue::Text(bucket.to_string())],
        ).await?;
        Ok(())
    }

    /// Check if versioning is enabled
    pub async fn is_enabled(&self, bucket: &str) -> bool {
        let rows = self
            .store
            .query(
                "SELECT enabled FROM stackhouse_bucket_versioning WHERE bucket = ?".to_string(),
                vec![SqlValue::Text(bucket.to_string())],
            )
            .await
            .unwrap_or_default();
        rows.first()
            .and_then(|r| r.iter().find(|(k, _)| k == "enabled"))
            .and_then(|(_, v)| v.as_str())
            .map(|s| s == "true")
            .unwrap_or(false)
    }

    /// Create a new version of an object
    pub async fn create_version(
        &self,
        bucket: &str,
        key: &str,
        data: &[u8],
        etag: &str,
    ) -> StackhouseResult<ObjectVersion> {
        let version_id = uuid::Uuid::new_v4().to_string();

        // Mark old versions as not latest
        self.store.execute(
            "UPDATE stackhouse_object_versions SET is_latest = FALSE WHERE bucket = ? AND key = ? AND is_latest = TRUE".to_string(),
            vec![SqlValue::Text(bucket.to_string()), SqlValue::Text(key.to_string())],
        ).await?;

        // Store versioned file
        let version_path = self
            .storage_path
            .join("versions")
            .join(bucket)
            .join(&version_id);
        if let Some(parent) = version_path.parent() {
            fs::create_dir_all(parent).await.ok();
        }
        fs::write(&version_path, data).await.map_err(|e| {
            StackhouseError::Internal(anyhow::anyhow!("Version write failed: {}", e))
        })?;

        // Insert new version
        self.store.execute(
            "INSERT INTO stackhouse_object_versions (version_id, bucket, key, size, etag, is_latest) VALUES (?, ?, ?, ?, ?, TRUE)".to_string(),
            vec![
                SqlValue::Text(version_id.clone()),
                SqlValue::Text(bucket.to_string()),
                SqlValue::Text(key.to_string()),
                SqlValue::Integer(data.len() as i64),
                SqlValue::Text(etag.to_string()),
            ],
        ).await?;

        Ok(ObjectVersion {
            version_id,
            bucket: bucket.to_string(),
            key: key.to_string(),
            size: data.len() as u64,
            etag: etag.to_string(),
            is_latest: true,
            is_delete_marker: false,
            last_modified: chrono::Utc::now().to_rfc3339(),
        })
    }

    /// Create a delete marker (soft delete for versioned objects)
    pub async fn create_delete_marker(
        &self,
        bucket: &str,
        key: &str,
    ) -> StackhouseResult<ObjectVersion> {
        let version_id = uuid::Uuid::new_v4().to_string();

        self.store.execute(
            "UPDATE stackhouse_object_versions SET is_latest = FALSE WHERE bucket = ? AND key = ? AND is_latest = TRUE".to_string(),
            vec![SqlValue::Text(bucket.to_string()), SqlValue::Text(key.to_string())],
        ).await?;

        self.store.execute(
            "INSERT INTO stackhouse_object_versions (version_id, bucket, key, size, etag, is_latest, is_delete_marker) VALUES (?, ?, ?, 0, '', TRUE, TRUE)".to_string(),
            vec![
                SqlValue::Text(version_id.clone()),
                SqlValue::Text(bucket.to_string()),
                SqlValue::Text(key.to_string()),
            ],
        ).await?;

        Ok(ObjectVersion {
            version_id,
            bucket: bucket.to_string(),
            key: key.to_string(),
            size: 0,
            etag: String::new(),
            is_latest: true,
            is_delete_marker: true,
            last_modified: chrono::Utc::now().to_rfc3339(),
        })
    }

    /// List versions of an object
    pub async fn list_versions(
        &self,
        bucket: &str,
        key: &str,
    ) -> StackhouseResult<Vec<ObjectVersion>> {
        let rows = self.store.query(
            "SELECT version_id, size, etag, is_latest, is_delete_marker, last_modified FROM stackhouse_object_versions WHERE bucket = ? AND key = ? ORDER BY last_modified DESC".to_string(),
            vec![SqlValue::Text(bucket.to_string()), SqlValue::Text(key.to_string())],
        ).await?;

        let versions = rows
            .into_iter()
            .map(|r| {
                let get = |k: &str| r.iter().find(|(key, _)| key == k).map(|(_, v)| v.clone());
                ObjectVersion {
                    version_id: get("version_id")
                        .and_then(|v| v.as_str().map(String::from))
                        .unwrap_or_default(),
                    bucket: bucket.to_string(),
                    key: key.to_string(),
                    size: get("size").and_then(|v| v.as_i64()).unwrap_or(0) as u64,
                    etag: get("etag")
                        .and_then(|v| v.as_str().map(String::from))
                        .unwrap_or_default(),
                    is_latest: get("is_latest")
                        .map(|v| v.as_str() == Some("true"))
                        .unwrap_or(false),
                    is_delete_marker: get("is_delete_marker")
                        .map(|v| v.as_str() == Some("true"))
                        .unwrap_or(false),
                    last_modified: get("last_modified")
                        .and_then(|v| v.as_str().map(String::from))
                        .unwrap_or_default(),
                }
            })
            .collect();

        Ok(versions)
    }

    /// Restore a specific version (make it the latest)
    pub async fn restore_version(
        &self,
        bucket: &str,
        key: &str,
        version_id: &str,
    ) -> StackhouseResult<ObjectVersion> {
        // Read versioned data
        let version_path = self
            .storage_path
            .join("versions")
            .join(bucket)
            .join(version_id);
        let data = fs::read(&version_path)
            .await
            .map_err(|_| StackhouseError::NotFound("Version data not found".into()))?;

        let etag = format!("\"restored-{}\"", &version_id[..8]);
        self.create_version(bucket, key, &data, &etag).await
    }

    /// Delete a specific version permanently
    pub async fn delete_version(&self, bucket: &str, version_id: &str) -> StackhouseResult<()> {
        self.store
            .execute(
                "DELETE FROM stackhouse_object_versions WHERE bucket = ? AND version_id = ?"
                    .to_string(),
                vec![
                    SqlValue::Text(bucket.to_string()),
                    SqlValue::Text(version_id.to_string()),
                ],
            )
            .await?;

        let version_path = self
            .storage_path
            .join("versions")
            .join(bucket)
            .join(version_id);
        fs::remove_file(&version_path).await.ok();
        Ok(())
    }
}
