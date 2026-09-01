//! # Realtime Module (Stackhouse-Realtime)
//!
//! Provides WebSocket-based realtime subscriptions for Stackhouse.
//! Clients can subscribe to table-level or row-level changes and receive
//! instant push notifications when data changes.
//!
//! ## Architecture
//!
//! Uses PostgreSQL LISTEN/NOTIFY for change detection combined with
//! tokio broadcast channels for WebSocket fan-out. This provides
//! reliable, low-latency realtime without requiring logical replication setup.
//!
//! ## Protocol
//!
//! Clients connect via WebSocket and send JSON messages to subscribe:
//! ```json
//! { "type": "subscribe", "table": "users", "event": "INSERT" }
//! { "type": "subscribe", "table": "users", "event": "*" }
//! { "type": "unsubscribe", "table": "users" }
//! ```
//!
//! Server pushes events:
//! ```json
//! { "type": "INSERT", "table": "users", "record": {...}, "timestamp": "..." }
//! { "type": "UPDATE", "table": "users", "old": {...}, "new": {...} }
//! { "type": "DELETE", "table": "users", "old": {...} }
//! ```

pub mod broadcast;
pub mod presence;

pub use broadcast::*;
pub use presence::*;

use axum::{
    extract::{
        ws::{Message, WebSocket, WebSocketUpgrade},
        State,
    },
    response::IntoResponse,
    routing::get,
    Router,
};
use dashmap::DashMap;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::sync::Arc;
use tokio::sync::broadcast as tokio_broadcast;
use tracing::{debug, info};

// ============================================================================
// Core Types
// ============================================================================

/// Types of realtime events
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "UPPERCASE")]
pub enum RealtimeEventType {
    Insert,
    Update,
    Delete,
    #[serde(rename = "*")]
    All,
}

/// A realtime event pushed to subscribers
#[derive(Debug, Clone, Serialize)]
pub struct RealtimeEvent {
    #[serde(rename = "type")]
    pub event_type: String,
    pub table: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub record: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub old_record: Option<Value>,
    pub timestamp: String,
}

/// Client subscription message
#[derive(Debug, Deserialize)]
pub struct SubscriptionMessage {
    #[serde(rename = "type")]
    pub msg_type: String,
    #[serde(default)]
    pub table: Option<String>,
    #[serde(default)]
    pub event: Option<String>,
    #[serde(default)]
    pub filter: Option<String>,
}

// ============================================================================
// Realtime Engine
// ============================================================================

/// The realtime engine manages subscriptions and event broadcasting
#[derive(Clone)]
pub struct RealtimeEngine {
    /// Channel map: "table_name" -> broadcast sender
    channels: Arc<DashMap<String, tokio_broadcast::Sender<RealtimeEvent>>>,
    /// Connected client count for monitoring
    client_count: Arc<std::sync::atomic::AtomicUsize>,
}

/// Shared state for realtime routes
#[derive(Clone)]
pub struct RealtimeState {
    pub realtime: RealtimeEngine,
}

impl RealtimeEngine {
    /// Create a new RealtimeEngine
    pub fn new() -> Self {
        info!("⚡ Initializing Realtime Engine...");
        Self {
            channels: Arc::new(DashMap::new()),
            client_count: Arc::new(std::sync::atomic::AtomicUsize::new(0)),
        }
    }

    /// Get or create a broadcast channel for a table
    pub fn get_channel(&self, table: &str) -> tokio_broadcast::Sender<RealtimeEvent> {
        self.channels
            .entry(table.to_string())
            .or_insert_with(|| {
                let (tx, _) = tokio_broadcast::channel(256);
                tx
            })
            .clone()
    }

    /// Broadcast an event to all subscribers of a table
    pub fn broadcast(&self, event: RealtimeEvent) {
        let tx = self.get_channel(&event.table);
        let table = event.table.clone();
        let event_type = event.event_type.clone();
        match tx.send(event) {
            Ok(count) => {
                debug!(
                    "📡 Broadcast {} event on '{}' to {} subscribers",
                    event_type, table, count
                );
            }
            Err(_) => {
                // No active subscribers — that's fine
            }
        }
    }

    /// Broadcast an INSERT event
    pub fn broadcast_insert(&self, table: &str, record: Value) {
        self.broadcast(RealtimeEvent {
            event_type: "INSERT".to_string(),
            table: table.to_string(),
            record: Some(record),
            old_record: None,
            timestamp: chrono::Utc::now().to_rfc3339(),
        });
    }

    /// Broadcast an UPDATE event
    pub fn broadcast_update(&self, table: &str, old_record: Option<Value>, new_record: Value) {
        self.broadcast(RealtimeEvent {
            event_type: "UPDATE".to_string(),
            table: table.to_string(),
            record: Some(new_record),
            old_record,
            timestamp: chrono::Utc::now().to_rfc3339(),
        });
    }

    /// Broadcast a DELETE event
    pub fn broadcast_delete(&self, table: &str, old_record: Value) {
        self.broadcast(RealtimeEvent {
            event_type: "DELETE".to_string(),
            table: table.to_string(),
            record: None,
            old_record: Some(old_record),
            timestamp: chrono::Utc::now().to_rfc3339(),
        });
    }

