//! # Database Module (Stackhouse-Store)
//!
//! Manages the persistent connection to PostgreSQL using sqlx.
//! This module handles database initialization, connection management, and provides
//! utilities for executing queries safely while maintaining compatibility with the existing API.

#[path = "../db/full_text_search.rs"]
pub mod full_text_search;
#[path = "../db/schema_migrations.rs"]
pub mod schema_migrations;

pub use full_text_search::*;
pub use schema_migrations::*;

use crate::error::{StackhouseError, StackhouseResult};
use base64::Engine;
use serde::{Deserialize, Serialize};
use sqlx::postgres::{PgPool, PgPoolOptions, PgRow};
use sqlx::{Column, Row, TypeInfo, ValueRef};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use tracing::info;

/// Row data returned from queries
pub type RowData = Vec<(String, serde_json::Value)>;

static TEST_SCHEMA_COUNTER: AtomicU64 = AtomicU64::new(0);

/// Helper to convert sqlite-style ? placeholders to postgres $1, $2
fn convert_question_marks(sql: &str) -> String {
    let mut result = String::with_capacity(sql.len() + 10);
    let mut param_idx = 1;

    for c in sql.chars() {
        if c == '?' {
            result.push('$');
            result.push_str(&param_idx.to_string());
            param_idx += 1;
        } else {
            result.push(c);
        }
    }
    result
}

fn next_test_schema_name() -> String {
    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    let counter = TEST_SCHEMA_COUNTER.fetch_add(1, Ordering::Relaxed);
    format!("stackhouse_test_{timestamp}_{counter}")
}

/// The Stackhouse-Store: manages database connections and provides query utilities
#[derive(Clone)]
pub struct StackhouseStore {
    pool: PgPool,
    url: String,
}

impl StackhouseStore {
    /// Creates a new StackhouseStore with the specified database URL
    pub async fn new(url: &str) -> StackhouseResult<Self> {
        info!("Initializing Stackhouse at: {}", url);

        let pool = Self::connect_pool(url, None, 20).await?;

        info!("✨ Stackhouse initialized successfully with PostgreSQL");

        Ok(Self {
            pool,
            url: url.to_string(),
        })
    }

    /// Creates a connection to a local test database for tests
    pub async fn in_memory() -> StackhouseResult<Self> {
        let url = std::env::var("STACKHOUSE_TEST_URL").unwrap_or_else(|_| {
            "postgres://postgres:postgres@localhost:5432/stackhouse_test".to_string()
        });

        let schema_name = next_test_schema_name();
        Self::create_test_schema(&url, &schema_name).await?;

        let pool = Self::connect_pool(&url, Some(schema_name.as_str()), 20).await?;
        info!("Using isolated test schema: {}", schema_name);

        Ok(Self { pool, url })
    }

    async fn connect_pool(
        url: &str,
        search_path: Option<&str>,
        max_connections: u32,
    ) -> StackhouseResult<PgPool> {
        let mut options = PgPoolOptions::new()
            .max_connections(max_connections)
            .acquire_timeout(Duration::from_secs(3));

        if let Some(schema_name) = search_path {
            let search_path_sql = format!("SET search_path TO {schema_name}, public");
            options = options.after_connect(move |conn, _meta| {
                let search_path_sql = search_path_sql.clone();
                Box::pin(async move {
                    sqlx::query(&search_path_sql).execute(conn).await?;
                    Ok(())
                })
            });
        }

        options
            .connect(url)
            .await
            .map_err(|e| StackhouseError::Database(format!("Failed to connect to database: {}", e)))
    }

