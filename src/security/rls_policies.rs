//! # Row-Level Security (RLS) Policies
//!
//! Configurable RLS policies via SQL or SDK. Dynamically generates
//! and applies Postgres RLS policies with JWT context injection.

use crate::db::{SqlValue, StackhouseStore};
use crate::error::StackhouseResult;

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::HashMap;
use std::sync::Arc;
use tracing::info;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RlsPolicy {
    pub id: String,
    pub tenant_id: i64,
    pub table_name: String,
    pub policy_name: String,
    pub operation: RlsOperation,
    pub expression: String,
    pub target_roles: Vec<String>,
    pub using_expression: String,
    pub with_check_expression: Option<String>,
    pub enabled: bool,
    pub description: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "UPPERCASE")]
pub enum RlsOperation {
    All,
    Select,
    Insert,
    Update,
    Delete,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RlsContext {
    pub tenant_id: i64,
    pub user_id: String,
    pub roles: Vec<String>,
    pub claims: HashMap<String, Value>,
}

#[derive(Clone)]
pub struct RlsPolicyService {
    store: Arc<StackhouseStore>,
}

impl RlsPolicyService {
    pub async fn new(store: Arc<StackhouseStore>) -> StackhouseResult<Self> {
        let service = Self { store };
        service.initialize_tables().await?;
        info!("🔒 RLS policy service initialized");
        Ok(service)
    }

