//! # Broadcast Channels Module (Stackhouse-Broadcast)
//!
//! Pub/sub broadcast channels for real-time communication.
//! Supports named channels, message history, and fan-out.

use axum::{
    extract::State,
    response::IntoResponse,
    routing::{get, post},
    Json, Router,
};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::HashMap;
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};
use tokio::sync::{broadcast, RwLock};
use tracing::info;

const MAX_HISTORY: usize = 100;
const CHANNEL_CAPACITY: usize = 1000;

#[derive(Clone, Debug, Serialize)]
pub struct BroadcastMessage {
    pub channel: String,
    pub event: String,
    pub payload: Value,
    pub sender_id: Option<String>,
    pub timestamp: u64,
}

struct ChannelState {
    sender: broadcast::Sender<BroadcastMessage>,
    history: Vec<BroadcastMessage>,
    subscriber_count: usize,
}

#[derive(Clone)]
pub struct BroadcastService {
    channels: Arc<RwLock<HashMap<String, ChannelState>>>,
}

impl BroadcastService {
    pub fn new() -> Self {
        info!("📢 Stackhouse-Broadcast initialized");
        Self {
            channels: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    pub async fn send(
        &self,
        channel: &str,
        event: &str,
        payload: Value,
        sender_id: Option<String>,
    ) {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs();
        let msg = BroadcastMessage {
            channel: channel.to_string(),
            event: event.to_string(),
            payload,
            sender_id,
            timestamp: now,
        };

        let mut channels = self.channels.write().await;
        let state = channels.entry(channel.to_string()).or_insert_with(|| {
            let (sender, _) = broadcast::channel(CHANNEL_CAPACITY);
            ChannelState {
                sender,
                history: Vec::new(),
                subscriber_count: 0,
            }
        });

        let _ = state.sender.send(msg.clone());
        state.history.push(msg);
        if state.history.len() > MAX_HISTORY {
            state.history.remove(0);
        }
    }

    pub async fn subscribe(&self, channel: &str) -> broadcast::Receiver<BroadcastMessage> {
        let mut channels = self.channels.write().await;
        let state = channels.entry(channel.to_string()).or_insert_with(|| {
            let (sender, _) = broadcast::channel(CHANNEL_CAPACITY);
            ChannelState {
                sender,
                history: Vec::new(),
                subscriber_count: 0,
            }
        });
        state.subscriber_count += 1;
        state.sender.subscribe()
    }

    pub async fn get_history(&self, channel: &str, limit: usize) -> Vec<BroadcastMessage> {
        let channels = self.channels.read().await;
        channels
            .get(channel)
            .map(|s| {
                s.history
                    .iter()
                    .rev()
                    .take(limit)
                    .cloned()
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default()
    }

    pub async fn list_channels(&self) -> Vec<Value> {
        let channels = self.channels.read().await;
        channels
            .iter()
            .map(|(name, state)| {
                json!({
                    "name": name,
                    "subscribers": state.subscriber_count,
                    "history_count": state.history.len(),
                })
            })
            .collect()
    }
}

// ============================================================================
// Handlers & Router
// ============================================================================

#[derive(Clone)]
pub struct BroadcastState {
    pub broadcast: Arc<BroadcastService>,
}

#[derive(Deserialize)]
struct SendRequest {
    channel: String,
    event: String,
    payload: Value,
    #[serde(default)]
    sender_id: Option<String>,
}

async fn send_handler(
    State(state): State<BroadcastState>,
    Json(req): Json<SendRequest>,
) -> impl IntoResponse {
    state
        .broadcast
        .send(&req.channel, &req.event, req.payload, req.sender_id)
        .await;
    Json(json!({"success": true, "message": "Message broadcast"}))
}

async fn history_handler(
    State(state): State<BroadcastState>,
    axum::extract::Path(channel): axum::extract::Path<String>,
) -> impl IntoResponse {
    let history = state.broadcast.get_history(&channel, 50).await;
    Json(json!({"success": true, "data": history}))
}

async fn list_channels_handler(State(state): State<BroadcastState>) -> impl IntoResponse {
    let channels = state.broadcast.list_channels().await;
    Json(json!({"success": true, "data": channels}))
}

pub fn create_broadcast_router(state: BroadcastState) -> Router {
    Router::new()
        .route("/broadcast/send", post(send_handler))
        .route("/broadcast/channels", get(list_channels_handler))
        .route("/broadcast/:channel/history", get(history_handler))
        .with_state(state)
}
