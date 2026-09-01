//! # Read Replicas & Multi-Region Failover
//!
//! Replica registration, automatic read routing, health checks,
//! and failover between primary and replicas.

use crate::auth::{extract_auth_user, AuthState};
use crate::db::{SqlValue, StackhouseStore};
use crate::error::{StackhouseError, StackhouseResult};

use axum::{
    extract::State,
    http::HeaderMap,
    response::IntoResponse,
    routing::{delete, get, post},
    Json, Router,
};
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::RwLock;
use tracing::info;

// ============================================================================
// Types
// ============================================================================

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReplicaNode {
    pub id: String,
    pub tenant_id: i64,
    pub name: String,
    pub host: String,
    pub port: u16,
    pub database: String,
    pub region: String,
    pub role: NodeRole,
    pub status: NodeStatus,
    pub replication_lag_ms: u64,
    pub connections_active: u32,
    pub connections_max: u32,
    pub last_health_check: Option<String>,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum NodeRole {
    Primary,
    Replica,
    Standby,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum NodeStatus {
    Healthy,
    Degraded,
    Unhealthy,
    Offline,
    Promoting,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FailoverEvent {
    pub id: String,
    pub old_primary_id: String,
    pub new_primary_id: String,
    pub reason: String,
    pub duration_ms: u64,
    pub timestamp: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReplicationStats {
    pub primary_id: String,
    pub replica_count: usize,
    pub avg_replication_lag_ms: u64,
    pub max_replication_lag_ms: u64,
    pub total_reads_routed: u64,
    pub reads_to_primary: u64,
    pub reads_to_replicas: u64,
}

// ============================================================================
// Replica Service
// ============================================================================

#[derive(Clone)]
pub struct ReplicaService {
    store: Arc<StackhouseStore>,
    nodes: Arc<RwLock<Vec<ReplicaNode>>>,
    reads_total: Arc<AtomicU64>,
    reads_primary: Arc<AtomicU64>,
    reads_replica: Arc<AtomicU64>,
    round_robin_index: Arc<AtomicU64>,
}

impl ReplicaService {
    pub async fn new(store: Arc<StackhouseStore>) -> StackhouseResult<Self> {
        let service = Self {
            store,
            nodes: Arc::new(RwLock::new(Vec::new())),
            reads_total: Arc::new(AtomicU64::new(0)),
            reads_primary: Arc::new(AtomicU64::new(0)),
            reads_replica: Arc::new(AtomicU64::new(0)),
            round_robin_index: Arc::new(AtomicU64::new(0)),
        };
        service.initialize_tables().await?;
        service.load_nodes().await?;
        service.start_health_checker();
        info!("🔄 Replica service initialized");
        Ok(service)
    }

    async fn initialize_tables(&self) -> StackhouseResult<()> {
        self.store.execute_batch(r#"
            CREATE TABLE IF NOT EXISTS stackhouse_replica_nodes (
                id TEXT PRIMARY KEY,
                tenant_id BIGINT NOT NULL,
                name TEXT NOT NULL,
                host TEXT NOT NULL,
                port INTEGER NOT NULL DEFAULT 5432,
                database_name TEXT NOT NULL DEFAULT 'postgres',
                region TEXT NOT NULL DEFAULT 'us-east-1',
                role TEXT NOT NULL DEFAULT 'replica',
                status TEXT NOT NULL DEFAULT 'healthy',
                replication_lag_ms BIGINT DEFAULT 0,
                connections_active INTEGER DEFAULT 0,
                connections_max INTEGER DEFAULT 100,
                last_health_check TIMESTAMPTZ,
                created_at TIMESTAMPTZ DEFAULT NOW()
            );
            CREATE TABLE IF NOT EXISTS stackhouse_failover_events (
                id TEXT PRIMARY KEY,
                old_primary_id TEXT NOT NULL,
                new_primary_id TEXT NOT NULL,
                reason TEXT NOT NULL,
                duration_ms BIGINT,
                timestamp TIMESTAMPTZ DEFAULT NOW()
            );
            CREATE INDEX IF NOT EXISTS idx_replica_nodes_tenant ON stackhouse_replica_nodes(tenant_id);
            CREATE INDEX IF NOT EXISTS idx_replica_nodes_role ON stackhouse_replica_nodes(role);
        "#.to_string()).await?;
        Ok(())
    }

    async fn load_nodes(&self) -> StackhouseResult<()> {
        let rows = self.store.query(
            "SELECT id, tenant_id, name, host, port, database_name, region, role, status, replication_lag_ms, connections_active, connections_max, last_health_check, created_at FROM stackhouse_replica_nodes".to_string(),
            vec![],
        ).await?;

        let mut nodes = self.nodes.write().await;
        for row in rows {
            let get = |key: &str| row.iter().find(|(k, _)| k == key).map(|(_, v)| v.clone());
            nodes.push(ReplicaNode {
                id: get("id")
                    .and_then(|v| v.as_str().map(String::from))
                    .unwrap_or_default(),
                tenant_id: get("tenant_id").and_then(|v| v.as_i64()).unwrap_or(0),
                name: get("name")
                    .and_then(|v| v.as_str().map(String::from))
                    .unwrap_or_default(),
                host: get("host")
                    .and_then(|v| v.as_str().map(String::from))
                    .unwrap_or_default(),
                port: get("port").and_then(|v| v.as_i64()).unwrap_or(5432) as u16,
                database: get("database_name")
                    .and_then(|v| v.as_str().map(String::from))
                    .unwrap_or_else(|| "postgres".into()),
                region: get("region")
                    .and_then(|v| v.as_str().map(String::from))
                    .unwrap_or_default(),
                role: match get("role")
                    .and_then(|v| v.as_str().map(String::from))
                    .unwrap_or_default()
                    .as_str()
                {
                    "primary" => NodeRole::Primary,
                    "standby" => NodeRole::Standby,
                    _ => NodeRole::Replica,
                },
                status: NodeStatus::Healthy,
                replication_lag_ms: get("replication_lag_ms")
                    .and_then(|v| v.as_i64())
                    .unwrap_or(0) as u64,
                connections_active: get("connections_active")
                    .and_then(|v| v.as_i64())
                    .unwrap_or(0) as u32,
                connections_max: get("connections_max")
                    .and_then(|v| v.as_i64())
                    .unwrap_or(100) as u32,
                last_health_check: get("last_health_check")
                    .and_then(|v| v.as_str().map(String::from)),
                created_at: get("created_at")
                    .and_then(|v| v.as_str().map(String::from))
                    .unwrap_or_default(),
            });
        }
        Ok(())
    }

    fn start_health_checker(&self) {
        let nodes = Arc::clone(&self.nodes);
        let store = Arc::clone(&self.store);

        tokio::spawn(async move {
            let mut interval = tokio::time::interval(Duration::from_secs(30));
            loop {
                interval.tick().await;
                let current_nodes = nodes.read().await.clone();
                for node in &current_nodes {
                    let healthy = Self::check_node_health(node).await;
                    let new_status = if healthy {
                        NodeStatus::Healthy
                    } else {
                        NodeStatus::Unhealthy
                    };

                    let mut write_nodes = nodes.write().await;
                    if let Some(n) = write_nodes.iter_mut().find(|n| n.id == node.id) {
                        n.status = new_status.clone();
                        n.last_health_check = Some(chrono::Utc::now().to_rfc3339());
                    }

                    let status_str = match new_status {
                        NodeStatus::Healthy => "healthy",
                        NodeStatus::Degraded => "degraded",
                        NodeStatus::Unhealthy => "unhealthy",
                        NodeStatus::Offline => "offline",
                        NodeStatus::Promoting => "promoting",
                    };
                    store.execute(
                        "UPDATE stackhouse_replica_nodes SET status = ?, last_health_check = NOW() WHERE id = ?".to_string(),
                        vec![SqlValue::Text(status_str.to_string()), SqlValue::Text(node.id.clone())],
                    ).await.ok();
                }
            }
        });
    }

    async fn check_node_health(node: &ReplicaNode) -> bool {
        let _url = format!("postgres://{}:{}/{}", node.host, node.port, node.database);
        // Attempt TCP connect as health check
        match tokio::time::timeout(
            Duration::from_secs(5),
            tokio::net::TcpStream::connect(format!("{}:{}", node.host, node.port)),
        )
        .await
        {
            Ok(Ok(_)) => true,
            _ => false,
        }
    }

    /// Route a read query to the best available replica
    pub async fn route_read(&self, tenant_id: i64) -> StackhouseResult<ReplicaNode> {
        self.reads_total.fetch_add(1, Ordering::Relaxed);

        let nodes = self.nodes.read().await;
        let healthy_replicas: Vec<&ReplicaNode> = nodes
            .iter()
            .filter(|n| {
                n.tenant_id == tenant_id
                    && n.role == NodeRole::Replica
                    && n.status == NodeStatus::Healthy
            })
            .collect();

        if healthy_replicas.is_empty() {
            // Fallback to primary
            self.reads_primary.fetch_add(1, Ordering::Relaxed);
            let primary = nodes
                .iter()
                .find(|n| n.tenant_id == tenant_id && n.role == NodeRole::Primary)
                .cloned()
                .ok_or_else(|| {
                    StackhouseError::Internal(anyhow::anyhow!("No primary node found"))
                })?;
            return Ok(primary);
        }

        self.reads_replica.fetch_add(1, Ordering::Relaxed);

        // Round-robin among healthy replicas
        let idx = self.round_robin_index.fetch_add(1, Ordering::Relaxed) as usize
            % healthy_replicas.len();
        Ok(healthy_replicas[idx].clone())
    }

    /// Register a new replica node
    pub async fn register_node(
        &self,
        tenant_id: i64,
        name: &str,
        host: &str,
        port: u16,
        database: &str,
        region: &str,
        role: NodeRole,
    ) -> StackhouseResult<ReplicaNode> {
        let id = uuid::Uuid::new_v4().to_string();
        let role_str = match &role {
            NodeRole::Primary => "primary",
            NodeRole::Replica => "replica",
            NodeRole::Standby => "standby",
        };

        self.store.execute(
            "INSERT INTO stackhouse_replica_nodes (id, tenant_id, name, host, port, database_name, region, role) VALUES (?, ?, ?, ?, ?, ?, ?, ?)".to_string(),
            vec![
                SqlValue::Text(id.clone()),
                SqlValue::Integer(tenant_id),
                SqlValue::Text(name.to_string()),
                SqlValue::Text(host.to_string()),
                SqlValue::Integer(port as i64),
                SqlValue::Text(database.to_string()),
                SqlValue::Text(region.to_string()),
                SqlValue::Text(role_str.to_string()),
            ],
        ).await?;

        let node = ReplicaNode {
            id,
            tenant_id,
            name: name.to_string(),
            host: host.to_string(),
            port,
            database: database.to_string(),
            region: region.to_string(),
            role,
            status: NodeStatus::Healthy,
            replication_lag_ms: 0,
            connections_active: 0,
            connections_max: 100,
            last_health_check: None,
            created_at: chrono::Utc::now().to_rfc3339(),
        };

        self.nodes.write().await.push(node.clone());
        info!("🔄 Replica node registered: {} ({}:{})", name, host, port);
        Ok(node)
    }

    /// Promote a replica to primary (failover)
    pub async fn promote_to_primary(
        &self,
        node_id: &str,
        tenant_id: i64,
    ) -> StackhouseResult<FailoverEvent> {
        let mut nodes = self.nodes.write().await;

        let old_primary = nodes
            .iter()
            .find(|n| n.tenant_id == tenant_id && n.role == NodeRole::Primary);
        let old_primary_id = old_primary.map(|n| n.id.clone()).unwrap_or_default();

        // Demote old primary
        if let Some(old) = nodes
            .iter_mut()
            .find(|n| n.tenant_id == tenant_id && n.role == NodeRole::Primary)
        {
            old.role = NodeRole::Replica;
        }

        // Promote new primary
        let new_primary = nodes
            .iter_mut()
            .find(|n| n.id == node_id)
            .ok_or_else(|| StackhouseError::NotFound("Node not found".into()))?;
        new_primary.role = NodeRole::Primary;
        new_primary.status = NodeStatus::Promoting;

        drop(nodes);

        // Record failover event
        let event_id = uuid::Uuid::new_v4().to_string();
        self.store.execute(
            "INSERT INTO stackhouse_failover_events (id, old_primary_id, new_primary_id, reason, duration_ms) VALUES (?, ?, ?, 'manual_promotion', 0)".to_string(),
            vec![
                SqlValue::Text(event_id.clone()),
                SqlValue::Text(old_primary_id.clone()),
                SqlValue::Text(node_id.to_string()),
            ],
        ).await?;

        // Update DB roles
        self.store.execute(
            "UPDATE stackhouse_replica_nodes SET role = 'replica' WHERE tenant_id = ? AND role = 'primary' AND id != ?".to_string(),
            vec![SqlValue::Integer(tenant_id), SqlValue::Text(node_id.to_string())],
        ).await?;
        self.store.execute(
            "UPDATE stackhouse_replica_nodes SET role = 'primary', status = 'healthy' WHERE id = ?".to_string(),
            vec![SqlValue::Text(node_id.to_string())],
        ).await?;

        info!("⚡ Failover complete: {} promoted to primary", node_id);

        Ok(FailoverEvent {
            id: event_id,
            old_primary_id,
            new_primary_id: node_id.to_string(),
            reason: "manual_promotion".to_string(),
            duration_ms: 0,
            timestamp: chrono::Utc::now().to_rfc3339(),
        })
    }

    /// Get replication stats
    pub async fn get_stats(&self, tenant_id: i64) -> ReplicationStats {
        let nodes = self.nodes.read().await;
        let tenant_nodes: Vec<&ReplicaNode> =
            nodes.iter().filter(|n| n.tenant_id == tenant_id).collect();
        let replicas: Vec<&ReplicaNode> = tenant_nodes
            .iter()
            .filter(|n| n.role == NodeRole::Replica)
            .cloned()
            .collect();

        let avg_lag = if replicas.is_empty() {
            0
        } else {
            replicas.iter().map(|r| r.replication_lag_ms).sum::<u64>() / replicas.len() as u64
        };
        let max_lag = replicas
            .iter()
            .map(|r| r.replication_lag_ms)
            .max()
            .unwrap_or(0);
        let primary_id = tenant_nodes
            .iter()
            .find(|n| n.role == NodeRole::Primary)
            .map(|n| n.id.clone())
            .unwrap_or_default();

        ReplicationStats {
            primary_id,
            replica_count: replicas.len(),
            avg_replication_lag_ms: avg_lag,
            max_replication_lag_ms: max_lag,
            total_reads_routed: self.reads_total.load(Ordering::Relaxed),
            reads_to_primary: self.reads_primary.load(Ordering::Relaxed),
            reads_to_replicas: self.reads_replica.load(Ordering::Relaxed),
        }
    }

    /// List nodes for a tenant
    pub async fn list_nodes(&self, tenant_id: i64) -> Vec<ReplicaNode> {
        let nodes = self.nodes.read().await;
        nodes
            .iter()
            .filter(|n| n.tenant_id == tenant_id)
            .cloned()
            .collect()
    }

    /// Remove a node
    pub async fn remove_node(&self, node_id: &str, tenant_id: i64) -> StackhouseResult<()> {
        self.store
            .execute(
                "DELETE FROM stackhouse_replica_nodes WHERE id = ? AND tenant_id = ?".to_string(),
                vec![
                    SqlValue::Text(node_id.to_string()),
                    SqlValue::Integer(tenant_id),
                ],
            )
            .await?;
        self.nodes.write().await.retain(|n| n.id != node_id);
        Ok(())
    }
}

// ============================================================================
// Router
// ============================================================================

#[derive(Clone)]
pub struct ReplicaState {
    pub replicas: Arc<ReplicaService>,
    pub auth: AuthState,
}

#[derive(Deserialize)]
struct RegisterNodeRequest {
    name: String,
    host: String,
    #[serde(default = "default_port")]
    port: u16,
    #[serde(default = "default_db")]
    database: String,
    #[serde(default = "default_region")]
    region: String,
    #[serde(default = "default_role")]
    role: String,
}
fn default_port() -> u16 {
    5432
}
fn default_db() -> String {
    "postgres".to_string()
}
fn default_region() -> String {
    "us-east-1".to_string()
}
fn default_role() -> String {
    "replica".to_string()
}

async fn register_node_handler(
    State(state): State<ReplicaState>,
    headers: HeaderMap,
    Json(req): Json<RegisterNodeRequest>,
) -> Result<impl IntoResponse, StackhouseError> {
    let user = extract_auth_user(&state.auth, &headers)?;
    let role = match req.role.as_str() {
        "primary" => NodeRole::Primary,
        "standby" => NodeRole::Standby,
        _ => NodeRole::Replica,
    };
    let node = state
        .replicas
        .register_node(
            user.id,
            &req.name,
            &req.host,
            req.port,
            &req.database,
            &req.region,
            role,
        )
        .await?;
    Ok(Json(json!({"success": true, "data": node})))
}

async fn list_nodes_handler(
    State(state): State<ReplicaState>,
    headers: HeaderMap,
) -> Result<impl IntoResponse, StackhouseError> {
    let user = extract_auth_user(&state.auth, &headers)?;
    let nodes = state.replicas.list_nodes(user.id).await;
    Ok(Json(json!({"success": true, "data": nodes})))
}

async fn promote_handler(
    State(state): State<ReplicaState>,
    headers: HeaderMap,
    axum::extract::Path(node_id): axum::extract::Path<String>,
) -> Result<impl IntoResponse, StackhouseError> {
    let user = extract_auth_user(&state.auth, &headers)?;
    let event = state.replicas.promote_to_primary(&node_id, user.id).await?;
    Ok(Json(json!({"success": true, "data": event})))
}

async fn stats_handler(
    State(state): State<ReplicaState>,
    headers: HeaderMap,
) -> Result<impl IntoResponse, StackhouseError> {
    let user = extract_auth_user(&state.auth, &headers)?;
    let stats = state.replicas.get_stats(user.id).await;
    Ok(Json(json!({"success": true, "data": stats})))
}

async fn remove_node_handler(
    State(state): State<ReplicaState>,
    headers: HeaderMap,
    axum::extract::Path(node_id): axum::extract::Path<String>,
) -> Result<impl IntoResponse, StackhouseError> {
    let user = extract_auth_user(&state.auth, &headers)?;
    state.replicas.remove_node(&node_id, user.id).await?;
    Ok(Json(json!({"success": true, "message": "Node removed"})))
}

pub fn create_replicas_router(state: ReplicaState) -> Router {
    Router::new()
        .route("/nodes", post(register_node_handler))
        .route("/nodes", get(list_nodes_handler))
        .route("/nodes/:id/promote", post(promote_handler))
        .route("/nodes/:id", delete(remove_node_handler))
        .route("/stats", get(stats_handler))
        .with_state(state)
}
