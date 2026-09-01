//! # Auto-Generated REST APIs from Schema
//!
//! Dynamically generates REST endpoints for every table in the database.
//! Endpoints support CRUD, filtering, sorting, pagination, and relations.
//! No manual route definition needed — tables become resources automatically.

use crate::db::{SqlValue, StackhouseStore};
use crate::error::{StackhouseError, StackhouseResult};

use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    response::IntoResponse,
    routing::{delete, get, post, put},
    Json, Router,
};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::HashMap;
use std::sync::Arc;
use tracing::info;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TableResource {
    pub table_name: String,
    pub columns: Vec<ColumnInfo>,
    pub primary_key: String,
    pub has_rls: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ColumnInfo {
    pub name: String,
    pub data_type: String,
    pub nullable: bool,
    pub default: Option<String>,
    pub is_primary_key: bool,
}

#[derive(Debug, Deserialize)]
pub struct ListQuery {
    #[serde(default)]
    pub filter: Option<String>,
    #[serde(default)]
    pub sort: Option<String>,
    #[serde(default = "default_limit")]
    pub limit: i64,
    #[serde(default)]
    pub offset: i64,
    #[serde(default)]
    pub select: Option<String>,
}

fn default_limit() -> i64 {
    100
}

#[derive(Clone)]
pub struct AutoRestService {
    store: Arc<StackhouseStore>,
    resources: Arc<tokio::sync::RwLock<HashMap<String, TableResource>>>,
}

impl AutoRestService {
    pub async fn new(store: Arc<StackhouseStore>) -> StackhouseResult<Self> {
        let service = Self {
            store,
            resources: Arc::new(tokio::sync::RwLock::new(HashMap::new())),
        };
        service.discover_resources().await?;
        info!("🌐 Auto-REST service initialized");
        Ok(service)
    }

    async fn discover_resources(&self) -> StackhouseResult<()> {
        let rows = self
            .store
            .query(
                r#"SELECT table_name FROM information_schema.tables
               WHERE table_schema = 'public' AND table_type = 'BASE TABLE'
               AND table_name LIKE 'stackhouse_%'"#
                    .to_string(),
                vec![],
            )
            .await?;

        let mut resources = self.resources.write().await;
        for row in rows {
            let table_name = row
                .iter()
                .find(|(k, _)| k == "table_name")
                .and_then(|(_, v)| v.as_str())
                .unwrap_or("");
            if let Ok(cols) = self.get_columns(table_name).await {
                let pk = cols
                    .iter()
                    .find(|c| c.is_primary_key)
                    .map(|c| c.name.clone())
                    .unwrap_or_else(|| "id".into());
                let has_rls = self.check_rls_enabled(table_name).await.unwrap_or(false);
                resources.insert(
                    table_name.to_string(),
                    TableResource {
                        table_name: table_name.to_string(),
                        columns: cols,
                        primary_key: pk,
                        has_rls,
                    },
                );
            }
        }
        Ok(())
    }

    async fn check_rls_enabled(&self, table_name: &str) -> StackhouseResult<bool> {
        let rows = self
            .store
            .query(
                format!(
                    "SELECT relrowsecurity FROM pg_class WHERE relname = '{}'",
                    table_name
                ),
                vec![],
            )
            .await?;
        Ok(rows
            .first()
            .and_then(|r| r.iter().find(|(k, _)| k == "relrowsecurity"))
            .and_then(|(_, v)| v.as_bool())
            .unwrap_or(false))
    }

    async fn get_columns(&self, table_name: &str) -> StackhouseResult<Vec<ColumnInfo>> {
        let rows = self.store.query(
            format!(r#"SELECT column_name, data_type, is_nullable, column_default,
                CASE WHEN column_name IN (
                    SELECT kcu.column_name FROM information_schema.table_constraints tc
                    JOIN information_schema.key_column_usage kcu ON tc.constraint_name = kcu.constraint_name
                    WHERE tc.table_name = '{}' AND tc.constraint_type = 'PRIMARY KEY'
                ) THEN true ELSE false END as is_primary_key
                FROM information_schema.columns WHERE table_name = '{}' ORDER BY ordinal_position"#, table_name, table_name),
            vec![],
        ).await?;

        Ok(rows
            .into_iter()
            .map(|row| ColumnInfo {
                name: row
                    .iter()
                    .find(|(k, _)| k == "column_name")
                    .and_then(|(_, v)| v.as_str())
                    .unwrap_or("")
                    .to_string(),
                data_type: row
                    .iter()
                    .find(|(k, _)| k == "data_type")
                    .and_then(|(_, v)| v.as_str())
                    .unwrap_or("")
                    .to_string(),
                nullable: row
                    .iter()
                    .find(|(k, _)| k == "is_nullable")
                    .and_then(|(_, v)| v.as_str())
                    == Some("YES"),
                default: row
                    .iter()
                    .find(|(k, _)| k == "column_default")
                    .and_then(|(_, v)| v.as_str())
                    .map(|s| s.to_string()),
                is_primary_key: row
                    .iter()
                    .find(|(k, _)| k == "is_primary_key")
                    .and_then(|(_, v)| v.as_str())
                    .map(|s| s == "true" || s == "t")
                    .unwrap_or(false),
            })
            .collect())
    }

    pub async fn create(&self, table: &str, data: Value) -> StackhouseResult<Value> {
        let resources = self.resources.read().await;
        let _resource = resources
            .get(table)
            .ok_or_else(|| StackhouseError::NotFound(format!("Resource '{}' not found", table)))?;

        let obj = data
            .as_object()
            .ok_or_else(|| StackhouseError::InvalidPayload("Expected JSON object".into()))?;
        let columns: Vec<String> = obj.keys().cloned().collect();
        let placeholders: Vec<String> = (1..=columns.len()).map(|_i| format!("?")).collect();
        let values: Vec<SqlValue> = columns
            .iter()
            .map(|col| SqlValue::Text(obj.get(col).unwrap_or(&json!(null)).to_string()))
            .collect();

        let sql = format!(
            "INSERT INTO {} ({}) VALUES ({}) RETURNING *",
            table,
            columns.join(", "),
            placeholders.join(", ")
        );

        let rows = self.store.query(sql, values).await?;
        if rows.is_empty() {
            return Err(StackhouseError::Internal(anyhow::anyhow!("Insert failed")));
        }
        Ok(json!(rows[0].iter().cloned().collect::<HashMap<_, _>>()))
    }

    pub async fn list(&self, table: &str, query: &ListQuery) -> StackhouseResult<Vec<Value>> {
        let resources = self.resources.read().await;
        let _resource = resources
            .get(table)
            .ok_or_else(|| StackhouseError::NotFound(format!("Resource '{}' not found", table)))?;

        let mut conditions = Vec::new();
        let mut params = Vec::new();

        if let Some(filter) = &query.filter {
            // Simple filter: field=value pairs separated by commas
            for part in filter.split(',') {
                if let Some((field, value)) = part.split_once('=') {
                    conditions.push(format!("{} = ?", field));
                    params.push(SqlValue::Text(value.to_string()));
                }
            }
        }

        let where_clause = if conditions.is_empty() {
            String::new()
        } else {
            format!(" WHERE {}", conditions.join(" AND "))
        };

        let order_clause = query
            .sort
            .as_ref()
            .map(|s| format!(" ORDER BY {}", s))
            .unwrap_or_default();
        let sql = format!(
            "SELECT * FROM {}{}{} LIMIT {} OFFSET {}",
            table, where_clause, order_clause, query.limit, query.offset
        );

        let rows = self.store.query(sql, params).await?;
        Ok(rows
            .into_iter()
            .map(|r| json!(r.iter().cloned().collect::<HashMap<_, _>>()))
            .collect())
    }

    pub async fn get(&self, table: &str, id: &str) -> StackhouseResult<Value> {
        let resources = self.resources.read().await;
        let resource = resources
            .get(table)
            .ok_or_else(|| StackhouseError::NotFound(format!("Resource '{}' not found", table)))?;

        let rows = self
            .store
            .query(
                format!("SELECT * FROM {} WHERE {} = ?", table, resource.primary_key),
                vec![SqlValue::Text(id.to_string())],
            )
            .await?;

        if rows.is_empty() {
            return Err(StackhouseError::NotFound("Record not found".into()));
        }
        Ok(json!(rows[0].iter().cloned().collect::<HashMap<_, _>>()))
    }

    pub async fn update(&self, table: &str, id: &str, data: Value) -> StackhouseResult<Value> {
        let resources = self.resources.read().await;
        let resource = resources
            .get(table)
            .ok_or_else(|| StackhouseError::NotFound(format!("Resource '{}' not found", table)))?;

        let obj = data
            .as_object()
            .ok_or_else(|| StackhouseError::InvalidPayload("Expected JSON object".into()))?;
        let sets: Vec<String> = obj.keys().map(|k| format!("{} = ?", k)).collect();
        let mut values: Vec<SqlValue> = obj
            .values()
            .map(|v| SqlValue::Text(v.to_string()))
            .collect();
        values.push(SqlValue::Text(id.to_string()));

        let sql = format!(
            "UPDATE {} SET {} WHERE {} = ? RETURNING *",
            table,
            sets.join(", "),
            resource.primary_key
        );

        let rows = self.store.query(sql, values).await?;
        if rows.is_empty() {
            return Err(StackhouseError::NotFound("Record not found".into()));
        }
        Ok(json!(rows[0].iter().cloned().collect::<HashMap<_, _>>()))
    }

    pub async fn delete(&self, table: &str, id: &str) -> StackhouseResult<()> {
        let resources = self.resources.read().await;
        let resource = resources
            .get(table)
            .ok_or_else(|| StackhouseError::NotFound(format!("Resource '{}' not found", table)))?;

        self.store
            .execute(
                format!("DELETE FROM {} WHERE {} = ?", table, resource.primary_key),
                vec![SqlValue::Text(id.to_string())],
            )
            .await?;
        Ok(())
    }

    pub async fn list_resources(&self) -> Vec<String> {
        self.resources.read().await.keys().cloned().collect()
    }
}

#[derive(Clone)]
pub struct AutoRestState {
    pub service: AutoRestService,
}

pub fn create_auto_rest_router(state: AutoRestState) -> Router {
    Router::new()
        .route("/tables", get(list_tables_handler))
        .route("/tables/:table", post(create_handler))
        .route("/tables/:table", get(list_handler))
        .route("/tables/:table/:id", get(get_handler))
        .route("/tables/:table/:id", put(update_handler))
        .route("/tables/:table/:id", delete(delete_handler))
        .with_state(state)
}

async fn list_tables_handler(
    State(state): State<AutoRestState>,
) -> Result<impl IntoResponse, StackhouseError> {
    let tables = state.service.list_resources().await;
    Ok(Json(json!({ "success": true, "data": tables })))
}

async fn create_handler(
    State(state): State<AutoRestState>,
    Path(table): Path<String>,
    Json(data): Json<Value>,
) -> Result<impl IntoResponse, StackhouseError> {
    let result = state.service.create(&table, data).await?;
    Ok((
        StatusCode::CREATED,
        Json(json!({ "success": true, "data": result })),
    ))
}

async fn list_handler(
    State(state): State<AutoRestState>,
    Path(table): Path<String>,
    Query(query): Query<ListQuery>,
) -> Result<impl IntoResponse, StackhouseError> {
    let results = state.service.list(&table, &query).await?;
    Ok(Json(json!({ "success": true, "data": results })))
}

async fn get_handler(
    State(state): State<AutoRestState>,
    Path((table, id)): Path<(String, String)>,
) -> Result<impl IntoResponse, StackhouseError> {
    let result = state.service.get(&table, &id).await?;
    Ok(Json(json!({ "success": true, "data": result })))
}

async fn update_handler(
    State(state): State<AutoRestState>,
    Path((table, id)): Path<(String, String)>,
    Json(data): Json<Value>,
) -> Result<impl IntoResponse, StackhouseError> {
    let result = state.service.update(&table, &id, data).await?;
    Ok(Json(json!({ "success": true, "data": result })))
}

async fn delete_handler(
    State(state): State<AutoRestState>,
    Path((table, id)): Path<(String, String)>,
) -> Result<impl IntoResponse, StackhouseError> {
    state.service.delete(&table, &id).await?;
    Ok(Json(json!({ "success": true, "message": "Deleted" })))
}