    /// Get the number of connected clients
    pub fn connected_clients(&self) -> usize {
        self.client_count.load(std::sync::atomic::Ordering::Relaxed)
    }

    /// Get the number of active channels
    pub fn active_channels(&self) -> usize {
        self.channels.len()
    }

    /// Handle a WebSocket connection
    async fn handle_socket(&self, mut socket: WebSocket) {
        let client_id = self
            .client_count
            .fetch_add(1, std::sync::atomic::Ordering::Relaxed)
            + 1;
        info!("🔌 Realtime client #{} connected", client_id);

        // Send welcome message
        let welcome = json!({
            "type": "connected",
            "message": "Connected to Stackhouse Realtime",
            "client_id": client_id,
        });
        if socket
            .send(Message::Text(welcome.to_string().into()))
            .await
            .is_err()
        {
            return;
        }

        // Track this client's subscriptions
        let mut subscriptions: Vec<(String, tokio_broadcast::Receiver<RealtimeEvent>)> = Vec::new();

        loop {
            // First: drain any pending events from subscriptions and send them
            let mut pending_events: Vec<String> = Vec::new();
            for (_table, rx) in &mut subscriptions {
                while let Ok(event) = rx.try_recv() {
                    if let Ok(event_json) = serde_json::to_string(&event) {
                        pending_events.push(event_json);
                    }
                }
            }

            // Send all pending events
            let mut disconnected = false;
            for event_json in pending_events {
                if socket.send(Message::Text(event_json.into())).await.is_err() {
                    disconnected = true;
                    break;
                }
            }
            if disconnected {
                break;
            }

            // Then: wait for an incoming message with a short timeout
            // This allows us to loop back and check subscriptions frequently
            let msg =
                tokio::time::timeout(tokio::time::Duration::from_millis(50), socket.recv()).await;

            match msg {
                Ok(Some(Ok(Message::Text(text)))) => {
                    match serde_json::from_str::<SubscriptionMessage>(&text) {
                        Ok(sub_msg) => match sub_msg.msg_type.as_str() {
                            "subscribe" => {
                                if let Some(table) = &sub_msg.table {
                                    let tx = self.get_channel(table);
                                    let rx = tx.subscribe();
                                    subscriptions.push((table.clone(), rx));

                                    let ack = json!({
                                        "type": "subscribed",
                                        "table": table,
                                        "event": sub_msg.event.as_deref().unwrap_or("*"),
                                    });
                                    if socket
                                        .send(Message::Text(ack.to_string().into()))
                                        .await
                                        .is_err()
                                    {
                                        break;
                                    }
                                    info!("📡 Client #{} subscribed to '{}'", client_id, table);
                                }
                            }
                            "unsubscribe" => {
                                if let Some(table) = &sub_msg.table {
                                    subscriptions.retain(|(t, _)| t != table);
                                    let ack = json!({
                                        "type": "unsubscribed",
                                        "table": table,
                                    });
                                    if socket
                                        .send(Message::Text(ack.to_string().into()))
                                        .await
                                        .is_err()
                                    {
                                        break;
                                    }
                                }
                            }
                            "ping" => {
                                let pong = json!({ "type": "pong" });
                                if socket
                                    .send(Message::Text(pong.to_string().into()))
                                    .await
                                    .is_err()
                                {
                                    break;
                                }
                            }
                            _ => {
                                let err = json!({
                                    "type": "error",
                                    "message": format!("Unknown message type: {}", sub_msg.msg_type),
                                });
                                let _ = socket.send(Message::Text(err.to_string().into())).await;
                            }
                        },
                        Err(e) => {
                            let err = json!({
                                "type": "error",
                                "message": format!("Invalid message format: {}", e),
                            });
                            let _ = socket.send(Message::Text(err.to_string().into())).await;
                        }
                    }
                }
                Ok(Some(Ok(Message::Close(_)))) | Ok(None) => break,
                Ok(Some(Err(_))) => break,
                Err(_) => {} // Timeout — loop back to check subscriptions
                _ => {}      // Ignore binary, ping, pong
            }
        }

        self.client_count
            .fetch_sub(1, std::sync::atomic::Ordering::Relaxed);
        info!("🔌 Realtime client #{} disconnected", client_id);
    }
}

// ============================================================================
// HTTP Handlers
// ============================================================================

/// GET /v1/realtime — WebSocket upgrade endpoint
async fn realtime_ws_handler(
    ws: WebSocketUpgrade,
    State(state): State<RealtimeState>,
) -> impl IntoResponse {
    ws.on_upgrade(move |socket| async move {
        state.realtime.handle_socket(socket).await;
    })
}

/// GET /v1/realtime/status — Realtime engine status
async fn realtime_status_handler(State(state): State<RealtimeState>) -> impl IntoResponse {
    axum::Json(json!({
        "success": true,
        "data": {
            "connected_clients": state.realtime.connected_clients(),
            "active_channels": state.realtime.active_channels(),
        }
    }))
}

// ============================================================================
// Router
// ============================================================================

/// Creates the realtime router with WebSocket support
pub fn create_realtime_router(state: RealtimeState) -> Router {
    Router::new()
        .route("/", get(realtime_ws_handler))
        .route("/status", get(realtime_status_handler))
        .with_state(state)
}
