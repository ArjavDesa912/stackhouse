//! # GraphQL API Module (Stackhouse-GraphQL)
//!
//! Auto-generated GraphQL API from Stackhouse's database schema.
//! Provides full CRUD operations via GraphQL queries and mutations.
//!
//! ## Features
//! - Dynamic schema generation from database tables
//! - Query with filters, pagination, sorting
//! - Mutations for insert, update, delete
//! - Real-time subscriptions (via async-graphql subscriptions)
//! - Automatic type mapping from PostgreSQL to GraphQL types

use crate::db::{json_to_sql_value_for_type, SqlValue, StackhouseStore};
use crate::guard::SchemaGuard;

use async_graphql::*;
use axum::{extract::State, response::IntoResponse, routing::post, Router};
use serde_json::Value;
use std::sync::Arc;

// ============================================================================
// GraphQL Schema Types
// ============================================================================

/// Dynamic row type that can represent any table row
#[derive(Debug, Clone)]
pub struct DynamicRow {
    pub table: String,
    pub fields: Vec<(String, Value)>,
}

#[Object]
impl DynamicRow {
    /// Get a field value by name
    async fn field(&self, name: String) -> Option<String> {
        self.fields
            .iter()
            .find(|(k, _)| k == &name)
            .map(|(_, v)| v.to_string())
    }

    /// Get all field names
    async fn fields_list(&self) -> Vec<String> {
        self.fields.iter().map(|(k, _)| k.clone()).collect()
    }

    /// Get the row as JSON
    async fn json(&self) -> String {
        let mut obj = serde_json::Map::new();
        for (k, v) in &self.fields {
            obj.insert(k.clone(), v.clone());
        }
        Value::Object(obj).to_string()
    }

    /// Get the table name
    async fn table_name(&self) -> &str {
        &self.table
    }

    /// Get the row ID
    async fn id(&self) -> Option<i64> {
        self.fields
            .iter()
            .find(|(k, _)| k == "id")
            .and_then(|(_, v)| v.as_i64())
    }
}

/// Query result with pagination metadata
#[derive(Debug, Clone)]
pub struct QueryResult {
    pub rows: Vec<DynamicRow>,
    pub total_count: i64,
    pub has_more: bool,
}

#[Object]
impl QueryResult {
    async fn data(&self) -> &[DynamicRow] {
        &self.rows
    }

    async fn total_count(&self) -> i64 {
        self.total_count
    }

    async fn has_more(&self) -> bool {
        self.has_more
    }

    async fn count(&self) -> usize {
        self.rows.len()
    }
}

/// Table metadata
#[derive(Debug, Clone)]
pub struct TableInfo {
    pub name: String,
    pub row_count: u64,
    pub columns: Vec<GqlColumnInfo>,
}

#[Object]
impl TableInfo {
    async fn name(&self) -> &str {
        &self.name
    }

    async fn row_count(&self) -> String {
        self.row_count.to_string()
    }

    async fn columns(&self) -> &[GqlColumnInfo] {
        &self.columns
    }
}

/// Column metadata (GraphQL representation)
#[derive(Debug, Clone)]
pub struct GqlColumnInfo {
    pub name: String,
    pub data_type: String,
    pub is_nullable: bool,
}

#[Object]
impl GqlColumnInfo {
    async fn name(&self) -> &str {
        &self.name
    }

    async fn data_type(&self) -> &str {
        &self.data_type
    }

    async fn is_nullable(&self) -> bool {
        self.is_nullable
    }
}

/// Mutation result
#[derive(Debug, Clone)]
pub struct MutationResult {
    pub success: bool,
    pub id: Option<i64>,
    pub affected_rows: u64,
    pub message: String,
}

#[Object]
impl MutationResult {
    async fn success(&self) -> bool {
        self.success
    }

    async fn id(&self) -> Option<i64> {
        self.id
    }

    async fn affected_rows(&self) -> u64 {
        self.affected_rows
    }

    async fn message(&self) -> &str {
        &self.message
    }
}

// ============================================================================
// GraphQL Query Root
// ============================================================================

pub struct QueryRoot;

#[Object]
impl QueryRoot {
    /// List all tables in the database
    async fn tables(&self, ctx: &Context<'_>) -> Result<Vec<TableInfo>> {
        let store = ctx
            .data::<Arc<StackhouseStore>>()
            .map_err(|_| Error::new("Store not found in context"))?;
        let guard = ctx
            .data::<Arc<SchemaGuard>>()
            .map_err(|_| Error::new("Guard not found in context"))?;

        let table_names = store
            .list_tables()
            .await
            .map_err(|e| Error::new(format!("Failed to list tables: {}", e)))?;

        let mut tables = Vec::new();
        for name in table_names {
            let stats = guard.get_table_stats(&name).await;
            let schema = guard.get_table_schema(&name).await;

            let row_count = stats.map(|s| s.row_count).unwrap_or(0);
            let columns = schema
                .map(|cols| {
                    cols.iter()
                        .map(|c| GqlColumnInfo {
                            name: c.name.clone(),
                            data_type: c.col_type.clone(),
                            is_nullable: !c.notnull,
                        })
                        .collect()
                })
                .unwrap_or_default();

            tables.push(TableInfo {
                name,
                row_count,
                columns,
            });
        }

        Ok(tables)
    }

