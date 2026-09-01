//! # Storage Lifecycle Policies
//!
//! Rule engine for auto-archive, auto-delete, and transitions to cold storage.

use crate::db::{SqlValue, StackhouseStore};
use crate::error::StackhouseResult;

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::sync::Arc;
use tracing::info;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LifecycleRule {
    pub id: String,
    pub bucket: String,
    pub name: String,
    pub prefix: String,
    pub action: LifecycleAction,
    pub condition: LifecycleCondition,
    pub enabled: bool,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LifecycleAction {
    Delete,
    TransitionToArchive,
    TransitionToGlacier,
    SetStorageClass(String),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LifecycleCondition {
    pub age_days: Option<u32>,
    pub created_before: Option<String>,
    pub num_newer_versions: Option<u32>,
    pub is_live: Option<bool>,
    pub matches_suffix: Option<Vec<String>>,
}

#[derive(Clone)]
pub struct LifecycleService {
    store: Arc<StackhouseStore>,
}

impl LifecycleService {
    pub async fn new(store: Arc<StackhouseStore>) -> StackhouseResult<Self> {
        let service = Self { store };
        service.initialize_tables().await?;
        service.start_lifecycle_worker();
        info!("♻️ Storage lifecycle service initialized");
        Ok(service)
    }

    async fn initialize_tables(&self) -> StackhouseResult<()> {
        self.store
            .execute_batch(
                r#"
            CREATE TABLE IF NOT EXISTS stackhouse_lifecycle_rules (
                id TEXT PRIMARY KEY,
                bucket TEXT NOT NULL,
                name TEXT NOT NULL,
                prefix TEXT DEFAULT '',
                action TEXT NOT NULL,
                condition_json TEXT NOT NULL DEFAULT '{}',
                enabled BOOLEAN DEFAULT TRUE,
                last_applied_at TIMESTAMPTZ,
                created_at TIMESTAMPTZ DEFAULT NOW()
            );
            CREATE INDEX IF NOT EXISTS idx_lifecycle_rules_bucket ON stackhouse_lifecycle_rules(bucket);
        "#
                .to_string(),
            )
            .await?;
        Ok(())
    }

    fn start_lifecycle_worker(&self) {
        let store = Arc::clone(&self.store);
        tokio::spawn(async move {
            let mut interval = tokio::time::interval(std::time::Duration::from_secs(3600)); // Hourly
            loop {
                interval.tick().await;
                if let Err(e) = Self::apply_rules(&store).await {
                    tracing::error!("Lifecycle worker error: {}", e);
                }
            }
        });
    }

    async fn apply_rules(store: &Arc<StackhouseStore>) -> StackhouseResult<()> {
        let rules = store.query(
            "SELECT id, bucket, prefix, action, condition_json FROM stackhouse_lifecycle_rules WHERE enabled = true".to_string(),
            vec![],
        ).await?;

        for rule in &rules {
            let bucket = rule
                .iter()
                .find(|(k, _)| k == "bucket")
                .and_then(|(_, v)| v.as_str())
                .unwrap_or("");
            let prefix = rule
                .iter()
                .find(|(k, _)| k == "prefix")
                .and_then(|(_, v)| v.as_str())
                .unwrap_or("");
            let action_str = rule
                .iter()
                .find(|(k, _)| k == "action")
                .and_then(|(_, v)| v.as_str())
                .unwrap_or("");
            let condition_str = rule
                .iter()
                .find(|(k, _)| k == "condition_json")
                .and_then(|(_, v)| v.as_str())
                .unwrap_or("{}");

            let condition: LifecycleCondition =
                serde_json::from_str(condition_str).unwrap_or(LifecycleCondition {
                    age_days: None,
                    created_before: None,
                    num_newer_versions: None,
                    is_live: None,
                    matches_suffix: None,
                });

            if let Some(age_days) = condition.age_days {
                match action_str {
                    "delete" => {
                        store.execute(
                            format!(
                                "DELETE FROM stackhouse_objects WHERE bucket_name = '{}' AND path LIKE '{}%' AND created_at < NOW() - INTERVAL '{} days'",
                                bucket, prefix, age_days
                            ),
                            vec![],
                        ).await.ok();
                    }
                    "transition_to_archive" => {
                        store.execute(
                            format!(
                                "UPDATE stackhouse_s3_objects SET storage_class = 'ARCHIVE' WHERE bucket = '{}' AND key LIKE '{}%' AND last_modified < NOW() - INTERVAL '{} days'",
                                bucket, prefix, age_days
                            ),
                            vec![],
                        ).await.ok();
                    }
                    _ => {}
                }
            }

            // Update last applied
            let rule_id = rule
                .iter()
                .find(|(k, _)| k == "id")
                .and_then(|(_, v)| v.as_str())
                .unwrap_or("");
            store
                .execute(
                    "UPDATE stackhouse_lifecycle_rules SET last_applied_at = NOW() WHERE id = ?"
                        .to_string(),
                    vec![SqlValue::Text(rule_id.to_string())],
                )
                .await
                .ok();
        }

        Ok(())
    }

    /// Add a lifecycle rule
    pub async fn add_rule(
        &self,
        bucket: &str,
        name: &str,
        prefix: &str,
        action: LifecycleAction,
        condition: LifecycleCondition,
    ) -> StackhouseResult<LifecycleRule> {
        let id = uuid::Uuid::new_v4().to_string();
        let action_str = match &action {
            LifecycleAction::Delete => "delete".to_string(),
            LifecycleAction::TransitionToArchive => "transition_to_archive".to_string(),
            LifecycleAction::TransitionToGlacier => "transition_to_glacier".to_string(),
            LifecycleAction::SetStorageClass(c) => format!("set_class:{}", c),
        };

        self.store.execute(
            "INSERT INTO stackhouse_lifecycle_rules (id, bucket, name, prefix, action, condition_json) VALUES (?, ?, ?, ?, ?, ?)".to_string(),
            vec![
                SqlValue::Text(id.clone()),
                SqlValue::Text(bucket.to_string()),
                SqlValue::Text(name.to_string()),
                SqlValue::Text(prefix.to_string()),
                SqlValue::Text(action_str),
                SqlValue::Text(serde_json::to_string(&condition).unwrap_or_default()),
            ],
        ).await?;

        Ok(LifecycleRule {
            id,
            bucket: bucket.to_string(),
            name: name.to_string(),
            prefix: prefix.to_string(),
            action,
            condition,
            enabled: true,
            created_at: chrono::Utc::now().to_rfc3339(),
        })
    }

    /// List rules for a bucket
    pub async fn list_rules(&self, bucket: &str) -> StackhouseResult<Vec<Value>> {
        let rows = self.store.query(
            "SELECT id, name, prefix, action, condition_json, enabled, last_applied_at, created_at FROM stackhouse_lifecycle_rules WHERE bucket = ? ORDER BY created_at".to_string(),
            vec![SqlValue::Text(bucket.to_string())],
        ).await?;
        Ok(rows
            .into_iter()
            .map(|r| json!(r.into_iter().collect::<std::collections::HashMap<_, _>>()))
            .collect())
    }

    /// Delete a rule
    pub async fn delete_rule(&self, rule_id: &str) -> StackhouseResult<()> {
        self.store
            .execute(
                "DELETE FROM stackhouse_lifecycle_rules WHERE id = ?".to_string(),
                vec![SqlValue::Text(rule_id.to_string())],
            )
            .await?;
        Ok(())
    }
}