    async fn create_test_schema(url: &str, schema_name: &str) -> StackhouseResult<()> {
        let pool = PgPoolOptions::new()
            .max_connections(1)
            .acquire_timeout(Duration::from_secs(3))
            .connect(url)
            .await
            .map_err(|e| {
                StackhouseError::Database(format!("Failed to connect to database: {}", e))
            })?;

        let create_schema_sql = format!("CREATE SCHEMA IF NOT EXISTS {schema_name}");
        sqlx::query(&create_schema_sql)
            .execute(&pool)
            .await
            .map_err(|e| {
                StackhouseError::Database(format!("Failed to create test schema: {}", e))
            })?;

        pool.close().await;
        Ok(())
    }

    /// Get the connection pool
    pub fn pool(&self) -> &PgPool {
        &self.pool
    }

    /// Execute a write query (INSERT, UPDATE, DELETE, ALTER)
    pub async fn execute(&self, sql: String, params: Vec<SqlValue>) -> StackhouseResult<u64> {
        let sql = convert_question_marks(&sql);
        let mut query = sqlx::query(&sql);
        for param in params {
            query = param.bind_to_query(query);
        }

        let result = query
            .execute(&self.pool)
            .await
            .map_err(|e| StackhouseError::Database(format!("Execute failed: {}", e)))?;

        Ok(result.rows_affected())
    }

    /// Execute an INSERT and return the generated ID
    pub async fn insert_returning_id(
        &self,
        sql: String,
        params: Vec<SqlValue>,
    ) -> StackhouseResult<i64> {
        let mut sql = convert_question_marks(&sql);
        let has_returning_clause = sql
            .to_uppercase()
            .split_whitespace()
            .any(|token| token == "RETURNING");
        if !has_returning_clause {
            sql.push_str(" RETURNING id");
        }

        let mut query = sqlx::query(&sql);
        for param in params {
            query = param.bind_to_query(query);
        }

        let row = query
            .fetch_one(&self.pool)
            .await
            .map_err(|e| StackhouseError::Database(format!("Insert failed: {}", e)))?;

        let id = row
            .try_get::<i64, _>("id")
            .or_else(|_| row.try_get::<i32, _>("id").map(i64::from))
            .map_err(|e| StackhouseError::Database(format!("Failed to get returning id: {}", e)))?;

        Ok(id)
    }

    /// Execute a simple query without parameters
    pub async fn execute_simple(&self, sql: String) -> StackhouseResult<u64> {
        let result = sqlx::query(&sql)
            .execute(&self.pool)
            .await
            .map_err(|e| StackhouseError::Database(format!("Execute failed: {}", e)))?;

        Ok(result.rows_affected())
    }

    /// Execute batch SQL
    pub async fn execute_batch(&self, sql: String) -> StackhouseResult<()> {
        for statement in sql.split(';') {
            let stmt = statement.trim();
            if !stmt.is_empty() {
                sqlx::query(stmt).execute(&self.pool).await.map_err(|e| {
                    StackhouseError::Database(format!("Batch execution failed: {}", e))
                })?;
            }
        }
        Ok(())
    }

    /// Query and return rows as JSON-like structure
    pub async fn query(
        &self,
        sql: String,
        params: Vec<SqlValue>,
    ) -> StackhouseResult<Vec<Vec<(String, serde_json::Value)>>> {
        let sql = convert_question_marks(&sql);
        let mut query = sqlx::query(&sql);
        for param in params {
            query = param.bind_to_query(query);
        }

        let rows = query
            .fetch_all(&self.pool)
            .await
            .map_err(|e| StackhouseError::Database(format!("Query failed: {}", e)))?;

        let mut rows_result = Vec::new();
        for row in rows {
            let mut row_data = Vec::new();
            for col in row.columns() {
                let name = col.name().to_string();
                let value = Self::get_value_from_row(&row, col.ordinal());
                row_data.push((name, value));
            }
            rows_result.push(row_data);
        }

        Ok(rows_result)
    }

    /// Query without parameters
    pub async fn query_simple(
        &self,
        sql: String,
    ) -> StackhouseResult<Vec<Vec<(String, serde_json::Value)>>> {
        self.query(sql, vec![]).await
    }

