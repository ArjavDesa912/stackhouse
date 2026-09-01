//! # Seed Data & Test Fixtures
//!
//! Loads test data and fixtures from JSON/YAML files into the database.
//! Supports deterministic IDs for reproducible tests.

use crate::db::{SqlValue, StackhouseStore};
use crate::error::{StackhouseError, StackhouseResult};

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::path::Path;
use std::sync::Arc;
use tracing::info;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FixtureSet {
    pub name: String,
    pub description: String,
    pub tables: Vec<FixtureTable>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FixtureTable {
    pub table: String,
    pub records: Vec<Value>,
    pub mode: InsertMode,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum InsertMode {
    Insert,
    Upsert,
    Replace,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FixtureResult {
    pub fixture: String,
    pub table: String,
    pub inserted: u64,
    pub updated: u64,
    pub errors: Vec<String>,
}

#[derive(Clone)]
pub struct FixtureLoader {
    store: Arc<StackhouseStore>,
}

impl FixtureLoader {
    pub fn new(store: Arc<StackhouseStore>) -> Self {
        Self { store }
    }

    /// Load fixtures from a JSON file
    pub async fn load_from_file<P: AsRef<Path>>(
        &self,
        path: P,
    ) -> StackhouseResult<Vec<FixtureResult>> {
        let content = tokio::fs::read_to_string(path)
            .await
            .map_err(|e| StackhouseError::Internal(anyhow::anyhow!("Read fixture file: {}", e)))?;
        let fixture: FixtureSet = serde_json::from_str(&content)
            .map_err(|e| StackhouseError::Internal(anyhow::anyhow!("Parse fixture: {}", e)))?;
        self.load(fixture).await
    }

    /// Load fixtures from JSON string
    pub async fn load_json(&self, json_str: &str) -> StackhouseResult<Vec<FixtureResult>> {
        let fixture: FixtureSet = serde_json::from_str(json_str)
            .map_err(|e| StackhouseError::Internal(anyhow::anyhow!("Parse fixture: {}", e)))?;
        self.load(fixture).await
    }

    pub async fn load(&self, fixture: FixtureSet) -> StackhouseResult<Vec<FixtureResult>> {
        info!("🌱 Loading fixture set: {}", fixture.name);
        let mut results = Vec::new();

        for table in &fixture.tables {
            let mut inserted = 0u64;
            let updated = 0u64;
            let mut errors = Vec::new();

            for record in &table.records {
                let obj = record.as_object().ok_or_else(|| {
                    StackhouseError::InvalidPayload("Fixture record must be an object".into())
                })?;
                let columns: Vec<String> = obj.keys().cloned().collect();
                let values: Vec<SqlValue> = columns
                    .iter()
                    .map(|col| SqlValue::Text(obj.get(col).unwrap_or(&json!(null)).to_string()))
                    .collect();

                let placeholders: Vec<String> =
                    (1..=columns.len()).map(|_i| format!("?")).collect();
                let sql = format!(
                    "INSERT INTO {} ({}) VALUES ({}) ON CONFLICT DO NOTHING",
                    table.table,
                    columns.join(", "),
                    placeholders.join(", ")
                );

                match self.store.execute(sql, values).await {
                    Ok(rows) => {
                        inserted += rows;
                    }
                    Err(e) => {
                        errors.push(format!("Record insert failed: {}", e));
                    }
                }
            }

            results.push(FixtureResult {
                fixture: fixture.name.clone(),
                table: table.table.clone(),
                inserted,
                updated,
                errors,
            });
        }

        info!(
            "✅ Fixture '{}' loaded: {} tables",
            fixture.name,
            results.len()
        );
        Ok(results)
    }

    /// Generate deterministic IDs for test records
    pub fn deterministic_id(table: &str, seed: &str) -> String {
        use sha2::{Digest, Sha256};
        let mut hasher = Sha256::new();
        hasher.update(format!("{}:{}", table, seed).as_bytes());
        hex::encode(&hasher.finalize()[..16])
    }

    /// Create a standard test fixture set
    pub fn standard_test_fixture() -> FixtureSet {
        FixtureSet {
            name: "standard_test".into(),
            description: "Standard test data for Stackhouse".into(),
            tables: vec![
                FixtureTable {
                    table: "stackhouse_users".into(),
                    mode: InsertMode::Upsert,
                    records: vec![
                        json!({"id": 1, "email": "admin@stackhouse.dev", "password_hash": "", "metadata": "{}"}),
                        json!({"id": 2, "email": "user@stackhouse.dev", "password_hash": "", "metadata": "{}"}),
                    ],
                },
                FixtureTable {
                    table: "stackhouse_buckets".into(),
                    mode: InsertMode::Upsert,
                    records: vec![
                        json!({"id": 1, "name": "test-public", "public": 1, "owner_id": 1}),
                        json!({"id": 2, "name": "test-private", "public": 0, "owner_id": 1}),
                    ],
                },
            ],
        }
    }
}
