//! # Versioned API with Deprecation Policy
//!
//! API versioning, deprecation warnings, sunset headers,
//! and public changelog management.

use crate::db::{SqlValue, StackhouseStore};
use crate::error::StackhouseResult;

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::HashMap;
use std::sync::Arc;
use tracing::info;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApiVersion {
    pub version: String,
    pub status: VersionStatus,
    pub released_at: String,
    pub deprecated_at: Option<String>,
    pub sunset_at: Option<String>,
    pub changelog: String,
    pub breaking_changes: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum VersionStatus {
    Stable,
    Beta,
    Deprecated,
    Sunset,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApiRouteVersion {
    pub path: String,
    pub method: String,
    pub versions: Vec<String>,
    pub deprecated_in: Option<String>,
    pub removed_in: Option<String>,
    pub alternative: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChangelogEntry {
    pub version: String,
    pub date: String,
    pub changes: Vec<ChangeItem>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChangeItem {
    pub type_field: String,
    pub description: String,
}

#[derive(Clone)]
pub struct VersionedApiService {
    store: Arc<StackhouseStore>,
    current_version: Arc<tokio::sync::RwLock<String>>,
}

impl VersionedApiService {
    pub async fn new(store: Arc<StackhouseStore>) -> StackhouseResult<Self> {
        let service = Self {
            store,
            current_version: Arc::new(tokio::sync::RwLock::new("v1".to_string())),
        };
        service.initialize_tables().await?;
        info!("📋 Versioned API service initialized");
        Ok(service)
    }

    async fn initialize_tables(&self) -> StackhouseResult<()> {
        self.store
            .execute_batch(
                r#"
            CREATE TABLE IF NOT EXISTS stackhouse_api_versions (
                version TEXT PRIMARY KEY,
                status TEXT DEFAULT 'stable',
                released_at TIMESTAMPTZ DEFAULT NOW(),
                deprecated_at TIMESTAMPTZ,
                sunset_at TIMESTAMPTZ,
                changelog TEXT DEFAULT '',
                breaking_changes JSONB DEFAULT '[]'
            );
            CREATE TABLE IF NOT EXISTS stackhouse_api_routes (
                id BIGSERIAL PRIMARY KEY,
                path TEXT NOT NULL,
                method TEXT NOT NULL,
                versions JSONB DEFAULT '[]',
                deprecated_in TEXT,
                removed_in TEXT,
                alternative TEXT,
                UNIQUE(path, method)
            );
            CREATE TABLE IF NOT EXISTS stackhouse_changelog (
                id BIGSERIAL PRIMARY KEY,
                version TEXT NOT NULL,
                date TIMESTAMPTZ DEFAULT NOW(),
                changes JSONB DEFAULT '[]'
            );
        "#
                .to_string(),
            )
            .await?;
        Ok(())
    }

    pub async fn register_version(&self, version: &ApiVersion) -> StackhouseResult<()> {
        let status_str = serde_json::to_string(&version.status)
            .unwrap_or_default()
            .trim_matches('"')
            .to_string();
        self.store.execute(
            "INSERT INTO stackhouse_api_versions (version, status, released_at, deprecated_at, sunset_at, changelog, breaking_changes) VALUES (?, ?, ?::timestamptz, ?::timestamptz, ?::timestamptz, ?, ?::jsonb) ON CONFLICT (version) DO UPDATE SET status = EXCLUDED.status, deprecated_at = EXCLUDED.deprecated_at, sunset_at = EXCLUDED.sunset_at".to_string(),
            vec![
                SqlValue::Text(version.version.clone()),
                SqlValue::Text(status_str),
                SqlValue::Text(version.released_at.clone()),
                SqlValue::Text(version.deprecated_at.clone().unwrap_or_default()),
                SqlValue::Text(version.sunset_at.clone().unwrap_or_default()),
                SqlValue::Text(version.changelog.clone()),
                SqlValue::Text(serde_json::to_string(&version.breaking_changes).unwrap_or_default()),
            ],
        ).await?;
        Ok(())
    }

    pub async fn deprecate_version(
        &self,
        version: &str,
        sunset_date: &str,
    ) -> StackhouseResult<()> {
        self.store.execute(
            "UPDATE stackhouse_api_versions SET status = 'deprecated', deprecated_at = NOW(), sunset_at = ?::timestamptz WHERE version = ?".to_string(),
            vec![SqlValue::Text(sunset_date.to_string()), SqlValue::Text(version.to_string())],
        ).await?;
        Ok(())
    }

    pub async fn register_route(
        &self,
        path: &str,
        method: &str,
        versions: Vec<String>,
        alternative: Option<&str>,
    ) -> StackhouseResult<()> {
        self.store.execute(
            "INSERT INTO stackhouse_api_routes (path, method, versions, alternative) VALUES (?, ?, ?::jsonb, ?) ON CONFLICT (path, method) DO UPDATE SET versions = EXCLUDED.versions".to_string(),
            vec![
                SqlValue::Text(path.to_string()),
                SqlValue::Text(method.to_string()),
                SqlValue::Text(serde_json::to_string(&versions).unwrap_or_default()),
                SqlValue::Text(alternative.unwrap_or("").to_string()),
            ],
        ).await?;
        Ok(())
    }

    pub async fn get_version_headers(
        &self,
        requested_version: &str,
    ) -> StackhouseResult<HashMap<String, String>> {
        let rows = self.store.query(
            "SELECT status, deprecated_at, sunset_at FROM stackhouse_api_versions WHERE version = ?".to_string(),
            vec![SqlValue::Text(requested_version.to_string())],
        ).await?;

        let mut headers = HashMap::new();
        if let Some(row) = rows.first() {
            let status = row
                .iter()
                .find(|(k, _)| k == "status")
                .and_then(|(_, v)| v.as_str())
                .unwrap_or("stable");
            if status == "deprecated" || status == "sunset" {
                if let Some(sunset) = row
                    .iter()
                    .find(|(k, _)| k == "sunset_at")
                    .and_then(|(_, v)| v.as_str())
                {
                    headers.insert("Sunset".to_string(), sunset.to_string());
                    headers.insert(
                        "Deprecation".to_string(),
                        format!("version=\"{}\"", requested_version),
                    );
                }
            }
        }
        headers.insert("API-Version".to_string(), requested_version.to_string());
        Ok(headers)
    }

    pub async fn add_changelog(
        &self,
        version: &str,
        changes: Vec<ChangeItem>,
    ) -> StackhouseResult<()> {
        self.store
            .execute(
                "INSERT INTO stackhouse_changelog (version, changes) VALUES (?, ?::jsonb)"
                    .to_string(),
                vec![
                    SqlValue::Text(version.to_string()),
                    SqlValue::Text(serde_json::to_string(&changes).unwrap_or_default()),
                ],
            )
            .await?;
        Ok(())
    }

    pub async fn get_changelog(&self) -> StackhouseResult<Vec<Value>> {
        let rows = self
            .store
            .query(
                "SELECT version, date, changes FROM stackhouse_changelog ORDER BY date DESC"
                    .to_string(),
                vec![],
            )
            .await?;
        Ok(rows
            .into_iter()
            .map(|r| json!(r.iter().cloned().collect::<HashMap<_, _>>()))
            .collect())
    }

    pub async fn list_versions(&self) -> StackhouseResult<Vec<Value>> {
        let rows = self.store.query(
            "SELECT version, status, released_at, deprecated_at, sunset_at FROM stackhouse_api_versions ORDER BY released_at DESC".to_string(),
            vec![],
        ).await?;
        Ok(rows
            .into_iter()
            .map(|r| json!(r.iter().cloned().collect::<HashMap<_, _>>()))
            .collect())
    }

    pub async fn is_version_supported(&self, version: &str) -> StackhouseResult<bool> {
        let rows = self
            .store
            .query(
                "SELECT status FROM stackhouse_api_versions WHERE version = ?".to_string(),
                vec![SqlValue::Text(version.to_string())],
            )
            .await?;
        if rows.is_empty() {
            return Ok(false);
        }
        let status = rows[0]
            .iter()
            .find(|(k, _)| k == "status")
            .and_then(|(_, v)| v.as_str())
            .unwrap_or("sunset");
        Ok(status != "sunset")
    }

    pub async fn get_latest_stable(&self) -> StackhouseResult<String> {
        let rows = self.store.query(
            "SELECT version FROM stackhouse_api_versions WHERE status = 'stable' ORDER BY released_at DESC LIMIT 1".to_string(),
            vec![],
        ).await?;
        Ok(rows
            .first()
            .and_then(|r| r.iter().find(|(k, _)| k == "version"))
            .and_then(|(_, v)| v.as_str())
            .unwrap_or("v1")
            .to_string())
    }
}
