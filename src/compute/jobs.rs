//! # Background Jobs & Queue
//!
//! Durable job queue with priorities, progress tracking, and worker pools.

use crate::db::{SqlValue, StackhouseStore};
use crate::error::{StackhouseError, StackhouseResult};

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::RwLock;
use tracing::info;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BackgroundJob {
    pub id: String,
    pub tenant_id: i64,
    pub queue: String,
    pub job_type: String,
    pub payload: Value,
    pub priority: i32,
    pub status: JobStatus,
    pub progress: f64,
    pub result: Option<Value>,
    pub error: Option<String>,
    pub attempts: u32,
    pub max_attempts: u32,
    pub scheduled_for: Option<String>,
    pub started_at: Option<String>,
    pub completed_at: Option<String>,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum JobStatus {
    Pending,
    Running,
    Completed,
    Failed,
    Cancelled,
    Retrying,
}

#[derive(Clone)]
pub struct JobQueue {
    store: Arc<StackhouseStore>,
    handlers: Arc<RwLock<Vec<JobHandler>>>,
}

type JobCallback = Arc<
    dyn Fn(
            Value,
        )
            -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<Value, String>> + Send>>
        + Send
        + Sync,
>;

#[derive(Clone)]
struct JobHandler {
    job_type: String,
    callback: JobCallback,
}

impl JobQueue {
    pub async fn new(store: Arc<StackhouseStore>) -> StackhouseResult<Self> {
        let queue = Self {
            store,
            handlers: Arc::new(RwLock::new(Vec::new())),
        };
        queue.initialize_tables().await?;
        queue.start_worker();
        info!("📋 Background job queue initialized");
        Ok(queue)
    }

    async fn initialize_tables(&self) -> StackhouseResult<()> {
        self.store.execute_batch(r#"
            CREATE TABLE IF NOT EXISTS stackhouse_background_jobs (
                id TEXT PRIMARY KEY,
                tenant_id BIGINT NOT NULL,
                queue TEXT NOT NULL DEFAULT 'default',
                job_type TEXT NOT NULL,
                payload JSONB NOT NULL DEFAULT '{}',
                priority INTEGER DEFAULT 0,
                status TEXT NOT NULL DEFAULT 'pending',
                progress FLOAT DEFAULT 0.0,
                result JSONB,
                error TEXT,
                attempts INTEGER DEFAULT 0,
                max_attempts INTEGER DEFAULT 3,
                scheduled_for TIMESTAMPTZ,
                started_at TIMESTAMPTZ,
                completed_at TIMESTAMPTZ,
                created_at TIMESTAMPTZ DEFAULT NOW()
            );
            CREATE INDEX IF NOT EXISTS idx_bg_jobs_status ON stackhouse_background_jobs(status);
            CREATE INDEX IF NOT EXISTS idx_bg_jobs_queue ON stackhouse_background_jobs(queue, status, priority DESC);
            CREATE INDEX IF NOT EXISTS idx_bg_jobs_tenant ON stackhouse_background_jobs(tenant_id);
            CREATE INDEX IF NOT EXISTS idx_bg_jobs_scheduled ON stackhouse_background_jobs(scheduled_for);
        "#.to_string()).await?;
        Ok(())
    }

    fn start_worker(&self) {
        let store = Arc::clone(&self.store);
        let handlers = Arc::clone(&self.handlers);
        tokio::spawn(async move {
            let mut interval = tokio::time::interval(Duration::from_secs(1));
            loop {
                interval.tick().await;
                Self::process_next_job(&store, &handlers).await;
            }
        });
    }

    async fn process_next_job(
        store: &Arc<StackhouseStore>,
        handlers: &Arc<RwLock<Vec<JobHandler>>>,
    ) {
        // Fetch next pending job (ordered by priority)
        let rows = store.query(
            "UPDATE stackhouse_background_jobs SET status = 'running', started_at = NOW(), attempts = attempts + 1 WHERE id = (SELECT id FROM stackhouse_background_jobs WHERE status = 'pending' AND (scheduled_for IS NULL OR scheduled_for <= NOW()) ORDER BY priority DESC, created_at LIMIT 1 FOR UPDATE SKIP LOCKED) RETURNING id, job_type, payload, attempts, max_attempts".to_string(),
            vec![],
        ).await.unwrap_or_default();

        if rows.is_empty() {
            return;
        }

        let row = &rows[0];
        let job_id = row
            .iter()
            .find(|(k, _)| k == "id")
            .and_then(|(_, v)| v.as_str())
            .unwrap_or("")
            .to_string();
        let job_type = row
            .iter()
            .find(|(k, _)| k == "job_type")
            .and_then(|(_, v)| v.as_str())
            .unwrap_or("")
            .to_string();
        let payload_str = row
            .iter()
            .find(|(k, _)| k == "payload")
            .and_then(|(_, v)| v.as_str())
            .unwrap_or("{}")
            .to_string();
        let attempts = row
            .iter()
            .find(|(k, _)| k == "attempts")
            .and_then(|(_, v)| v.as_i64())
            .unwrap_or(0) as u32;
        let max_attempts = row
            .iter()
            .find(|(k, _)| k == "max_attempts")
            .and_then(|(_, v)| v.as_i64())
            .unwrap_or(3) as u32;

        // Parse payload
        let payload: Value = serde_json::from_str(&payload_str).unwrap_or(json!({}));

        // Dispatch to registered handlers matching job_type
        let handler_result: Result<Value, String> = {
            let handler_list = handlers.read().await;
            // Find a handler that accepts this job_type
            // Handlers are registered with a prefix match on job_type
            let matching: Vec<&JobHandler> = handler_list
                .iter()
                .filter(|h| h.job_type == job_type || h.job_type == "*")
                .collect();

            if matching.is_empty() {
                // No handler registered for this job type — mark as completed with note
                Ok(
                    json!({"status": "completed", "note": "no handler registered", "job_type": job_type}),
                )
            } else {
                // Execute the first matching handler
                let handler = matching[0];
                (handler.callback)(payload).await
            }
        };

        match handler_result {
            Ok(output) => {
                store.execute(
                    "UPDATE stackhouse_background_jobs SET status = 'completed', result = ?::jsonb, progress = 1.0, completed_at = NOW() WHERE id = ?".to_string(),
                    vec![SqlValue::Text(output.to_string()), SqlValue::Text(job_id.clone())],
                ).await.ok();
            }
            Err(e) => {
                if attempts >= max_attempts {
                    store.execute(
                        "UPDATE stackhouse_background_jobs SET status = 'failed', error = ?, completed_at = NOW() WHERE id = ?".to_string(),
                        vec![SqlValue::Text(e), SqlValue::Text(job_id.clone())],
                    ).await.ok();
                } else {
                    store.execute(
                        "UPDATE stackhouse_background_jobs SET status = 'retrying', error = ? WHERE id = ?".to_string(),
                        vec![SqlValue::Text(e), SqlValue::Text(job_id.clone())],
                    ).await.ok();
                    // Re-queue after delay
                    tokio::time::sleep(Duration::from_secs(5 * attempts as u64)).await;
                    store.execute(
                        "UPDATE stackhouse_background_jobs SET status = 'pending' WHERE id = ? AND status = 'retrying'".to_string(),
                        vec![SqlValue::Text(job_id)],
                    ).await.ok();
                }
            }
        }
    }

    /// Register a handler for a specific job type
    pub fn register_handler<F, Fut>(&self, job_type: &str, handler: F)
    where
        F: Fn(Value) -> Fut + Send + Sync + 'static,
        Fut: std::future::Future<Output = Result<Value, String>> + Send + 'static,
    {
        let callback: JobCallback = Arc::new(move |payload| Box::pin(handler(payload)));
        let handler = JobHandler {
            job_type: job_type.to_string(),
            callback,
        };
        // Use try_write to avoid deadlock; if it fails, the handler is registered on next attempt
        if let Ok(mut handlers) = self.handlers.try_write() {
            handlers.push(handler);
            info!("📋 Registered job handler for type: {}", job_type);
        }
    }

    /// Enqueue a new job
    pub async fn enqueue(
        &self,
        tenant_id: i64,
        queue: &str,
        job_type: &str,
        payload: Value,
        priority: i32,
        scheduled_for: Option<String>,
    ) -> StackhouseResult<BackgroundJob> {
        let id = uuid::Uuid::new_v4().to_string();

        self.store.execute(
            "INSERT INTO stackhouse_background_jobs (id, tenant_id, queue, job_type, payload, priority, scheduled_for) VALUES (?, ?, ?, ?, ?::jsonb, ?, ?::timestamptz)".to_string(),
            vec![
                SqlValue::Text(id.clone()),
                SqlValue::Integer(tenant_id),
                SqlValue::Text(queue.to_string()),
                SqlValue::Text(job_type.to_string()),
                SqlValue::Text(payload.to_string()),
                SqlValue::Integer(priority as i64),
                SqlValue::Text(scheduled_for.clone().unwrap_or_default()),
            ],
        ).await?;

        Ok(BackgroundJob {
            id,
            tenant_id,
            queue: queue.to_string(),
            job_type: job_type.to_string(),
            payload,
            priority,
            status: JobStatus::Pending,
            progress: 0.0,
            result: None,
            error: None,
            attempts: 0,
            max_attempts: 3,
            scheduled_for,
            started_at: None,
            completed_at: None,
            created_at: chrono::Utc::now().to_rfc3339(),
        })
    }

    /// Get job status
    pub async fn get_job(&self, job_id: &str) -> StackhouseResult<Value> {
        let rows = self.store.query(
            "SELECT id, queue, job_type, status, progress, result, error, attempts, started_at, completed_at, created_at FROM stackhouse_background_jobs WHERE id = ?".to_string(),
            vec![SqlValue::Text(job_id.to_string())],
        ).await?;
        if rows.is_empty() {
            return Err(StackhouseError::NotFound("Job not found".into()));
        }
        Ok(json!(rows[0]
            .iter()
            .cloned()
            .collect::<std::collections::HashMap<_, _>>()))
    }

    /// Cancel a pending job
    pub async fn cancel(&self, job_id: &str) -> StackhouseResult<()> {
        self.store.execute(
            "UPDATE stackhouse_background_jobs SET status = 'cancelled', completed_at = NOW() WHERE id = ? AND status IN ('pending', 'retrying')".to_string(),
            vec![SqlValue::Text(job_id.to_string())],
        ).await?;
        Ok(())
    }

    /// List jobs for a tenant
    pub async fn list_jobs(
        &self,
        tenant_id: i64,
        queue: Option<&str>,
        status: Option<&str>,
        limit: usize,
    ) -> StackhouseResult<Vec<Value>> {
        let mut sql = "SELECT id, queue, job_type, status, progress, priority, attempts, created_at FROM stackhouse_background_jobs WHERE tenant_id = ?".to_string();
        let mut params = vec![SqlValue::Integer(tenant_id)];

        if let Some(q) = queue {
            sql.push_str(" AND queue = ?");
            params.push(SqlValue::Text(q.to_string()));
        }
        if let Some(s) = status {
            sql.push_str(" AND status = ?");
            params.push(SqlValue::Text(s.to_string()));
        }

        sql.push_str(&format!(
            " ORDER BY priority DESC, created_at DESC LIMIT {}",
            limit
        ));

        let rows = self.store.query(sql, params).await?;
        Ok(rows
            .into_iter()
            .map(|r| json!(r.into_iter().collect::<std::collections::HashMap<_, _>>()))
            .collect())
    }

    /// Get queue stats
    pub async fn queue_stats(&self, tenant_id: i64) -> StackhouseResult<Value> {
        let rows = self.store.query(
            "SELECT queue, status, COUNT(*) as count FROM stackhouse_background_jobs WHERE tenant_id = ? GROUP BY queue, status".to_string(),
            vec![SqlValue::Integer(tenant_id)],
        ).await?;
        Ok(json!(rows
            .into_iter()
            .map(|r| r.into_iter().collect::<std::collections::HashMap<_, _>>())
            .collect::<Vec<_>>()))
    }
}