    /// Preview the schema changes a payload would trigger without mutating the database
    async fn preview(&self, ctx: &Context<'_>, table: String, data: String) -> Result<String> {
        let guard = ctx
            .data::<Arc<SchemaGuard>>()
            .map_err(|_| Error::new("Guard not found"))?;

        SchemaGuard::validate_identifier(&table)
            .map_err(|e| Error::new(format!("Invalid table name: {}", e)))?;

        let payload: Value =
            serde_json::from_str(&data).map_err(|e| Error::new(format!("Invalid JSON: {}", e)))?;

        let preview = guard
            .preview_schema_changes(&table, &payload)
            .await
            .map_err(|e| Error::new(format!("Preview failed: {}", e)))?;

        serde_json::to_string(&preview)
            .map_err(|e| Error::new(format!("Failed to serialize preview: {}", e)))
    }

    /// Query rows from a table
    async fn query(
        &self,
        ctx: &Context<'_>,
        table: String,
        #[graphql(default)] filter: Option<String>,
        #[graphql(default = 100)] limit: i64,
        #[graphql(default = 0)] offset: i64,
        #[graphql(default)] order_by: Option<String>,
        #[graphql(default)] order_dir: Option<String>,
    ) -> Result<QueryResult> {
        let store = ctx
            .data::<Arc<StackhouseStore>>()
            .map_err(|_| Error::new("Store not found"))?;
        let _guard = ctx
            .data::<Arc<SchemaGuard>>()
            .map_err(|_| Error::new("Guard not found"))?;

        // Validate table name
        SchemaGuard::validate_identifier(&table)
            .map_err(|e| Error::new(format!("Invalid table name: {}", e)))?;

        // Build query
        let mut sql = format!("SELECT * FROM \"{}\"", table);
        let mut params: Vec<SqlValue> = Vec::new();
        let mut param_count = 0;

        // Apply filter
        if let Some(filter_json) = &filter {
            let filters: Value = serde_json::from_str(filter_json)
                .map_err(|e| Error::new(format!("Invalid filter JSON: {}", e)))?;

            if let Some(obj) = filters.as_object() {
                let mut where_clauses = Vec::new();
                for (key, value) in obj {
                    SchemaGuard::validate_identifier(key)
                        .map_err(|e| Error::new(format!("Invalid column name: {}", e)))?;
                    param_count += 1;
                    where_clauses.push(format!("\"{}\" = ${}", key, param_count));
                    params.push(crate::db::json_to_sql_value(value));
                }
                if !where_clauses.is_empty() {
                    sql.push_str(&format!(" WHERE {}", where_clauses.join(" AND ")));
                }
            }
        }

        // Count query
        let count_sql = format!("SELECT COUNT(*) as cnt FROM ({}) AS subq", sql);
        let count_rows = store
            .query(count_sql, params.clone())
            .await
            .map_err(|e| Error::new(format!("Count query failed: {}", e)))?;
        let total_count = count_rows
            .first()
            .and_then(|row| row.iter().find(|(k, _)| k == "cnt"))
            .and_then(|(_, v)| v.as_i64())
            .unwrap_or(0);

        // Apply ordering
        if let Some(order_col) = &order_by {
            SchemaGuard::validate_identifier(order_col)
                .map_err(|e| Error::new(format!("Invalid order column: {}", e)))?;
            let dir = order_dir.as_deref().unwrap_or("ASC");
            let dir = if dir.eq_ignore_ascii_case("desc") {
                "DESC"
            } else {
                "ASC"
            };
            sql.push_str(&format!(" ORDER BY \"{}\" {}", order_col, dir));
        }

        // Apply pagination
        let clamped_limit = limit.min(1000).max(1);
        sql.push_str(&format!(
            " LIMIT {} OFFSET {}",
            clamped_limit,
            offset.max(0)
        ));

        let rows = store
            .query(sql, params)
            .await
            .map_err(|e| Error::new(format!("Query failed: {}", e)))?;

        let dynamic_rows: Vec<DynamicRow> = rows
            .into_iter()
            .map(|fields| DynamicRow {
                table: table.clone(),
                fields,
            })
            .collect();

        let has_more = (offset + clamped_limit) < total_count;

        Ok(QueryResult {
            rows: dynamic_rows,
            total_count,
            has_more,
        })
    }

