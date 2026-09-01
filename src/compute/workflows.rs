//! # DAG-Based Workflow Engine
//!
//! Multi-step automations with directed acyclic graph execution,
//! conditional branching, parallel steps, retries, and state persistence.
//! Inspired by Temporal / Inngest patterns.

use crate::db::{SqlValue, StackhouseStore};
use crate::error::{StackhouseError, StackhouseResult};

use dashmap::DashMap;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use tracing::{info, warn};

// ============================================================================
// Types
// ============================================================================

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Workflow {
    pub id: String,
    pub tenant_id: i64,
    pub name: String,
    pub description: String,
    pub steps: Vec<WorkflowStep>,
    pub triggers: Vec<WorkflowTrigger>,
    pub config: WorkflowConfig,
    pub version: u32,
    pub enabled: bool,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkflowStep {
    pub id: String,
    pub name: String,
    pub step_type: StepType,
    pub config: Value,
    pub depends_on: Vec<String>,   // step IDs this depends on
    pub condition: Option<String>, // JS/expression to evaluate
    pub retry: RetryConfig,
    pub timeout_secs: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StepType {
    Function {
        function_id: String,
    },
    HttpRequest {
        method: String,
        url: String,
        headers: HashMap<String, String>,
    },
    AiAgent {
        agent_id: String,
        prompt: String,
    },
    Delay {
        seconds: u64,
    },
    Condition {
        expression: String,
        if_true: String,
        if_false: String,
    },
    Parallel {
        step_ids: Vec<String>,
    },
    SubWorkflow {
        workflow_id: String,
    },
    Transform {
        expression: String,
    },
    Webhook {
        url: String,
        method: String,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RetryConfig {
    pub max_retries: u32,
    pub initial_delay_ms: u64,
    pub backoff_multiplier: f64,
    pub max_delay_ms: u64,
}

impl Default for RetryConfig {
    fn default() -> Self {
        Self {
            max_retries: 3,
            initial_delay_ms: 1000,
            backoff_multiplier: 2.0,
            max_delay_ms: 60_000,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkflowConfig {
    pub max_duration_secs: u64,
    pub concurrency_limit: u32,
    pub on_failure: FailurePolicy,
}

impl Default for WorkflowConfig {
    fn default() -> Self {
        Self {
            max_duration_secs: 3600,
            concurrency_limit: 10,
            on_failure: FailurePolicy::StopAll,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FailurePolicy {
    StopAll,
    ContinueOthers,
    Retry,
    Compensate,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WorkflowTrigger {
    Event {
        topic: String,
        filter: Option<String>,
    },
    Cron {
        expression: String,
    },
    Webhook {
        path: String,
    },
    Manual,
}

// ============================================================================
// Execution State
// ============================================================================

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkflowRun {
    pub run_id: String,
    pub workflow_id: String,
    pub tenant_id: i64,
    pub status: RunStatus,
    pub input: Value,
    pub output: Option<Value>,
    pub step_states: HashMap<String, StepState>,
    pub started_at: String,
    pub completed_at: Option<String>,
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum RunStatus {
    Pending,
    Running,
    Completed,
    Failed,
    Cancelled,
    Paused,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StepState {
    pub step_id: String,
    pub status: StepStatus,
    pub input: Option<Value>,
    pub output: Option<Value>,
    pub attempts: u32,
    pub error: Option<String>,
    pub started_at: Option<String>,
    pub completed_at: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum StepStatus {
    Pending,
    Running,
    Completed,
    Failed,
    Skipped,
    Waiting,
}

// ============================================================================
// Workflow Engine
// ============================================================================

#[derive(Clone)]
pub struct DagWorkflowEngine {
    store: Arc<StackhouseStore>,
    active_runs: Arc<DashMap<String, WorkflowRun>>,
}

impl DagWorkflowEngine {
    pub async fn new(store: Arc<StackhouseStore>) -> StackhouseResult<Self> {
        let engine = Self {
            store,
            active_runs: Arc::new(DashMap::new()),
        };
        engine.initialize_tables().await?;
        info!("🔀 DAG Workflow engine initialized");
        Ok(engine)
    }

    async fn initialize_tables(&self) -> StackhouseResult<()> {
        self.store.execute_batch(r#"
            CREATE TABLE IF NOT EXISTS stackhouse_workflows (
                id TEXT PRIMARY KEY,
                tenant_id BIGINT NOT NULL,
                name TEXT NOT NULL,
                description TEXT DEFAULT '',
                steps JSONB NOT NULL DEFAULT '[]',
                triggers JSONB DEFAULT '[]',
                config JSONB DEFAULT '{}',
                version INTEGER DEFAULT 1,
                enabled BOOLEAN DEFAULT TRUE,
                created_at TIMESTAMPTZ DEFAULT NOW()
            );
            CREATE TABLE IF NOT EXISTS stackhouse_workflow_runs (
                run_id TEXT PRIMARY KEY,
                workflow_id TEXT NOT NULL,
                tenant_id BIGINT NOT NULL,
                status TEXT DEFAULT 'pending',
                input JSONB DEFAULT '{}',
                output JSONB,
                step_states JSONB DEFAULT '{}',
                error TEXT,
                started_at TIMESTAMPTZ DEFAULT NOW(),
                completed_at TIMESTAMPTZ
            );
            CREATE INDEX IF NOT EXISTS idx_workflows_tenant ON stackhouse_workflows(tenant_id);
            CREATE INDEX IF NOT EXISTS idx_workflow_runs_workflow ON stackhouse_workflow_runs(workflow_id);
            CREATE INDEX IF NOT EXISTS idx_workflow_runs_status ON stackhouse_workflow_runs(status);
        "#.to_string()).await?;
        Ok(())
    }

    /// Create or update a workflow
    pub async fn upsert_workflow(&self, workflow: &Workflow) -> StackhouseResult<()> {
        self.store.execute(
            r#"INSERT INTO stackhouse_workflows (id, tenant_id, name, description, steps, triggers, config, version, enabled)
               VALUES (?, ?, ?, ?, ?::jsonb, ?::jsonb, ?::jsonb, ?, ?)
               ON CONFLICT (id) DO UPDATE SET
               name = EXCLUDED.name, description = EXCLUDED.description,
               steps = EXCLUDED.steps, triggers = EXCLUDED.triggers,
               config = EXCLUDED.config, version = stackhouse_workflows.version + 1,
               enabled = EXCLUDED.enabled"#.to_string(),
            vec![
                SqlValue::Text(workflow.id.clone()),
                SqlValue::Integer(workflow.tenant_id),
                SqlValue::Text(workflow.name.clone()),
                SqlValue::Text(workflow.description.clone()),
                SqlValue::Text(serde_json::to_string(&workflow.steps).unwrap_or_default()),
                SqlValue::Text(serde_json::to_string(&workflow.triggers).unwrap_or_default()),
                SqlValue::Text(serde_json::to_string(&workflow.config).unwrap_or_default()),
                SqlValue::Integer(workflow.version as i64),
                SqlValue::Text(workflow.enabled.to_string()),
            ],
        ).await?;
        Ok(())
    }

    /// Start a workflow run
    pub async fn start_run(
        &self,
        workflow_id: &str,
        tenant_id: i64,
        input: Value,
    ) -> StackhouseResult<WorkflowRun> {
        // Load workflow definition
        let rows = self.store.query(
            "SELECT steps, config FROM stackhouse_workflows WHERE id = ? AND tenant_id = ? AND enabled = true".to_string(),
            vec![SqlValue::Text(workflow_id.to_string()), SqlValue::Integer(tenant_id)],
        ).await?;

        if rows.is_empty() {
            return Err(StackhouseError::NotFound(
                "Workflow not found or disabled".into(),
            ));
        }

        let steps_str = rows[0]
            .iter()
            .find(|(k, _)| k == "steps")
            .and_then(|(_, v)| v.as_str())
            .unwrap_or("[]");
        let steps: Vec<WorkflowStep> = serde_json::from_str(steps_str).unwrap_or_default();

        // Steps referenced by a Parallel step are only executed inside that group,
        // so mark them as Waiting instead of Pending.
        let mut parallel_children: HashSet<String> = HashSet::new();
        for step in &steps {
            if let StepType::Parallel { step_ids } = &step.step_type {
                for id in step_ids {
                    parallel_children.insert(id.clone());
                }
            }
        }

        // Initialize step states
        let mut step_states = HashMap::new();
        for step in &steps {
            let initial_status = if parallel_children.contains(&step.id) {
                StepStatus::Waiting
            } else {
                StepStatus::Pending
            };
            step_states.insert(
                step.id.clone(),
                StepState {
                    step_id: step.id.clone(),
                    status: initial_status,
                    input: None,
                    output: None,
                    attempts: 0,
                    error: None,
                    started_at: None,
                    completed_at: None,
                },
            );
        }

        let run = WorkflowRun {
            run_id: uuid::Uuid::new_v4().to_string(),
            workflow_id: workflow_id.to_string(),
            tenant_id,
            status: RunStatus::Running,
            input: input.clone(),
            output: None,
            step_states: step_states.clone(),
            started_at: chrono::Utc::now().to_rfc3339(),
            completed_at: None,
            error: None,
        };

        // Persist run
        self.store.execute(
            "INSERT INTO stackhouse_workflow_runs (run_id, workflow_id, tenant_id, status, input, step_states) VALUES (?, ?, ?, 'running', ?::jsonb, ?::jsonb)".to_string(),
            vec![
                SqlValue::Text(run.run_id.clone()),
                SqlValue::Text(workflow_id.to_string()),
                SqlValue::Integer(tenant_id),
                SqlValue::Text(input.to_string()),
                SqlValue::Text(serde_json::to_string(&step_states).unwrap_or_default()),
            ],
        ).await?;

        // Store in active runs
        self.active_runs.insert(run.run_id.clone(), run.clone());

        // Execute workflow DAG
        self.execute_dag(run.run_id.clone(), steps, input).await;

        Ok(run)
    }

    /// Execute DAG steps respecting dependencies
    async fn execute_dag(&self, run_id: String, steps: Vec<WorkflowStep>, input: Value) {
        let tenant_id = {
            if let Some(run) = self.active_runs.get(&run_id) {
                run.tenant_id
            } else {
                0
            }
        };

        let mut completed: HashMap<String, Value> = HashMap::new();
        completed.insert("$input".to_string(), input.clone());

        loop {
            // Find steps ready to run (all dependencies satisfied)
            let ready: Vec<&WorkflowStep> = steps
                .iter()
                .filter(|s| {
                    let state = self.get_step_status_sync(&run_id, &s.id);
                    matches!(state, StepStatus::Pending)
                        && s.depends_on.iter().all(|dep| completed.contains_key(dep))
                })
                .collect();

            if ready.is_empty() {
                // Check if all done or stuck
                let all_done = steps.iter().all(|s| {
                    let state = self.get_step_status_sync(&run_id, &s.id);
                    matches!(
                        state,
                        StepStatus::Completed | StepStatus::Skipped | StepStatus::Failed
                    )
                });

                if all_done {
                    self.complete_run(&run_id, RunStatus::Completed, Some(json!(completed)))
                        .await;
                    break;
                } else {
                    // Some steps failed and no more ready — fail the run
                    self.complete_run(&run_id, RunStatus::Failed, None).await;
                    break;
                }
            }

            // Execute ready steps (could be parallel)
            for step in ready {
                let step_input = self.resolve_step_input(step, &completed);
                let result = self
                    .execute_step(&run_id, tenant_id, &steps, step, &step_input)
                    .await;

                match result {
                    Ok(output) => {
                        completed.insert(step.id.clone(), output.clone());
                        self.update_step_state(
                            &run_id,
                            &step.id,
                            StepStatus::Completed,
                            Some(output),
                        )
                        .await;
                    }
                    Err(e) => {
                        warn!("Step {} failed: {}", step.id, e);
                        self.update_step_state(&run_id, &step.id, StepStatus::Failed, None)
                            .await;
                    }
                }
            }
        }
    }

    async fn execute_step(
        &self,
        run_id: &str,
        tenant_id: i64,
        steps: &[WorkflowStep],
        step: &WorkflowStep,
        input: &Value,
    ) -> StackhouseResult<Value> {
        match &step.step_type {
            StepType::Delay { seconds } => {
                tokio::time::sleep(std::time::Duration::from_secs(*seconds)).await;
                Ok(json!({"delayed": seconds}))
            }
            StepType::HttpRequest {
                method,
                url,
                headers,
            } => {
                let client = reqwest::Client::new();
                let mut req = match method.to_uppercase().as_str() {
                    "POST" => client.post(url),
                    "PUT" => client.put(url),
                    "DELETE" => client.delete(url),
                    "PATCH" => client.patch(url),
                    _ => client.get(url),
                };
                for (k, v) in headers {
                    req = req.header(k.as_str(), v.as_str());
                }
                if method.to_uppercase() == "POST"
                    || method.to_uppercase() == "PUT"
                    || method.to_uppercase() == "PATCH"
                {
                    req = req.json(input);
                }
                let resp = req
                    .send()
                    .await
                    .map_err(|e| StackhouseError::Internal(anyhow::anyhow!("HTTP step: {}", e)))?;
                let status = resp.status().as_u16();
                let body: Value = resp.json().await.unwrap_or(json!(null));
                Ok(json!({"status": status, "body": body}))
            }
            StepType::Transform { expression } => {
                // Apply a simple JSON path transform expression
                // Supports dot-notation path extraction: e.g. "data.result" extracts input.data.result
                let result = expression.split('.').fold(input.clone(), |acc, key| {
                    acc.get(key).cloned().unwrap_or(json!(null))
                });
                Ok(result)
            }
            StepType::Condition {
                expression,
                if_true,
                if_false,
            } => {
                // Evaluate condition: check if expression resolves to a truthy value
                let value = expression.split('.').fold(input.clone(), |acc, key| {
                    acc.get(key).cloned().unwrap_or(json!(null))
                });
                let is_truthy = match &value {
                    Value::Null => false,
                    Value::Bool(b) => *b,
                    Value::Number(n) => n.as_f64().map(|f| f != 0.0).unwrap_or(false),
                    Value::String(s) => !s.is_empty() && s.to_lowercase() != "false",
                    Value::Array(a) => !a.is_empty(),
                    Value::Object(o) => !o.is_empty(),
                };
                let branch = if is_truthy { if_true } else { if_false };
                Ok(json!({"branch": branch, "condition_met": is_truthy}))
            }
            StepType::Function { function_id } => {
                // Invoke a serverless function by querying the functions table and executing
                let rows = self.store.query(
                    "SELECT source_code, runtime, entrypoint FROM stackhouse_functions WHERE id = ? AND status = 'active'".to_string(),
                    vec![SqlValue::Text(function_id.clone())],
                ).await?;

                if rows.is_empty() {
                    return Err(StackhouseError::NotFound(format!(
                        "Function {} not found for workflow step",
                        function_id
                    )));
                }

                let row = &rows[0];
                let source = row
                    .iter()
                    .find(|(k, _)| k == "source_code")
                    .and_then(|(_, v)| v.as_str())
                    .unwrap_or("");
                let runtime = row
                    .iter()
                    .find(|(k, _)| k == "runtime")
                    .and_then(|(_, v)| v.as_str())
                    .unwrap_or("javascript");

                // Execute the function using the same JS engine as the functions service
                if runtime == "javascript" || runtime == "typescript" {
                    let input_str = input.to_string();
                    let wrapped_source = format!(
                        r#"(function(input) {{
                            let module = {{}};
                            let exports = {{}};
                            let handler;
                            {source}
                            let fn_handler = (typeof handler !== 'undefined') ? handler
                                : (exports && exports.handler) ? exports.handler
                                : (module && module.exports) ? module.exports
                                : null;
                            if (typeof fn_handler === 'function') {{
                                return fn_handler(input);
                            }}
                            return eval({source_expr});
                        }})({input_str})"#,
                        source = source,
                        source_expr =
                            serde_json::to_string(source).unwrap_or_else(|_| "\"\"".to_string()),
                        input_str = input_str,
                    );

                    let result = tokio::task::spawn_blocking(move || {
                        let mut ctx = boa_engine::Context::default();
                        let source = boa_engine::Source::from_bytes(wrapped_source.as_bytes());
                        ctx.eval(source)
                            .map_err(|e| {
                                StackhouseError::Internal(anyhow::anyhow!(
                                    "JS execution error: {}",
                                    e
                                ))
                            })
                            .and_then(|v| {
                                v.to_json(&mut ctx).map_err(|e| {
                                    StackhouseError::Internal(anyhow::anyhow!(
                                        "JS serialization error: {}",
                                        e
                                    ))
                                })
                            })
                    })
                    .await
                    .map_err(|e| {
                        StackhouseError::Internal(anyhow::anyhow!("JS task panicked: {}", e))
                    })??;
                    Ok(result)
                } else {
                    Err(StackhouseError::Internal(anyhow::anyhow!(
                        "Unsupported function runtime: {}",
                        runtime
                    )))
                }
            }
            StepType::Webhook { url, method } => {
                let client = reqwest::Client::new();
                let mut req = match method.to_uppercase().as_str() {
                    "POST" => client.post(url),
                    "PUT" => client.put(url),
                    "DELETE" => client.delete(url),
                    _ => client.get(url),
                };
                req = req.header("Content-Type", "application/json");
                if method.to_uppercase() != "GET" {
                    req = req.json(input);
                }
                let resp = req.send().await.map_err(|e| {
                    StackhouseError::Internal(anyhow::anyhow!("Webhook step: {}", e))
                })?;
                let status = resp.status().as_u16();
                let body: Value = resp.json().await.unwrap_or(json!(null));
                Ok(json!({"status": status, "body": body, "webhook_url": url}))
            }
            StepType::AiAgent { agent_id, prompt } => {
                // Look up the agent configuration and invoke it
                let rows = self
                    .store
                    .query(
                        "SELECT config FROM stackhouse_ai_agents WHERE id = ?".to_string(),
                        vec![SqlValue::Text(agent_id.clone())],
                    )
                    .await?;

                if rows.is_empty() {
                    return Err(StackhouseError::NotFound(format!(
                        "AI Agent {} not found",
                        agent_id
                    )));
                }

                let config_str = rows[0]
                    .iter()
                    .find(|(k, _)| k == "config")
                    .and_then(|(_, v)| v.as_str())
                    .unwrap_or("{}");
                let config: Value = serde_json::from_str(config_str).unwrap_or(json!({}));

                let provider = config
                    .get("provider")
                    .and_then(|v| v.as_str())
                    .unwrap_or("openai");
                let model = config
                    .get("model")
                    .and_then(|v| v.as_str())
                    .unwrap_or("gpt-4");
                let api_key = config.get("api_key").and_then(|v| v.as_str()).unwrap_or("");
                let api_url = config
                    .get("api_url")
                    .and_then(|v| v.as_str())
                    .unwrap_or("https://api.openai.com/v1/chat/completions");

                if api_key.is_empty() {
                    return Err(StackhouseError::Internal(anyhow::anyhow!(
                        "AI Agent {} has no API key configured",
                        agent_id
                    )));
                }

                // Build the chat completion request
                let user_prompt = format!("{}\n\nInput data: {}", prompt, input);
                let request_body = json!({
                    "model": model,
                    "messages": [
                        {"role": "system", "content": "You are a workflow step executor. Process the input and return a JSON result."},
                        {"role": "user", "content": user_prompt}
                    ],
                    "temperature": 0.3
                });

                let client = reqwest::Client::new();
                let resp = client
                    .post(api_url)
                    .header("Authorization", format!("Bearer {}", api_key))
                    .header("Content-Type", "application/json")
                    .json(&request_body)
                    .send()
                    .await
                    .map_err(|e| {
                        StackhouseError::Internal(anyhow::anyhow!(
                            "AI Agent API call failed: {}",
                            e
                        ))
                    })?;

                let result: Value = resp.json().await.map_err(|e| {
                    StackhouseError::Internal(anyhow::anyhow!(
                        "AI Agent response parse error: {}",
                        e
                    ))
                })?;

                // Extract the response content
                let content = result
                    .get("choices")
                    .and_then(|c| c.get(0))
                    .and_then(|c| c.get("message"))
                    .and_then(|m| m.get("content"))
                    .and_then(|c| c.as_str())
                    .unwrap_or("");

                // Try to parse the content as JSON, fall back to string
                let parsed =
                    serde_json::from_str::<Value>(content).unwrap_or(json!({"response": content}));

                Ok(
                    json!({"agent_id": agent_id, "result": parsed, "model": model, "provider": provider}),
                )
            }
            StepType::SubWorkflow { workflow_id } => {
                // Start a sub-workflow run and wait for it to complete. Box::pin avoids
                // recursive async-future sizing while keeping the call in the same task.
                let sub_run =
                    Box::pin(self.start_run(workflow_id, tenant_id, input.clone())).await?;

                // Fetch the completed run record for the final output.
                let run = self.get_run(&sub_run.run_id).await?;
                let status = run.get("status").and_then(|v| v.as_str()).unwrap_or("");
                if status == "completed" {
                    let output = run.get("output").cloned().unwrap_or(json!(null));
                    return Ok(json!({
                        "sub_workflow_id": workflow_id,
                        "run_id": sub_run.run_id,
                        "status": "completed",
                        "output": output,
                    }));
                }
                if status == "failed" || status == "cancelled" {
                    return Err(StackhouseError::Internal(anyhow::anyhow!(
                        "Sub-workflow {} finished with status: {}",
                        workflow_id,
                        status
                    )));
                }

                // start_run should not return before completion; poll just in case.
                let timeout = std::time::Duration::from_secs(if step.timeout_secs > 0 {
                    step.timeout_secs
                } else {
                    3600
                });
                let start = tokio::time::Instant::now();
                loop {
                    if start.elapsed() > timeout {
                        return Err(StackhouseError::Internal(anyhow::anyhow!(
                            "Sub-workflow {} timed out",
                            workflow_id
                        )));
                    }
                    tokio::time::sleep(std::time::Duration::from_millis(100)).await;

                    let run = self.get_run(&sub_run.run_id).await?;
                    let status = run.get("status").and_then(|v| v.as_str()).unwrap_or("");

                    if status == "completed" {
                        let output = run.get("output").cloned().unwrap_or(json!(null));
                        return Ok(json!({
                            "sub_workflow_id": workflow_id,
                            "run_id": sub_run.run_id,
                            "status": "completed",
                            "output": output,
                        }));
                    }
                    if status == "failed" || status == "cancelled" {
                        return Err(StackhouseError::Internal(anyhow::anyhow!(
                            "Sub-workflow {} finished with status: {}",
                            workflow_id,
                            status
                        )));
                    }
                }
            }
            StepType::Parallel { step_ids } => {
                // Look up the referenced steps and execute them concurrently.
                let mut sub_steps: Vec<&WorkflowStep> = Vec::new();
                for step_id in step_ids {
                    let sub = steps.iter().find(|s| s.id == *step_id).ok_or_else(|| {
                        StackhouseError::NotFound(format!(
                            "Parallel step {} not found in workflow",
                            step_id
                        ))
                    })?;
                    sub_steps.push(sub);
                }

                // Mark sub-steps as running and launch them concurrently.
                for sub in &sub_steps {
                    self.update_step_state(run_id, &sub.id, StepStatus::Running, None)
                        .await;
                }

                let futures: Vec<_> = sub_steps
                    .iter()
                    .map(|sub| self.execute_step(run_id, tenant_id, steps, sub, input))
                    .collect();
                let outputs = futures::future::join_all(futures).await;

                let mut result_map = serde_json::Map::new();
                for (sub, output) in sub_steps.iter().zip(outputs.into_iter()) {
                    match output {
                        Ok(value) => {
                            self.update_step_state(
                                run_id,
                                &sub.id,
                                StepStatus::Completed,
                                Some(value.clone()),
                            )
                            .await;
                            result_map.insert(sub.id.clone(), value);
                        }
                        Err(e) => {
                            warn!("Parallel sub-step {} failed: {}", sub.id, e);
                            self.update_step_state(
                                run_id,
                                &sub.id,
                                StepStatus::Failed,
                                Some(json!({"error": e.to_string()})),
                            )
                            .await;
                            result_map.insert(sub.id.clone(), json!({"error": e.to_string()}));
                        }
                    }
                }

                Ok(Value::Object(result_map))
            }
        }
    }

    fn resolve_step_input(&self, step: &WorkflowStep, completed: &HashMap<String, Value>) -> Value {
        if step.depends_on.is_empty() {
            completed.get("$input").cloned().unwrap_or(json!({}))
        } else if step.depends_on.len() == 1 {
            completed
                .get(&step.depends_on[0])
                .cloned()
                .unwrap_or(json!({}))
        } else {
            let mut merged = json!({});
            for dep in &step.depends_on {
                if let Some(val) = completed.get(dep) {
                    merged[dep] = val.clone();
                }
            }
            merged
        }
    }

    fn get_step_status_sync(&self, run_id: &str, step_id: &str) -> StepStatus {
        if let Some(run) = self.active_runs.get(run_id) {
            if let Some(state) = run.step_states.get(step_id) {
                return state.status.clone();
            }
        }
        StepStatus::Pending
    }

    async fn update_step_state(
        &self,
        run_id: &str,
        step_id: &str,
        status: StepStatus,
        output: Option<Value>,
    ) {
        let state_json =
            json!({step_id: { "status": status_str(&status), "output": output.as_ref() }})
                .to_string();

        if let Some(mut run) = self.active_runs.get_mut(run_id) {
            if let Some(state) = run.step_states.get_mut(step_id) {
                state.status = status;
                state.output = output;
                state.completed_at = Some(chrono::Utc::now().to_rfc3339());
            }
        }

        // Persist step state update
        self.store.execute(
            "UPDATE stackhouse_workflow_runs SET step_states = step_states || ?::jsonb WHERE run_id = ?".to_string(),
            vec![
                SqlValue::Text(state_json),
                SqlValue::Text(run_id.to_string()),
            ],
        ).await.ok();
    }

    async fn complete_run(&self, run_id: &str, status: RunStatus, output: Option<Value>) {
        let final_status = status_str_run(&status);
        self.store.execute(
            "UPDATE stackhouse_workflow_runs SET status = ?, output = ?::jsonb, completed_at = NOW() WHERE run_id = ?".to_string(),
            vec![
                SqlValue::Text(final_status.to_string()),
                SqlValue::Text(output.as_ref().map(|v| v.to_string()).unwrap_or_default()),
                SqlValue::Text(run_id.to_string()),
            ],
        ).await.ok();
        self.active_runs.remove(run_id);
    }

    /// Get run status
    pub async fn get_run(&self, run_id: &str) -> StackhouseResult<Value> {
        let rows = self
            .store
            .query(
                "SELECT * FROM stackhouse_workflow_runs WHERE run_id = ?".to_string(),
                vec![SqlValue::Text(run_id.to_string())],
            )
            .await?;
        if rows.is_empty() {
            return Err(StackhouseError::NotFound("Run not found".into()));
        }
        Ok(json!(rows[0].iter().cloned().collect::<HashMap<_, _>>()))
    }

    /// List workflows
    pub async fn list_workflows(&self, tenant_id: i64) -> StackhouseResult<Vec<Value>> {
        let rows = self.store.query(
            "SELECT id, name, description, version, enabled, created_at FROM stackhouse_workflows WHERE tenant_id = ? ORDER BY created_at DESC".to_string(),
            vec![SqlValue::Integer(tenant_id)],
        ).await?;
        Ok(rows
            .into_iter()
            .map(|r| json!(r.into_iter().collect::<HashMap<_, _>>()))
            .collect())
    }

    /// Cancel a running workflow
    pub async fn cancel_run(&self, run_id: &str) -> StackhouseResult<()> {
        self.complete_run(run_id, RunStatus::Cancelled, None).await;
        Ok(())
    }
}

fn status_str(status: &StepStatus) -> &'static str {
    match status {
        StepStatus::Pending => "pending",
        StepStatus::Running => "running",
        StepStatus::Completed => "completed",
        StepStatus::Failed => "failed",
        StepStatus::Skipped => "skipped",
        StepStatus::Waiting => "waiting",
    }
}

fn status_str_run(status: &RunStatus) -> &'static str {
    match status {
        RunStatus::Pending => "pending",
        RunStatus::Running => "running",
        RunStatus::Completed => "completed",
        RunStatus::Failed => "failed",
        RunStatus::Cancelled => "cancelled",
        RunStatus::Paused => "paused",
    }
}
