//! # Dashboard UI Backend — Schema Editing, Data Browsing, SQL Editor, Log Viewer
//!
//! Powers the web-based dashboard for schema management, data exploration,
//! SQL queries, and log inspection.

use crate::db::{SqlValue, StackhouseStore};
use crate::error::StackhouseResult;

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::HashMap;
use std::sync::Arc;
use tracing::info;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SchemaEdit {
    pub table_name: String,
    pub operation: SchemaOp,
    pub column_name: String,
    pub column_type: Option<String>,
    pub nullable: Option<bool>,
    pub default: Option<Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SchemaOp {
    AddColumn,
    DropColumn,
    AlterColumn,
    CreateTable,
    CreateIndex,
    DropIndex,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SqlEditorQuery {
    pub tenant_id: i64,
    pub sql: String,
    pub params: Vec<SqlValue>,
    pub save_name: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SavedQuery {
    pub id: String,
    pub tenant_id: i64,
    pub user_id: String,
    pub name: String,
    pub sql: String,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DataBrowserQuery {
    pub tenant_id: i64,
    pub table: String,
    pub filter: Option<String>,
    pub sort: Option<String>,
    pub limit: i64,
    pub offset: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LogViewerQuery {
    pub tenant_id: i64,
    pub service: Option<String>,
    pub level: Option<String>,
    pub message_query: Option<String>,
    pub from: String,
    pub to: String,
    pub limit: usize,
}

#[derive(Clone)]
pub struct DashboardService {
    store: Arc<StackhouseStore>,
}

impl DashboardService {
    pub async fn new(store: Arc<StackhouseStore>) -> StackhouseResult<Self> {
        let service = Self { store };
        service.initialize_tables().await?;
        info!("🎛️ Dashboard service initialized");
        Ok(service)
    }

    async fn initialize_tables(&self) -> StackhouseResult<()> {
        self.store.execute_batch(r#"
            CREATE TABLE IF NOT EXISTS stackhouse_saved_queries (
                id TEXT PRIMARY KEY,
                tenant_id BIGINT NOT NULL,
                user_id TEXT NOT NULL,
                name TEXT NOT NULL,
                sql TEXT NOT NULL,
                created_at TIMESTAMPTZ DEFAULT NOW()
            );
            CREATE INDEX IF NOT EXISTS idx_saved_queries_user ON stackhouse_saved_queries(tenant_id, user_id);
        "#.to_string()).await?;
        Ok(())
    }

    // ===== Schema Editing =====

    pub async fn apply_schema_edit(
        &self,
        tenant_id: i64,
        edit: &SchemaEdit,
    ) -> StackhouseResult<String> {
        let sql = match edit.operation {
            SchemaOp::AddColumn => {
                let col_type = edit.column_type.as_deref().unwrap_or("TEXT");
                let nullable = if edit.nullable.unwrap_or(true) {
                    ""
                } else {
                    "NOT NULL"
                };
                format!(
                    "ALTER TABLE {} ADD COLUMN IF NOT EXISTS {} {} {}",
                    edit.table_name, edit.column_name, col_type, nullable
                )
            }
            SchemaOp::DropColumn => {
                format!(
                    "ALTER TABLE {} DROP COLUMN IF EXISTS {}",
                    edit.table_name, edit.column_name
                )
            }
            SchemaOp::AlterColumn => {
                let col_type = edit.column_type.as_deref().unwrap_or("TEXT");
                format!(
                    "ALTER TABLE {} ALTER COLUMN {} TYPE {}",
                    edit.table_name, edit.column_name, col_type
                )
            }
            SchemaOp::CreateTable => {
                format!("CREATE TABLE IF NOT EXISTS {} (id BIGSERIAL PRIMARY KEY, created_at TIMESTAMPTZ DEFAULT NOW())", edit.table_name)
            }
            SchemaOp::CreateIndex => {
                let index_name = format!("idx_{}_{}", edit.table_name, edit.column_name);
                format!(
                    "CREATE INDEX IF NOT EXISTS {} ON {} ({})",
                    index_name, edit.table_name, edit.column_name
                )
            }
            SchemaOp::DropIndex => {
                let index_name = format!("idx_{}_{}", edit.table_name, edit.column_name);
                format!("DROP INDEX IF EXISTS {}", index_name)
            }
        };

        self.store.execute(sql.clone(), vec![]).await?;
        info!("🔧 Schema edit applied on tenant {}: {}", tenant_id, sql);
        Ok(sql)
    }

    pub async fn get_schema(&self, _tenant_id: i64) -> StackhouseResult<Vec<Value>> {
        let rows = self.store.query(
            "SELECT table_name, column_name, data_type, is_nullable, column_default FROM information_schema.columns WHERE table_schema = 'public' AND table_name LIKE 'stackhouse_%' ORDER BY table_name, ordinal_position".to_string(),
            vec![],
        ).await?;
        Ok(rows
            .into_iter()
            .map(|r| json!(r.iter().cloned().collect::<HashMap<_, _>>()))
            .collect())
    }

    // ===== Data Browsing =====

    pub async fn browse_data(&self, q: &DataBrowserQuery) -> StackhouseResult<Vec<Value>> {
        let mut conditions = vec!["1=1".to_string()];
        let mut params = Vec::new();

        if let Some(filter) = &q.filter {
            for part in filter.split(',') {
                if let Some((field, value)) = part.split_once('=') {
                    conditions.push(format!("{} = ?", field));
                    params.push(SqlValue::Text(value.to_string()));
                }
            }
        }

        let order_clause = q
            .sort
            .as_ref()
            .map(|s| format!(" ORDER BY {}", s))
            .unwrap_or_default();
        let sql = format!(
            "SELECT * FROM {} WHERE {} {} LIMIT {} OFFSET {}",
            q.table,
            conditions.join(" AND "),
            order_clause,
            q.limit,
            q.offset
        );

        let rows = self.store.query(sql, params).await?;
        Ok(rows
            .into_iter()
            .map(|r| json!(r.iter().cloned().collect::<HashMap<_, _>>()))
            .collect())
    }

    pub async fn get_tables(&self) -> StackhouseResult<Vec<String>> {
        let rows = self.store.query(
            "SELECT table_name FROM information_schema.tables WHERE table_schema = 'public' AND table_type = 'BASE TABLE' AND table_name LIKE 'stackhouse_%' ORDER BY table_name".to_string(),
            vec![],
        ).await?;
        Ok(rows
            .into_iter()
            .filter_map(|r| {
                r.iter()
                    .find(|(k, _)| k == "table_name")
                    .and_then(|(_, v)| v.as_str())
                    .map(|s| s.to_string())
            })
            .collect())
    }

    // ===== SQL Editor =====

    pub async fn run_sql(
        &self,
        _tenant_id: i64,
        sql: &str,
        params: Vec<SqlValue>,
    ) -> StackhouseResult<Value> {
        let is_select = sql.trim_start().to_lowercase().starts_with("select");

        if is_select {
            let rows = self.store.query(sql.to_string(), params).await?;
            let data: Vec<HashMap<String, Value>> = rows
                .into_iter()
                .map(|r| r.iter().map(|(k, v)| (k.clone(), v.clone())).collect())
                .collect();
            Ok(json!({"success": true, "data": data, "type": "query"}))
        } else {
            let affected = self.store.execute(sql.to_string(), params).await?;
            Ok(json!({"success": true, "rows_affected": affected, "type": "command"}))
        }
    }

    pub async fn save_query(
        &self,
        tenant_id: i64,
        user_id: &str,
        name: &str,
        sql: &str,
    ) -> StackhouseResult<SavedQuery> {
        let id = uuid::Uuid::new_v4().to_string();
        self.store.execute(
            "INSERT INTO stackhouse_saved_queries (id, tenant_id, user_id, name, sql) VALUES (?, ?, ?, ?, ?)".to_string(),
            vec![
                SqlValue::Text(id.clone()), SqlValue::Integer(tenant_id),
                SqlValue::Text(user_id.to_string()), SqlValue::Text(name.to_string()),
                SqlValue::Text(sql.to_string()),
            ],
        ).await?;

        Ok(SavedQuery {
            id,
            tenant_id,
            user_id: user_id.to_string(),
            name: name.to_string(),
            sql: sql.to_string(),
            created_at: chrono::Utc::now().to_rfc3339(),
        })
    }

    pub async fn list_saved_queries(
        &self,
        tenant_id: i64,
        user_id: &str,
    ) -> StackhouseResult<Vec<Value>> {
        let rows = self.store.query(
            "SELECT id, name, sql, created_at FROM stackhouse_saved_queries WHERE tenant_id = ? AND user_id = ? ORDER BY created_at DESC".to_string(),
            vec![SqlValue::Integer(tenant_id), SqlValue::Text(user_id.to_string())],
        ).await?;
        Ok(rows
            .into_iter()
            .map(|r| json!(r.iter().cloned().collect::<HashMap<_, _>>()))
            .collect())
    }

    // ===== Log Viewer =====

    pub async fn view_logs(&self, q: &LogViewerQuery) -> StackhouseResult<Vec<Value>> {
        let mut conditions = vec!["1=1".to_string()];
        let mut params = Vec::new();

        if let Some(service) = &q.service {
            conditions.push("service = ?".to_string());
            params.push(SqlValue::Text(service.clone()));
        }
        if let Some(level) = &q.level {
            conditions.push("level = ?".to_string());
            params.push(SqlValue::Text(level.clone()));
        }
        if let Some(msg) = &q.message_query {
            conditions.push("message ILIKE ?".to_string());
            params.push(SqlValue::Text(format!("%{}%", msg)));
        }

        let sql = format!(
            "SELECT service, level, message, attributes, trace_id, timestamp FROM stackhouse_structured_logs WHERE {} AND timestamp >= ?::timestamptz AND timestamp <= ?::timestamptz ORDER BY timestamp DESC LIMIT {}",
            conditions.join(" AND "), q.limit
        );

        let rows = self
            .store
            .query(
                sql,
                vec![SqlValue::Text(q.from.clone()), SqlValue::Text(q.to.clone())],
            )
            .await?;
        Ok(rows
            .into_iter()
            .map(|r| json!(r.iter().cloned().collect::<HashMap<_, _>>()))
            .collect())
    }
}