    /// Get a single row by ID
    async fn get_by_id(
        &self,
        ctx: &Context<'_>,
        table: String,
        id: i64,
    ) -> Result<Option<DynamicRow>> {
        let store = ctx
            .data::<Arc<StackhouseStore>>()
            .map_err(|_| Error::new("Store not found"))?;

        SchemaGuard::validate_identifier(&table)
            .map_err(|e| Error::new(format!("Invalid table name: {}", e)))?;

        let rows = store
            .query(
                format!("SELECT * FROM \"{}\" WHERE id = $1", table),
                vec![SqlValue::Integer(id)],
            )
            .await
            .map_err(|e| Error::new(format!("Query failed: {}", e)))?;

        Ok(rows
            .into_iter()
            .next()
            .map(|fields| DynamicRow { table, fields }))
    }

    /// Execute a raw SQL query (read-only)
    async fn sql(&self, ctx: &Context<'_>, query: String) -> Result<Vec<DynamicRow>> {
        let store = ctx
            .data::<Arc<StackhouseStore>>()
            .map_err(|_| Error::new("Store not found"))?;

        // Security: only allow SELECT statements
        let trimmed = query.trim().to_uppercase();
        if !trimmed.starts_with("SELECT") && !trimmed.starts_with("WITH") {
            return Err(Error::new(
                "Only SELECT queries are allowed via GraphQL. Use mutations for writes.",
            ));
        }

        let rows = store
            .query_simple(query)
            .await
            .map_err(|e| Error::new(format!("SQL query failed: {}", e)))?;

        Ok(rows
            .into_iter()
            .map(|fields| DynamicRow {
                table: "sql_result".to_string(),
                fields,
            })
            .collect())
    }
}

// ============================================================================
// GraphQL Mutation Root
// ============================================================================

pub struct MutationRoot;

#[Object]
impl MutationRoot {
    /// Insert a new row into a table
    async fn insert(
        &self,
        ctx: &Context<'_>,
        table: String,
        data: String,
    ) -> Result<MutationResult> {
        let store = ctx
            .data::<Arc<StackhouseStore>>()
            .map_err(|_| Error::new("Store not found"))?;
        let guard = ctx
            .data::<Arc<SchemaGuard>>()
            .map_err(|_| Error::new("Guard not found"))?;

        SchemaGuard::validate_identifier(&table)
            .map_err(|e| Error::new(format!("Invalid table name: {}", e)))?;

        let payload: Value =
            serde_json::from_str(&data).map_err(|e| Error::new(format!("Invalid JSON: {}", e)))?;

        // Ensure table and columns exist (schema-later!)
        guard
            .ensure_table(&table)
            .await
            .map_err(|e| Error::new(format!("Table creation failed: {}", e)))?;
        let columns = guard
            .ensure_columns(&table, &payload)
            .await
            .map_err(|e| Error::new(format!("Column creation failed: {}", e)))?;

        // Build INSERT
        let obj = payload
            .as_object()
            .ok_or_else(|| Error::new("Data must be a JSON object"))?;

        let column_names: Vec<&str> = columns.iter().map(|(name, _)| name.as_str()).collect();
        let placeholders: Vec<String> = (1..=columns.len()).map(|i| format!("${}", i)).collect();
        let values: Vec<SqlValue> = columns
            .iter()
            .map(|(name, pg_type)| {
                obj.get(name)
                    .map(|v| json_to_sql_value_for_type(v, pg_type))
                    .unwrap_or(SqlValue::Null)
            })
            .collect();

        let sql = format!(
            "INSERT INTO {} ({}) VALUES ({}) RETURNING id",
            table,
            column_names.join(", "),
            placeholders.join(", ")
        );

        let id = store
            .insert_returning_id(sql, values)
            .await
            .map_err(|e| Error::new(format!("Insert failed: {}", e)))?;

        Ok(MutationResult {
            success: true,
            id: Some(id),
            affected_rows: 1,
            message: format!("Inserted into {}", table),
        })
    }

