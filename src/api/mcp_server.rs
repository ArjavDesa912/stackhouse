//! # MCP Server (Model Context Protocol)
//!
//! Allows AI coding tools (Cursor, Claude Code, Windsurf) to query the Stackhouse
//! platform directly. Exposes database schema, tables, functions, and docs
//! as MCP resources and tools.

use crate::auth::ApiKeyService;
use crate::db::{SqlValue, StackhouseStore};
use axum::{extract::State, http::HeaderMap, routing::post, Json, Router};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::sync::Arc;
use tracing::info;

// ============================================================================
// MCP Protocol Types
// ============================================================================

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpRequest {
    pub jsonrpc: String,
    pub id: Option<Value>,
    pub method: String,
    pub params: Option<Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpResponse {
    pub jsonrpc: String,
    pub id: Option<Value>,
    pub result: Option<Value>,
    pub error: Option<McpError>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpError {
    pub code: i32,
    pub message: String,
    pub data: Option<Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpTool {
    pub name: String,
    pub description: String,
    pub input_schema: Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpResource {
    pub uri: String,
    pub name: String,
    pub description: String,
    pub mime_type: String,
}

// ============================================================================
// MCP Server
// ============================================================================

#[derive(Clone)]
pub struct McpServer {
    store: Arc<StackhouseStore>,
    api_keys: Arc<ApiKeyService>,
}

impl McpServer {
    pub fn new(store: Arc<StackhouseStore>, api_keys: Arc<ApiKeyService>) -> Self {
        info!("🤖 MCP server initialized");
        Self { store, api_keys }
    }

    /// Handle an MCP JSON-RPC request
    pub async fn handle_request(
        &self,
        request: McpRequest,
        api_key: Option<String>,
    ) -> McpResponse {
        let id = request.id.clone();
        let result = match request.method.as_str() {
            "initialize" => self.handle_initialize().await,
            "tools/list" => self.handle_list_tools().await,
            "tools/call" => self.handle_tool_call(request.params, api_key).await,
            "resources/list" => self.handle_list_resources().await,
            "resources/read" => self.handle_read_resource(request.params).await,
            _ => Err(McpError {
                code: -32601,
                message: "Method not found".into(),
                data: None,
            }),
        };

        match result {
            Ok(value) => McpResponse {
                jsonrpc: "2.0".into(),
                id,
                result: Some(value),
                error: None,
            },
            Err(err) => McpResponse {
                jsonrpc: "2.0".into(),
                id,
                result: None,
                error: Some(err),
            },
        }
    }

    async fn handle_initialize(&self) -> Result<Value, McpError> {
        Ok(json!({
            "protocolVersion": "2024-11-05",
            "capabilities": {
                "tools": { "listChanged": false },
                "resources": { "subscribe": false, "listChanged": false },
            },
            "serverInfo": {
                "name": "stackhouse-mcp",
                "version": "1.0.0",
            }
        }))
    }

    async fn handle_list_tools(&self) -> Result<Value, McpError> {
        let tools = vec![
            McpTool {
                name: "query_sql".into(),
                description: "Execute a read-only SQL query against the Stackhouse database".into(),
                input_schema: json!({
                    "type": "object",
                    "properties": {
                        "sql": { "type": "string", "description": "SQL query to execute (SELECT only)" }
                    },
                    "required": ["sql"]
                }),
            },
            McpTool {
                name: "list_tables".into(),
                description: "List all tables in the database with column info".into(),
                input_schema: json!({ "type": "object", "properties": {} }),
            },
            McpTool {
                name: "describe_table".into(),
                description: "Get full schema details for a specific table".into(),
                input_schema: json!({
                    "type": "object",
                    "properties": {
                        "table_name": { "type": "string", "description": "Name of the table" }
                    },
                    "required": ["table_name"]
                }),
            },
            McpTool {
                name: "list_functions".into(),
                description: "List all deployed edge functions".into(),
                input_schema: json!({ "type": "object", "properties": {} }),
            },
            McpTool {
                name: "list_vector_collections".into(),
                description: "List vector/embedding collections".into(),
                input_schema: json!({ "type": "object", "properties": {} }),
            },
            McpTool {
                name: "search_vectors".into(),
                description: "Semantic search across vector collections".into(),
                input_schema: json!({
                    "type": "object",
                    "properties": {
                        "collection": { "type": "string" },
                        "query": { "type": "string" },
                        "limit": { "type": "integer", "default": 5 }
                    },
                    "required": ["collection", "query"]
                }),
            },
            McpTool {
                name: "get_api_docs".into(),
                description: "Get API documentation for Stackhouse endpoints".into(),
                input_schema: json!({
                    "type": "object",
                    "properties": {
                        "endpoint": { "type": "string", "description": "API endpoint path (e.g. /v1/auth/signup)" }
                    }
                }),
            },
            // === Scoped write tools (require mcp:write scope) ===
            McpTool {
                name: "create_table".into(),
                description: "Create a new collection/table in the database".into(),
                input_schema: json!({
                    "type": "object",
                    "properties": {
                        "name": { "type": "string", "description": "Collection/table name (will be prefixed with stackhouse_)" },
                        "columns": { "type": "array", "items": { "type": "string" }, "description": "Optional list of column definitions, e.g. [\"data JSONB\"]" }
                    },
                    "required": ["name"]
                }),
            },
            McpTool {
                name: "push_data".into(),
                description: "Insert a JSON record into a collection/table".into(),
                input_schema: json!({
                    "type": "object",
                    "properties": {
                        "table": { "type": "string", "description": "Target collection/table name" },
                        "record": { "type": "object", "description": "JSON object to insert" }
                    },
                    "required": ["table", "record"]
                }),
            },
            McpTool {
                name: "deploy_function".into(),
                description: "Deploy an edge function from source code".into(),
                input_schema: json!({
                    "type": "object",
                    "properties": {
                        "name": { "type": "string" },
                        "slug": { "type": "string" },
                        "runtime": { "type": "string", "enum": ["javascript", "python"], "default": "javascript" },
                        "source_code": { "type": "string" }
                    },
                    "required": ["name", "source_code"]
                }),
            },
            McpTool {
                name: "create_bucket".into(),
                description: "Create a new storage bucket".into(),
                input_schema: json!({
                    "type": "object",
                    "properties": {
                        "name": { "type": "string" },
                        "public": { "type": "boolean", "default": false }
                    },
                    "required": ["name"]
                }),
            },
            McpTool {
                name: "create_rls_policy".into(),
                description: "Create a Row-Level Security policy on a table".into(),
                input_schema: json!({
                    "type": "object",
                    "properties": {
                        "tenant_id": { "type": "integer" },
                        "table_name": { "type": "string" },
                        "policy_name": { "type": "string" },
                        "operation": { "type": "string", "enum": ["ALL", "SELECT", "INSERT", "UPDATE", "DELETE"] },
                        "using_expression": { "type": "string" },
                        "with_check_expression": { "type": "string" },
                        "description": { "type": "string" }
                    },
                    "required": ["tenant_id", "table_name", "policy_name", "using_expression"]
                }),
            },
        ];

        Ok(json!({ "tools": tools }))
    }

    async fn handle_tool_call(
        &self,
        params: Option<Value>,
        api_key: Option<String>,
    ) -> Result<Value, McpError> {
        let params = params.ok_or(McpError {
            code: -32602,
            message: "Missing params".into(),
            data: None,
        })?;
        let tool_name = params["name"].as_str().unwrap_or("");
        let arguments = &params["arguments"];

        // Validate write tools require an API key with mcp:write scope
        let write_tools = [
            "create_table",
            "push_data",
            "deploy_function",
            "create_bucket",
            "create_rls_policy",
        ];
        if write_tools.contains(&tool_name) {
            let key = api_key.as_deref().unwrap_or("");
            if key.is_empty() {
                return Ok(json!({
                    "content": [{"type": "text", "text": "Error: X-Api-Key header required for write tools"}],
                    "isError": true,
                }));
            }
            match self.api_keys.validate_key(key).await {
                Ok((_, scopes)) => {
                    if !ApiKeyService::has_scope(&scopes, "mcp:write") {
                        return Ok(json!({
                            "content": [{"type": "text", "text": "Error: API key missing mcp:write scope"}],
                            "isError": true,
                        }));
                    }
                }
                Err(_) => {
                    return Ok(json!({
                        "content": [{"type": "text", "text": "Error: Invalid or revoked API key"}],
                        "isError": true,
                    }));
                }
            }
        }

        match tool_name {
            "query_sql" => {
                let sql = arguments["sql"].as_str().unwrap_or("");
                if !sql.trim().to_uppercase().starts_with("SELECT") {
                    return Ok(json!({
                        "content": [{"type": "text", "text": "Error: Only SELECT queries are allowed"}],
                        "isError": true,
                    }));
                }
                match self.store.query(sql.to_string(), vec![]).await {
                    Ok(rows) => {
                        let text = serde_json::to_string_pretty(&rows).unwrap_or_default();
                        Ok(json!({ "content": [{"type": "text", "text": text}] }))
                    }
                    Err(e) => Ok(json!({
                        "content": [{"type": "text", "text": format!("Query error: {}", e)}],
                        "isError": true,
                    })),
                }
            }
            "list_tables" => {
                let rows = self.store.query(
                    "SELECT table_name, table_type FROM information_schema.tables WHERE table_schema = 'public' ORDER BY table_name".to_string(),
                    vec![],
                ).await.unwrap_or_default();
                let text = serde_json::to_string_pretty(&rows).unwrap_or_default();
                Ok(json!({ "content": [{"type": "text", "text": text}] }))
            }
            "describe_table" => {
                let table = arguments["table_name"].as_str().unwrap_or("");
                let rows = self.store.query(
                    format!("SELECT column_name, data_type, is_nullable, column_default FROM information_schema.columns WHERE table_name = '{}' ORDER BY ordinal_position", table),
                    vec![],
                ).await.unwrap_or_default();
                let text = serde_json::to_string_pretty(&rows).unwrap_or_default();
                Ok(json!({ "content": [{"type": "text", "text": text}] }))
            }
            "list_functions" => {
                let rows = self
                    .store
                    .query(
                        "SELECT id, name, runtime, status FROM stackhouse_functions ORDER BY name"
                            .to_string(),
                        vec![],
                    )
                    .await
                    .unwrap_or_default();
                let text = serde_json::to_string_pretty(&rows).unwrap_or_default();
                Ok(json!({ "content": [{"type": "text", "text": text}] }))
            }
            "list_vector_collections" => {
                let text = "Use GET /v1/vectors/collections to list collections.".to_string();
                Ok(json!({ "content": [{"type": "text", "text": text}] }))
            }
            "search_vectors" => {
                let collection = arguments["collection"].as_str().unwrap_or("");
                let query = arguments["query"].as_str().unwrap_or("");
                let text = format!("Semantic search in '{}' for: '{}'\nUse POST /v1/vectors/{}/search with body {{\"query\": \"{}\"}}", collection, query, collection, query);
                Ok(json!({ "content": [{"type": "text", "text": text}] }))
            }
            "get_api_docs" => {
                let endpoint = arguments["endpoint"].as_str().unwrap_or("");
                let docs = self.get_endpoint_docs(endpoint);
                Ok(json!({ "content": [{"type": "text", "text": docs}] }))
            }
            "create_table" => self.tool_create_table(arguments).await,
            "push_data" => self.tool_push_data(arguments).await,
            "deploy_function" => self.tool_deploy_function(arguments).await,
            "create_bucket" => self.tool_create_bucket(arguments).await,
            "create_rls_policy" => self.tool_create_rls_policy(arguments).await,
            _ => Err(McpError {
                code: -32602,
                message: format!("Unknown tool: {}", tool_name),
                data: None,
            }),
        }
    }

    async fn handle_list_resources(&self) -> Result<Value, McpError> {
        let resources = vec![
            McpResource {
                uri: "stackhouse://schema/tables".into(),
                name: "Database Tables".into(),
                description: "All tables in the Stackhouse database".into(),
                mime_type: "application/json".into(),
            },
            McpResource {
                uri: "stackhouse://api/endpoints".into(),
                name: "API Endpoints".into(),
                description: "All available REST API endpoints".into(),
                mime_type: "application/json".into(),
            },
            McpResource {
                uri: "stackhouse://config/environment".into(),
                name: "Environment Config".into(),
                description: "Current Stackhouse configuration (non-sensitive)".into(),
                mime_type: "application/json".into(),
            },
        ];
        Ok(json!({ "resources": resources }))
    }

    async fn handle_read_resource(&self, params: Option<Value>) -> Result<Value, McpError> {
        let params = params.ok_or(McpError {
            code: -32602,
            message: "Missing params".into(),
            data: None,
        })?;
        let uri = params["uri"].as_str().unwrap_or("");

        match uri {
            "stackhouse://schema/tables" => {
                let rows = self.store.query(
                    "SELECT table_name FROM information_schema.tables WHERE table_schema = 'public'".to_string(),
                    vec![],
                ).await.unwrap_or_default();
                let text = serde_json::to_string_pretty(&rows).unwrap_or_default();
                Ok(
                    json!({ "contents": [{"uri": uri, "mimeType": "application/json", "text": text}] }),
                )
            }
            "stackhouse://api/endpoints" => {
                let endpoints = self.get_all_endpoints();
                Ok(
                    json!({ "contents": [{"uri": uri, "mimeType": "application/json", "text": endpoints}] }),
                )
            }
            _ => Err(McpError {
                code: -32602,
                message: "Resource not found".into(),
                data: None,
            }),
        }
    }

    fn get_endpoint_docs(&self, endpoint: &str) -> String {
        match endpoint {
            "/v1/auth/signup" => "POST /v1/auth/signup\nBody: {\"email\": \"...\", \"password\": \"...\"}\nReturns: {\"user\": {...}, \"token\": \"...\"}".into(),
            "/v1/auth/login" => "POST /v1/auth/login\nBody: {\"email\": \"...\", \"password\": \"...\"}\nReturns: {\"user\": {...}, \"token\": \"...\", \"refresh_token\": \"...\"}".into(),
            "/v1/push/:col" => "POST /v1/push/:collection\nBody: any JSON object\nThe schema evolves automatically.\nReturns: {\"id\": \"...\", \"collection\": \"...\"}".into(),
            "/v1/query/:col" => "GET /v1/query/:collection?filter=...&sort=...&limit=N\nReturns: [{...}, ...]".into(),
            _ => format!("Documentation for '{}' not available. Try /v1/auth/signup, /v1/push/:col, /v1/query/:col", endpoint),
        }
    }

    fn get_all_endpoints(&self) -> String {
        serde_json::to_string_pretty(&json!({
            "auth": ["/v1/auth/signup", "/v1/auth/login", "/v1/auth/refresh", "/v1/auth/logout"],
            "data": ["/v1/push/:col", "/v1/query/:col", "/v1/update/:col/:id", "/v1/delete/:col/:id"],
            "graphql": ["/v1/graphql"],
            "vectors": ["/v1/vectors/:col/search", "/v1/vectors/:col/upsert"],
            "storage": ["/v1/storage/upload", "/v1/storage/download/:path"],
            "realtime": ["ws /v1/realtime"],
            "functions": ["/v1/functions/invoke/:name"],
            "admin": ["/v1/admin/extensions", "/v1/admin/branches", "/v1/admin/backups"],
            "platform": ["/v1/platform/projects"],
        })).unwrap_or_default()
    }

    async fn tool_create_table(&self, arguments: &Value) -> Result<Value, McpError> {
        let name = arguments["name"].as_str().unwrap_or("").trim();
        if name.is_empty() {
            return Ok(
                json!({ "content": [{"type": "text", "text": "Error: table name is required"}], "isError": true }),
            );
        }
        let table = format!("stackhouse_{}", name.trim_start_matches("stackhouse_"));
        let columns: Vec<String> = arguments["columns"]
            .as_array()
            .map(|arr| {
                arr.iter()
                    .filter_map(|v| v.as_str().map(String::from))
                    .collect()
            })
            .unwrap_or_default();
        let columns_sql = if columns.is_empty() {
            "id BIGSERIAL PRIMARY KEY, data JSONB NOT NULL, created_at TIMESTAMPTZ DEFAULT NOW(), updated_at TIMESTAMPTZ DEFAULT NOW()".to_string()
        } else {
            columns.join(", ")
        };

        let sql = format!("CREATE TABLE IF NOT EXISTS {} ({})", table, columns_sql);
        match self.store.execute(sql, vec![]).await {
            Ok(_) => Ok(
                json!({ "content": [{"type": "text", "text": format!("Created table {}", table)}] }),
            ),
            Err(e) => Ok(
                json!({ "content": [{"type": "text", "text": format!("Error: {}", e)}], "isError": true }),
            ),
        }
    }

    async fn tool_push_data(&self, arguments: &Value) -> Result<Value, McpError> {
        let table = arguments["table"].as_str().unwrap_or("").trim();
        if table.is_empty() {
            return Ok(
                json!({ "content": [{"type": "text", "text": "Error: table is required"}], "isError": true }),
            );
        }
        let record = arguments["record"].clone();
        if record.is_null() {
            return Ok(
                json!({ "content": [{"type": "text", "text": "Error: record is required"}], "isError": true }),
            );
        }

        let table = format!("stackhouse_{}", table.trim_start_matches("stackhouse_"));
        let sql = format!(
            "INSERT INTO {} (data) VALUES (?::jsonb) RETURNING id",
            table
        );
        let data = serde_json::to_string(&record).unwrap_or_default();

        match self.store.query(sql, vec![SqlValue::Text(data)]).await {
            Ok(rows) => {
                let id = rows
                    .first()
                    .and_then(|r| r.first())
                    .and_then(|(_, v)| v.as_i64())
                    .unwrap_or(0);
                Ok(
                    json!({ "content": [{"type": "text", "text": format!("Inserted record into {} with id {}", table, id)}] }),
                )
            }
            Err(e) => Ok(
                json!({ "content": [{"type": "text", "text": format!("Error: {}", e)}], "isError": true }),
            ),
        }
    }

    async fn tool_deploy_function(&self, arguments: &Value) -> Result<Value, McpError> {
        let name = arguments["name"].as_str().unwrap_or("").to_string();
        let source = arguments["source_code"].as_str().unwrap_or("").to_string();
        if name.is_empty() || source.is_empty() {
            return Ok(
                json!({ "content": [{"type": "text", "text": "Error: name and source_code are required"}], "isError": true }),
            );
        }
        let slug = arguments["slug"].as_str().unwrap_or(&name).to_string();
        let runtime = arguments["runtime"]
            .as_str()
            .unwrap_or("javascript")
            .to_string();
        let id = uuid::Uuid::new_v4().to_string();

        match self.store.execute(
            "INSERT INTO stackhouse_functions (id, name, slug, source_code, runtime, status) VALUES (?, ?, ?, ?, ?, 'active')".to_string(),
            vec![
                SqlValue::Text(id.clone()),
                SqlValue::Text(name),
                SqlValue::Text(slug),
                SqlValue::Text(source),
                SqlValue::Text(runtime),
            ],
        ).await {
            Ok(_) => Ok(json!({ "content": [{"type": "text", "text": format!("Deployed function {}", id)}] })),
            Err(e) => Ok(json!({ "content": [{"type": "text", "text": format!("Error: {}", e)}], "isError": true })),
        }
    }

    async fn tool_create_bucket(&self, arguments: &Value) -> Result<Value, McpError> {
        let name = arguments["name"].as_str().unwrap_or("").trim();
        if name.is_empty() {
            return Ok(
                json!({ "content": [{"type": "text", "text": "Error: bucket name is required"}], "isError": true }),
            );
        }
        let public = arguments["public"].as_bool().unwrap_or(false);

        match self.store.execute(
            "INSERT INTO stackhouse_buckets (name, public, owner_id) VALUES (?, ?, NULL) ON CONFLICT (name) DO NOTHING".to_string(),
            vec![SqlValue::Text(name.to_string()), SqlValue::Integer(if public { 1 } else { 0 })],
        ).await {
            Ok(_) => Ok(json!({ "content": [{"type": "text", "text": format!("Created bucket {} (public={})", name, public)}] })),
            Err(e) => Ok(json!({ "content": [{"type": "text", "text": format!("Error: {}", e)}], "isError": true })),
        }
    }

    async fn tool_create_rls_policy(&self, arguments: &Value) -> Result<Value, McpError> {
        let tenant_id = arguments["tenant_id"].as_i64().unwrap_or(0);
        let table_name = arguments["table_name"].as_str().unwrap_or("").to_string();
        let policy_name = arguments["policy_name"].as_str().unwrap_or("").to_string();
        let using = arguments["using_expression"]
            .as_str()
            .unwrap_or("")
            .to_string();
        if table_name.is_empty() || policy_name.is_empty() || using.is_empty() {
            return Ok(
                json!({ "content": [{"type": "text", "text": "Error: tenant_id, table_name, policy_name, and using_expression are required"}], "isError": true }),
            );
        }
        let operation = arguments["operation"]
            .as_str()
            .unwrap_or("ALL")
            .to_uppercase();
        let with_check = arguments["with_check_expression"]
            .as_str()
            .unwrap_or("")
            .to_string();
        let description = arguments["description"].as_str().unwrap_or("").to_string();
        let id = uuid::Uuid::new_v4().to_string();

        let effective_check = if with_check.is_empty() {
            using.clone()
        } else {
            with_check.clone()
        };

        match self.store.execute(
            "INSERT INTO stackhouse_rls_policies (id, tenant_id, table_name, policy_name, operation, expression, target_roles, using_expression, with_check_expression, enabled, description) VALUES (?, ?, ?, ?, ?, ?, ?::jsonb, ?, ?, ?, ?)".to_string(),
            vec![
                SqlValue::Text(id),
                SqlValue::Integer(tenant_id),
                SqlValue::Text(table_name.clone()),
                SqlValue::Text(policy_name.clone()),
                SqlValue::Text(operation.clone()),
                SqlValue::Text(using.clone()),
                SqlValue::Text("[]".to_string()),
                SqlValue::Text(using.clone()),
                SqlValue::Text(effective_check),
                SqlValue::Text("true".to_string()),
                SqlValue::Text(description),
            ],
        ).await {
            Ok(_) => {
                // Best-effort enable RLS on the target table
                let _ = self.store.execute(
                    format!("ALTER TABLE IF EXISTS {} ENABLE ROW LEVEL SECURITY", table_name),
                    vec![],
                ).await;
                let _ = self.store.execute(
                    format!("CREATE POLICY IF NOT EXISTS {} ON {} AS PERMISSIVE FOR {} TO PUBLIC USING ({})",
                        policy_name, table_name, operation, with_check),
                    vec![],
                ).await;
                Ok(json!({ "content": [{"type": "text", "text": format!("Created RLS policy {} on {}", policy_name, table_name)}] }))
            }
            Err(e) => Ok(json!({ "content": [{"type": "text", "text": format!("Error: {}", e)}], "isError": true })),
        }
    }
}

// ============================================================================
// Axum Router
// ============================================================================

#[derive(Clone)]
pub struct McpState {
    pub mcp: McpServer,
}

pub fn create_mcp_router(state: McpState) -> Router {
    Router::new()
        .route("/mcp", post(handle_mcp_request))
        .with_state(state)
}

async fn handle_mcp_request(
    State(state): State<McpState>,
    headers: HeaderMap,
    Json(request): Json<McpRequest>,
) -> Json<McpResponse> {
    let api_key = headers
        .get("x-api-key")
        .or_else(|| headers.get("X-Api-Key"))
        .and_then(|v| v.to_str().ok())
        .map(|s| s.to_string());
    Json(state.mcp.handle_request(request, api_key).await)
}
