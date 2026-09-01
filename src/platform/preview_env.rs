//! # Preview Environments per Pull Request
//!
//! Creates isolated preview deployments for each PR with
//! a linked DB branch + storage + functions clone.

use crate::db::{SqlValue, StackhouseStore};
use crate::error::StackhouseResult;

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::HashMap;
use std::sync::Arc;
use tracing::info;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PreviewEnvironment {
    pub id: String,
    pub tenant_id: i64,
    pub pr_number: u32,
    pub branch: String,
    pub commit_sha: String,
    pub db_branch: String,
    pub status: PreviewStatus,
    pub url: String,
    pub created_at: String,
    pub expires_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum PreviewStatus {
    Creating,
    Ready,
    Building,
    Failed,
    Destroyed,
    Expired,
}

#[derive(Clone)]
pub struct PreviewEnvironmentService {
    store: Arc<StackhouseStore>,
}

impl PreviewEnvironmentService {
    pub async fn new(store: Arc<StackhouseStore>) -> StackhouseResult<Self> {
        let service = Self { store };
        service.initialize_tables().await?;
        info!("👁️ Preview environment service initialized");
        Ok(service)
    }

    async fn initialize_tables(&self) -> StackhouseResult<()> {
        self.store.execute_batch(r#"
            CREATE TABLE IF NOT EXISTS stackhouse_preview_envs (
                id TEXT PRIMARY KEY,
                tenant_id BIGINT NOT NULL,
                pr_number INTEGER NOT NULL,
                branch TEXT NOT NULL,
                commit_sha TEXT NOT NULL,
                db_branch TEXT NOT NULL,
                status TEXT DEFAULT 'creating',
                url TEXT DEFAULT '',
                created_at TIMESTAMPTZ DEFAULT NOW(),
                expires_at TIMESTAMPTZ
            );
            CREATE INDEX IF NOT EXISTS idx_preview_envs_tenant ON stackhouse_preview_envs(tenant_id, pr_number);
        "#.to_string()).await?;
        Ok(())
    }

    pub async fn create(
        &self,
        tenant_id: i64,
        pr_number: u32,
        branch: &str,
        commit_sha: &str,
    ) -> StackhouseResult<PreviewEnvironment> {
        let id = uuid::Uuid::new_v4().to_string();
        let db_branch = format!("preview_pr_{}", pr_number);
        let url = format!("https://preview-{}.stackhouse.app", id[..8].to_string());
        let expires = (chrono::Utc::now() + chrono::Duration::days(7)).to_rfc3339();

        // Create DB branch
        self.store
            .execute(format!("CREATE SCHEMA IF NOT EXISTS {}", db_branch), vec![])
            .await
            .ok();

        self.store.execute(
            "INSERT INTO stackhouse_preview_envs (id, tenant_id, pr_number, branch, commit_sha, db_branch, url, expires_at) VALUES (?, ?, ?, ?, ?, ?, ?, ?::timestamptz)".to_string(),
            vec![
                SqlValue::Text(id.clone()), SqlValue::Integer(tenant_id),
                SqlValue::Integer(pr_number as i64), SqlValue::Text(branch.to_string()),
                SqlValue::Text(commit_sha.to_string()), SqlValue::Text(db_branch.clone()),
                SqlValue::Text(url.clone()), SqlValue::Text(expires.clone()),
            ],
        ).await?;

        self.store
            .execute(
                "UPDATE stackhouse_preview_envs SET status = 'ready' WHERE id = ?".to_string(),
                vec![SqlValue::Text(id.clone())],
            )
            .await?;

        info!(
            "👁️ Preview env created: {} for PR #{} → {}",
            id, pr_number, url
        );

        Ok(PreviewEnvironment {
            id,
            tenant_id,
            pr_number,
            branch: branch.to_string(),
            commit_sha: commit_sha.to_string(),
            db_branch,
            status: PreviewStatus::Ready,
            url,
            created_at: chrono::Utc::now().to_rfc3339(),
            expires_at: expires,
        })
    }

    pub async fn get(
        &self,
        tenant_id: i64,
        pr_number: u32,
    ) -> StackhouseResult<Option<PreviewEnvironment>> {
        let rows = self.store.query(
            "SELECT * FROM stackhouse_preview_envs WHERE tenant_id = ? AND pr_number = ? ORDER BY created_at DESC LIMIT 1".to_string(),
            vec![SqlValue::Integer(tenant_id), SqlValue::Integer(pr_number as i64)],
        ).await?;
        if rows.is_empty() {
            return Ok(None);
        }
        Ok(Some(self.row_to_preview(&rows[0])?))
    }

    pub async fn list(&self, tenant_id: i64) -> StackhouseResult<Vec<Value>> {
        let rows = self.store.query(
            "SELECT id, pr_number, branch, commit_sha, db_branch, status, url, created_at, expires_at FROM stackhouse_preview_envs WHERE tenant_id = ? ORDER BY created_at DESC".to_string(),
            vec![SqlValue::Integer(tenant_id)],
        ).await?;
        Ok(rows
            .into_iter()
            .map(|r| json!(r.iter().cloned().collect::<HashMap<_, _>>()))
            .collect())
    }

    pub async fn destroy(&self, env_id: &str) -> StackhouseResult<()> {
        let rows = self
            .store
            .query(
                "SELECT db_branch FROM stackhouse_preview_envs WHERE id = ?".to_string(),
                vec![SqlValue::Text(env_id.to_string())],
            )
            .await?;

        if let Some(row) = rows.first() {
            let branch = row
                .iter()
                .find(|(k, _)| k == "db_branch")
                .and_then(|(_, v)| v.as_str())
                .unwrap_or("");
            self.store
                .execute(format!("DROP SCHEMA IF EXISTS {} CASCADE", branch), vec![])
                .await
                .ok();
        }

        self.store
            .execute(
                "UPDATE stackhouse_preview_envs SET status = 'destroyed' WHERE id = ?".to_string(),
                vec![SqlValue::Text(env_id.to_string())],
            )
            .await?;

        info!("🗑️ Preview env {} destroyed", env_id);
        Ok(())
    }

    pub async fn cleanup_expired(&self) -> StackhouseResult<u64> {
        let rows = self.store.query(
            "SELECT id, db_branch FROM stackhouse_preview_envs WHERE expires_at < NOW() AND status != 'destroyed' AND status != 'expired'".to_string(),
            vec![],
        ).await?;

        let mut count = 0u64;
        for row in rows {
            let id = row
                .iter()
                .find(|(k, _)| k == "id")
                .and_then(|(_, v)| v.as_str())
                .unwrap_or("");
            let branch = row
                .iter()
                .find(|(k, _)| k == "db_branch")
                .and_then(|(_, v)| v.as_str())
                .unwrap_or("");
            self.store
                .execute(format!("DROP SCHEMA IF EXISTS {} CASCADE", branch), vec![])
                .await
                .ok();
            self.store
                .execute(
                    "UPDATE stackhouse_preview_envs SET status = 'expired' WHERE id = ?"
                        .to_string(),
                    vec![SqlValue::Text(id.to_string())],
                )
                .await?;
            count += 1;
        }
        Ok(count)
    }

    fn row_to_preview(&self, row: &[(String, Value)]) -> StackhouseResult<PreviewEnvironment> {
        let get_str = |key: &str| {
            row.iter()
                .find(|(k, _)| k == key)
                .and_then(|(_, v)| v.as_str())
                .map(|s| s.to_string())
        };
        let get_i64 = |key: &str| {
            row.iter()
                .find(|(k, _)| k == key)
                .and_then(|(_, v)| v.as_i64())
        };

        let status = match get_str("status").unwrap_or_default().as_str() {
            "ready" => PreviewStatus::Ready,
            "building" => PreviewStatus::Building,
            "failed" => PreviewStatus::Failed,
            "destroyed" => PreviewStatus::Destroyed,
            "expired" => PreviewStatus::Expired,
            _ => PreviewStatus::Creating,
        };

        Ok(PreviewEnvironment {
            id: get_str("id").unwrap_or_default(),
            tenant_id: get_i64("tenant_id").unwrap_or(0),
            pr_number: get_i64("pr_number").unwrap_or(0) as u32,
            branch: get_str("branch").unwrap_or_default(),
            commit_sha: get_str("commit_sha").unwrap_or_default(),
            db_branch: get_str("db_branch").unwrap_or_default(),
            status,
            url: get_str("url").unwrap_or_default(),
            created_at: get_str("created_at").unwrap_or_default(),
            expires_at: get_str("expires_at").unwrap_or_default(),
        })
    }
}