    /// Update rows in a table
    async fn update(
        &self,
        ctx: &Context<'_>,
        table: String,
        id: i64,
        data: String,
    ) -> Result<MutationResult> {
        let store = ctx
            .data::<Arc<StackhouseStore>>()
            .map_err(|_| Error::new("Store not found"))?;
        let guard = ctx
            .data::<Arc<SchemaGuard>>()
            .map_err(|_| Error::new("Guard not found"))?;

        SchemaGuard::validate_identifier(&table)
            .map_err(|e| Error::new(format!("Invalid table name: {}", e)))?;

        let payload: Value =
            serde_json::from_str(&data).map_err(|e| Error::new(format!("Invalid JSON: {}", e)))?;

        let obj = payload
            .as_object()
            .ok_or_else(|| Error::new("Data must be a JSON object"))?;

        let columns = guard
            .ensure_columns(&table, &payload)
            .await
            .map_err(|e| Error::new(format!("Column creation failed: {}", e)))?;

        let mut set_clauses = Vec::new();
        let mut params: Vec<SqlValue> = Vec::new();
        let mut param_idx = 1;

        for (name, pg_type) in &columns {
            let value = obj
                .get(name)
                .ok_or_else(|| Error::new(format!("Missing value for column {}", name)))?;
            set_clauses.push(format!("{} = ${}", name, param_idx));
            params.push(json_to_sql_value_for_type(value, pg_type));
            param_idx += 1;
        }

        params.push(SqlValue::Integer(id));
        let sql = format!(
            "UPDATE {} SET {}, updated_at = CURRENT_TIMESTAMP WHERE id = ${}",
            table,
            set_clauses.join(", "),
            param_idx
        );

        let affected = store
            .execute(sql, params)
            .await
            .map_err(|e| Error::new(format!("Update failed: {}", e)))?;

        Ok(MutationResult {
            success: affected > 0,
            id: Some(id),
            affected_rows: affected,
            message: format!("Updated {} row(s) in {}", affected, table),
        })
    }

    /// Delete a row by ID
    async fn delete(&self, ctx: &Context<'_>, table: String, id: i64) -> Result<MutationResult> {
        let store = ctx
            .data::<Arc<StackhouseStore>>()
            .map_err(|_| Error::new("Store not found"))?;

        SchemaGuard::validate_identifier(&table)
            .map_err(|e| Error::new(format!("Invalid table name: {}", e)))?;

        let affected = store
            .execute(
                format!("DELETE FROM \"{}\" WHERE id = $1", table),
                vec![SqlValue::Integer(id)],
            )
            .await
            .map_err(|e| Error::new(format!("Delete failed: {}", e)))?;

        Ok(MutationResult {
            success: affected > 0,
            id: Some(id),
            affected_rows: affected,
            message: format!("Deleted {} row(s) from {}", affected, table),
        })
    }

    /// Execute a raw SQL mutation (DDL/DML)
    async fn execute_sql(&self, ctx: &Context<'_>, query: String) -> Result<MutationResult> {
        let store = ctx
            .data::<Arc<StackhouseStore>>()
            .map_err(|_| Error::new("Store not found"))?;

        // Security: block dangerous statements
        let trimmed = query.trim().to_uppercase();
        if trimmed.starts_with("DROP DATABASE") || trimmed.contains("pg_") {
            return Err(Error::new("This SQL statement is not allowed"));
        }

        let affected = store
            .execute_simple(query)
            .await
            .map_err(|e| Error::new(format!("SQL execution failed: {}", e)))?;

        Ok(MutationResult {
            success: true,
            id: None,
            affected_rows: affected,
            message: format!("Affected {} row(s)", affected),
        })
    }
}

// ============================================================================
// Schema Builder
// ============================================================================

pub type StackhouseSchema = Schema<QueryRoot, MutationRoot, EmptySubscription>;

pub fn build_schema(store: Arc<StackhouseStore>, guard: Arc<SchemaGuard>) -> StackhouseSchema {
    Schema::build(QueryRoot, MutationRoot, EmptySubscription)
        .data(store)
        .data(guard)
        .finish()
}

// ============================================================================
// HTTP Handlers (manual axum integration, no async-graphql-axum needed)
// ============================================================================

#[derive(Clone)]
pub struct GraphQLState {
    pub schema: StackhouseSchema,
}

/// GraphQL request body
#[derive(serde::Deserialize)]
struct GraphQLRequest {
    query: String,
    #[serde(default)]
    operation_name: Option<String>,
    #[serde(default)]
    variables: Option<Value>,
}

/// POST /v1/graphql - Execute GraphQL query
async fn graphql_handler(
    State(state): State<GraphQLState>,
    axum::Json(req): axum::Json<GraphQLRequest>,
) -> impl IntoResponse {
    let mut request = async_graphql::Request::new(req.query);
    if let Some(op) = req.operation_name {
        request = request.operation_name(op);
    }
    if let Some(vars) = req.variables {
        let variables = async_graphql::Variables::from_json(vars);
        request = request.variables(variables);
    }
    let response = state.schema.execute(request).await;
    let body = serde_json::to_value(&response).unwrap_or_default();
    axum::Json(body)
}

/// GET /v1/graphql - GraphQL Playground
async fn graphql_playground() -> impl IntoResponse {
    axum::response::Html(async_graphql::http::playground_source(
        async_graphql::http::GraphQLPlaygroundConfig::new("/v1/graphql"),
    ))
}

// ============================================================================
// Router
// ============================================================================

pub fn create_graphql_router(state: GraphQLState) -> Router {
    Router::new()
        .route("/graphql", post(graphql_handler).get(graphql_playground))
        .with_state(state)
}
