//! # Data Processing Layer
//!
//! Provides named, queryable datasets that sit on top of raw tables.
//! A dataset is a saved SQL query plus metadata (name, description,
//! projected columns). Worksheets consume datasets instead of raw tables.

use crate::db::{SqlValue, StackhouseStore};
use crate::error::{StackhouseError, StackhouseResult};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::sync::Arc;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DatasetField {
    pub name: String,
    #[serde(rename = "type")]
    pub data_type: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Dataset {
    pub id: String,
    pub name: String,
    pub description: String,
    pub source_sql: String,
    pub fields: Vec<DatasetField>,
    pub tenant_id: i64,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateDatasetRequest {
    pub name: String,
    pub description: Option<String>,
    pub source_sql: String,
    pub fields: Vec<DatasetField>,
}

#[derive(Clone)]
pub struct DatasetService {
    store: Arc<StackhouseStore>,
}

impl DatasetService {
    pub fn new(store: Arc<StackhouseStore>) -> Self {
        Self { store }
    }

    async fn ensure_table(&self) -> StackhouseResult<()> {
        self.store
            .execute_batch(
                r#"
                CREATE TABLE IF NOT EXISTS vibe_datasets (
                    id TEXT PRIMARY KEY,
                    name TEXT NOT NULL,
                    description TEXT NOT NULL DEFAULT '',
                    source_sql TEXT NOT NULL,
                    fields JSONB NOT NULL DEFAULT '[]'::jsonb,
                    tenant_id BIGINT NOT NULL DEFAULT 0,
                    created_at TIMESTAMPTZ NOT NULL DEFAULT now()
                );
                CREATE INDEX IF NOT EXISTS idx_vibe_datasets_tenant ON vibe_datasets(tenant_id);
                "#
                .to_string(),
            )
            .await
    }

    pub async fn list(&self, tenant_id: i64) -> StackhouseResult<Vec<Dataset>> {
        self.ensure_table().await?;
        let rows = self
            .store
            .query(
                "SELECT id, name, description, source_sql, fields, tenant_id, created_at::TEXT FROM vibe_datasets WHERE tenant_id = $1 ORDER BY name"
                    .to_string(),
                vec![SqlValue::Integer(tenant_id)],
            )
            .await?;

        rows.into_iter()
            .map(|row| row_to_dataset(row))
            .collect::<StackhouseResult<Vec<_>>>()
    }

    pub async fn create(
        &self,
        tenant_id: i64,
        req: CreateDatasetRequest,
    ) -> StackhouseResult<Dataset> {
        self.ensure_table().await?;
        let id = uuid::Uuid::new_v4().to_string();
        let fields_json = serde_json::to_value(req.fields).map_err(|e| {
            StackhouseError::internal(format!("dataset fields serialization: {}", e))
        })?;

        self.store
            .execute(
                "INSERT INTO vibe_datasets (id, name, description, source_sql, fields, tenant_id) VALUES ($1, $2, $3, $4, $5, $6)"
                    .to_string(),
                vec![
                    SqlValue::Text(id.clone()),
                    SqlValue::Text(req.name),
                    SqlValue::Text(req.description.unwrap_or_default()),
                    SqlValue::Text(req.source_sql),
                    SqlValue::Json(fields_json),
                    SqlValue::Integer(tenant_id),
                ],
            )
            .await?;

        self.get(&id).await
    }

    pub async fn get(&self, id: &str) -> StackhouseResult<Dataset> {
        self.ensure_table().await?;
        let rows = self
            .store
            .query(
                "SELECT id, name, description, source_sql, fields, tenant_id, created_at::TEXT FROM vibe_datasets WHERE id = $1 LIMIT 1"
                    .to_string(),
                vec![SqlValue::Text(id.to_string())],
            )
            .await?;

        row_to_dataset(
            rows.into_iter()
                .next()
                .ok_or_else(|| StackhouseError::NotFound(format!("dataset '{}' not found", id)))?,
        )
    }

    pub async fn query(&self, id: &str, limit: Option<i64>) -> StackhouseResult<Vec<Value>> {
        let dataset = self.get(id).await?;
        let mut sql = dataset.source_sql;
        if let Some(lim) = limit {
            sql = format!("{} LIMIT {}", sql.trim_end_matches(';'), lim);
        }

        let raw = self.store.query_simple(sql).await?;
        Ok(raw
            .into_iter()
            .map(|row| {
                let mut map = serde_json::Map::new();
                for (k, v) in row {
                    map.insert(k, v);
                }
                Value::Object(map)
            })
            .collect())
    }
}

fn row_to_dataset(row: Vec<(String, Value)>) -> StackhouseResult<Dataset> {
    let get = |key: &str| {
        row.iter()
            .find(|(k, _)| k == key)
            .map(|(_, v)| v.clone())
            .ok_or_else(|| StackhouseError::internal(format!("missing dataset column: {}", key)))
    };

    let fields: Vec<DatasetField> = serde_json::from_value(get("fields")?)
        .map_err(|e| StackhouseError::internal(format!("dataset fields parse error: {}", e)))?;

    Ok(Dataset {
        id: get("id")?.as_str().unwrap_or_default().to_string(),
        name: get("name")?.as_str().unwrap_or_default().to_string(),
        description: get("description")?.as_str().unwrap_or_default().to_string(),
        source_sql: get("source_sql")?.as_str().unwrap_or_default().to_string(),
        fields,
        tenant_id: get("tenant_id")?.as_i64().unwrap_or(0),
        created_at: get("created_at")?.as_str().unwrap_or_default().to_string(),
    })
}
