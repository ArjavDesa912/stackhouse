//! # S3-Compatible Object Storage API
//!
//! Implements a subset of the AWS S3 REST API for compatibility with existing tools.
//! Supports PutObject, GetObject, DeleteObject, ListBucket, multipart uploads.

use crate::db::{SqlValue, StackhouseStore};
use crate::error::{StackhouseError, StackhouseResult};

use axum::{
    body::Body,
    extract::{Path, Query, State},
    http::{header, HeaderMap, StatusCode},
    response::{IntoResponse, Response},
    routing::{delete, get, head, put},
    Router,
};
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::fs;
use tokio::io::AsyncWriteExt;
use tracing::info;

// ============================================================================
// Types
// ============================================================================

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct S3Object {
    pub key: String,
    pub size: u64,
    pub etag: String,
    pub content_type: String,
    pub last_modified: String,
    pub version_id: Option<String>,
    pub storage_class: String,
}

#[derive(Debug, Clone, Serialize)]
struct ListBucketResult {
    name: String,
    prefix: String,
    max_keys: u32,
    is_truncated: bool,
    contents: Vec<S3Object>,
    common_prefixes: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MultipartUpload {
    pub upload_id: String,
    pub key: String,
    pub bucket: String,
    pub parts: Vec<UploadPart>,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UploadPart {
    pub part_number: u32,
    pub etag: String,
    pub size: u64,
}

// ============================================================================
// S3 Service
// ============================================================================

#[derive(Clone)]
pub struct S3CompatService {
    store: Arc<StackhouseStore>,
    storage_path: PathBuf,
}

impl S3CompatService {
    pub async fn new(store: Arc<StackhouseStore>, storage_path: PathBuf) -> StackhouseResult<Self> {
        let service = Self {
            store,
            storage_path,
        };
        service.initialize_tables().await?;
        info!("☁️ S3-compatible storage API initialized");
        Ok(service)
    }

    async fn initialize_tables(&self) -> StackhouseResult<()> {
        self.store.execute_batch(r#"
            CREATE TABLE IF NOT EXISTS stackhouse_s3_objects (
                id BIGSERIAL PRIMARY KEY,
                bucket TEXT NOT NULL,
                key TEXT NOT NULL,
                size BIGINT NOT NULL DEFAULT 0,
                etag TEXT NOT NULL,
                content_type TEXT DEFAULT 'application/octet-stream',
                storage_class TEXT DEFAULT 'STANDARD',
                version_id TEXT,
                metadata JSONB DEFAULT '{}',
                last_modified TIMESTAMPTZ DEFAULT NOW(),
                UNIQUE(bucket, key, version_id)
            );
            CREATE TABLE IF NOT EXISTS stackhouse_s3_multipart_uploads (
                upload_id TEXT PRIMARY KEY,
                bucket TEXT NOT NULL,
                key TEXT NOT NULL,
                created_at TIMESTAMPTZ DEFAULT NOW()
            );
            CREATE TABLE IF NOT EXISTS stackhouse_s3_upload_parts (
                upload_id TEXT NOT NULL REFERENCES stackhouse_s3_multipart_uploads(upload_id) ON DELETE CASCADE,
                part_number INTEGER NOT NULL,
                etag TEXT NOT NULL,
                size BIGINT NOT NULL,
                PRIMARY KEY (upload_id, part_number)
            );
            CREATE INDEX IF NOT EXISTS idx_s3_objects_bucket_key ON stackhouse_s3_objects(bucket, key);
        "#.to_string()).await?;
        Ok(())
    }

    /// PUT Object
    pub async fn put_object(
        &self,
        bucket: &str,
        key: &str,
        body: Vec<u8>,
        content_type: &str,
        metadata: Option<serde_json::Value>,
    ) -> StackhouseResult<S3Object> {
        let size = body.len() as u64;
        let etag = format!("\"{}\"", Self::compute_etag(&body));

        // Write to filesystem
        let file_path = self.object_path(bucket, key);
        if let Some(parent) = file_path.parent() {
            fs::create_dir_all(parent).await.map_err(|e| {
                StackhouseError::Internal(anyhow::anyhow!("Failed to create directory: {}", e))
            })?;
        }
        let mut file = fs::File::create(&file_path).await.map_err(|e| {
            StackhouseError::Internal(anyhow::anyhow!("Failed to create file: {}", e))
        })?;
        file.write_all(&body).await.map_err(|e| {
            StackhouseError::Internal(anyhow::anyhow!("Failed to write file: {}", e))
        })?;

        // Upsert metadata
        self.store.execute(
            r#"INSERT INTO stackhouse_s3_objects (bucket, key, size, etag, content_type, metadata, last_modified)
               VALUES (?, ?, ?, ?, ?, ?::jsonb, NOW())
               ON CONFLICT (bucket, key, version_id) DO UPDATE SET size = EXCLUDED.size, etag = EXCLUDED.etag,
               content_type = EXCLUDED.content_type, metadata = EXCLUDED.metadata, last_modified = NOW()"#.to_string(),
            vec![
                SqlValue::Text(bucket.to_string()),
                SqlValue::Text(key.to_string()),
                SqlValue::Integer(size as i64),
                SqlValue::Text(etag.clone()),
                SqlValue::Text(content_type.to_string()),
                SqlValue::Text(metadata.unwrap_or(json!({})).to_string()),
            ],
        ).await?;

        Ok(S3Object {
            key: key.to_string(),
            size,
            etag,
            content_type: content_type.to_string(),
            last_modified: chrono::Utc::now().to_rfc3339(),
            version_id: None,
            storage_class: "STANDARD".to_string(),
        })
    }

    /// GET Object
    pub async fn get_object(
        &self,
        bucket: &str,
        key: &str,
    ) -> StackhouseResult<(Vec<u8>, S3Object)> {
        let rows = self.store.query(
            "SELECT size, etag, content_type, last_modified FROM stackhouse_s3_objects WHERE bucket = ? AND key = ? ORDER BY last_modified DESC LIMIT 1".to_string(),
            vec![SqlValue::Text(bucket.to_string()), SqlValue::Text(key.to_string())],
        ).await?;

        if rows.is_empty() {
            return Err(StackhouseError::NotFound("NoSuchKey".into()));
        }

        let row = &rows[0];
        let file_path = self.object_path(bucket, key);
        let body = fs::read(&file_path)
            .await
            .map_err(|_| StackhouseError::NotFound("Object data not found on disk".into()))?;

        let obj = S3Object {
            key: key.to_string(),
            size: row
                .iter()
                .find(|(k, _)| k == "size")
                .and_then(|(_, v)| v.as_i64())
                .unwrap_or(0) as u64,
            etag: row
                .iter()
                .find(|(k, _)| k == "etag")
                .and_then(|(_, v)| v.as_str())
                .unwrap_or("")
                .to_string(),
            content_type: row
                .iter()
                .find(|(k, _)| k == "content_type")
                .and_then(|(_, v)| v.as_str())
                .unwrap_or("application/octet-stream")
                .to_string(),
            last_modified: row
                .iter()
                .find(|(k, _)| k == "last_modified")
                .and_then(|(_, v)| v.as_str())
                .unwrap_or("")
                .to_string(),
            version_id: None,
            storage_class: "STANDARD".to_string(),
        };

        Ok((body, obj))
    }

    /// DELETE Object
    pub async fn delete_object(&self, bucket: &str, key: &str) -> StackhouseResult<()> {
        self.store
            .execute(
                "DELETE FROM stackhouse_s3_objects WHERE bucket = ? AND key = ?".to_string(),
                vec![
                    SqlValue::Text(bucket.to_string()),
                    SqlValue::Text(key.to_string()),
                ],
            )
            .await?;

        let file_path = self.object_path(bucket, key);
        fs::remove_file(&file_path).await.ok();
        Ok(())
    }

    /// HEAD Object
    pub async fn head_object(&self, bucket: &str, key: &str) -> StackhouseResult<S3Object> {
        let rows = self.store.query(
            "SELECT size, etag, content_type, last_modified FROM stackhouse_s3_objects WHERE bucket = ? AND key = ? LIMIT 1".to_string(),
            vec![SqlValue::Text(bucket.to_string()), SqlValue::Text(key.to_string())],
        ).await?;

        if rows.is_empty() {
            return Err(StackhouseError::NotFound("NoSuchKey".into()));
        }

        let row = &rows[0];
        Ok(S3Object {
            key: key.to_string(),
            size: row
                .iter()
                .find(|(k, _)| k == "size")
                .and_then(|(_, v)| v.as_i64())
                .unwrap_or(0) as u64,
            etag: row
                .iter()
                .find(|(k, _)| k == "etag")
                .and_then(|(_, v)| v.as_str())
                .unwrap_or("")
                .to_string(),
            content_type: row
                .iter()
                .find(|(k, _)| k == "content_type")
                .and_then(|(_, v)| v.as_str())
                .unwrap_or("")
                .to_string(),
            last_modified: row
                .iter()
                .find(|(k, _)| k == "last_modified")
                .and_then(|(_, v)| v.as_str())
                .unwrap_or("")
                .to_string(),
            version_id: None,
            storage_class: "STANDARD".to_string(),
        })
    }

    /// List objects in a bucket
    pub async fn list_objects(
        &self,
        bucket: &str,
        prefix: &str,
        max_keys: u32,
        delimiter: Option<&str>,
    ) -> StackhouseResult<ListBucketResult> {
        let rows = self.store.query(
            format!("SELECT key, size, etag, content_type, last_modified, storage_class FROM stackhouse_s3_objects WHERE bucket = ? AND key LIKE '{}%' ORDER BY key LIMIT {}", prefix, max_keys),
            vec![SqlValue::Text(bucket.to_string())],
        ).await?;

        let mut contents = Vec::new();
        let mut common_prefixes = Vec::new();

        for row in &rows {
            let key = row
                .iter()
                .find(|(k, _)| k == "key")
                .and_then(|(_, v)| v.as_str())
                .unwrap_or("");

            if let Some(delim) = delimiter {
                let after_prefix = &key[prefix.len()..];
                if let Some(pos) = after_prefix.find(delim) {
                    let cp = format!("{}{}{}", prefix, &after_prefix[..pos], delim);
                    if !common_prefixes.contains(&cp) {
                        common_prefixes.push(cp);
                    }
                    continue;
                }
            }

            contents.push(S3Object {
                key: key.to_string(),
                size: row
                    .iter()
                    .find(|(k, _)| k == "size")
                    .and_then(|(_, v)| v.as_i64())
                    .unwrap_or(0) as u64,
                etag: row
                    .iter()
                    .find(|(k, _)| k == "etag")
                    .and_then(|(_, v)| v.as_str())
                    .unwrap_or("")
                    .to_string(),
                content_type: row
                    .iter()
                    .find(|(k, _)| k == "content_type")
                    .and_then(|(_, v)| v.as_str())
                    .unwrap_or("")
                    .to_string(),
                last_modified: row
                    .iter()
                    .find(|(k, _)| k == "last_modified")
                    .and_then(|(_, v)| v.as_str())
                    .unwrap_or("")
                    .to_string(),
                version_id: None,
                storage_class: row
                    .iter()
                    .find(|(k, _)| k == "storage_class")
                    .and_then(|(_, v)| v.as_str())
                    .unwrap_or("STANDARD")
                    .to_string(),
            });
        }

        Ok(ListBucketResult {
            name: bucket.to_string(),
            prefix: prefix.to_string(),
            max_keys,
            is_truncated: contents.len() >= max_keys as usize,
            contents,
            common_prefixes,
        })
    }

    /// Initiate multipart upload
    pub async fn create_multipart_upload(
        &self,
        bucket: &str,
        key: &str,
    ) -> StackhouseResult<String> {
        let upload_id = uuid::Uuid::new_v4().to_string();
        self.store
            .execute(
                "INSERT INTO stackhouse_s3_multipart_uploads (upload_id, bucket, key) VALUES (?, ?, ?)"
                    .to_string(),
                vec![
                    SqlValue::Text(upload_id.clone()),
                    SqlValue::Text(bucket.to_string()),
                    SqlValue::Text(key.to_string()),
                ],
            )
            .await?;
        Ok(upload_id)
    }

    /// Upload a part
    pub async fn upload_part(
        &self,
        upload_id: &str,
        part_number: u32,
        body: Vec<u8>,
    ) -> StackhouseResult<String> {
        let etag = format!("\"{}\"", Self::compute_etag(&body));
        let size = body.len() as u64;

        // Store part on disk
        let part_path = self
            .storage_path
            .join("multipart")
            .join(upload_id)
            .join(format!("{}", part_number));
        if let Some(parent) = part_path.parent() {
            fs::create_dir_all(parent).await.ok();
        }
        fs::write(&part_path, &body)
            .await
            .map_err(|e| StackhouseError::Internal(anyhow::anyhow!("Part write failed: {}", e)))?;

        self.store.execute(
            "INSERT INTO stackhouse_s3_upload_parts (upload_id, part_number, etag, size) VALUES (?, ?, ?, ?) ON CONFLICT (upload_id, part_number) DO UPDATE SET etag = EXCLUDED.etag, size = EXCLUDED.size".to_string(),
            vec![
                SqlValue::Text(upload_id.to_string()),
                SqlValue::Integer(part_number as i64),
                SqlValue::Text(etag.clone()),
                SqlValue::Integer(size as i64),
            ],
        ).await?;

        Ok(etag)
    }

    /// Complete multipart upload
    pub async fn complete_multipart_upload(&self, upload_id: &str) -> StackhouseResult<S3Object> {
        let upload_rows = self
            .store
            .query(
                "SELECT bucket, key FROM stackhouse_s3_multipart_uploads WHERE upload_id = ?"
                    .to_string(),
                vec![SqlValue::Text(upload_id.to_string())],
            )
            .await?;

        if upload_rows.is_empty() {
            return Err(StackhouseError::NotFound("Upload not found".into()));
        }

        let row = &upload_rows[0];
        let bucket = row
            .iter()
            .find(|(k, _)| k == "bucket")
            .and_then(|(_, v)| v.as_str())
            .unwrap_or("")
            .to_string();
        let key = row
            .iter()
            .find(|(k, _)| k == "key")
            .and_then(|(_, v)| v.as_str())
            .unwrap_or("")
            .to_string();

        // Get parts in order
        let part_rows = self.store.query(
            "SELECT part_number, size FROM stackhouse_s3_upload_parts WHERE upload_id = ? ORDER BY part_number".to_string(),
            vec![SqlValue::Text(upload_id.to_string())],
        ).await?;

        // Concatenate parts
        let file_path = self.object_path(&bucket, &key);
        if let Some(parent) = file_path.parent() {
            fs::create_dir_all(parent).await.ok();
        }
        let mut output = fs::File::create(&file_path)
            .await
            .map_err(|e| StackhouseError::Internal(anyhow::anyhow!("File create failed: {}", e)))?;

        let mut total_size: u64 = 0;
        for part_row in &part_rows {
            let part_num = part_row
                .iter()
                .find(|(k, _)| k == "part_number")
                .and_then(|(_, v)| v.as_i64())
                .unwrap_or(0);
            let part_path = self
                .storage_path
                .join("multipart")
                .join(upload_id)
                .join(format!("{}", part_num));
            if let Ok(data) = fs::read(&part_path).await {
                total_size += data.len() as u64;
                output.write_all(&data).await.ok();
            }
        }

        let etag = format!(
            "\"{}-{}\"",
            uuid::Uuid::new_v4()
                .to_string()
                .split('-')
                .next()
                .unwrap_or(""),
            part_rows.len()
        );

        // Record in metadata
        self.store.execute(
            "INSERT INTO stackhouse_s3_objects (bucket, key, size, etag, content_type, last_modified) VALUES (?, ?, ?, ?, 'application/octet-stream', NOW())".to_string(),
            vec![
                SqlValue::Text(bucket.clone()),
                SqlValue::Text(key.clone()),
                SqlValue::Integer(total_size as i64),
                SqlValue::Text(etag.clone()),
            ],
        ).await?;

        // Cleanup multipart
        self.store
            .execute(
                "DELETE FROM stackhouse_s3_multipart_uploads WHERE upload_id = ?".to_string(),
                vec![SqlValue::Text(upload_id.to_string())],
            )
            .await
            .ok();
        fs::remove_dir_all(self.storage_path.join("multipart").join(upload_id))
            .await
            .ok();

        Ok(S3Object {
            key,
            size: total_size,
            etag,
            content_type: "application/octet-stream".to_string(),
            last_modified: chrono::Utc::now().to_rfc3339(),
            version_id: None,
            storage_class: "STANDARD".to_string(),
        })
    }

    fn object_path(&self, bucket: &str, key: &str) -> PathBuf {
        self.storage_path.join("s3").join(bucket).join(key)
    }

    fn compute_etag(data: &[u8]) -> String {
        use sha2::{Digest, Sha256};
        let hash = Sha256::digest(data);
        hex::encode(&hash[..16])
    }
}

// ============================================================================
// S3-compatible Router (REST-style)
// ============================================================================

#[derive(Clone)]
pub struct S3State {
    pub s3: Arc<S3CompatService>,
}

#[derive(Deserialize)]
struct ListQuery {
    #[serde(default)]
    prefix: String,
    #[serde(default = "default_max_keys")]
    max_keys: u32,
    #[serde(default)]
    delimiter: Option<String>,
}
fn default_max_keys() -> u32 {
    1000
}

async fn put_object_handler(
    State(state): State<S3State>,
    Path((bucket, key)): Path<(String, String)>,
    headers: HeaderMap,
    body: axum::body::Bytes,
) -> Result<impl IntoResponse, StackhouseError> {
    let content_type = headers
        .get("content-type")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("application/octet-stream");
    let obj = state
        .s3
        .put_object(&bucket, &key, body.to_vec(), content_type, None)
        .await?;
    Ok((StatusCode::OK, [(header::ETAG, obj.etag)], ""))
}

async fn get_object_handler(
    State(state): State<S3State>,
    Path((bucket, key)): Path<(String, String)>,
) -> Result<Response<Body>, StackhouseError> {
    let (data, obj) = state.s3.get_object(&bucket, &key).await?;
    Ok(Response::builder()
        .status(StatusCode::OK)
        .header(header::CONTENT_TYPE, obj.content_type)
        .header(header::CONTENT_LENGTH, obj.size.to_string())
        .header(header::ETAG, obj.etag)
        .body(Body::from(data))
        .unwrap())
}

async fn head_object_handler(
    State(state): State<S3State>,
    Path((bucket, key)): Path<(String, String)>,
) -> Result<Response<Body>, StackhouseError> {
    let obj = state.s3.head_object(&bucket, &key).await?;
    Ok(Response::builder()
        .status(StatusCode::OK)
        .header(header::CONTENT_TYPE, obj.content_type)
        .header(header::CONTENT_LENGTH, obj.size.to_string())
        .header(header::ETAG, obj.etag)
        .body(Body::empty())
        .unwrap())
}

async fn delete_object_handler(
    State(state): State<S3State>,
    Path((bucket, key)): Path<(String, String)>,
) -> Result<impl IntoResponse, StackhouseError> {
    state.s3.delete_object(&bucket, &key).await?;
    Ok(StatusCode::NO_CONTENT)
}

async fn list_objects_handler(
    State(state): State<S3State>,
    Path(bucket): Path<String>,
    Query(params): Query<ListQuery>,
) -> Result<impl IntoResponse, StackhouseError> {
    let result = state
        .s3
        .list_objects(
            &bucket,
            &params.prefix,
            params.max_keys,
            params.delimiter.as_deref(),
        )
        .await?;
    Ok(axum::Json(json!(result)))
}

pub fn create_s3_router(state: S3State) -> Router {
    Router::new()
        .route("/:bucket", get(list_objects_handler))
        .route("/:bucket/*key", put(put_object_handler))
        .route("/:bucket/*key", get(get_object_handler))
        .route("/:bucket/*key", head(head_object_handler))
        .route("/:bucket/*key", delete(delete_object_handler))
        .with_state(state)
}
