//! # Change Data Capture (CDC) Subscriptions
//!
//! Postgres logical replication slot management, WAL → event stream,
//! push to WebSocket/SSE clients.

use crate::auth::{extract_auth_user, AuthState};
use crate::db::{SqlValue, StackhouseStore};
use crate::error::{StackhouseError, StackhouseResult};

use axum::response::sse::{Event, KeepAlive};
use axum::{
    extract::State,
    http::HeaderMap,
    response::{IntoResponse, Sse},
    routing::{delete, get, post},
    Json, Router,
};
use base64::Engine;
use futures::stream::Stream;
use pg_walstream::{EventType, Lsn, PgOutputDecoder};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::HashMap;
use std::convert::Infallible;
use std::str::FromStr;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::broadcast;
use tokio::sync::RwLock;
use tracing::{info, warn};

// ============================================================================
// Types
// ============================================================================

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CdcSubscription {
    pub id: String,
    pub tenant_id: i64,
    pub name: String,
    pub table_name: String,
    pub operations: Vec<CdcOperation>,
    pub filter: Option<String>, // SQL WHERE clause
    pub delivery: DeliveryMethod,
    pub status: SubscriptionStatus,
    pub created_at: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum CdcOperation {
    Insert,
    Update,
    Delete,
    Truncate,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DeliveryMethod {
    Sse,
    Webhook { url: String, secret: String },
    WebSocket,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SubscriptionStatus {
    Active,
    Paused,
    Error,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CdcEvent {
    pub id: String,
    pub subscription_id: String,
    pub table_name: String,
    pub operation: String,
    pub old_data: Option<Value>,
    pub new_data: Option<Value>,
    pub timestamp: String,
    pub lsn: String,
}

fn op_str_from_cdc_op(op: &CdcOperation) -> &'static str {
    match op {
        CdcOperation::Insert => "insert",
        CdcOperation::Update => "update",
        CdcOperation::Delete => "delete",
        CdcOperation::Truncate => "truncate",
    }
}

fn op_str(event_type: &EventType) -> &'static str {
    match event_type {
        EventType::Insert { .. } => "insert",
        EventType::Update { .. } => "update",
        EventType::Delete { .. } => "delete",
        EventType::Truncate(_) => "truncate",
        _ => "",
    }
}

// ============================================================================
// CDC Service
// ============================================================================

#[derive(Clone)]
pub struct CdcService {
    store: Arc<StackhouseStore>,
    event_bus: broadcast::Sender<CdcEvent>,
    subscriptions: Arc<RwLock<HashMap<String, CdcSubscription>>>,
    last_processed_lsn: Arc<RwLock<HashMap<String, String>>>,
}

impl CdcService {
    pub async fn new(store: Arc<StackhouseStore>) -> StackhouseResult<Self> {
        let (tx, _) = broadcast::channel(10000);
        let service = Self {
            store,
            event_bus: tx,
            subscriptions: Arc::new(RwLock::new(HashMap::new())),
            last_processed_lsn: Arc::new(RwLock::new(HashMap::new())),
        };
        service.initialize_tables().await?;
        service.initialize_publication_and_slot().await?;
        service.load_subscriptions().await?;
        service.start_wal_listener();
        info!("📡 CDC (Change Data Capture) service initialized");
        Ok(service)
    }

    async fn initialize_publication_and_slot(&self) -> StackhouseResult<()> {
        // One publication for all CDC tables; each subscription adds its table here.
        self.store
            .execute_simple("CREATE PUBLICATION IF NOT EXISTS stackhouse_cdc_pub".to_string())
            .await
            .ok();

        // Logical replication slot for the built-in pgoutput decoder.
        let slot_sql = r#"
            DO $$
            BEGIN
                PERFORM pg_create_logical_replication_slot('stackhouse_cdc_slot', 'pgoutput');
            EXCEPTION
                WHEN duplicate_object THEN
                    RAISE NOTICE 'replication slot stackhouse_cdc_slot already exists';
            END $$;
        "#;
        self.store.execute(slot_sql.to_string(), vec![]).await.ok();
        Ok(())
    }

    async fn initialize_tables(&self) -> StackhouseResult<()> {
        self.store.execute_batch(r#"
            CREATE TABLE IF NOT EXISTS stackhouse_cdc_subscriptions (
                id TEXT PRIMARY KEY,
                tenant_id BIGINT NOT NULL,
                name TEXT NOT NULL,
                table_name TEXT NOT NULL,
                operations TEXT NOT NULL DEFAULT '["insert","update","delete"]',
                filter TEXT,
                delivery TEXT NOT NULL DEFAULT '{"sse": null}',
                status TEXT NOT NULL DEFAULT 'active',
                created_at TIMESTAMPTZ DEFAULT NOW()
            );
            CREATE TABLE IF NOT EXISTS stackhouse_cdc_events (
                id TEXT PRIMARY KEY,
                subscription_id TEXT NOT NULL,
                table_name TEXT NOT NULL,
                operation TEXT NOT NULL,
                old_data JSONB,
                new_data JSONB,
                lsn TEXT,
                timestamp TIMESTAMPTZ DEFAULT NOW()
            );
            CREATE INDEX IF NOT EXISTS idx_cdc_subs_tenant ON stackhouse_cdc_subscriptions(tenant_id);
            CREATE INDEX IF NOT EXISTS idx_cdc_events_sub ON stackhouse_cdc_events(subscription_id);
            CREATE INDEX IF NOT EXISTS idx_cdc_events_time ON stackhouse_cdc_events(timestamp);
        "#.to_string()).await?;
        Ok(())
    }

    async fn load_subscriptions(&self) -> StackhouseResult<()> {
        let rows = self.store.query(
            "SELECT id, tenant_id, name, table_name, operations, filter, delivery, status, created_at FROM stackhouse_cdc_subscriptions WHERE status = 'active'".to_string(),
            vec![],
        ).await?;

        let mut subs = self.subscriptions.write().await;
        for row in rows {
            let get = |key: &str| row.iter().find(|(k, _)| k == key).map(|(_, v)| v.clone());
            let id = get("id")
                .and_then(|v| v.as_str().map(String::from))
                .unwrap_or_default();
            let operations: Vec<CdcOperation> = get("operations")
                .and_then(|v| v.as_str().and_then(|s| serde_json::from_str(s).ok()))
                .unwrap_or_else(|| {
                    vec![
                        CdcOperation::Insert,
                        CdcOperation::Update,
                        CdcOperation::Delete,
                    ]
                });

            subs.insert(
                id.clone(),
                CdcSubscription {
                    id,
                    tenant_id: get("tenant_id").and_then(|v| v.as_i64()).unwrap_or(0),
                    name: get("name")
                        .and_then(|v| v.as_str().map(String::from))
                        .unwrap_or_default(),
                    table_name: get("table_name")
                        .and_then(|v| v.as_str().map(String::from))
                        .unwrap_or_default(),
                    operations,
                    filter: get("filter").and_then(|v| v.as_str().map(String::from)),
                    delivery: DeliveryMethod::Sse,
                    status: SubscriptionStatus::Active,
                    created_at: get("created_at")
                        .and_then(|v| v.as_str().map(String::from))
                        .unwrap_or_default(),
                },
            );
        }
        Ok(())
    }

    fn start_wal_listener(&self) {
        let store = Arc::clone(&self.store);
        let event_bus = self.event_bus.clone();
        let subscriptions = Arc::clone(&self.subscriptions);
        let last_processed_lsn = Arc::clone(&self.last_processed_lsn);

        tokio::spawn(async move {
            if let Err(e) =
                Self::poll_slot(store, event_bus, subscriptions, last_processed_lsn).await
            {
                warn!("CDC WAL listener exited: {}", e);
            }
        });
    }

    async fn poll_slot(
        store: Arc<StackhouseStore>,
        event_bus: broadcast::Sender<CdcEvent>,
        subscriptions: Arc<RwLock<HashMap<String, CdcSubscription>>>,
        last_processed_lsn: Arc<RwLock<HashMap<String, String>>>,
    ) -> StackhouseResult<()> {
        let mut interval = tokio::time::interval(Duration::from_millis(500));
        let mut decoder = PgOutputDecoder::with_protocol_version(1);

        loop {
            interval.tick().await;

            let sql = "SELECT lsn, xid, data FROM pg_logical_slot_get_binary_changes('stackhouse_cdc_slot', NULL, 1000, 'proto_version', '1', 'publication_names', 'stackhouse_cdc_pub')";
            let rows = match store.query(sql.to_string(), vec![]).await {
                Ok(rows) => rows,
                Err(e) => {
                    warn!("CDC slot read failed: {}", e);
                    continue;
                }
            };

            for row in rows {
                let get = |key: &str| row.iter().find(|(k, _)| k == key).map(|(_, v)| v.clone());
                let lsn_str = get("lsn")
                    .and_then(|v| v.as_str().map(String::from))
                    .unwrap_or_default();
                let data_b64 = get("data")
                    .and_then(|v| v.as_str().map(String::from))
                    .unwrap_or_default();

                if lsn_str.is_empty() || data_b64.is_empty() {
                    continue;
                }

                let lsn = match Lsn::from_str(&lsn_str) {
                    Ok(lsn) => lsn,
                    Err(e) => {
                        warn!("CDC failed to parse LSN '{}': {}", lsn_str, e);
                        continue;
                    }
                };

                let data = match base64::engine::general_purpose::STANDARD.decode(&data_b64) {
                    Ok(v) => v,
                    Err(e) => {
                        warn!("CDC failed to decode base64 data: {}", e);
                        continue;
                    }
                };

                let change = match decoder.decode_message(data, lsn) {
                    Ok(Some(change)) => change,
                    Ok(None) => continue,
                    Err(e) => {
                        warn!("CDC failed to decode pgoutput message: {}", e);
                        continue;
                    }
                };

                let events = Self::change_event_to_cdc_events(&subscriptions, change).await;
                for event in events {
                    let _ = event_bus.send(event.clone());
                    if !event.lsn.is_empty() {
                        let mut guard = last_processed_lsn.write().await;
                        guard.insert("stackhouse_cdc_slot".to_string(), event.lsn.clone());
                    }
                }
            }
        }
    }

    async fn change_event_to_cdc_events(
        subscriptions: &Arc<RwLock<HashMap<String, CdcSubscription>>>,
        change: pg_walstream::ChangeEvent,
    ) -> Vec<CdcEvent> {
        let base_event = |table: &str, op: &str, old: Option<Value>, new: Option<Value>| CdcEvent {
            id: uuid::Uuid::new_v4().to_string(),
            subscription_id: String::new(),
            table_name: table.to_string(),
            operation: op.to_string(),
            old_data: old,
            new_data: new,
            timestamp: chrono::Utc::now().to_rfc3339(),
            lsn: change.lsn.to_string(),
        };

        let subs = subscriptions.read().await;

        let operation = op_str(&change.event_type);
        let table_name: String;
        let old_data: Option<Value>;
        let new_data: Option<Value>;

        match &change.event_type {
            EventType::Insert { table, data, .. } => {
                table_name = table.to_string();
                new_data = data.deserialize_into::<Value>().ok();
                old_data = None;
            }
            EventType::Update {
                table,
                old_data: old,
                new_data: new,
                ..
            } => {
                table_name = table.to_string();
                old_data = old
                    .as_ref()
                    .and_then(|r| r.deserialize_into::<Value>().ok());
                new_data = new.deserialize_into::<Value>().ok();
            }
            EventType::Delete {
                table,
                old_data: old,
                ..
            } => {
                table_name = table.to_string();
                old_data = old.deserialize_into::<Value>().ok();
                new_data = None;
            }
            EventType::Truncate(tables) => {
                let mut events = Vec::new();
                for t in tables {
                    for sub in subs.values() {
                        if sub.table_name == **t && sub.operations.contains(&CdcOperation::Truncate)
                        {
                            let mut ev = base_event(t, "truncate", None, None);
                            ev.subscription_id = sub.id.clone();
                            events.push(ev);
                        }
                    }
                }
                return events;
            }
            _ => return Vec::new(),
        };

        let mut events = Vec::new();
        for sub in subs.values() {
            if sub.table_name == table_name
                && sub
                    .operations
                    .iter()
                    .any(|o| op_str_from_cdc_op(o) == operation)
            {
                let mut ev =
                    base_event(&table_name, &operation, old_data.clone(), new_data.clone());
                ev.subscription_id = sub.id.clone();
                events.push(ev);
            }
        }

        events
    }

    /// Create a new CDC subscription
    pub async fn create_subscription(
        &self,
        tenant_id: i64,
        name: &str,
        table_name: &str,
        operations: Vec<CdcOperation>,
        filter: Option<String>,
    ) -> StackhouseResult<CdcSubscription> {
        let id = uuid::Uuid::new_v4().to_string();

        // Add the table to the shared CDC publication and ensure full row images
        // are available in the WAL (required for old/new tuples on updates/deletes).
        let table_ident = pg_walstream::quote_ident(table_name)
            .map_err(|e| StackhouseError::Database(format!("invalid table identifier: {:?}", e)))?;
        let pub_sql = format!(
            r#"
            CREATE PUBLICATION IF NOT EXISTS stackhouse_cdc_pub;
            DO $$
            BEGIN
                ALTER PUBLICATION stackhouse_cdc_pub ADD TABLE {table_ident};
            EXCEPTION
                WHEN duplicate_object THEN
                    RAISE NOTICE 'table already in publication';
            END $$;
            ALTER TABLE {table_ident} REPLICA IDENTITY FULL;
            "#
        );
        self.store.execute(pub_sql, vec![]).await.ok();

        self.store.execute(
            "INSERT INTO stackhouse_cdc_subscriptions (id, tenant_id, name, table_name, operations, filter) VALUES (?, ?, ?, ?, ?, ?)".to_string(),
            vec![
                SqlValue::Text(id.clone()),
                SqlValue::Integer(tenant_id),
                SqlValue::Text(name.to_string()),
                SqlValue::Text(table_name.to_string()),
                SqlValue::Text(serde_json::to_string(&operations).unwrap_or_default()),
                SqlValue::Text(filter.clone().unwrap_or_default()),
            ],
        ).await?;

        let sub = CdcSubscription {
            id: id.clone(),
            tenant_id,
            name: name.to_string(),
            table_name: table_name.to_string(),
            operations,
            filter,
            delivery: DeliveryMethod::Sse,
            status: SubscriptionStatus::Active,
            created_at: chrono::Utc::now().to_rfc3339(),
        };

        self.subscriptions.write().await.insert(id, sub.clone());
        info!("📡 CDC subscription created for table: {}", table_name);
        Ok(sub)
    }

    /// Subscribe to SSE event stream
    pub fn subscribe_sse(
        &self,
        subscription_id: String,
    ) -> impl Stream<Item = Result<Event, Infallible>> {
        let mut rx = self.event_bus.subscribe();
        let sub_id = subscription_id;

        async_stream::stream! {
            loop {
                match rx.recv().await {
                    Ok(event) => {
                        if event.subscription_id == sub_id || sub_id == "*" {
                            let data = serde_json::to_string(&event).unwrap_or_default();
                            yield Ok(Event::default().data(data).event("change"));
                        }
                    }
                    Err(broadcast::error::RecvError::Lagged(n)) => {
                        yield Ok(Event::default().data(format!("{{\"warning\":\"missed {} events\"}}", n)).event("lag"));
                    }
                    Err(_) => break,
                }
            }
        }
    }

    /// List subscriptions for a tenant
    pub async fn list_subscriptions(&self, tenant_id: i64) -> StackhouseResult<Vec<Value>> {
        let rows = self.store.query(
            "SELECT id, name, table_name, operations, filter, status, created_at FROM stackhouse_cdc_subscriptions WHERE tenant_id = ? ORDER BY created_at DESC".to_string(),
            vec![SqlValue::Integer(tenant_id)],
        ).await?;
        Ok(rows
            .into_iter()
            .map(|r| json!(r.into_iter().collect::<std::collections::HashMap<_, _>>()))
            .collect())
    }

    /// Delete a subscription
    pub async fn delete_subscription(&self, sub_id: &str, tenant_id: i64) -> StackhouseResult<()> {
        let removed = {
            let mut subs = self.subscriptions.write().await;
            subs.remove(sub_id)
        };

        self.store
            .execute(
                "DELETE FROM stackhouse_cdc_subscriptions WHERE id = ? AND tenant_id = ?"
                    .to_string(),
                vec![
                    SqlValue::Text(sub_id.to_string()),
                    SqlValue::Integer(tenant_id),
                ],
            )
            .await?;

        if let Some(sub) = removed {
            let subs = self.subscriptions.read().await;
            let still_watched = subs.values().any(|s| s.table_name == sub.table_name);
            drop(subs);

            if !still_watched {
                let table_ident = pg_walstream::quote_ident(&sub.table_name).map_err(|e| {
                    StackhouseError::Database(format!("invalid table identifier: {:?}", e))
                })?;
                let drop_sql = format!(
                    r#"
                    DO $$
                    BEGIN
                        ALTER PUBLICATION stackhouse_cdc_pub DROP TABLE {table_ident};
                    EXCEPTION
                        WHEN undefined_table OR duplicate_object THEN
                            RAISE NOTICE 'table not in publication';
                    END $$;
                    "#
                );
                self.store.execute(drop_sql, vec![]).await.ok();
            }
        }

        Ok(())
    }

    /// Emit a CDC event (called from mutation handlers)
    pub fn emit_event(&self, event: CdcEvent) {
        let _ = self.event_bus.send(event);
    }
}

// ============================================================================
// Router
// ============================================================================

#[derive(Clone)]
pub struct CdcState {
    pub cdc: Arc<CdcService>,
    pub auth: AuthState,
}

#[derive(Deserialize)]
struct CreateSubscriptionRequest {
    name: String,
    table_name: String,
    #[serde(default = "default_operations")]
    operations: Vec<String>,
    #[serde(default)]
    filter: Option<String>,
}
fn default_operations() -> Vec<String> {
    vec!["insert".into(), "update".into(), "delete".into()]
}

async fn create_subscription_handler(
    State(state): State<CdcState>,
    headers: HeaderMap,
    Json(req): Json<CreateSubscriptionRequest>,
) -> Result<impl IntoResponse, StackhouseError> {
    let user = extract_auth_user(&state.auth, &headers)?;
    let ops: Vec<CdcOperation> = req
        .operations
        .iter()
        .filter_map(|o| match o.as_str() {
            "insert" => Some(CdcOperation::Insert),
            "update" => Some(CdcOperation::Update),
            "delete" => Some(CdcOperation::Delete),
            "truncate" => Some(CdcOperation::Truncate),
            _ => None,
        })
        .collect();
    let sub = state
        .cdc
        .create_subscription(user.id, &req.name, &req.table_name, ops, req.filter)
        .await?;
    Ok(Json(json!({"success": true, "data": sub})))
}

async fn list_subscriptions_handler(
    State(state): State<CdcState>,
    headers: HeaderMap,
) -> Result<impl IntoResponse, StackhouseError> {
    let user = extract_auth_user(&state.auth, &headers)?;
    let subs = state.cdc.list_subscriptions(user.id).await?;
    Ok(Json(json!({"success": true, "data": subs})))
}

async fn delete_subscription_handler(
    State(state): State<CdcState>,
    headers: HeaderMap,
    axum::extract::Path(sub_id): axum::extract::Path<String>,
) -> Result<impl IntoResponse, StackhouseError> {
    let user = extract_auth_user(&state.auth, &headers)?;
    state.cdc.delete_subscription(&sub_id, user.id).await?;
    Ok(Json(
        json!({"success": true, "message": "Subscription deleted"}),
    ))
}

async fn sse_handler(
    State(state): State<CdcState>,
    headers: HeaderMap,
    axum::extract::Path(sub_id): axum::extract::Path<String>,
) -> Result<Sse<impl Stream<Item = Result<Event, Infallible>>>, StackhouseError> {
    let _user = extract_auth_user(&state.auth, &headers)?;
    let stream = state.cdc.subscribe_sse(sub_id);
    Ok(Sse::new(stream).keep_alive(KeepAlive::default()))
}

pub fn create_cdc_router(state: CdcState) -> Router {
    Router::new()
        .route("/subscriptions", post(create_subscription_handler))
        .route("/subscriptions", get(list_subscriptions_handler))
        .route("/subscriptions/:id", delete(delete_subscription_handler))
        .route("/stream/:id", get(sse_handler))
        .with_state(state)
}
