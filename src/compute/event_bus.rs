//! # Internal Pub/Sub Event Bus
//!
//! Topic-based event routing with subscriptions, filtering, and replay.

use crate::db::{SqlValue, StackhouseStore};
use crate::error::StackhouseResult;

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::broadcast;
use tokio::sync::RwLock;
use tracing::info;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EventMessage {
    pub id: String,
    pub topic: String,
    pub event_type: String,
    pub payload: Value,
    pub metadata: HashMap<String, String>,
    pub timestamp: String,
    pub tenant_id: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EventSubscription {
    pub id: String,
    pub tenant_id: i64,
    pub topic: String,
    pub filter: Option<String>,
    pub handler_type: HandlerType,
    pub handler_config: Value,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HandlerType {
    Function,
    Webhook,
    Queue,
}

#[derive(Clone)]
pub struct EventBus {
    store: Arc<StackhouseStore>,
    channels: Arc<RwLock<HashMap<String, broadcast::Sender<EventMessage>>>>,
}

impl EventBus {
    pub async fn new(store: Arc<StackhouseStore>) -> StackhouseResult<Self> {
        let bus = Self {
            store,
            channels: Arc::new(RwLock::new(HashMap::new())),
        };
        bus.initialize_tables().await?;
        info!("📨 Event bus initialized");
        Ok(bus)
    }

    async fn initialize_tables(&self) -> StackhouseResult<()> {
        self.store.execute_batch(r#"
            CREATE TABLE IF NOT EXISTS stackhouse_event_topics (
                name TEXT PRIMARY KEY,
                tenant_id BIGINT NOT NULL,
                description TEXT,
                retention_hours INTEGER DEFAULT 168,
                created_at TIMESTAMPTZ DEFAULT NOW()
            );
            CREATE TABLE IF NOT EXISTS stackhouse_event_messages (
                id TEXT PRIMARY KEY,
                topic TEXT NOT NULL,
                event_type TEXT NOT NULL,
                payload JSONB NOT NULL,
                metadata JSONB DEFAULT '{}',
                tenant_id BIGINT NOT NULL,
                timestamp TIMESTAMPTZ DEFAULT NOW()
            );
            CREATE TABLE IF NOT EXISTS stackhouse_event_subscriptions (
                id TEXT PRIMARY KEY,
                tenant_id BIGINT NOT NULL,
                topic TEXT NOT NULL,
                filter TEXT,
                handler_type TEXT NOT NULL,
                handler_config JSONB NOT NULL DEFAULT '{}',
                created_at TIMESTAMPTZ DEFAULT NOW()
            );
            CREATE INDEX IF NOT EXISTS idx_event_messages_topic ON stackhouse_event_messages(topic);
            CREATE INDEX IF NOT EXISTS idx_event_messages_time ON stackhouse_event_messages(timestamp);
            CREATE INDEX IF NOT EXISTS idx_event_subs_topic ON stackhouse_event_subscriptions(topic);
        "#.to_string()).await?;
        Ok(())
    }

    /// Publish an event to a topic
    pub async fn publish(
        &self,
        tenant_id: i64,
        topic: &str,
        event_type: &str,
        payload: Value,
        metadata: HashMap<String, String>,
    ) -> StackhouseResult<EventMessage> {
        let id = uuid::Uuid::new_v4().to_string();
        let msg = EventMessage {
            id: id.clone(),
            topic: topic.to_string(),
            event_type: event_type.to_string(),
            payload: payload.clone(),
            metadata: metadata.clone(),
            timestamp: chrono::Utc::now().to_rfc3339(),
            tenant_id,
        };

        // Persist event
        self.store.execute(
            "INSERT INTO stackhouse_event_messages (id, topic, event_type, payload, metadata, tenant_id) VALUES (?, ?, ?, ?::jsonb, ?::jsonb, ?)".to_string(),
            vec![
                SqlValue::Text(id),
                SqlValue::Text(topic.to_string()),
                SqlValue::Text(event_type.to_string()),
                SqlValue::Text(payload.to_string()),
                SqlValue::Text(serde_json::to_string(&metadata).unwrap_or_default()),
                SqlValue::Integer(tenant_id),
            ],
        ).await?;

        // Broadcast to in-memory subscribers
        let channels = self.channels.read().await;
        if let Some(tx) = channels.get(topic) {
            let _ = tx.send(msg.clone());
        }

        Ok(msg)
    }

    /// Subscribe to a topic
    pub async fn subscribe(
        &self,
        tenant_id: i64,
        topic: &str,
        handler_type: HandlerType,
        handler_config: Value,
        filter: Option<String>,
    ) -> StackhouseResult<EventSubscription> {
        let id = uuid::Uuid::new_v4().to_string();
        let handler_type_str = serde_json::to_string(&handler_type)
            .unwrap_or_default()
            .trim_matches('"')
            .to_string();

        self.store.execute(
            "INSERT INTO stackhouse_event_subscriptions (id, tenant_id, topic, filter, handler_type, handler_config) VALUES (?, ?, ?, ?, ?, ?::jsonb)".to_string(),
            vec![
                SqlValue::Text(id.clone()),
                SqlValue::Integer(tenant_id),
                SqlValue::Text(topic.to_string()),
                SqlValue::Text(filter.clone().unwrap_or_default()),
                SqlValue::Text(handler_type_str),
                SqlValue::Text(handler_config.to_string()),
            ],
        ).await?;

        // Ensure broadcast channel exists
        let mut channels = self.channels.write().await;
        channels.entry(topic.to_string()).or_insert_with(|| {
            let (tx, _) = broadcast::channel(10000);
            tx
        });

        Ok(EventSubscription {
            id,
            tenant_id,
            topic: topic.to_string(),
            filter,
            handler_type,
            handler_config,
            created_at: chrono::Utc::now().to_rfc3339(),
        })
    }

    /// Get a receiver for real-time events on a topic
    pub async fn get_receiver(&self, topic: &str) -> broadcast::Receiver<EventMessage> {
        let mut channels = self.channels.write().await;
        let tx = channels.entry(topic.to_string()).or_insert_with(|| {
            let (tx, _) = broadcast::channel(10000);
            tx
        });
        tx.subscribe()
    }

    /// Replay events from a topic
    pub async fn replay(
        &self,
        topic: &str,
        since: &str,
        limit: usize,
    ) -> StackhouseResult<Vec<EventMessage>> {
        let rows = self.store.query(
            format!("SELECT id, topic, event_type, payload, metadata, tenant_id, timestamp FROM stackhouse_event_messages WHERE topic = ? AND timestamp >= ?::timestamptz ORDER BY timestamp LIMIT {}", limit),
            vec![SqlValue::Text(topic.to_string()), SqlValue::Text(since.to_string())],
        ).await?;

        let messages = rows
            .into_iter()
            .map(|r| {
                let get = |key: &str| r.iter().find(|(k, _)| k == key).map(|(_, v)| v.clone());
                EventMessage {
                    id: get("id")
                        .and_then(|v| v.as_str().map(String::from))
                        .unwrap_or_default(),
                    topic: get("topic")
                        .and_then(|v| v.as_str().map(String::from))
                        .unwrap_or_default(),
                    event_type: get("event_type")
                        .and_then(|v| v.as_str().map(String::from))
                        .unwrap_or_default(),
                    payload: get("payload").unwrap_or(json!({})),
                    metadata: get("metadata")
                        .and_then(|v| v.as_str().and_then(|s| serde_json::from_str(s).ok()))
                        .unwrap_or_default(),
                    timestamp: get("timestamp")
                        .and_then(|v| v.as_str().map(String::from))
                        .unwrap_or_default(),
                    tenant_id: get("tenant_id").and_then(|v| v.as_i64()).unwrap_or(0),
                }
            })
            .collect();

        Ok(messages)
    }

    /// Create a topic
    pub async fn create_topic(
        &self,
        tenant_id: i64,
        name: &str,
        description: &str,
    ) -> StackhouseResult<()> {
        self.store.execute(
            "INSERT INTO stackhouse_event_topics (name, tenant_id, description) VALUES (?, ?, ?) ON CONFLICT (name) DO NOTHING".to_string(),
            vec![
                SqlValue::Text(name.to_string()),
                SqlValue::Integer(tenant_id),
                SqlValue::Text(description.to_string()),
            ],
        ).await?;
        Ok(())
    }

    /// List topics
    pub async fn list_topics(&self, tenant_id: i64) -> StackhouseResult<Vec<Value>> {
        let rows = self.store.query(
            "SELECT name, description, retention_hours, created_at FROM stackhouse_event_topics WHERE tenant_id = ? ORDER BY name".to_string(),
            vec![SqlValue::Integer(tenant_id)],
        ).await?;
        Ok(rows
            .into_iter()
            .map(|r| json!(r.into_iter().collect::<std::collections::HashMap<_, _>>()))
            .collect())
    }
}