    /// Helper to extract value from a Postgres row
    fn get_value_from_row(row: &PgRow, idx: usize) -> serde_json::Value {
        let col = row.column(idx);
        let type_name = col.type_info().name();

        if let Ok(value) = row.try_get_raw(idx) {
            if value.is_null() {
                return serde_json::Value::Null;
            }
        } else {
            return serde_json::Value::Null;
        }

        match type_name {
            "INT4" | "INT8" | "INT2" => {
                if let Ok(v) = row.try_get::<i64, _>(idx) {
                    serde_json::json!(v)
                } else if let Ok(v) = row.try_get::<i32, _>(idx) {
                    serde_json::json!(v)
                } else if let Ok(v) = row.try_get::<i16, _>(idx) {
                    serde_json::json!(v)
                } else {
                    serde_json::Value::Null
                }
            }
            "FLOAT4" | "FLOAT8" | "NUMERIC" => {
                if let Ok(v) = row.try_get::<f64, _>(idx) {
                    serde_json::json!(v)
                } else if let Ok(v) = row.try_get::<f32, _>(idx) {
                    serde_json::json!(v)
                } else {
                    serde_json::Value::Null
                }
            }
            "BOOL" => {
                if let Ok(v) = row.try_get::<bool, _>(idx) {
                    serde_json::json!(v)
                } else {
                    serde_json::Value::Null
                }
            }
            "VARCHAR" | "TEXT" | "BPCHAR" | "PG_LSN" => {
                if let Ok(v) = row.try_get::<String, _>(idx) {
                    // Try to parse as JSON if it looks like JSON
                    if v.starts_with('{') || v.starts_with('[') {
                        if let Ok(parsed) = serde_json::from_str::<serde_json::Value>(&v) {
                            return parsed;
                        }
                    }
                    serde_json::json!(v)
                } else {
                    serde_json::Value::Null
                }
            }
            "JSON" | "JSONB" => {
                if let Ok(v) = row.try_get::<serde_json::Value, _>(idx) {
                    v
                } else {
                    serde_json::Value::Null
                }
            }
            "BYTEA" => {
                if let Ok(v) = row.try_get::<Vec<u8>, _>(idx) {
                    serde_json::json!(base64::engine::general_purpose::STANDARD.encode(&v))
                } else {
                    serde_json::Value::Null
                }
            }
            "TIMESTAMP" | "TIMESTAMPTZ" | "DATE" | "TIME" => {
                // Try to get as chrono::DateTime and format
                if let Ok(v) = row.try_get::<chrono::DateTime<chrono::Utc>, _>(idx) {
                    serde_json::json!(v.to_rfc3339())
                } else if let Ok(v) = row.try_get::<chrono::NaiveDateTime, _>(idx) {
                    serde_json::json!(v.to_string())
                } else {
                    serde_json::Value::Null
                }
            }
            "UUID" => {
                if let Ok(v) = row.try_get::<uuid::Uuid, _>(idx) {
                    serde_json::json!(v.to_string())
                } else {
                    serde_json::Value::Null
                }
            }
            _ => {
                // Fallback to string representation if possible
                if let Ok(v) = row.try_get::<String, _>(idx) {
                    serde_json::json!(v)
                } else {
                    serde_json::Value::Null
                }
            }
        }
    }

    /// Get the database connection string
    pub fn path(&self) -> &str {
        &self.url
    }

    /// Check if database is in test mode
    pub fn is_in_memory(&self) -> bool {
        self.url.contains("stackhouse_test")
    }

    /// Get all table names in the database
    pub async fn list_tables(&self) -> StackhouseResult<Vec<String>> {
        let query_str = "
            SELECT table_name
            FROM information_schema.tables
            WHERE table_schema = current_schema()
        ";
        let rows = self.query_simple(query_str.to_string()).await?;

        let tables: Vec<String> = rows
            .iter()
            .filter_map(|row| {
                row.first()
                    .and_then(|(_, v)| v.as_str().map(|s| s.to_string()))
            })
            .collect();

        Ok(tables)
    }

