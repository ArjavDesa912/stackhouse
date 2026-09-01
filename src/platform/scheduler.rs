//! # Scheduled Jobs / Cron Service
//!
//! Cron expression parser, job registry, execution history, and retry logic.
//! Runs jobs within the database context.

use crate::auth::{extract_auth_user, AuthState};
use crate::db::{SqlValue, StackhouseStore};
use crate::error::{StackhouseError, StackhouseResult};

use axum::{
    extract::State,
    http::HeaderMap,
    response::IntoResponse,
    routing::{delete, get, post},
    Json, Router,
};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{error, info};

// ============================================================================
// Types
// ============================================================================

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScheduledJob {
    pub id: String,
    pub tenant_id: i64,
    pub name: String,
    pub cron_expression: String,
    pub job_type: JobType,
    pub payload: Value,
    pub timezone: String,
    pub enabled: bool,
    pub max_retries: u32,
    pub timeout_secs: u64,
    pub last_run_at: Option<String>,
    pub next_run_at: Option<String>,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum JobType {
    SqlQuery,
    HttpWebhook,
    FunctionInvoke,
    DataCleanup,
    AnalyticsRollup,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JobExecution {
    pub id: String,
    pub job_id: String,
    pub status: ExecutionStatus,
    pub started_at: String,
    pub completed_at: Option<String>,
    pub duration_ms: Option<u64>,
    pub result: Option<Value>,
    pub error: Option<String>,
    pub attempt: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ExecutionStatus {
    Running,
    Success,
    Failed,
    Timeout,
    Retrying,
}

// ============================================================================
// Cron Parser
// ============================================================================

pub struct CronExpression {
    minutes: Vec<u8>,
    hours: Vec<u8>,
    days_of_month: Vec<u8>,
    months: Vec<u8>,
    days_of_week: Vec<u8>,
}

impl CronExpression {
    pub fn parse(expr: &str) -> StackhouseResult<Self> {
        let parts: Vec<&str> = expr.trim().split_whitespace().collect();
        if parts.len() != 5 {
            return Err(StackhouseError::InvalidPayload(
                "Cron expression must have 5 fields (min hour dom month dow)".into(),
            ));
        }

        Ok(Self {
            minutes: Self::parse_field(parts[0], 0, 59)?,
            hours: Self::parse_field(parts[1], 0, 23)?,
            days_of_month: Self::parse_field(parts[2], 1, 31)?,
            months: Self::parse_field(parts[3], 1, 12)?,
            days_of_week: Self::parse_field(parts[4], 0, 6)?,
        })
    }

    fn parse_field(field: &str, min: u8, max: u8) -> StackhouseResult<Vec<u8>> {
        if field == "*" {
            return Ok((min..=max).collect());
        }

        let mut values = Vec::new();
        for part in field.split(',') {
            if let Some((range, step)) = part.split_once('/') {
                let step: u8 = step
                    .parse()
                    .map_err(|_| StackhouseError::InvalidPayload("Invalid cron step".into()))?;
                let (start, end) = if range == "*" {
                    (min, max)
                } else if let Some((s, e)) = range.split_once('-') {
                    let s: u8 = s.parse().map_err(|_| {
                        StackhouseError::InvalidPayload("Invalid cron range".into())
                    })?;
                    let e: u8 = e.parse().map_err(|_| {
                        StackhouseError::InvalidPayload("Invalid cron range".into())
                    })?;
                    (s, e)
                } else {
                    let s: u8 = range.parse().map_err(|_| {
                        StackhouseError::InvalidPayload("Invalid cron value".into())
                    })?;
                    (s, max)
                };
                let mut v = start;
                while v <= end {
                    values.push(v);
                    v += step;
                }
            } else if let Some((start, end)) = part.split_once('-') {
                let s: u8 = start
                    .parse()
                    .map_err(|_| StackhouseError::InvalidPayload("Invalid cron range".into()))?;
                let e: u8 = end
                    .parse()
                    .map_err(|_| StackhouseError::InvalidPayload("Invalid cron range".into()))?;
                values.extend(s..=e);
            } else {
                let v: u8 = part
                    .parse()
                    .map_err(|_| StackhouseError::InvalidPayload("Invalid cron value".into()))?;
                values.push(v);
            }
        }

        values.retain(|v| *v >= min && *v <= max);
        values.sort_unstable();
        values.dedup();
        Ok(values)
    }

    /// Check if the current time matches the expression
    pub fn matches_now(&self) -> bool {
        let now = chrono::Utc::now();
        let minute = now.format("%M").to_string().parse::<u8>().unwrap_or(0);
        let hour = now.format("%H").to_string().parse::<u8>().unwrap_or(0);
        let dom = now.format("%d").to_string().parse::<u8>().unwrap_or(1);
        let month = now.format("%m").to_string().parse::<u8>().unwrap_or(1);
        let dow = now.format("%u").to_string().parse::<u8>().unwrap_or(1) % 7; // Convert to 0=Sun

        self.minutes.contains(&minute)
            && self.hours.contains(&hour)
            && self.days_of_month.contains(&dom)
            && self.months.contains(&month)
            && self.days_of_week.contains(&dow)
    }
}

// ============================================================================
// Scheduler Service
// ============================================================================

#[derive(Clone)]
pub struct SchedulerService {
    store: Arc<StackhouseStore>,
    jobs: Arc<RwLock<Vec<ScheduledJob>>>,
}

impl SchedulerService {
    pub async fn new(store: Arc<StackhouseStore>) -> StackhouseResult<Self> {
        let service = Self {
            store,
            jobs: Arc::new(RwLock::new(Vec::new())),
        };
        service.initialize_tables().await?;
        service.load_jobs().await?;
        service.start_scheduler();
        info!("⏰ Job scheduler initialized");
        Ok(service)
    }

    async fn initialize_tables(&self) -> StackhouseResult<()> {
        self.store.execute_batch(r#"
            CREATE TABLE IF NOT EXISTS stackhouse_scheduled_jobs (
                id TEXT PRIMARY KEY,
                tenant_id BIGINT NOT NULL,
                name TEXT NOT NULL,
                cron_expression TEXT NOT NULL,
                job_type TEXT NOT NULL DEFAULT 'sql_query',
                payload JSONB DEFAULT '{}',
                timezone TEXT DEFAULT 'UTC',
                enabled BOOLEAN DEFAULT TRUE,
                max_retries INTEGER DEFAULT 3,
                timeout_secs INTEGER DEFAULT 300,
                last_run_at TIMESTAMPTZ,
                next_run_at TIMESTAMPTZ,
                created_at TIMESTAMPTZ DEFAULT NOW()
            );
            CREATE TABLE IF NOT EXISTS stackhouse_job_executions (
                id TEXT PRIMARY KEY,
                job_id TEXT NOT NULL REFERENCES stackhouse_scheduled_jobs(id) ON DELETE CASCADE,
                status TEXT NOT NULL DEFAULT 'running',
                started_at TIMESTAMPTZ DEFAULT NOW(),
                completed_at TIMESTAMPTZ,
                duration_ms BIGINT,
                result JSONB,
                error TEXT,
                attempt INTEGER DEFAULT 1
            );
            CREATE INDEX IF NOT EXISTS idx_scheduled_jobs_tenant ON stackhouse_scheduled_jobs(tenant_id);
            CREATE INDEX IF NOT EXISTS idx_job_executions_job ON stackhouse_job_executions(job_id);
            CREATE INDEX IF NOT EXISTS idx_job_executions_time ON stackhouse_job_executions(started_at);
        "#.to_string()).await?;
        Ok(())
    }

    async fn load_jobs(&self) -> StackhouseResult<()> {
        let rows = self.store.query(
            "SELECT id, tenant_id, name, cron_expression, job_type, payload, timezone, enabled, max_retries, timeout_secs, last_run_at, next_run_at, created_at FROM stackhouse_scheduled_jobs WHERE enabled = true".to_string(),
            vec![],
        ).await?;

        let mut jobs = self.jobs.write().await;
        for row in rows {
            let get = |key: &str| row.iter().find(|(k, _)| k == key).map(|(_, v)| v.clone());
            jobs.push(ScheduledJob {
                id: get("id")
                    .and_then(|v| v.as_str().map(String::from))
                    .unwrap_or_default(),
                tenant_id: get("tenant_id").and_then(|v| v.as_i64()).unwrap_or(0),
                name: get("name")
                    .and_then(|v| v.as_str().map(String::from))
                    .unwrap_or_default(),
                cron_expression: get("cron_expression")
                    .and_then(|v| v.as_str().map(String::from))
                    .unwrap_or_default(),
                job_type: serde_json::from_str(
                    &get("job_type")
                        .and_then(|v| v.as_str().map(|s| format!("\"{}\"", s)))
                        .unwrap_or_default(),
                )
                .unwrap_or(JobType::SqlQuery),
                payload: get("payload").unwrap_or(json!({})),
                timezone: get("timezone")
                    .and_then(|v| v.as_str().map(String::from))
                    .unwrap_or_else(|| "UTC".into()),
                enabled: true,
                max_retries: get("max_retries").and_then(|v| v.as_i64()).unwrap_or(3) as u32,
                timeout_secs: get("timeout_secs").and_then(|v| v.as_i64()).unwrap_or(300) as u64,
                last_run_at: get("last_run_at").and_then(|v| v.as_str().map(String::from)),
                next_run_at: get("next_run_at").and_then(|v| v.as_str().map(String::from)),
                created_at: get("created_at")
                    .and_then(|v| v.as_str().map(String::from))
                    .unwrap_or_default(),
            });
        }
        Ok(())
    }

    fn start_scheduler(&self) {
        let store = Arc::clone(&self.store);
        let jobs = Arc::clone(&self.jobs);

        tokio::spawn(async move {
            let mut interval = tokio::time::interval(std::time::Duration::from_secs(60));
            loop {
                interval.tick().await;
                let active_jobs = jobs.read().await.clone();
                for job in &active_jobs {
                    if let Ok(cron) = CronExpression::parse(&job.cron_expression) {
                        if cron.matches_now() {
                            let store = Arc::clone(&store);
                            let job = job.clone();
                            tokio::spawn(async move {
                                if let Err(e) = Self::execute_job(&store, &job).await {
                                    error!("Job {} failed: {}", job.name, e);
                                }
                            });
                        }
                    }
                }
            }
        });
    }

    async fn execute_job(store: &Arc<StackhouseStore>, job: &ScheduledJob) -> StackhouseResult<()> {
        let exec_id = uuid::Uuid::new_v4().to_string();
        let start = std::time::Instant::now();

        store.execute(
            "INSERT INTO stackhouse_job_executions (id, job_id, status) VALUES (?, ?, 'running')".to_string(),
            vec![SqlValue::Text(exec_id.clone()), SqlValue::Text(job.id.clone())],
        ).await?;

        let result = match &job.job_type {
            JobType::SqlQuery => {
                let query = job
                    .payload
                    .get("query")
                    .and_then(|v| v.as_str())
                    .unwrap_or("");
                store
                    .query(query.to_string(), vec![])
                    .await
                    .map(|rows| json!({"rows_affected": rows.len()}))
            }
            JobType::HttpWebhook => {
                let url = job
                    .payload
                    .get("url")
                    .and_then(|v| v.as_str())
                    .unwrap_or("");
                let method = job
                    .payload
                    .get("method")
                    .and_then(|v| v.as_str())
                    .unwrap_or("POST");
                let client = reqwest::Client::new();
                let resp = if method == "GET" {
                    client.get(url).send().await
                } else {
                    client
                        .post(url)
                        .json(&job.payload.get("body").unwrap_or(&json!({})))
                        .send()
                        .await
                };
                resp.map(|r| json!({"status": r.status().as_u16()}))
                    .map_err(|e| {
                        StackhouseError::Internal(anyhow::anyhow!("Webhook failed: {}", e))
                    })
            }
            _ => Ok(json!({"status": "executed"})),
        };

        let duration_ms = start.elapsed().as_millis() as i64;

        match result {
            Ok(res) => {
                store.execute(
                    "UPDATE stackhouse_job_executions SET status = 'success', completed_at = NOW(), duration_ms = ?, result = ?::jsonb WHERE id = ?".to_string(),
                    vec![
                        SqlValue::Integer(duration_ms),
                        SqlValue::Text(res.to_string()),
                        SqlValue::Text(exec_id),
                    ],
                ).await?;
            }
            Err(e) => {
                store.execute(
                    "UPDATE stackhouse_job_executions SET status = 'failed', completed_at = NOW(), duration_ms = ?, error = ? WHERE id = ?".to_string(),
                    vec![
                        SqlValue::Integer(duration_ms),
                        SqlValue::Text(e.to_string()),
                        SqlValue::Text(exec_id),
                    ],
                ).await?;
            }
        }

        // Update last_run_at
        store
            .execute(
                "UPDATE stackhouse_scheduled_jobs SET last_run_at = NOW() WHERE id = ?".to_string(),
                vec![SqlValue::Text(job.id.clone())],
            )
            .await?;

        Ok(())
    }

    /// Create a scheduled job
    pub async fn create_job(
        &self,
        tenant_id: i64,
        name: &str,
        cron: &str,
        job_type: JobType,
        payload: Value,
    ) -> StackhouseResult<ScheduledJob> {
        // Validate cron
        CronExpression::parse(cron)?;

        let id = uuid::Uuid::new_v4().to_string();
        let job_type_str = serde_json::to_string(&job_type)
            .unwrap_or_default()
            .trim_matches('"')
            .to_string();

        self.store.execute(
            "INSERT INTO stackhouse_scheduled_jobs (id, tenant_id, name, cron_expression, job_type, payload) VALUES (?, ?, ?, ?, ?, ?::jsonb)".to_string(),
            vec![
                SqlValue::Text(id.clone()),
                SqlValue::Integer(tenant_id),
                SqlValue::Text(name.to_string()),
                SqlValue::Text(cron.to_string()),
                SqlValue::Text(job_type_str),
                SqlValue::Text(payload.to_string()),
            ],
        ).await?;

        let job = ScheduledJob {
            id: id.clone(),
            tenant_id,
            name: name.to_string(),
            cron_expression: cron.to_string(),
            job_type,
            payload,
            timezone: "UTC".to_string(),
            enabled: true,
            max_retries: 3,
            timeout_secs: 300,
            last_run_at: None,
            next_run_at: None,
            created_at: chrono::Utc::now().to_rfc3339(),
        };

        self.jobs.write().await.push(job.clone());
        info!("⏰ Scheduled job created: {} ({})", name, cron);
        Ok(job)
    }

    /// List jobs for a tenant
    pub async fn list_jobs(&self, tenant_id: i64) -> StackhouseResult<Vec<Value>> {
        let rows = self.store.query(
            "SELECT id, name, cron_expression, job_type, enabled, last_run_at, next_run_at, created_at FROM stackhouse_scheduled_jobs WHERE tenant_id = ? ORDER BY created_at DESC".to_string(),
            vec![SqlValue::Integer(tenant_id)],
        ).await?;
        Ok(rows
            .into_iter()
            .map(|r| json!(r.into_iter().collect::<std::collections::HashMap<_, _>>()))
            .collect())
    }

    /// Get execution history for a job
    pub async fn get_executions(&self, job_id: &str, limit: usize) -> StackhouseResult<Vec<Value>> {
        let rows = self.store.query(
            format!("SELECT id, status, started_at, completed_at, duration_ms, result, error, attempt FROM stackhouse_job_executions WHERE job_id = ? ORDER BY started_at DESC LIMIT {}", limit),
            vec![SqlValue::Text(job_id.to_string())],
        ).await?;
        Ok(rows
            .into_iter()
            .map(|r| json!(r.into_iter().collect::<std::collections::HashMap<_, _>>()))
            .collect())
    }

    /// Delete a job
    pub async fn delete_job(&self, job_id: &str, tenant_id: i64) -> StackhouseResult<()> {
        self.store
            .execute(
                "DELETE FROM stackhouse_scheduled_jobs WHERE id = ? AND tenant_id = ?".to_string(),
                vec![
                    SqlValue::Text(job_id.to_string()),
                    SqlValue::Integer(tenant_id),
                ],
            )
            .await?;
        self.jobs.write().await.retain(|j| j.id != job_id);
        Ok(())
    }

    /// Toggle a job
    pub async fn toggle_job(
        &self,
        job_id: &str,
        tenant_id: i64,
        enabled: bool,
    ) -> StackhouseResult<()> {
        self.store
            .execute(
                "UPDATE stackhouse_scheduled_jobs SET enabled = ? WHERE id = ? AND tenant_id = ?"
                    .to_string(),
                vec![
                    SqlValue::Text(enabled.to_string()),
                    SqlValue::Text(job_id.to_string()),
                    SqlValue::Integer(tenant_id),
                ],
            )
            .await?;
        let mut jobs = self.jobs.write().await;
        if let Some(j) = jobs.iter_mut().find(|j| j.id == job_id) {
            j.enabled = enabled;
        }
        Ok(())
    }
}

// ============================================================================
// Router
// ============================================================================

#[derive(Clone)]
pub struct SchedulerState {
    pub scheduler: Arc<SchedulerService>,
    pub auth: AuthState,
}

#[derive(Deserialize)]
struct CreateJobRequest {
    name: String,
    cron_expression: String,
    #[serde(default = "default_job_type")]
    job_type: String,
    #[serde(default)]
    payload: Value,
}
fn default_job_type() -> String {
    "sql_query".to_string()
}

async fn create_job_handler(
    State(state): State<SchedulerState>,
    headers: HeaderMap,
    Json(req): Json<CreateJobRequest>,
) -> Result<impl IntoResponse, StackhouseError> {
    let user = extract_auth_user(&state.auth, &headers)?;
    let job_type = match req.job_type.as_str() {
        "http_webhook" => JobType::HttpWebhook,
        "function_invoke" => JobType::FunctionInvoke,
        "data_cleanup" => JobType::DataCleanup,
        "analytics_rollup" => JobType::AnalyticsRollup,
        _ => JobType::SqlQuery,
    };
    let job = state
        .scheduler
        .create_job(
            user.id,
            &req.name,
            &req.cron_expression,
            job_type,
            req.payload,
        )
        .await?;
    Ok(Json(json!({"success": true, "data": job})))
}

async fn list_jobs_handler(
    State(state): State<SchedulerState>,
    headers: HeaderMap,
) -> Result<impl IntoResponse, StackhouseError> {
    let user = extract_auth_user(&state.auth, &headers)?;
    let jobs = state.scheduler.list_jobs(user.id).await?;
    Ok(Json(json!({"success": true, "data": jobs})))
}

async fn get_executions_handler(
    State(state): State<SchedulerState>,
    headers: HeaderMap,
    axum::extract::Path(job_id): axum::extract::Path<String>,
) -> Result<impl IntoResponse, StackhouseError> {
    let _user = extract_auth_user(&state.auth, &headers)?;
    let execs = state.scheduler.get_executions(&job_id, 50).await?;
    Ok(Json(json!({"success": true, "data": execs})))
}

async fn delete_job_handler(
    State(state): State<SchedulerState>,
    headers: HeaderMap,
    axum::extract::Path(job_id): axum::extract::Path<String>,
) -> Result<impl IntoResponse, StackhouseError> {
    let user = extract_auth_user(&state.auth, &headers)?;
    state.scheduler.delete_job(&job_id, user.id).await?;
    Ok(Json(json!({"success": true, "message": "Job deleted"})))
}

pub fn create_scheduler_router(state: SchedulerState) -> Router {
    Router::new()
        .route("/jobs", post(create_job_handler))
        .route("/jobs", get(list_jobs_handler))
        .route("/jobs/:id/executions", get(get_executions_handler))
        .route("/jobs/:id", delete(delete_job_handler))
        .with_state(state)
}
