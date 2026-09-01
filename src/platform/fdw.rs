//! # Foreign Data Wrappers (FDW) Management
//!
//! Create and manage foreign data wrappers for external data sources
//! (Postgres, MySQL, S3, HTTP endpoints).

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
use tracing::info;

// ============================================================================
// Types
// ============================================================================

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ForeignServer {
    pub id: String,
    pub tenant_id: i64,
    pub name: String,
    pub wrapper_type: FdwType,
    pub host: String,
    pub port: u16,
    pub database: String,
    pub options: Value,
    pub status: String,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FdwType {
    Postgres,
    Mysql,
    S3Csv,
    HttpApi,
    Redis,
    Mongodb,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ForeignTable {
    pub id: String,
    pub server_id: String,
    pub local_name: String,
    pub remote_schema: String,
    pub remote_table: String,
    pub columns: Vec<ForeignColumn>,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ForeignColumn {
    pub name: String,
    pub data_type: String,
    pub nullable: bool,
}

// ============================================================================
// FDW Service
// ============================================================================

#[derive(Clone)]
pub struct FdwService {
    store: Arc<StackhouseStore>,
}

impl FdwService {
    pub async fn new(store: Arc<StackhouseStore>) -> StackhouseResult<Self> {
        let service = Self { store };
        service.initialize_tables().await?;
        info!("🌐 Foreign Data Wrapper service initialized");
        Ok(service)
    }

    async fn initialize_tables(&self) -> StackhouseResult<()> {
        self.store.execute_batch(r#"
            CREATE TABLE IF NOT EXISTS stackhouse_foreign_servers (
                id TEXT PRIMARY KEY,
                tenant_id BIGINT NOT NULL,
                name TEXT NOT NULL,
                wrapper_type TEXT NOT NULL,
                host TEXT NOT NULL,
                port INTEGER NOT NULL,
                database_name TEXT,
                username TEXT,
                password_encrypted TEXT,
                options JSONB DEFAULT '{}',
                status TEXT DEFAULT 'active',
                created_at TIMESTAMPTZ DEFAULT NOW()
            );
            CREATE TABLE IF NOT EXISTS stackhouse_foreign_tables (
                id TEXT PRIMARY KEY,
                server_id TEXT NOT NULL REFERENCES stackhouse_foreign_servers(id) ON DELETE CASCADE,
                tenant_id BIGINT NOT NULL,
                local_name TEXT NOT NULL,
                remote_schema TEXT DEFAULT 'public',
                remote_table TEXT NOT NULL,
                columns JSONB NOT NULL DEFAULT '[]',
                created_at TIMESTAMPTZ DEFAULT NOW()
            );
            CREATE INDEX IF NOT EXISTS idx_foreign_servers_tenant ON stackhouse_foreign_servers(tenant_id);
            CREATE INDEX IF NOT EXISTS idx_foreign_tables_server ON stackhouse_foreign_tables(server_id);
        "#.to_string()).await?;
        Ok(())
    }

    /// Create a foreign server connection
    pub async fn create_server(
        &self,
        tenant_id: i64,
        name: &str,
        wrapper_type: FdwType,
        host: &str,
        port: u16,
        database: &str,
        username: &str,
        password: &str,
        options: Value,
    ) -> StackhouseResult<ForeignServer> {
        let id = uuid::Uuid::new_v4().to_string();
        let type_str = serde_json::to_string(&wrapper_type)
            .unwrap_or_default()
            .trim_matches('"')
            .to_string();

        // Create actual FDW in Postgres
        let fdw_name = match &wrapper_type {
            FdwType::Postgres => "postgres_fdw",
            FdwType::Mysql => "mysql_fdw",
            FdwType::S3Csv => "s3_fdw",
            FdwType::HttpApi => "http_fdw",
            FdwType::Redis => "redis_fdw",
            FdwType::Mongodb => "mongo_fdw",
        };

        // Install extension and create server
        let create_sql = format!(
            r#"
            CREATE EXTENSION IF NOT EXISTS {fdw_name};
            CREATE SERVER IF NOT EXISTS {server_name} FOREIGN DATA WRAPPER {fdw_name}
                OPTIONS (host '{host}', port '{port}', dbname '{database}');
            CREATE USER MAPPING IF NOT EXISTS FOR CURRENT_USER SERVER {server_name}
                OPTIONS (user '{username}', password '{password}');
            "#,
            fdw_name = fdw_name,
            server_name = format!("stackhouse_fdw_{}", name),
            host = host,
            port = port,
            database = database,
            username = username,
            password = password,
        );

        self.store.execute_batch(create_sql).await.ok(); // May fail if extension not available

        self.store.execute(
            "INSERT INTO stackhouse_foreign_servers (id, tenant_id, name, wrapper_type, host, port, database_name, username, password_encrypted, options) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?::jsonb)".to_string(),
            vec![
                SqlValue::Text(id.clone()),
                SqlValue::Integer(tenant_id),
                SqlValue::Text(name.to_string()),
                SqlValue::Text(type_str),
                SqlValue::Text(host.to_string()),
                SqlValue::Integer(port as i64),
                SqlValue::Text(database.to_string()),
                SqlValue::Text(username.to_string()),
                SqlValue::Text("[encrypted]".to_string()),
                SqlValue::Text(options.to_string()),
            ],
        ).await?;

        info!("🌐 Foreign server created: {} ({}:{})", name, host, port);

        Ok(ForeignServer {
            id,
            tenant_id,
            name: name.to_string(),
            wrapper_type,
            host: host.to_string(),
            port,
            database: database.to_string(),
            options,
            status: "active".to_string(),
            created_at: chrono::Utc::now().to_rfc3339(),
        })
    }

    /// Import a foreign table
    pub async fn import_table(
        &self,
        tenant_id: i64,
        server_id: &str,
        local_name: &str,
        remote_schema: &str,
        remote_table: &str,
        columns: Vec<ForeignColumn>,
    ) -> StackhouseResult<ForeignTable> {
        let id = uuid::Uuid::new_v4().to_string();

        // Get server info for actual SQL
        let server_rows = self
            .store
            .query(
                "SELECT name FROM stackhouse_foreign_servers WHERE id = ? AND tenant_id = ?"
                    .to_string(),
                vec![
                    SqlValue::Text(server_id.to_string()),
                    SqlValue::Integer(tenant_id),
                ],
            )
            .await?;

        if server_rows.is_empty() {
            return Err(StackhouseError::NotFound("Foreign server not found".into()));
        }

        let server_name = server_rows[0]
            .iter()
            .find(|(k, _)| k == "name")
            .and_then(|(_, v)| v.as_str())
            .unwrap_or("");

        // Build column definitions
        let col_defs: Vec<String> = columns
            .iter()
            .map(|c| {
                format!(
                    "{} {} {}",
                    c.name,
                    c.data_type,
                    if c.nullable { "NULL" } else { "NOT NULL" }
                )
            })
            .collect();

        // Create the foreign table in Postgres
        let create_ft_sql = format!(
            "CREATE FOREIGN TABLE IF NOT EXISTS {} ({}) SERVER {} OPTIONS (schema_name '{}', table_name '{}');",
            local_name,
            col_defs.join(", "),
            format!("stackhouse_fdw_{}", server_name),
            remote_schema,
            remote_table,
        );

        self.store.execute_batch(create_ft_sql).await.ok();

        self.store.execute(
            "INSERT INTO stackhouse_foreign_tables (id, server_id, tenant_id, local_name, remote_schema, remote_table, columns) VALUES (?, ?, ?, ?, ?, ?, ?::jsonb)".to_string(),
            vec![
                SqlValue::Text(id.clone()),
                SqlValue::Text(server_id.to_string()),
                SqlValue::Integer(tenant_id),
                SqlValue::Text(local_name.to_string()),
                SqlValue::Text(remote_schema.to_string()),
                SqlValue::Text(remote_table.to_string()),
                SqlValue::Text(serde_json::to_string(&columns).unwrap_or_default()),
            ],
        ).await?;

        Ok(ForeignTable {
            id,
            server_id: server_id.to_string(),
            local_name: local_name.to_string(),
            remote_schema: remote_schema.to_string(),
            remote_table: remote_table.to_string(),
            columns,
            created_at: chrono::Utc::now().to_rfc3339(),
        })
    }

    /// List foreign servers
    pub async fn list_servers(&self, tenant_id: i64) -> StackhouseResult<Vec<Value>> {
        let rows = self.store.query(
            "SELECT id, name, wrapper_type, host, port, database_name, status, created_at FROM stackhouse_foreign_servers WHERE tenant_id = ? ORDER BY created_at DESC".to_string(),
            vec![SqlValue::Integer(tenant_id)],
        ).await?;
        Ok(rows
            .into_iter()
            .map(|r| json!(r.into_iter().collect::<std::collections::HashMap<_, _>>()))
            .collect())
    }

    /// List foreign tables for a server
    pub async fn list_tables(&self, server_id: &str) -> StackhouseResult<Vec<Value>> {
        let rows = self.store.query(
            "SELECT id, local_name, remote_schema, remote_table, columns, created_at FROM stackhouse_foreign_tables WHERE server_id = ? ORDER BY local_name".to_string(),
            vec![SqlValue::Text(server_id.to_string())],
        ).await?;
        Ok(rows
            .into_iter()
            .map(|r| json!(r.into_iter().collect::<std::collections::HashMap<_, _>>()))
            .collect())
    }

    /// Delete a foreign server (cascades to tables)
    pub async fn delete_server(&self, server_id: &str, tenant_id: i64) -> StackhouseResult<()> {
        self.store
            .execute(
                "DELETE FROM stackhouse_foreign_servers WHERE id = ? AND tenant_id = ?".to_string(),
                vec![
                    SqlValue::Text(server_id.to_string()),
                    SqlValue::Integer(tenant_id),
                ],
            )
            .await?;
        Ok(())
    }

    /// Test a server connection
    pub async fn test_connection(
        &self,
        server_id: &str,
        tenant_id: i64,
    ) -> StackhouseResult<Value> {
        let rows = self
            .store
            .query(
                "SELECT host, port FROM stackhouse_foreign_servers WHERE id = ? AND tenant_id = ?"
                    .to_string(),
                vec![
                    SqlValue::Text(server_id.to_string()),
                    SqlValue::Integer(tenant_id),
                ],
            )
            .await?;

        if rows.is_empty() {
            return Err(StackhouseError::NotFound("Server not found".into()));
        }

        let row = &rows[0];
        let host = row
            .iter()
            .find(|(k, _)| k == "host")
            .and_then(|(_, v)| v.as_str())
            .unwrap_or("");
        let port = row
            .iter()
            .find(|(k, _)| k == "port")
            .and_then(|(_, v)| v.as_i64())
            .unwrap_or(5432) as u16;

        let reachable = tokio::time::timeout(
            std::time::Duration::from_secs(5),
            tokio::net::TcpStream::connect(format!("{}:{}", host, port)),
        )
        .await
        .map(|r| r.is_ok())
        .unwrap_or(false);

        Ok(json!({
            "reachable": reachable,
            "host": host,
            "port": port,
        }))
    }
}

// ============================================================================
// Router
// ============================================================================

#[derive(Clone)]
pub struct FdwState {
    pub fdw: Arc<FdwService>,
    pub auth: AuthState,
}

#[derive(Deserialize)]
struct CreateServerRequest {
    name: String,
    wrapper_type: String,
    host: String,
    #[serde(default = "default_fdw_port")]
    port: u16,
    #[serde(default)]
    database: String,
    #[serde(default)]
    username: String,
    #[serde(default)]
    password: String,
    #[serde(default)]
    options: Value,
}
fn default_fdw_port() -> u16 {
    5432
}

#[derive(Deserialize)]
struct ImportTableRequest {
    local_name: String,
    #[serde(default = "default_schema")]
    remote_schema: String,
    remote_table: String,
    columns: Vec<ForeignColumn>,
}
fn default_schema() -> String {
    "public".to_string()
}

async fn create_server_handler(
    State(state): State<FdwState>,
    headers: HeaderMap,
    Json(req): Json<CreateServerRequest>,
) -> Result<impl IntoResponse, StackhouseError> {
    let user = extract_auth_user(&state.auth, &headers)?;
    let wrapper_type = match req.wrapper_type.as_str() {
        "mysql" => FdwType::Mysql,
        "s3_csv" => FdwType::S3Csv,
        "http_api" => FdwType::HttpApi,
        "redis" => FdwType::Redis,
        "mongodb" => FdwType::Mongodb,
        _ => FdwType::Postgres,
    };
    let server = state
        .fdw
        .create_server(
            user.id,
            &req.name,
            wrapper_type,
            &req.host,
            req.port,
            &req.database,
            &req.username,
            &req.password,
            req.options,
        )
        .await?;
    Ok(Json(json!({"success": true, "data": server})))
}

async fn list_servers_handler(
    State(state): State<FdwState>,
    headers: HeaderMap,
) -> Result<impl IntoResponse, StackhouseError> {
    let user = extract_auth_user(&state.auth, &headers)?;
    let servers = state.fdw.list_servers(user.id).await?;
    Ok(Json(json!({"success": true, "data": servers})))
}

async fn import_table_handler(
    State(state): State<FdwState>,
    headers: HeaderMap,
    axum::extract::Path(server_id): axum::extract::Path<String>,
    Json(req): Json<ImportTableRequest>,
) -> Result<impl IntoResponse, StackhouseError> {
    let user = extract_auth_user(&state.auth, &headers)?;
    let table = state
        .fdw
        .import_table(
            user.id,
            &server_id,
            &req.local_name,
            &req.remote_schema,
            &req.remote_table,
            req.columns,
        )
        .await?;
    Ok(Json(json!({"success": true, "data": table})))
}

async fn list_tables_handler(
    State(state): State<FdwState>,
    axum::extract::Path(server_id): axum::extract::Path<String>,
) -> Result<impl IntoResponse, StackhouseError> {
    let tables = state.fdw.list_tables(&server_id).await?;
    Ok(Json(json!({"success": true, "data": tables})))
}

async fn delete_server_handler(
    State(state): State<FdwState>,
    headers: HeaderMap,
    axum::extract::Path(server_id): axum::extract::Path<String>,
) -> Result<impl IntoResponse, StackhouseError> {
    let user = extract_auth_user(&state.auth, &headers)?;
    state.fdw.delete_server(&server_id, user.id).await?;
    Ok(Json(json!({"success": true, "message": "Server deleted"})))
}

async fn test_connection_handler(
    State(state): State<FdwState>,
    headers: HeaderMap,
    axum::extract::Path(server_id): axum::extract::Path<String>,
) -> Result<impl IntoResponse, StackhouseError> {
    let user = extract_auth_user(&state.auth, &headers)?;
    let result = state.fdw.test_connection(&server_id, user.id).await?;
    Ok(Json(json!({"success": true, "data": result})))
}

pub fn create_fdw_router(state: FdwState) -> Router {
    Router::new()
        .route("/servers", post(create_server_handler))
        .route("/servers", get(list_servers_handler))
        .route("/servers/:id", delete(delete_server_handler))
        .route("/servers/:id/tables", post(import_table_handler))
        .route("/servers/:id/tables", get(list_tables_handler))
        .route("/servers/:id/test", post(test_connection_handler))
        .with_state(state)
}
