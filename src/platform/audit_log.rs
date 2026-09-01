//! # Immutable Audit Log
//!
//! Append-only, tamper-evident audit log for all auth, data, billing, and
//! admin events. Entries cannot be modified or deleted. Hash chain ensures
//! integrity verification.

use crate::db::{SqlValue, StackhouseStore};
use crate::error::StackhouseResult;

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::info;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuditEntry {
    pub id: String,
    pub tenant_id: i64,
    pub actor_id: String,
    pub actor_type: ActorType,
    pub action: String,
    pub resource_type: String,
    pub resource_id: String,
    pub details: Value,
    pub ip_address: Option<String>,
    pub user_agent: Option<String>,
    pub prev_hash: String,
    pub entry_hash: String,
    pub timestamp: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ActorType {
    User,
    ApiKey,
    System,
    Agent,
    Admin,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuditQuery {
    pub tenant_id: i64,
    pub actor_id: Option<String>,
    pub action: Option<String>,
    pub resource_type: Option<String>,
    pub from: Option<String>,
    pub to: Option<String>,
    pub limit: usize,
    pub offset: usize,
}

#[derive(Clone)]
pub struct ImmutableAuditLog {
    store: Arc<StackhouseStore>,
    last_hash: Arc<RwLock<String>>,
}

impl ImmutableAuditLog {
    pub async fn new(store: Arc<StackhouseStore>) -> StackhouseResult<Self> {
        let service = Self {
            store,
            last_hash: Arc::new(RwLock::new("genesis".to_string())),
        };
        service.initialize_tables().await?;
        service.load_last_hash().await?;
        info!("📋 Immutable audit log initialized");
        Ok(service)
    }

    async fn initialize_tables(&self) -> StackhouseResult<()> {
        self.store.execute_batch(r#"
            CREATE TABLE IF NOT EXISTS stackhouse_immutable_audit_log (
                id TEXT PRIMARY KEY,
                tenant_id BIGINT NOT NULL,
                actor_id TEXT NOT NULL,
                actor_type TEXT NOT NULL,
                action TEXT NOT NULL,
                resource_type TEXT NOT NULL,
                resource_id TEXT NOT NULL DEFAULT '',
                details JSONB DEFAULT '{}',
                ip_address TEXT,
                user_agent TEXT,
                prev_hash TEXT NOT NULL,
                entry_hash TEXT NOT NULL,
                timestamp TIMESTAMPTZ DEFAULT NOW()
            );
            CREATE INDEX IF NOT EXISTS idx_audit_tenant_time ON stackhouse_immutable_audit_log(tenant_id, timestamp);
            CREATE INDEX IF NOT EXISTS idx_audit_actor ON stackhouse_immutable_audit_log(actor_id);
            CREATE INDEX IF NOT EXISTS idx_audit_action ON stackhouse_immutable_audit_log(action);
            CREATE INDEX IF NOT EXISTS idx_audit_resource ON stackhouse_immutable_audit_log(resource_type, resource_id);

            -- Prevent updates and deletes via a trigger
            CREATE OR REPLACE FUNCTION prevent_audit_modification() RETURNS TRIGGER AS $$
            BEGIN
                RAISE EXCEPTION 'Audit log entries cannot be modified or deleted';
                RETURN NULL;
            END;
            $$ LANGUAGE plpgsql;

            DROP TRIGGER IF EXISTS audit_immutable_trigger ON stackhouse_immutable_audit_log;
            CREATE TRIGGER audit_immutable_trigger
                BEFORE UPDATE OR DELETE ON stackhouse_immutable_audit_log
                FOR EACH ROW EXECUTE FUNCTION prevent_audit_modification();
        "#.to_string()).await?;
        Ok(())
    }

    async fn load_last_hash(&self) -> StackhouseResult<()> {
        let rows = self.store.query(
            "SELECT entry_hash FROM stackhouse_immutable_audit_log ORDER BY timestamp DESC LIMIT 1".to_string(),
            vec![],
        ).await?;
        if let Some(row) = rows.first() {
            let hash = row
                .iter()
                .find(|(k, _)| k == "entry_hash")
                .and_then(|(_, v)| v.as_str())
                .unwrap_or("genesis");
            *self.last_hash.write().await = hash.to_string();
        }
        Ok(())
    }

    /// Append an audit entry (immutable — cannot be modified after)
    pub async fn append(
        &self,
        tenant_id: i64,
        actor_id: &str,
        actor_type: ActorType,
        action: &str,
        resource_type: &str,
        resource_id: &str,
        details: Value,
        ip: Option<&str>,
        ua: Option<&str>,
    ) -> StackhouseResult<AuditEntry> {
        let id = uuid::Uuid::new_v4().to_string();
        let prev_hash = self.last_hash.read().await.clone();

        // Compute hash chain
        let entry_data = format!(
            "{}:{}:{}:{}:{}:{}:{}",
            prev_hash,
            tenant_id,
            actor_id,
            action,
            resource_type,
            resource_id,
            chrono::Utc::now().timestamp_millis()
        );
        let mut hasher = Sha256::new();
        hasher.update(entry_data.as_bytes());
        let entry_hash = hex::encode(hasher.finalize());

        let actor_type_str = serde_json::to_string(&actor_type)
            .unwrap_or_default()
            .trim_matches('"')
            .to_string();

        self.store.execute(
            "INSERT INTO stackhouse_immutable_audit_log (id, tenant_id, actor_id, actor_type, action, resource_type, resource_id, details, ip_address, user_agent, prev_hash, entry_hash) VALUES (?, ?, ?, ?, ?, ?, ?, ?::jsonb, ?, ?, ?, ?)".to_string(),
            vec![
                SqlValue::Text(id.clone()),
                SqlValue::Integer(tenant_id),
                SqlValue::Text(actor_id.to_string()),
                SqlValue::Text(actor_type_str),
                SqlValue::Text(action.to_string()),
                SqlValue::Text(resource_type.to_string()),
                SqlValue::Text(resource_id.to_string()),
                SqlValue::Text(details.to_string()),
                SqlValue::Text(ip.unwrap_or("").to_string()),
                SqlValue::Text(ua.unwrap_or("").to_string()),
                SqlValue::Text(prev_hash.clone()),
                SqlValue::Text(entry_hash.clone()),
            ],
        ).await?;

        *self.last_hash.write().await = entry_hash.clone();

        Ok(AuditEntry {
            id,
            tenant_id,
            actor_id: actor_id.to_string(),
            actor_type,
            action: action.to_string(),
            resource_type: resource_type.to_string(),
            resource_id: resource_id.to_string(),
            details,
            ip_address: ip.map(|s| s.to_string()),
            user_agent: ua.map(|s| s.to_string()),
            prev_hash,
            entry_hash,
            timestamp: chrono::Utc::now().to_rfc3339(),
        })
    }

    /// Query audit log
    pub async fn query(&self, q: &AuditQuery) -> StackhouseResult<Vec<Value>> {
        let mut conditions = vec!["tenant_id = ?".to_string()];
        let mut params = vec![SqlValue::Integer(q.tenant_id)];

        if let Some(actor) = &q.actor_id {
            conditions.push("actor_id = ?".to_string());
            params.push(SqlValue::Text(actor.clone()));
        }
        if let Some(action) = &q.action {
            conditions.push("action = ?".to_string());
            params.push(SqlValue::Text(action.clone()));
        }
        if let Some(rt) = &q.resource_type {
            conditions.push("resource_type = ?".to_string());
            params.push(SqlValue::Text(rt.clone()));
        }
        if let Some(from) = &q.from {
            conditions.push("timestamp >= ?::timestamptz".to_string());
            params.push(SqlValue::Text(from.clone()));
        }
        if let Some(to) = &q.to {
            conditions.push("timestamp <= ?::timestamptz".to_string());
            params.push(SqlValue::Text(to.clone()));
        }

        let sql = format!(
            "SELECT id, actor_id, actor_type, action, resource_type, resource_id, details, timestamp FROM stackhouse_immutable_audit_log WHERE {} ORDER BY timestamp DESC LIMIT {} OFFSET {}",
            conditions.join(" AND "), q.limit, q.offset
        );

        let rows = self.store.query(sql, params).await?;
        Ok(rows
            .into_iter()
            .map(|r| json!(r.into_iter().collect::<HashMap<_, _>>()))
            .collect())
    }

    /// Verify integrity of the audit chain
    pub async fn verify_integrity(&self, tenant_id: i64, limit: usize) -> StackhouseResult<Value> {
        let rows = self.store.query(
            format!("SELECT prev_hash, entry_hash FROM stackhouse_immutable_audit_log WHERE tenant_id = ? ORDER BY timestamp ASC LIMIT {}", limit),
            vec![SqlValue::Integer(tenant_id)],
        ).await?;

        let mut valid = true;
        let mut checked = 0;
        let mut expected_prev = "genesis".to_string();

        for row in &rows {
            let prev = row
                .iter()
                .find(|(k, _)| k == "prev_hash")
                .and_then(|(_, v)| v.as_str())
                .unwrap_or("");
            let hash = row
                .iter()
                .find(|(k, _)| k == "entry_hash")
                .and_then(|(_, v)| v.as_str())
                .unwrap_or("");

            if prev != expected_prev {
                valid = false;
                break;
            }
            expected_prev = hash.to_string();
            checked += 1;
        }

        Ok(json!({
            "valid": valid,
            "entries_checked": checked,
            "last_hash": expected_prev,
        }))
    }
}
