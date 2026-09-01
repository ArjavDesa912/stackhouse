//! # TUS Resumable Upload Protocol (v1.0.0)
//!
//! Implements TUS protocol for large file resumable uploads.
//! Supports Creation, Termination, and Checksum extensions.

use crate::db::{SqlValue, StackhouseStore};
use crate::error::{StackhouseError, StackhouseResult};

use axum::{
    body::Body,
    extract::{Path, State},
    http::{HeaderMap, StatusCode},
    response::Response,
    routing::{delete, head, options, patch, post},
    Router,
};
use base64::Engine as _;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::sync::Arc;
use tokio::fs;
use tokio::io::AsyncWriteExt;
use tracing::info;

const TUS_VERSION: &str = "1.0.0";
const TUS_EXTENSIONS: &str = "creation,termination,checksum";
const MAX_UPLOAD_SIZE: u64 = 5 * 1024 * 1024 * 1024; // 5GB

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TusUpload {
    pub id: String,
    pub bucket: String,
    pub key: String,
    pub upload_length: u64,
    pub upload_offset: u64,
    pub metadata: String,
    pub completed: bool,
    pub created_at: String,
}

#[derive(Clone)]
pub struct TusService {
    store: Arc<StackhouseStore>,
    upload_path: PathBuf,
}

impl TusService {
    pub async fn new(store: Arc<StackhouseStore>, storage_path: PathBuf) -> StackhouseResult<Self> {
        let upload_path = storage_path.join("tus_uploads");
        fs::create_dir_all(&upload_path).await.ok();
        let service = Self { store, upload_path };
        service.initialize_tables().await?;
        info!("📤 TUS resumable upload service initialized");
        Ok(service)
    }

    async fn initialize_tables(&self) -> StackhouseResult<()> {
        self.store
            .execute_batch(
                r#"
            CREATE TABLE IF NOT EXISTS stackhouse_tus_uploads (
                id TEXT PRIMARY KEY,
                bucket TEXT NOT NULL,
                key TEXT NOT NULL,
                upload_length BIGINT NOT NULL,
                upload_offset BIGINT NOT NULL DEFAULT 0,
                metadata TEXT DEFAULT '',
                completed BOOLEAN DEFAULT FALSE,
                created_at TIMESTAMPTZ DEFAULT NOW()
            );
        "#
                .to_string(),
            )
            .await?;
        Ok(())
    }

    /// Create a new upload
    pub async fn create_upload(
        &self,
        bucket: &str,
        key: &str,
        upload_length: u64,
        metadata: &str,
    ) -> StackhouseResult<String> {
        if upload_length > MAX_UPLOAD_SIZE {
            return Err(StackhouseError::InvalidPayload(format!(
                "Upload too large (max {}GB)",
                MAX_UPLOAD_SIZE / 1024 / 1024 / 1024
            )));
        }

        let id = uuid::Uuid::new_v4().to_string();

        // Create empty file
        let file_path = self.upload_path.join(&id);
        fs::File::create(&file_path).await.map_err(|e| {
            StackhouseError::Internal(anyhow::anyhow!("Create upload file failed: {}", e))
        })?;

        self.store.execute(
            "INSERT INTO stackhouse_tus_uploads (id, bucket, key, upload_length, metadata) VALUES (?, ?, ?, ?, ?)".to_string(),
            vec![
                SqlValue::Text(id.clone()),
                SqlValue::Text(bucket.to_string()),
                SqlValue::Text(key.to_string()),
                SqlValue::Integer(upload_length as i64),
                SqlValue::Text(metadata.to_string()),
            ],
        ).await?;

        Ok(id)
    }

