//! # Realtime Presence Module (Stackhouse-Presence)
//!
//! User presence tracking for collaborative apps.
//! Tracks online/offline status, typing indicators, custom states.

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
use tokio::sync::RwLock;
use tracing::info;

const PRESENCE_TIMEOUT_SECS: u64 = 30;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct PresenceEntry {
    pub user_id: String,
    pub channel: String,
    pub status: String,
    pub metadata: Value,
    pub last_seen: u64,
}

#[derive(Clone)]
pub struct PresenceService {
    state: Arc<RwLock<HashMap<String, HashMap<String, PresenceEntry>>>>,
}

impl PresenceService {
    pub fn new() -> Self {
        let service = Self {
            state: Arc::new(RwLock::new(HashMap::new())),
        };

        // Background cleanup task
        let state_clone = Arc::clone(&service.state);
        tokio::spawn(async move {
            loop {
                tokio::time::sleep(tokio::time::Duration::from_secs(10)).await;
                let now = SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .unwrap()
                    .as_secs();
                let mut state = state_clone.write().await;
                for (_channel, users) in state.iter_mut() {
                    users.retain(|_, entry| now - entry.last_seen < PRESENCE_TIMEOUT_SECS);
                }
                state.retain(|_, users| !users.is_empty());
            }
        });

        info!("👥 Stackhouse-Presence initialized");
        service
    }

    pub async fn track(&self, channel: &str, user_id: &str, status: &str, metadata: Value) {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs();
        let entry = PresenceEntry {
            user_id: user_id.to_string(),
            channel: channel.to_string(),
            status: status.to_string(),
            metadata,
            last_seen: now,
        };

        let mut state = self.state.write().await;
        state
            .entry(channel.to_string())
            .or_insert_with(HashMap::new)
            .insert(user_id.to_string(), entry);
    }

    pub async fn untrack(&self, channel: &str, user_id: &str) {
        let mut state = self.state.write().await;
        if let Some(users) = state.get_mut(channel) {
            users.remove(user_id);
        }
    }

    pub async fn get_channel(&self, channel: &str) -> Vec<PresenceEntry> {
        let state = self.state.read().await;
        state
            .get(channel)
            .map(|users| users.values().cloned().collect())
            .unwrap_or_default()
    }

    pub async fn get_channels(&self) -> Vec<String> {
        let state = self.state.read().await;
        state.keys().cloned().collect()
    }
}

// ============================================================================
// Handlers & Router
// ============================================================================

#[derive(Clone)]
pub struct PresenceState {
    pub presence: Arc<PresenceService>,
}

#[derive(Deserialize)]
struct TrackRequest {
    channel: String,
    user_id: String,
    #[serde(default = "default_status")]
    status: String,
    #[serde(default)]
    metadata: Value,
}
fn default_status() -> String {
    "online".to_string()
}

#[derive(Deserialize)]
struct UntrackRequest {
    channel: String,
    user_id: String,
}

async fn track_handler(
    State(state): State<PresenceState>,
    Json(req): Json<TrackRequest>,
) -> impl IntoResponse {
    state
        .presence
        .track(&req.channel, &req.user_id, &req.status, req.metadata)
        .await;
    Json(json!({"success": true}))
}

async fn untrack_handler(
    State(state): State<PresenceState>,
    Json(req): Json<UntrackRequest>,
) -> impl IntoResponse {
    state.presence.untrack(&req.channel, &req.user_id).await;
    Json(json!({"success": true}))
}

async fn channel_handler(
    State(state): State<PresenceState>,
    axum::extract::Path(channel): axum::extract::Path<String>,
) -> impl IntoResponse {
    let users = state.presence.get_channel(&channel).await;
    Json(json!({"success": true, "data": users, "count": users.len()}))
}

async fn channels_handler(State(state): State<PresenceState>) -> impl IntoResponse {
    let channels = state.presence.get_channels().await;
    Json(json!({"success": true, "data": channels}))
}

pub fn create_presence_router(state: PresenceState) -> Router {
    Router::new()
        .route("/presence/track", post(track_handler))
        .route("/presence/untrack", post(untrack_handler))
        .route("/presence/:channel", get(channel_handler))
        .route("/presence", get(channels_handler))
        .with_state(state)
}
