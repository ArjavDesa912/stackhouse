//! # Full-Text Search & Trigram Search
//!
//! Built-in full-text search using Postgres tsvector and trigram
//! similarity. No external Elasticsearch required.

use crate::db::{SqlValue, StackhouseStore};
use crate::error::StackhouseResult;

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::HashMap;
use std::sync::Arc;
use tracing::info;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchIndex {
    pub id: String,
    pub tenant_id: i64,
    pub table_name: String,
    pub columns: Vec<String>,
    pub language: String,
    pub index_type: IndexType,
    pub weights: HashMap<String, f64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum IndexType {
    FullText,
    Trigram,
    Combined,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchQuery {
    pub tenant_id: i64,
    pub table_name: String,
    pub query: String,
    pub columns: Option<Vec<String>>,
    pub index_type: Option<IndexType>,
    pub limit: usize,
    pub min_rank: Option<f64>,
    pub filters: Option<HashMap<String, String>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchResult {
    pub document_id: String,
    pub rank: f64,
    pub highlights: Vec<String>,
    pub document: Value,
    pub search_type: String,
}

#[derive(Clone)]
pub struct FullTextSearchService {
    store: Arc<StackhouseStore>,
}

impl FullTextSearchService {
    pub async fn new(store: Arc<StackhouseStore>) -> StackhouseResult<Self> {
        let service = Self { store };
        service.initialize_tables().await?;
        service.ensure_extensions().await?;
        info!("🔍 Full-text search service initialized");
        Ok(service)
    }

    async fn initialize_tables(&self) -> StackhouseResult<()> {
        self.store.execute_batch(r#"
            CREATE TABLE IF NOT EXISTS stackhouse_search_indexes (
                id TEXT PRIMARY KEY,
                tenant_id BIGINT NOT NULL,
                table_name TEXT NOT NULL,
                columns JSONB NOT NULL DEFAULT '[]',
                language TEXT DEFAULT 'english',
                index_type TEXT DEFAULT 'full_text',
                weights JSONB DEFAULT '{}',
                created_at TIMESTAMPTZ DEFAULT NOW()
            );
            CREATE INDEX IF NOT EXISTS idx_search_indexes_tenant ON stackhouse_search_indexes(tenant_id, table_name);
        "#.to_string()).await?;
        Ok(())
    }

    async fn ensure_extensions(&self) -> StackhouseResult<()> {
        self.store
            .execute("CREATE EXTENSION IF NOT EXISTS pg_trgm".to_string(), vec![])
            .await
            .ok();
        Ok(())
    }

    /// Create a full-text search index on a table
    pub async fn create_index(
        &self,
        tenant_id: i64,
        table_name: &str,
        columns: Vec<String>,
        index_type: IndexType,
        language: Option<&str>,
    ) -> StackhouseResult<SearchIndex> {
        let id = uuid::Uuid::new_v4().to_string();
        let lang = language.unwrap_or("english");
        let type_str = serde_json::to_string(&index_type)
            .unwrap_or_default()
            .trim_matches('"')
            .to_string();

        self.store.execute(
            "INSERT INTO stackhouse_search_indexes (id, tenant_id, table_name, columns, language, index_type) VALUES (?, ?, ?, ?::jsonb, ?, ?)".to_string(),
            vec![
                SqlValue::Text(id.clone()),
                SqlValue::Integer(tenant_id),
                SqlValue::Text(table_name.to_string()),
                SqlValue::Text(serde_json::to_string(&columns).unwrap_or_default()),
                SqlValue::Text(lang.to_string()),
                SqlValue::Text(type_str),
            ],
        ).await?;

        // Create the actual Postgres GIN index
        let col_expr = columns
            .iter()
            .map(|c| format!("coalesce({}, '')", c))
            .collect::<Vec<_>>()
            .join(" || ' ' || ");
        let index_name = format!("idx_ft_{}_{}", table_name, id[..8].to_string());

        match index_type {
            IndexType::FullText | IndexType::Combined => {
                self.store
                    .execute(
                        format!(
                            "CREATE INDEX IF NOT EXISTS {} ON {} USING GIN (to_tsvector('{}', {}))",
                            index_name, table_name, lang, col_expr
                        ),
                        vec![],
                    )
                    .await
                    .ok();
            }
            IndexType::Trigram => {
                self.store
                    .execute(
                        format!(
                            "CREATE INDEX IF NOT EXISTS {} ON {} USING GIN ({} gin_trgm_ops)",
                            index_name, table_name, col_expr
                        ),
                        vec![],
                    )
                    .await
                    .ok();
            }
        }

        Ok(SearchIndex {
            id,
            tenant_id,
            table_name: table_name.to_string(),
            columns,
            language: lang.to_string(),
            index_type,
            weights: HashMap::new(),
        })
    }

    /// Search using full-text or trigram
    pub async fn search(&self, q: &SearchQuery) -> StackhouseResult<Vec<SearchResult>> {
        let index_type = q.index_type.clone().unwrap_or(IndexType::Combined);

        match index_type {
            IndexType::FullText => self.search_fulltext(q).await,
            IndexType::Trigram => self.search_trigram(q).await,
            IndexType::Combined => {
                let mut ft = self.search_fulltext(q).await?;
                let mut tri = self.search_trigram(q).await?;
                ft.append(&mut tri);
                ft.sort_by(|a, b| b.rank.partial_cmp(&a.rank).unwrap());
                ft.truncate(q.limit);
                Ok(ft)
            }
        }
    }

    async fn search_fulltext(&self, q: &SearchQuery) -> StackhouseResult<Vec<SearchResult>> {
        let col_expr = q
            .columns
            .as_ref()
            .map(|cols| {
                cols.iter()
                    .map(|c| format!("coalesce({}, '')", c))
                    .collect::<Vec<_>>()
                    .join(" || ' ' || ")
            })
            .unwrap_or_else(|| "coalesce(content, '')".to_string());

        let where_clause = if let Some(filters) = &q.filters {
            let conditions: Vec<String> = filters
                .iter()
                .map(|(k, v)| format!("{} = '{}'", k, v))
                .collect();
            format!(
                "WHERE tenant_id = {} AND {}",
                q.tenant_id,
                conditions.join(" AND ")
            )
        } else {
            format!("WHERE tenant_id = {}", q.tenant_id)
        };

        let min_rank = q.min_rank.unwrap_or(0.0);

        let sql = format!(
            "SELECT id, ts_rank_cd(to_tsvector('english', {}), plainto_tsquery('english', ?)) as rank, {} as content
             FROM {}
             {} AND to_tsvector('english', {}) @@ plainto_tsquery('english', ?)
             AND ts_rank_cd(to_tsvector('english', {}), plainto_tsquery('english', ?)) > ?
             ORDER BY rank DESC LIMIT {}",
            col_expr, col_expr, q.table_name, where_clause, col_expr, col_expr, q.limit
        );

        let rows = self
            .store
            .query(
                sql,
                vec![
                    SqlValue::Text(q.query.clone()),
                    SqlValue::Text(q.query.clone()),
                    SqlValue::Text(q.query.clone()),
                    SqlValue::Text(min_rank.to_string()),
                ],
            )
            .await?;

        Ok(rows
            .into_iter()
            .map(|row| {
                let doc_id = row
                    .iter()
                    .find(|(k, _)| k == "id")
                    .and_then(|(_, v)| v.as_str())
                    .unwrap_or("")
                    .to_string();
                let rank = row
                    .iter()
                    .find(|(k, _)| k == "rank")
                    .and_then(|(_, v)| v.as_f64())
                    .unwrap_or(0.0);
                let content = row
                    .iter()
                    .find(|(k, _)| k == "content")
                    .and_then(|(_, v)| v.as_str())
                    .unwrap_or("")
                    .to_string();
                SearchResult {
                    document_id: doc_id,
                    rank,
                    highlights: vec![content.clone()],
                    document: json!({"content": content}),
                    search_type: "full_text".to_string(),
                }
            })
            .collect())
    }

    async fn search_trigram(&self, q: &SearchQuery) -> StackhouseResult<Vec<SearchResult>> {
        let col_expr = q
            .columns
            .as_ref()
            .and_then(|cols| cols.first().cloned())
            .unwrap_or_else(|| "content".to_string());

        let where_clause = if let Some(filters) = &q.filters {
            let conditions: Vec<String> = filters
                .iter()
                .map(|(k, v)| format!("{} = '{}'", k, v))
                .collect();
            format!(
                "WHERE tenant_id = {} AND {}",
                q.tenant_id,
                conditions.join(" AND ")
            )
        } else {
            format!("WHERE tenant_id = {}", q.tenant_id)
        };

        let sql = format!(
            "SELECT id, similarity({}, ?) as sim, {} as content
             FROM {}
             {} AND {} %% ?
             ORDER BY sim DESC LIMIT {}",
            col_expr, col_expr, q.table_name, where_clause, col_expr, q.limit
        );

        let rows = self
            .store
            .query(
                sql,
                vec![
                    SqlValue::Text(q.query.clone()),
                    SqlValue::Text(q.query.clone()),
                ],
            )
            .await?;

        Ok(rows
            .into_iter()
            .map(|row| {
                let doc_id = row
                    .iter()
                    .find(|(k, _)| k == "id")
                    .and_then(|(_, v)| v.as_str())
                    .unwrap_or("")
                    .to_string();
                let rank = row
                    .iter()
                    .find(|(k, _)| k == "sim")
                    .and_then(|(_, v)| v.as_f64())
                    .unwrap_or(0.0);
                let content = row
                    .iter()
                    .find(|(k, _)| k == "content")
                    .and_then(|(_, v)| v.as_str())
                    .unwrap_or("")
                    .to_string();
                SearchResult {
                    document_id: doc_id,
                    rank,
                    highlights: vec![content.clone()],
                    document: json!({"content": content}),
                    search_type: "trigram".to_string(),
                }
            })
            .collect())
    }

    /// Suggest completions using trigram similarity
    pub async fn suggest(
        &self,
        tenant_id: i64,
        table_name: &str,
        column: &str,
        prefix: &str,
        limit: usize,
    ) -> StackhouseResult<Vec<String>> {
        let rows = self.store.query(
            format!("SELECT DISTINCT {} FROM {} WHERE tenant_id = ? AND {} % ? ORDER BY similarity({}, ?) DESC LIMIT {}",
                column, table_name, column, column, limit),
            vec![SqlValue::Integer(tenant_id), SqlValue::Text(prefix.to_string()), SqlValue::Text(prefix.to_string())],
        ).await?;

        Ok(rows
            .into_iter()
            .filter_map(|row| {
                row.iter()
                    .find(|(k, _)| k == column)
                    .and_then(|(_, v)| v.as_str())
                    .map(|s| s.to_string())
            })
            .collect())
    }
}