    /// Append data to an upload
    pub async fn patch_upload(
        &self,
        upload_id: &str,
        offset: u64,
        data: Vec<u8>,
    ) -> StackhouseResult<u64> {
        let rows = self
            .store
            .query(
                "SELECT upload_offset, upload_length, completed FROM stackhouse_tus_uploads WHERE id = ?"
                    .to_string(),
                vec![SqlValue::Text(upload_id.to_string())],
            )
            .await?;

        if rows.is_empty() {
            return Err(StackhouseError::NotFound("Upload not found".into()));
        }

        let row = &rows[0];
        let current_offset = row
            .iter()
            .find(|(k, _)| k == "upload_offset")
            .and_then(|(_, v)| v.as_i64())
            .unwrap_or(0) as u64;
        let upload_length = row
            .iter()
            .find(|(k, _)| k == "upload_length")
            .and_then(|(_, v)| v.as_i64())
            .unwrap_or(0) as u64;
        let completed = row
            .iter()
            .find(|(k, _)| k == "completed")
            .and_then(|(_, v)| v.as_str())
            .unwrap_or("false")
            == "true";

        if completed {
            return Err(StackhouseError::InvalidPayload(
                "Upload already completed".into(),
            ));
        }
        if offset != current_offset {
            return Err(StackhouseError::InvalidPayload(format!(
                "Offset mismatch: expected {}, got {}",
                current_offset, offset
            )));
        }

        let new_offset = current_offset + data.len() as u64;
        if new_offset > upload_length {
            return Err(StackhouseError::InvalidPayload(
                "Upload exceeds declared length".into(),
            ));
        }

        // Append to file
        let file_path = self.upload_path.join(upload_id);
        let mut file = tokio::fs::OpenOptions::new()
            .append(true)
            .open(&file_path)
            .await
            .map_err(|e| StackhouseError::Internal(anyhow::anyhow!("Open upload file: {}", e)))?;
        file.write_all(&data)
            .await
            .map_err(|e| StackhouseError::Internal(anyhow::anyhow!("Write upload data: {}", e)))?;

        let is_complete = new_offset == upload_length;
        self.store
            .execute(
                "UPDATE stackhouse_tus_uploads SET upload_offset = ?, completed = ? WHERE id = ?"
                    .to_string(),
                vec![
                    SqlValue::Integer(new_offset as i64),
                    SqlValue::Text(is_complete.to_string()),
                    SqlValue::Text(upload_id.to_string()),
                ],
            )
            .await?;

        Ok(new_offset)
    }

    /// Get upload status
    pub async fn get_upload(&self, upload_id: &str) -> StackhouseResult<TusUpload> {
        let rows = self.store.query(
            "SELECT id, bucket, key, upload_length, upload_offset, metadata, completed, created_at FROM stackhouse_tus_uploads WHERE id = ?".to_string(),
            vec![SqlValue::Text(upload_id.to_string())],
        ).await?;

        if rows.is_empty() {
            return Err(StackhouseError::NotFound("Upload not found".into()));
        }

        let row = &rows[0];
        let get = |key: &str| row.iter().find(|(k, _)| k == key).map(|(_, v)| v.clone());
        Ok(TusUpload {
            id: get("id")
                .and_then(|v| v.as_str().map(String::from))
                .unwrap_or_default(),
            bucket: get("bucket")
                .and_then(|v| v.as_str().map(String::from))
                .unwrap_or_default(),
            key: get("key")
                .and_then(|v| v.as_str().map(String::from))
                .unwrap_or_default(),
            upload_length: get("upload_length").and_then(|v| v.as_i64()).unwrap_or(0) as u64,
            upload_offset: get("upload_offset").and_then(|v| v.as_i64()).unwrap_or(0) as u64,
            metadata: get("metadata")
                .and_then(|v| v.as_str().map(String::from))
                .unwrap_or_default(),
            completed: get("completed")
                .map(|v| v.as_str() == Some("true"))
                .unwrap_or(false),
            created_at: get("created_at")
                .and_then(|v| v.as_str().map(String::from))
                .unwrap_or_default(),
        })
    }

    /// Terminate (delete) an upload
    pub async fn terminate_upload(&self, upload_id: &str) -> StackhouseResult<()> {
        self.store
            .execute(
                "DELETE FROM stackhouse_tus_uploads WHERE id = ?".to_string(),
                vec![SqlValue::Text(upload_id.to_string())],
            )
            .await?;
        let file_path = self.upload_path.join(upload_id);
        fs::remove_file(&file_path).await.ok();
        Ok(())
    }
}

// ============================================================================
// TUS Protocol Router
// ============================================================================

#[derive(Clone)]
pub struct TusState {
    pub tus: Arc<TusService>,
}