    /// High-level Insert: Insert a JSON document into a table.
    /// Note: This is a convenience method that builds a simple INSERT statement.
    /// For full schema evolution, use the API layer's SchemaGuard.
    pub async fn insert(&self, table: &str, data: serde_json::Value) -> StackhouseResult<u64> {
        let obj = data.as_object().ok_or_else(|| {
            StackhouseError::InvalidPayload("Data must be a JSON object".to_string())
        })?;

        let columns: Vec<String> = obj.keys().cloned().collect();
        let placeholders: Vec<String> = (1..=columns.len()).map(|i| format!("${}", i)).collect();

        let sql = format!(
            "INSERT INTO {} ({}) VALUES ({})",
            table,
            columns.join(", "),
            placeholders.join(", ")
        );

        let params: Vec<SqlValue> = columns
            .iter()
            .map(|col| json_to_sql_value(obj.get(col).unwrap_or(&serde_json::Value::Null)))
            .collect();

        self.execute(sql, params).await
    }

    /// High-level Scan: Fetch all documents from a table as a Vec of JSON Values.
    pub async fn scan(&self, table: &str) -> StackhouseResult<Vec<serde_json::Value>> {
        let sql = format!("SELECT * FROM {}", table);
        let rows = self.query_simple(sql).await?;

        let mut results = Vec::new();
        for row in rows {
            let mut obj = serde_json::Map::new();
            for (key, value) in row {
                obj.insert(key, value);
            }
            results.push(serde_json::Value::Object(obj));
        }

        Ok(results)
    }

    /// High-level Delete: Remove a document by its internal numeric ID.
    pub async fn delete(&self, table: &str, id: u64) -> StackhouseResult<u64> {
        let sql = format!("DELETE FROM {} WHERE id = $1", table);
        self.execute(sql, vec![SqlValue::Integer(id as i64)]).await
    }
}

/// SQL Value wrapper for parameters
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum SqlValue {
    Null,
    Integer(i64),
    Real(f64),
    Text(String),
    Blob(Vec<u8>),
    Boolean(bool),
    Json(serde_json::Value),
}

impl SqlValue {
    pub fn bind_to_query<'q>(
        self,
        query: sqlx::query::Query<'q, sqlx::Postgres, sqlx::postgres::PgArguments>,
    ) -> sqlx::query::Query<'q, sqlx::Postgres, sqlx::postgres::PgArguments> {
        match self {
            SqlValue::Null => query.bind(None::<String>),
            SqlValue::Integer(i) => query.bind(i),
            SqlValue::Real(f) => query.bind(f),
            SqlValue::Text(t) => query.bind(t),
            SqlValue::Blob(b) => query.bind(b),
            SqlValue::Boolean(b) => query.bind(b),
            SqlValue::Json(j) => query.bind(j),
        }
    }
}

/// Convert a JSON value to a `SqlValue` suitable for inserting into a column
/// of the given `PgType`. This is required for schema-lateral writes because
/// a column may have been widened to JSONB, and a bare string/number/boolean
/// parameter cannot be assigned to a JSONB column without an explicit cast.
pub fn json_to_sql_value_for_type(
    value: &serde_json::Value,
    pg_type: &crate::inference::PgType,
) -> SqlValue {
    use crate::inference::PgType;

    if matches!(value, serde_json::Value::Null) {
        return SqlValue::Null;
    }

    match (value, pg_type) {
        (_, PgType::Jsonb) => SqlValue::Json(value.clone()),
        (serde_json::Value::Bool(b), PgType::Boolean) => SqlValue::Boolean(*b),
        (serde_json::Value::Number(n), PgType::BigInt) => {
            if let Some(i) = n.as_i64() {
                SqlValue::Integer(i)
            } else {
                SqlValue::Text(n.to_string())
            }
        }
        (serde_json::Value::Number(n), PgType::DoublePrecision) => {
            if let Some(f) = n.as_f64() {
                SqlValue::Real(f)
            } else {
                SqlValue::Text(n.to_string())
            }
        }
        (serde_json::Value::String(s), PgType::Text)
        | (serde_json::Value::String(s), PgType::Uuid)
        | (serde_json::Value::String(s), PgType::Date)
        | (serde_json::Value::String(s), PgType::TimestampTz) => SqlValue::Text(s.clone()),
        // Fallback: stringify for text-like targets, otherwise wrap as JSON.
        _ => SqlValue::Text(value.to_string()),
    }
}

