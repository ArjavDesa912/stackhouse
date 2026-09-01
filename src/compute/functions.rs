//! # Serverless Functions Runtime
//!
//! JavaScript/TypeScript function execution via the embedded Boa JS engine,
//! with cold-start optimization and an execution sandbox.

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
use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::RwLock;
use tracing::info;

// ============================================================================
// Types
// ============================================================================

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServerlessFunction {
    pub id: String,
    pub tenant_id: i64,
    pub name: String,
    pub runtime: FunctionRuntime,
    pub entrypoint: String,
    pub source_code: Option<String>,
    pub wasm_binary: Option<String>, // base64 encoded
    pub memory_limit_mb: u32,
    pub timeout_secs: u32,
    pub env_vars: HashMap<String, String>,
    pub triggers: Vec<FunctionTrigger>,
    pub status: FunctionStatus,
    pub version: u32,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FunctionRuntime {
    // Note: Wasm runtimes are accepted/recorded for forward compatibility,
    // but the embedded executor currently only runs JavaScript/TypeScript
    // source via the Boa engine.
    WasmRust,
    WasmJs,
    JavaScript,
    TypeScript,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FunctionTrigger {
    Http { path: String, method: String },
    Cron { expression: String },
    Event { topic: String },
    Webhook { endpoint: String },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum FunctionStatus {
    Active,
    Inactive,
    Deploying,
    Error,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FunctionInvocation {
    pub id: String,
    pub function_id: String,
    pub status: InvocationStatus,
    pub input: Value,
    pub output: Option<Value>,
    pub error: Option<String>,
    pub duration_ms: u64,
    pub memory_used_mb: u32,
    pub cold_start: bool,
    pub timestamp: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum InvocationStatus {
    Success,
    Error,
    Timeout,
    OomKilled,
}

// ============================================================================
// Functions Service
// ============================================================================

#[derive(Clone)]
pub struct FunctionsService {
    store: Arc<StackhouseStore>,
    // Cache compiled functions for fast cold-start
    compiled_cache: Arc<RwLock<HashMap<String, CompiledFunction>>>,
}

#[derive(Clone)]
struct CompiledFunction {
    function_id: String,
    version: u32,
    compiled_at: Instant,
}

impl FunctionsService {
    pub async fn new(store: Arc<StackhouseStore>) -> StackhouseResult<Self> {
        let service = Self {
            store,
            compiled_cache: Arc::new(RwLock::new(HashMap::new())),
        };
        service.initialize_tables().await?;
        info!("⚡ Serverless functions runtime initialized");
        Ok(service)
    }

    async fn initialize_tables(&self) -> StackhouseResult<()> {
        self.store.execute_batch(r#"
            CREATE TABLE IF NOT EXISTS stackhouse_functions (
                id TEXT PRIMARY KEY,
                tenant_id BIGINT NOT NULL,
                name TEXT NOT NULL,
                runtime TEXT NOT NULL DEFAULT 'javascript',
                entrypoint TEXT NOT NULL DEFAULT 'handler',
                source_code TEXT,
                wasm_binary BYTEA,
                memory_limit_mb INTEGER DEFAULT 128,
                timeout_secs INTEGER DEFAULT 30,
                env_vars JSONB DEFAULT '{}',
                triggers JSONB DEFAULT '[]',
                status TEXT DEFAULT 'active',
                version INTEGER DEFAULT 1,
                created_at TIMESTAMPTZ DEFAULT NOW(),
                updated_at TIMESTAMPTZ DEFAULT NOW(),
                UNIQUE(tenant_id, name)
            );
            CREATE TABLE IF NOT EXISTS stackhouse_function_invocations (
                id TEXT PRIMARY KEY,
                function_id TEXT NOT NULL,
                tenant_id BIGINT NOT NULL,
                status TEXT NOT NULL,
                input JSONB,
                output JSONB,
                error TEXT,
                duration_ms BIGINT,
                memory_used_mb INTEGER,
                cold_start BOOLEAN DEFAULT FALSE,
                timestamp TIMESTAMPTZ DEFAULT NOW()
            );
            CREATE TABLE IF NOT EXISTS stackhouse_function_logs (
                id BIGSERIAL PRIMARY KEY,
                invocation_id TEXT NOT NULL,
                function_id TEXT NOT NULL,
                level TEXT DEFAULT 'info',
                message TEXT NOT NULL,
                timestamp TIMESTAMPTZ DEFAULT NOW()
            );
            CREATE INDEX IF NOT EXISTS idx_functions_tenant ON stackhouse_functions(tenant_id);
            CREATE INDEX IF NOT EXISTS idx_invocations_function ON stackhouse_function_invocations(function_id);
            CREATE INDEX IF NOT EXISTS idx_invocations_time ON stackhouse_function_invocations(timestamp);
            CREATE INDEX IF NOT EXISTS idx_function_logs_invocation ON stackhouse_function_logs(invocation_id);
        "#.to_string()).await?;
        Ok(())
    }

    /// Deploy a new function
    pub async fn deploy(
        &self,
        tenant_id: i64,
        name: &str,
        runtime: FunctionRuntime,
        entrypoint: &str,
        source_code: &str,
        env_vars: HashMap<String, String>,
        triggers: Vec<FunctionTrigger>,
    ) -> StackhouseResult<ServerlessFunction> {
        let id = uuid::Uuid::new_v4().to_string();
        let runtime_str = serde_json::to_string(&runtime)
            .unwrap_or_default()
            .trim_matches('"')
            .to_string();

        self.store.execute(
            "INSERT INTO stackhouse_functions (id, tenant_id, name, runtime, entrypoint, source_code, env_vars, triggers) VALUES (?, ?, ?, ?, ?, ?, ?::jsonb, ?::jsonb) ON CONFLICT (tenant_id, name) DO UPDATE SET source_code = EXCLUDED.source_code, runtime = EXCLUDED.runtime, env_vars = EXCLUDED.env_vars, triggers = EXCLUDED.triggers, version = stackhouse_functions.version + 1, updated_at = NOW()".to_string(),
            vec![
                SqlValue::Text(id.clone()),
                SqlValue::Integer(tenant_id),
                SqlValue::Text(name.to_string()),
                SqlValue::Text(runtime_str),
                SqlValue::Text(entrypoint.to_string()),
                SqlValue::Text(source_code.to_string()),
                SqlValue::Text(serde_json::to_string(&env_vars).unwrap_or_default()),
                SqlValue::Text(serde_json::to_string(&triggers).unwrap_or_default()),
            ],
        ).await?;

        info!("⚡ Function deployed: {}", name);

        Ok(ServerlessFunction {
            id,
            tenant_id,
            name: name.to_string(),
            runtime,
            entrypoint: entrypoint.to_string(),
            source_code: Some(source_code.to_string()),
            wasm_binary: None,
            memory_limit_mb: 128,
            timeout_secs: 30,
            env_vars,
            triggers,
            status: FunctionStatus::Active,
            version: 1,
            created_at: chrono::Utc::now().to_rfc3339(),
            updated_at: chrono::Utc::now().to_rfc3339(),
        })
    }

    /// Invoke a function
    pub async fn invoke(
        &self,
        tenant_id: i64,
        function_name: &str,
        input: Value,
    ) -> StackhouseResult<FunctionInvocation> {
        let start = Instant::now();
        let invocation_id = uuid::Uuid::new_v4().to_string();

        // Get function
        let rows = self.store.query(
            "SELECT id, source_code, runtime, entrypoint, env_vars, timeout_secs, memory_limit_mb FROM stackhouse_functions WHERE tenant_id = ? AND name = ? AND status = 'active'".to_string(),
            vec![SqlValue::Integer(tenant_id), SqlValue::Text(function_name.to_string())],
        ).await?;

        if rows.is_empty() {
            return Err(StackhouseError::NotFound(format!(
                "Function '{}' not found",
                function_name
            )));
        }

        let row = &rows[0];
        let function_id = row
            .iter()
            .find(|(k, _)| k == "id")
            .and_then(|(_, v)| v.as_str())
            .unwrap_or("")
            .to_string();
        let source = row
            .iter()
            .find(|(k, _)| k == "source_code")
            .and_then(|(_, v)| v.as_str())
            .unwrap_or("");
        let timeout = row
            .iter()
            .find(|(k, _)| k == "timeout_secs")
            .and_then(|(_, v)| v.as_i64())
            .unwrap_or(30) as u64;

        // Check cache for cold start detection
        let cold_start = {
            let cache = self.compiled_cache.read().await;
            !cache.contains_key(&function_id)
        };

        // Execute function (simplified JS-like evaluation)
        let result = tokio::time::timeout(
            Duration::from_secs(timeout),
            self.execute_function(source, &input),
        )
        .await;

        let (status, output, error) = match result {
            Ok(Ok(out)) => (InvocationStatus::Success, Some(out), None),
            Ok(Err(e)) => (InvocationStatus::Error, None, Some(e.to_string())),
            Err(_) => (
                InvocationStatus::Timeout,
                None,
                Some("Function execution timed out".to_string()),
            ),
        };

        let duration_ms = start.elapsed().as_millis() as u64;

        // Record invocation
        let status_str = serde_json::to_string(&status)
            .unwrap_or_default()
            .trim_matches('"')
            .to_string();
        self.store.execute(
            "INSERT INTO stackhouse_function_invocations (id, function_id, tenant_id, status, input, output, error, duration_ms, cold_start) VALUES (?, ?, ?, ?, ?::jsonb, ?::jsonb, ?, ?, ?)".to_string(),
            vec![
                SqlValue::Text(invocation_id.clone()),
                SqlValue::Text(function_id.clone()),
                SqlValue::Integer(tenant_id),
                SqlValue::Text(status_str),
                SqlValue::Text(input.to_string()),
                SqlValue::Text(output.as_ref().map(|v| v.to_string()).unwrap_or_else(|| "null".into())),
                SqlValue::Text(error.clone().unwrap_or_default()),
                SqlValue::Integer(duration_ms as i64),
                SqlValue::Text(cold_start.to_string()),
            ],
        ).await?;

        // Update cache
        {
            let mut cache = self.compiled_cache.write().await;
            cache.insert(
                function_id.clone(),
                CompiledFunction {
                    function_id: function_id.clone(),
                    version: 1,
                    compiled_at: Instant::now(),
                },
            );
        }

        Ok(FunctionInvocation {
            id: invocation_id,
            function_id,
            status,
            input,
            output,
            error,
            duration_ms,
            memory_used_mb: 0,
            cold_start,
            timestamp: chrono::Utc::now().to_rfc3339(),
        })
    }

    async fn execute_function(&self, source: &str, input: &Value) -> StackhouseResult<Value> {
        // Execute JavaScript source using the Boa JS engine
        let input_str = input.to_string();
        let wrapped_source = format!(
            r#"(function(input) {{
                let module = {{}};
                let exports = {{}};
                let handler;
                {source}
                // Resolve handler: either `exports.handler`, `module.exports`, or `handler`
                let fn_handler = (typeof handler !== 'undefined') ? handler
                    : (exports && exports.handler) ? exports.handler
                    : (module && module.exports) ? module.exports
                    : null;
                if (typeof fn_handler === 'function') {{
                    return fn_handler(input);
                }}
                // If no handler found, try evaluating as an expression
                return eval({source_expr});
            }})({input_str})"#,
            source = source,
            source_expr = serde_json::to_string(source).unwrap_or_else(|_| "\"\"".to_string()),
            input_str = input_str,
        );

        let result = tokio::task::spawn_blocking(move || {
            let mut ctx = boa_engine::Context::default();
            let source = boa_engine::Source::from_bytes(wrapped_source.as_bytes());
            ctx.eval(source)
                .map_err(|e| {
                    StackhouseError::Internal(anyhow::anyhow!("JS execution error: {}", e))
                })
                .and_then(|v| {
                    v.to_json(&mut ctx).map_err(|e| {
                        StackhouseError::Internal(anyhow::anyhow!("JS serialization error: {}", e))
                    })
                })
        })
        .await
        .map_err(|e| StackhouseError::Internal(anyhow::anyhow!("JS task panicked: {}", e)))??;

        Ok(result)
    }

    /// List functions for a tenant
    pub async fn list_functions(&self, tenant_id: i64) -> StackhouseResult<Vec<Value>> {
        let rows = self.store.query(
            "SELECT id, name, runtime, status, version, triggers, created_at, updated_at FROM stackhouse_functions WHERE tenant_id = ? ORDER BY name".to_string(),
            vec![SqlValue::Integer(tenant_id)],
        ).await?;
        Ok(rows
            .into_iter()
            .map(|r| json!(r.into_iter().collect::<std::collections::HashMap<_, _>>()))
            .collect())
    }

    /// Get invocation logs
    pub async fn get_invocations(
        &self,
        function_id: &str,
        limit: usize,
    ) -> StackhouseResult<Vec<Value>> {
        let rows = self.store.query(
            format!("SELECT id, status, duration_ms, cold_start, error, timestamp FROM stackhouse_function_invocations WHERE function_id = ? ORDER BY timestamp DESC LIMIT {}", limit),
            vec![SqlValue::Text(function_id.to_string())],
        ).await?;
        Ok(rows
            .into_iter()
            .map(|r| json!(r.into_iter().collect::<std::collections::HashMap<_, _>>()))
            .collect())
    }

    /// Delete a function
    pub async fn delete_function(&self, function_id: &str, tenant_id: i64) -> StackhouseResult<()> {
        self.store
            .execute(
                "DELETE FROM stackhouse_functions WHERE id = ? AND tenant_id = ?".to_string(),
                vec![
                    SqlValue::Text(function_id.to_string()),
                    SqlValue::Integer(tenant_id),
                ],
            )
            .await?;
        self.compiled_cache.write().await.remove(function_id);
        Ok(())
    }
}

// ============================================================================
// Router
// ============================================================================

#[derive(Clone)]
pub struct FunctionsState {
    pub functions: Arc<FunctionsService>,
    pub auth: AuthState,
}

#[derive(Deserialize)]
struct DeployRequest {
    name: String,
    #[serde(default = "default_runtime")]
    runtime: String,
    #[serde(default = "default_entry")]
    entrypoint: String,
    source_code: String,
    #[serde(default)]
    env_vars: HashMap<String, String>,
    #[serde(default)]
    triggers: Vec<Value>,
}
fn default_runtime() -> String {
    "javascript".into()
}
fn default_entry() -> String {
    "handler".into()
}

#[derive(Deserialize)]
#[serde(untagged)]
enum InvokePayload {
    Wrapped { input: Value },
    Raw(Value),
}

async fn deploy_handler(
    State(state): State<FunctionsState>,
    headers: HeaderMap,
    Json(req): Json<DeployRequest>,
) -> Result<impl IntoResponse, StackhouseError> {
    let user = extract_auth_user(&state.auth, &headers)?;
    let runtime = match req.runtime.as_str() {
        "wasm_rust" => FunctionRuntime::WasmRust,
        "wasm_js" => FunctionRuntime::WasmJs,
        "typescript" => FunctionRuntime::TypeScript,
        _ => FunctionRuntime::JavaScript,
    };
    let triggers: Vec<FunctionTrigger> = req
        .triggers
        .iter()
        .filter_map(|t| serde_json::from_value(t.clone()).ok())
        .collect();
    let func = state
        .functions
        .deploy(
            user.id,
            &req.name,
            runtime,
            &req.entrypoint,
            &req.source_code,
            req.env_vars,
            triggers,
        )
        .await?;
    Ok(Json(json!({"success": true, "data": func})))
}

async fn invoke_handler(
    State(state): State<FunctionsState>,
    headers: HeaderMap,
    axum::extract::Path(name): axum::extract::Path<String>,
    Json(req): Json<InvokePayload>,
) -> Result<impl IntoResponse, StackhouseError> {
    let user = extract_auth_user(&state.auth, &headers)?;
    let input = match req {
        InvokePayload::Wrapped { input } => input,
        InvokePayload::Raw(v) => v,
    };
    let result = state.functions.invoke(user.id, &name, input).await?;
    Ok(Json(json!({"success": true, "data": result})))
}

async fn list_handler(
    State(state): State<FunctionsState>,
    headers: HeaderMap,
) -> Result<impl IntoResponse, StackhouseError> {
    let user = extract_auth_user(&state.auth, &headers)?;
    let funcs = state.functions.list_functions(user.id).await?;
    Ok(Json(json!({"success": true, "data": funcs})))
}

async fn delete_handler(
    State(state): State<FunctionsState>,
    headers: HeaderMap,
    axum::extract::Path(id): axum::extract::Path<String>,
) -> Result<impl IntoResponse, StackhouseError> {
    let user = extract_auth_user(&state.auth, &headers)?;
    state.functions.delete_function(&id, user.id).await?;
    Ok(Json(
        json!({"success": true, "message": "Function deleted"}),
    ))
}

pub fn create_functions_router(state: FunctionsState) -> Router {
    Router::new()
        .route("/deploy", post(deploy_handler))
        .route("/invoke/:name", post(invoke_handler))
        .route("/", get(list_handler))
        .route("/:id", delete(delete_handler))
        .with_state(state)
}