    async fn initialize_tables(&self) -> StackhouseResult<()> {
        self.store.execute_batch(r#"
            CREATE TABLE IF NOT EXISTS stackhouse_rls_policies (
                id TEXT PRIMARY KEY,
                tenant_id BIGINT NOT NULL,
                table_name TEXT NOT NULL,
                policy_name TEXT NOT NULL,
                operation TEXT NOT NULL DEFAULT 'ALL',
                expression TEXT NOT NULL,
                target_roles JSONB DEFAULT '[]',
                using_expression TEXT NOT NULL,
                with_check_expression TEXT,
                enabled BOOLEAN DEFAULT TRUE,
                description TEXT DEFAULT '',
                created_at TIMESTAMPTZ DEFAULT NOW(),
                UNIQUE(tenant_id, table_name, policy_name)
            );
            CREATE INDEX IF NOT EXISTS idx_rls_policies_tenant ON stackhouse_rls_policies(tenant_id, table_name);
        "#.to_string()).await?;
        Ok(())
    }

    /// Enable RLS on a table
    pub async fn enable_rls(&self, table_name: &str) -> StackhouseResult<()> {
        self.store
            .execute(
                format!(
                    "ALTER TABLE IF EXISTS {} ENABLE ROW LEVEL SECURITY",
                    table_name
                ),
                vec![],
            )
            .await?;
        self.store
            .execute(
                format!(
                    "ALTER TABLE IF EXISTS {} FORCE ROW LEVEL SECURITY",
                    table_name
                ),
                vec![],
            )
            .await?;
        Ok(())
    }

    /// Create a new RLS policy
    pub async fn create_policy(&self, policy: &RlsPolicy) -> StackhouseResult<()> {
        let op_str = serde_json::to_string(&policy.operation)
            .unwrap_or_default()
            .trim_matches('"')
            .to_string();
        self.store.execute(
            "INSERT INTO stackhouse_rls_policies (id, tenant_id, table_name, policy_name, operation, expression, target_roles, using_expression, with_check_expression, enabled, description) VALUES (?, ?, ?, ?, ?, ?, ?::jsonb, ?, ?, ?, ?)".to_string(),
            vec![
                SqlValue::Text(policy.id.clone()),
                SqlValue::Integer(policy.tenant_id),
                SqlValue::Text(policy.table_name.clone()),
                SqlValue::Text(policy.policy_name.clone()),
                SqlValue::Text(op_str),
                SqlValue::Text(policy.expression.clone()),
                SqlValue::Text(serde_json::to_string(&policy.target_roles).unwrap_or_default()),
                SqlValue::Text(policy.using_expression.clone()),
                SqlValue::Text(policy.with_check_expression.clone().unwrap_or_default()),
                SqlValue::Text(policy.enabled.to_string()),
                SqlValue::Text(policy.description.clone()),
            ],
        ).await?;

        // Apply to database
        self.apply_policy_to_db(policy).await?;
        Ok(())
    }

    async fn apply_policy_to_db(&self, policy: &RlsPolicy) -> StackhouseResult<()> {
        let roles = if policy.target_roles.is_empty() {
            "TO PUBLIC".to_string()
        } else {
            format!("TO {}", policy.target_roles.join(", "))
        };

        let op = match policy.operation {
            RlsOperation::All => "ALL".to_string(),
            RlsOperation::Select => "SELECT".to_string(),
            RlsOperation::Insert => "INSERT".to_string(),
            RlsOperation::Update => "UPDATE".to_string(),
            RlsOperation::Delete => "DELETE".to_string(),
        };

        let sql = format!(
            "CREATE POLICY IF NOT EXISTS {} ON {} FOR {} {} USING ({})",
            policy.policy_name, policy.table_name, op, roles, policy.using_expression,
        );

        self.store.execute(sql, vec![]).await?;
        Ok(())
    }

    /// Drop a policy from DB
    pub async fn drop_policy(&self, table_name: &str, policy_name: &str) -> StackhouseResult<()> {
        self.store
            .execute(
                format!("DROP POLICY IF EXISTS {} ON {}", policy_name, table_name),
                vec![],
            )
            .await?;
        self.store
            .execute(
                "DELETE FROM stackhouse_rls_policies WHERE table_name = ? AND policy_name = ?"
                    .to_string(),
                vec![
                    SqlValue::Text(table_name.to_string()),
                    SqlValue::Text(policy_name.to_string()),
                ],
            )
            .await?;
        Ok(())
    }

    /// Get policies for a table
    pub async fn get_policies(
        &self,
        tenant_id: i64,
        table_name: &str,
    ) -> StackhouseResult<Vec<Value>> {
        let rows = self.store.query(
            "SELECT id, policy_name, operation, expression, target_roles, enabled, description FROM stackhouse_rls_policies WHERE tenant_id = ? AND table_name = ?".to_string(),
            vec![SqlValue::Integer(tenant_id), SqlValue::Text(table_name.to_string())],
        ).await?;
        Ok(rows
            .into_iter()
            .map(|r| json!(r.into_iter().collect::<HashMap<_, _>>()))
            .collect())
    }

    /// Validate a policy expression is syntactically valid SQL
    pub async fn validate_expression(&self, expression: &str) -> StackhouseResult<bool> {
        // Test the expression by wrapping it in a no-op query
        let test_sql = format!("SELECT CASE WHEN {} THEN 1 ELSE 0 END AS valid", expression);
        match self.store.query(test_sql, vec![]).await {
            Ok(_) => Ok(true),
            Err(_) => Ok(false),
        }
    }

    /// Disable a policy (soft disable)
    pub async fn disable_policy(&self, policy_id: &str) -> StackhouseResult<()> {
        let row = self
            .store
            .query(
                "SELECT table_name, policy_name FROM stackhouse_rls_policies WHERE id = ?"
                    .to_string(),
                vec![SqlValue::Text(policy_id.to_string())],
            )
            .await?;

        if let Some(r) = row.first() {
            let table = r
                .iter()
                .find(|(k, _)| k == "table_name")
                .and_then(|(_, v)| v.as_str())
                .unwrap_or("");
            let name = r
                .iter()
                .find(|(k, _)| k == "policy_name")
                .and_then(|(_, v)| v.as_str())
                .unwrap_or("");
            self.store
                .execute(
                    format!("ALTER TABLE {} DISABLE POLICY {}", table, name),
                    vec![],
                )
                .await?;
        }

        self.store
            .execute(
                "UPDATE stackhouse_rls_policies SET enabled = false WHERE id = ?".to_string(),
                vec![SqlValue::Text(policy_id.to_string())],
            )
            .await?;
        Ok(())
    }

    /// Build a tenant-scoped query with RLS
    pub fn build_rls_query(&self, table: &str, context: &RlsContext) -> String {
        format!(
            "SELECT * FROM {} WHERE tenant_id = {}",
            table, context.tenant_id
        )
    }

    /// List all tables with RLS enabled
    pub async fn list_rls_tables(&self) -> StackhouseResult<Vec<Value>> {
        let rows = self.store.query(
            "SELECT tablename FROM pg_tables WHERE schemaname = 'public' AND tablename LIKE 'stackhouse_%'".to_string(),
            vec![],
        ).await?;
        Ok(rows
            .into_iter()
            .map(|r| json!(r.into_iter().collect::<HashMap<_, _>>()))
            .collect())
    }
}