/// Convert JSON value to SqlValue
pub fn json_to_sql_value(value: &serde_json::Value) -> SqlValue {
    match value {
        serde_json::Value::Null => SqlValue::Null,
        serde_json::Value::Bool(b) => SqlValue::Boolean(*b),
        serde_json::Value::Number(n) => {
            if let Some(i) = n.as_i64() {
                SqlValue::Integer(i)
            } else if let Some(f) = n.as_f64() {
                SqlValue::Real(f)
            } else {
                SqlValue::Text(n.to_string())
            }
        }
        serde_json::Value::String(s) => SqlValue::Text(s.clone()),
        serde_json::Value::Array(_) | serde_json::Value::Object(_) => SqlValue::Json(value.clone()),
    }
}

#[cfg(test)]
mod tests {
    use super::{SqlValue, StackhouseStore};
    use std::time::{SystemTime, UNIX_EPOCH};

    #[tokio::test]
    async fn test_in_memory_stores_are_isolated() {
        let store_a = StackhouseStore::in_memory().await.unwrap();
        let store_b = StackhouseStore::in_memory().await.unwrap();

        let suffix = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let table_name = format!("test_scope_{suffix}");
        let create_sql = format!(
            "CREATE TABLE IF NOT EXISTS {table_name} (id SERIAL PRIMARY KEY, email TEXT UNIQUE NOT NULL)"
        );
        let insert_sql = format!("INSERT INTO {table_name} (email) VALUES (?)");

        store_a.execute_batch(create_sql.clone()).await.unwrap();
        store_b.execute_batch(create_sql).await.unwrap();

        store_a
            .execute(
                insert_sql.clone(),
                vec![SqlValue::Text("same@example.com".to_string())],
            )
            .await
            .unwrap();

        store_b
            .execute(
                insert_sql,
                vec![SqlValue::Text("same@example.com".to_string())],
            )
            .await
            .unwrap();
    }

    #[tokio::test]
    async fn test_insert_returning_id_supports_serial_columns() {
        let store = StackhouseStore::in_memory().await.unwrap();
        let suffix = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let table_name = format!("test_serial_{suffix}");

        store
            .execute_batch(format!(
                "CREATE TABLE {table_name} (id SERIAL PRIMARY KEY, email TEXT UNIQUE NOT NULL)"
            ))
            .await
            .unwrap();

        let id = store
            .insert_returning_id(
                format!("INSERT INTO {table_name} (email) VALUES (?)"),
                vec![SqlValue::Text("serial@example.com".to_string())],
            )
            .await
            .unwrap();

        assert_eq!(id, 1);
    }

    #[tokio::test]
    async fn test_insert_returning_id_ignores_returning_in_identifiers() {
        let store = StackhouseStore::in_memory().await.unwrap();
        let suffix = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let table_name = format!("test_returning_{suffix}");

        store
            .execute_batch(format!(
                "CREATE TABLE {table_name} (id SERIAL PRIMARY KEY, email TEXT UNIQUE NOT NULL)"
            ))
            .await
            .unwrap();

        let id = store
            .insert_returning_id(
                format!("INSERT INTO {table_name} (email) VALUES (?)"),
                vec![SqlValue::Text("identifier@example.com".to_string())],
            )
            .await
            .unwrap();

        assert_eq!(id, 1);
    }
}
