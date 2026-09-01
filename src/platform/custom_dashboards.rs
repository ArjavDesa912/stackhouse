//! # Custom Dashboards & Saved Metric Queries
//!
//! User-defined dashboards with saved metric queries, chart widgets,
//! and layout persistence.

use crate::db::{SqlValue, StackhouseStore};
use crate::error::StackhouseResult;

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::HashMap;
use std::sync::Arc;
use tracing::info;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Dashboard {
    pub id: String,
    pub tenant_id: i64,
    pub user_id: String,
    pub name: String,
    pub description: String,
    pub layout: DashboardLayout,
    pub widgets: Vec<Widget>,
    pub is_public: bool,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DashboardLayout {
    pub columns: u32,
    pub row_height: u32,
}

impl Default for DashboardLayout {
    fn default() -> Self {
        Self {
            columns: 3,
            row_height: 200,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Widget {
    pub id: String,
    pub widget_type: String,
    pub title: String,
    pub query: SavedQuery,
    pub position: WidgetPosition,
    pub config: HashMap<String, Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WidgetPosition {
    pub x: u32,
    pub y: u32,
    pub width: u32,
    pub height: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SavedQuery {
    pub id: String,
    pub name: String,
    pub query_type: String,
    pub query_config: Value,
    pub refresh_interval: u32,
}

#[derive(Clone)]
pub struct CustomDashboardService {
    store: Arc<StackhouseStore>,
}

impl CustomDashboardService {
    pub async fn new(store: Arc<StackhouseStore>) -> StackhouseResult<Self> {
        let service = Self { store };
        service.initialize_tables().await?;
        info!("📊 Custom dashboard service initialized");
        Ok(service)
    }

    async fn initialize_tables(&self) -> StackhouseResult<()> {
        self.store.execute_batch(r#"
            CREATE TABLE IF NOT EXISTS stackhouse_dashboards (
                id TEXT PRIMARY KEY,
                tenant_id BIGINT NOT NULL,
                user_id TEXT NOT NULL,
                name TEXT NOT NULL,
                description TEXT DEFAULT '',
                layout JSONB DEFAULT '{}',
                widgets JSONB DEFAULT '[]',
                is_public BOOLEAN DEFAULT FALSE,
                created_at TIMESTAMPTZ DEFAULT NOW(),
                updated_at TIMESTAMPTZ DEFAULT NOW()
            );
            CREATE TABLE IF NOT EXISTS stackhouse_saved_queries (
                id TEXT PRIMARY KEY,
                tenant_id BIGINT NOT NULL,
                user_id TEXT NOT NULL,
                name TEXT NOT NULL,
                query_type TEXT NOT NULL,
                query_config JSONB NOT NULL,
                refresh_interval INTEGER DEFAULT 60,
                created_at TIMESTAMPTZ DEFAULT NOW()
            );
            CREATE INDEX IF NOT EXISTS idx_dashboards_tenant ON stackhouse_dashboards(tenant_id, user_id);
            CREATE INDEX IF NOT EXISTS idx_saved_queries_tenant ON stackhouse_saved_queries(tenant_id, user_id);
        "#.to_string()).await?;
        Ok(())
    }

    pub async fn create_dashboard(
        &self,
        tenant_id: i64,
        user_id: &str,
        name: &str,
        description: &str,
    ) -> StackhouseResult<Dashboard> {
        let id = uuid::Uuid::new_v4().to_string();
        let now = chrono::Utc::now().to_rfc3339();
        let dashboard = Dashboard {
            id: id.clone(),
            tenant_id,
            user_id: user_id.to_string(),
            name: name.to_string(),
            description: description.to_string(),
            layout: DashboardLayout::default(),
            widgets: Vec::new(),
            is_public: false,
            created_at: now.clone(),
            updated_at: now,
        };

        self.store.execute(
            "INSERT INTO stackhouse_dashboards (id, tenant_id, user_id, name, description, layout, widgets, is_public) VALUES (?, ?, ?, ?, ?, ?::jsonb, ?::jsonb, ?)".to_string(),
            vec![
                SqlValue::Text(id.clone()), SqlValue::Integer(tenant_id), SqlValue::Text(user_id.to_string()),
                SqlValue::Text(name.to_string()), SqlValue::Text(description.to_string()),
                SqlValue::Text(serde_json::to_string(&dashboard.layout).unwrap_or_default()),
                SqlValue::Text(serde_json::to_string(&dashboard.widgets).unwrap_or_default()),
                SqlValue::Text("false".to_string()),
            ],
        ).await?;
        Ok(dashboard)
    }

    pub async fn add_widget(&self, dashboard_id: &str, widget: &Widget) -> StackhouseResult<()> {
        let rows = self
            .store
            .query(
                "SELECT widgets FROM stackhouse_dashboards WHERE id = ?".to_string(),
                vec![SqlValue::Text(dashboard_id.to_string())],
            )
            .await?;

        if let Some(row) = rows.first() {
            let widgets_str = row
                .iter()
                .find(|(k, _)| k == "widgets")
                .and_then(|(_, v)| v.as_str())
                .unwrap_or("[]");
            let mut widgets: Vec<Widget> = serde_json::from_str(widgets_str).unwrap_or_default();
            widgets.push(widget.clone());

            self.store.execute(
                "UPDATE stackhouse_dashboards SET widgets = ?::jsonb, updated_at = NOW() WHERE id = ?".to_string(),
                vec![
                    SqlValue::Text(serde_json::to_string(&widgets).unwrap_or_default()),
                    SqlValue::Text(dashboard_id.to_string()),
                ],
            ).await?;
        }
        Ok(())
    }

    pub async fn save_query(
        &self,
        tenant_id: i64,
        user_id: &str,
        query: &SavedQuery,
    ) -> StackhouseResult<()> {
        self.store.execute(
            "INSERT INTO stackhouse_saved_queries (id, tenant_id, user_id, name, query_type, query_config, refresh_interval) VALUES (?, ?, ?, ?, ?, ?::jsonb, ?)".to_string(),
            vec![
                SqlValue::Text(query.id.clone()), SqlValue::Integer(tenant_id), SqlValue::Text(user_id.to_string()),
                SqlValue::Text(query.name.clone()), SqlValue::Text(query.query_type.clone()),
                SqlValue::Text(query.query_config.to_string()), SqlValue::Integer(query.refresh_interval as i64),
            ],
        ).await?;
        Ok(())
    }

    pub async fn list_dashboards(
        &self,
        tenant_id: i64,
        user_id: &str,
    ) -> StackhouseResult<Vec<Value>> {
        let rows = self.store.query(
            "SELECT id, name, description, is_public, created_at, updated_at FROM stackhouse_dashboards WHERE tenant_id = ? AND (user_id = ? OR is_public = true)".to_string(),
            vec![SqlValue::Integer(tenant_id), SqlValue::Text(user_id.to_string())],
        ).await?;
        Ok(rows
            .into_iter()
            .map(|r| json!(r.iter().cloned().collect::<HashMap<_, _>>()))
            .collect())
    }

    pub async fn get_dashboard(&self, dashboard_id: &str) -> StackhouseResult<Option<Dashboard>> {
        let rows = self
            .store
            .query(
                "SELECT * FROM stackhouse_dashboards WHERE id = ?".to_string(),
                vec![SqlValue::Text(dashboard_id.to_string())],
            )
            .await?;
        if rows.is_empty() {
            return Ok(None);
        }
        Ok(Some(self.row_to_dashboard(&rows[0])?))
    }

    pub async fn delete_dashboard(&self, dashboard_id: &str) -> StackhouseResult<()> {
        self.store
            .execute(
                "DELETE FROM stackhouse_dashboards WHERE id = ?".to_string(),
                vec![SqlValue::Text(dashboard_id.to_string())],
            )
            .await?;
        Ok(())
    }

    pub async fn list_saved_queries(
        &self,
        tenant_id: i64,
        user_id: &str,
    ) -> StackhouseResult<Vec<Value>> {
        let rows = self.store.query(
            "SELECT id, name, query_type, refresh_interval, created_at FROM stackhouse_saved_queries WHERE tenant_id = ? AND user_id = ?".to_string(),
            vec![SqlValue::Integer(tenant_id), SqlValue::Text(user_id.to_string())],
        ).await?;
        Ok(rows
            .into_iter()
            .map(|r| json!(r.iter().cloned().collect::<HashMap<_, _>>()))
            .collect())
    }

    fn row_to_dashboard(&self, row: &[(String, Value)]) -> StackhouseResult<Dashboard> {
        let get_str = |key: &str| {
            row.iter()
                .find(|(k, _)| k == key)
                .and_then(|(_, v)| v.as_str())
                .map(|s| s.to_string())
        };
        let get_i64 = |key: &str| {
            row.iter()
                .find(|(k, _)| k == key)
                .and_then(|(_, v)| v.as_i64())
        };

        let layout_str = get_str("layout").unwrap_or_default();
        let widgets_str = get_str("widgets").unwrap_or_default();

        Ok(Dashboard {
            id: get_str("id").unwrap_or_default(),
            tenant_id: get_i64("tenant_id").unwrap_or(0),
            user_id: get_str("user_id").unwrap_or_default(),
            name: get_str("name").unwrap_or_default(),
            description: get_str("description").unwrap_or_default(),
            layout: serde_json::from_str(&layout_str).unwrap_or_default(),
            widgets: serde_json::from_str(&widgets_str).unwrap_or_default(),
            is_public: row
                .iter()
                .find(|(k, _)| k == "is_public")
                .and_then(|(_, v)| v.as_str())
                .map(|s| s == "true" || s == "t")
                .unwrap_or(false),
            created_at: get_str("created_at").unwrap_or_default(),
            updated_at: get_str("updated_at").unwrap_or_default(),
        })
    }
}