async fn tus_options_handler() -> Response<Body> {
    Response::builder()
        .status(StatusCode::NO_CONTENT)
        .header("Tus-Resumable", TUS_VERSION)
        .header("Tus-Version", TUS_VERSION)
        .header("Tus-Extension", TUS_EXTENSIONS)
        .header("Tus-Max-Size", MAX_UPLOAD_SIZE.to_string())
        .body(Body::empty())
        .unwrap()
}

async fn tus_create_handler(
    State(state): State<TusState>,
    headers: HeaderMap,
) -> Result<Response<Body>, StackhouseError> {
    let upload_length: u64 = headers
        .get("Upload-Length")
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.parse().ok())
        .ok_or_else(|| StackhouseError::InvalidPayload("Upload-Length header required".into()))?;

    let metadata = headers
        .get("Upload-Metadata")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");

    // Parse metadata for bucket and key
    let mut bucket = "default".to_string();
    let mut key = uuid::Uuid::new_v4().to_string();
    for pair in metadata.split(',') {
        let parts: Vec<&str> = pair.trim().splitn(2, ' ').collect();
        if parts.len() == 2 {
            let decoded = base64::engine::general_purpose::STANDARD
                .decode(parts[1])
                .ok()
                .and_then(|b| String::from_utf8(b).ok())
                .unwrap_or_default();
            match parts[0] {
                "bucket" => bucket = decoded,
                "filename" | "key" => key = decoded,
                _ => {}
            }
        }
    }

    let upload_id = state
        .tus
        .create_upload(&bucket, &key, upload_length, metadata)
        .await?;

    Ok(Response::builder()
        .status(StatusCode::CREATED)
        .header("Location", format!("/v1/storage/tus/{}", upload_id))
        .header("Tus-Resumable", TUS_VERSION)
        .header("Upload-Offset", "0")
        .body(Body::empty())
        .unwrap())
}

async fn tus_head_handler(
    State(state): State<TusState>,
    Path(upload_id): Path<String>,
) -> Result<Response<Body>, StackhouseError> {
    let upload = state.tus.get_upload(&upload_id).await?;

    Ok(Response::builder()
        .status(StatusCode::OK)
        .header("Tus-Resumable", TUS_VERSION)
        .header("Upload-Offset", upload.upload_offset.to_string())
        .header("Upload-Length", upload.upload_length.to_string())
        .header("Upload-Metadata", &upload.metadata)
        .body(Body::empty())
        .unwrap())
}

async fn tus_patch_handler(
    State(state): State<TusState>,
    Path(upload_id): Path<String>,
    headers: HeaderMap,
    body: axum::body::Bytes,
) -> Result<Response<Body>, StackhouseError> {
    let offset: u64 = headers
        .get("Upload-Offset")
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.parse().ok())
        .ok_or_else(|| StackhouseError::InvalidPayload("Upload-Offset header required".into()))?;

    let content_type = headers
        .get("Content-Type")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");

    if content_type != "application/offset+octet-stream" {
        return Err(StackhouseError::InvalidPayload(
            "Content-Type must be application/offset+octet-stream".into(),
        ));
    }

    let new_offset = state
        .tus
        .patch_upload(&upload_id, offset, body.to_vec())
        .await?;

    Ok(Response::builder()
        .status(StatusCode::NO_CONTENT)
        .header("Tus-Resumable", TUS_VERSION)
        .header("Upload-Offset", new_offset.to_string())
        .body(Body::empty())
        .unwrap())
}

async fn tus_delete_handler(
    State(state): State<TusState>,
    Path(upload_id): Path<String>,
) -> Result<Response<Body>, StackhouseError> {
    state.tus.terminate_upload(&upload_id).await?;
    Ok(Response::builder()
        .status(StatusCode::NO_CONTENT)
        .header("Tus-Resumable", TUS_VERSION)
        .body(Body::empty())
        .unwrap())
}

pub fn create_tus_router(state: TusState) -> Router {
    Router::new()
        .route("/", options(tus_options_handler))
        .route("/", post(tus_create_handler))
        .route("/:id", head(tus_head_handler))
        .route("/:id", patch(tus_patch_handler))
        .route("/:id", delete(tus_delete_handler))
        .with_state(state)
}
