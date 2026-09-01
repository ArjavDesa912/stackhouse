//! # Storage Module (Stackhouse-Storage)
//!
//! Provides file storage capabilities for Stackhouse, similar to Supabase Storage.
//!
//! ## Features
//! - Bucket-based organization (public/private)
//! - File upload, download, delete, list operations
//! - SQLite metadata tracking with filesystem storage
//!
//! ## System Tables
//! - `stackhouse_buckets` - Stores bucket configuration
//! - `stackhouse_objects` - Tracks file metadata

pub mod acl;
pub mod backups;
pub mod cdn;
pub mod explorer;
pub mod lifecycle;
pub mod s3_compat;
pub mod scanning;
pub mod tus;
pub mod vectors;
pub mod versioning;

pub use acl::*;
pub use backups::*;
pub use cdn::*;
pub use explorer::*;
pub use lifecycle::*;
pub use s3_compat::*;
pub use scanning::*;
pub use tus::*;
pub use vectors::*;
pub use versioning::*;

use crate::db::{SqlValue, StackhouseStore};
use crate::error::{StackhouseError, StackhouseResult};

use axum::{
    extract::{Multipart, Path, Query, State},
    http::{header, StatusCode},
    response::IntoResponse,
    routing::{delete, get, post},
    Json, Router,
};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::path::PathBuf;
use std::sync::Arc;
use tokio::fs;
use tokio::io::AsyncWriteExt;
use tracing::{debug, info};

// ============================================================================
// Configuration
// ============================================================================

/// Default storage directory (relative to current working directory)
const DEFAULT_STORAGE_PATH: &str = "./stackhouse_storage";

/// Maximum file size (100 MB)
const MAX_FILE_SIZE: usize = 100 * 1024 * 1024;

// ============================================================================
// Core Types
// ============================================================================

/// Storage service managing buckets and files
#[derive(Clone)]
pub struct StorageService {
    store: Arc<StackhouseStore>,
    storage_path: PathBuf,
}

/// Bucket metadata
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Bucket {
    pub id: i64,
    pub name: String,
    pub public: bool,
    pub created_at: String,
    pub owner_id: Option<i64>,
}

/// Storage object metadata
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StorageObject {
    pub id: i64,
    pub bucket_name: String,
    pub path: String,
    pub size: i64,
    pub mime_type: String,
    pub created_at: String,
    pub updated_at: String,
    pub owner_id: Option<i64>,
}

// ============================================================================
// Request/Response DTOs
// ============================================================================

#[derive(Debug, Deserialize)]
pub struct CreateBucketRequest {
    pub name: String,
    #[serde(default)]
    pub public: bool,
}

#[derive(Debug, Deserialize)]
pub struct ListObjectsQuery {
    #[serde(default)]
    pub prefix: Option<String>,
    #[serde(default = "default_limit")]
    pub limit: i64,
    #[serde(default)]
    pub offset: i64,
}

fn default_limit() -> i64 {
    100
}

// ============================================================================
// StorageService Implementation
// ============================================================================

impl StorageService {
    /// Creates a new StorageService
    pub async fn new(
        store: Arc<StackhouseStore>,
        storage_path: Option<PathBuf>,
    ) -> StackhouseResult<Self> {
        let path = storage_path.unwrap_or_else(|| PathBuf::from(DEFAULT_STORAGE_PATH));

        let service = Self {
            store,
            storage_path: path,
        };

        // Initialize tables
        service.initialize_tables().await?;

        info!(
            "📁 Stackhouse-Storage initialized at {:?}",
            service.storage_path
        );
        Ok(service)
    }

    /// Initialize storage tables
    async fn initialize_tables(&self) -> StackhouseResult<()> {
        // Create buckets table
        self.store
            .execute_batch(
                r#"
            CREATE TABLE IF NOT EXISTS stackhouse_buckets (
                id SERIAL PRIMARY KEY,
                name TEXT UNIQUE NOT NULL,
                public INTEGER DEFAULT 0,
                owner_id INTEGER,
                created_at TIMESTAMP WITH TIME ZONE DEFAULT CURRENT_TIMESTAMP,
                FOREIGN KEY (owner_id) REFERENCES stackhouse_users(id) ON DELETE SET NULL
            );
            CREATE INDEX IF NOT EXISTS idx_stackhouse_buckets_name ON stackhouse_buckets(name);
            "#
                .to_string(),
            )
            .await?;

        // Create objects table
        self.store
            .execute_batch(
                r#"
            CREATE TABLE IF NOT EXISTS stackhouse_objects (
                id SERIAL PRIMARY KEY,
                bucket_name TEXT NOT NULL,
                path TEXT NOT NULL,
                size INTEGER NOT NULL,
                mime_type TEXT NOT NULL,
                owner_id INTEGER,
                created_at TIMESTAMP WITH TIME ZONE DEFAULT CURRENT_TIMESTAMP,
                updated_at TIMESTAMP WITH TIME ZONE DEFAULT CURRENT_TIMESTAMP,
                UNIQUE(bucket_name, path),
                FOREIGN KEY (bucket_name) REFERENCES stackhouse_buckets(name) ON DELETE CASCADE,
                FOREIGN KEY (owner_id) REFERENCES stackhouse_users(id) ON DELETE SET NULL
            );
            CREATE INDEX IF NOT EXISTS idx_stackhouse_objects_bucket ON stackhouse_objects(bucket_name);
            CREATE INDEX IF NOT EXISTS idx_stackhouse_objects_path ON stackhouse_objects(bucket_name, path);
            "#
                .to_string(),
            )
            .await?;

        debug!("Storage tables initialized");
        Ok(())
    }

    /// Ensure storage directory exists
    async fn ensure_storage_dir(&self) -> StackhouseResult<()> {
        fs::create_dir_all(&self.storage_path).await.map_err(|e| {
            StackhouseError::Storage(format!("Failed to create storage directory: {}", e))
        })
    }

    /// Get the file path for an object
    fn get_file_path(&self, bucket: &str, path: &str) -> PathBuf {
        self.storage_path.join(bucket).join(path)
    }

    /// Validate bucket name
    fn validate_bucket_name(&self, name: &str) -> StackhouseResult<()> {
        if name.is_empty() || name.len() > 63 {
            return Err(StackhouseError::InvalidPayload(
                "Bucket name must be 1-63 characters".to_string(),
            ));
        }

        // Only lowercase letters, numbers, and hyphens
        if !name
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
        {
            return Err(StackhouseError::InvalidPayload(
                "Bucket name can only contain lowercase letters, numbers, and hyphens".to_string(),
            ));
        }

        // Must start with a letter
        if !name
            .chars()
            .next()
            .map(|c| c.is_ascii_lowercase())
            .unwrap_or(false)
        {
            return Err(StackhouseError::InvalidPayload(
                "Bucket name must start with a letter".to_string(),
            ));
        }

        Ok(())
    }

    /// Validate object path
    fn validate_object_path(&self, path: &str) -> StackhouseResult<()> {
        if path.is_empty() || path.len() > 1024 {
            return Err(StackhouseError::InvalidPayload(
                "Object path must be 1-1024 characters".to_string(),
            ));
        }

        // Prevent path traversal
        if path.contains("..") || path.starts_with('/') {
            return Err(StackhouseError::InvalidPayload(
                "Invalid object path".to_string(),
            ));
        }

        Ok(())
    }

    // ========================================================================
    // Bucket Operations
    // ========================================================================

    /// Create a new bucket
    pub async fn create_bucket(
        &self,
        req: CreateBucketRequest,
        owner_id: Option<i64>,
    ) -> StackhouseResult<Bucket> {
        self.validate_bucket_name(&req.name)?;

        // Check if bucket already exists
        let existing = self
            .store
            .query(
                "SELECT id FROM stackhouse_buckets WHERE name = ?".to_string(),
                vec![SqlValue::Text(req.name.clone())],
            )
            .await?;

        if !existing.is_empty() {
            return Err(StackhouseError::Conflict(
                "Bucket already exists".to_string(),
            ));
        }

        // Insert bucket
        self.store
            .execute(
                "INSERT INTO stackhouse_buckets (name, public, owner_id) VALUES (?, ?, ?::bigint)"
                    .to_string(),
                vec![
                    SqlValue::Text(req.name.clone()),
                    SqlValue::Integer(if req.public { 1 } else { 0 }),
                    owner_id.map(SqlValue::Integer).unwrap_or(SqlValue::Null),
                ],
            )
            .await?;

        info!("Created bucket: {}", req.name);
        self.get_bucket(&req.name).await
    }

    /// Get bucket by name
    pub async fn get_bucket(&self, name: &str) -> StackhouseResult<Bucket> {
        let rows = self
            .store
            .query(
                "SELECT id, name, public, owner_id, created_at FROM stackhouse_buckets WHERE name = ?"
                    .to_string(),
                vec![SqlValue::Text(name.to_string())],
            )
            .await?;

        if rows.is_empty() {
            return Err(StackhouseError::NotFound("Bucket not found".to_string()));
        }

        self.row_to_bucket(&rows[0])
    }

    /// List all buckets
    pub async fn list_buckets(&self) -> StackhouseResult<Vec<Bucket>> {
        let rows = self
            .store
            .query_simple(
                "SELECT id, name, public, owner_id, created_at FROM stackhouse_buckets ORDER BY name"
                    .to_string(),
            )
            .await?;

        rows.iter().map(|row| self.row_to_bucket(row)).collect()
    }

    /// Delete a bucket (must be empty)
    pub async fn delete_bucket(&self, name: &str) -> StackhouseResult<()> {
        // Check if bucket exists
        let _ = self.get_bucket(name).await?;

        // Check if bucket is empty
        let objects = self
            .store
            .query(
                "SELECT COUNT(*) as count FROM stackhouse_objects WHERE bucket_name = ?"
                    .to_string(),
                vec![SqlValue::Text(name.to_string())],
            )
            .await?;

        if let Some(row) = objects.first() {
            if let Some((_, count)) = row.first() {
                if count.as_i64().unwrap_or(0) > 0 {
                    return Err(StackhouseError::Conflict(
                        "Bucket is not empty. Delete all objects first.".to_string(),
                    ));
                }
            }
        }

        // Delete bucket directory
        let bucket_path = self.storage_path.join(name);
        if bucket_path.exists() {
            fs::remove_dir_all(&bucket_path)
                .await
                .map_err(|e| StackhouseError::Storage(format!("Failed to delete bucket: {}", e)))?;
        }

        // Delete from database
        self.store
            .execute(
                "DELETE FROM stackhouse_buckets WHERE name = ?".to_string(),
                vec![SqlValue::Text(name.to_string())],
            )
            .await?;

        info!("Deleted bucket: {}", name);
        Ok(())
    }

    /// Check if bucket is public
    pub async fn is_bucket_public(&self, name: &str) -> StackhouseResult<bool> {
        let bucket = self.get_bucket(name).await?;
        Ok(bucket.public)
    }

    // ========================================================================
    // Object Operations
    // ========================================================================

    /// Upload a file to a bucket
    pub async fn upload_object(
        &self,
        bucket: &str,
        path: &str,
        data: Vec<u8>,
        mime_type: &str,
        owner_id: Option<i64>,
    ) -> StackhouseResult<StorageObject> {
        // Validate inputs
        let _ = self.get_bucket(bucket).await?;
        self.validate_object_path(path)?;

        // Check file size
        if data.len() > MAX_FILE_SIZE {
            return Err(StackhouseError::InvalidPayload(format!(
                "File too large. Maximum size is {} bytes",
                MAX_FILE_SIZE
            )));
        }

        // Ensure storage directory exists
        self.ensure_storage_dir().await?;

        // Create file path
        let file_path = self.get_file_path(bucket, path);

        // Create parent directories
        if let Some(parent) = file_path.parent() {
            fs::create_dir_all(parent).await.map_err(|e| {
                StackhouseError::Storage(format!("Failed to create directory: {}", e))
            })?;
        }

        // Write file
        let mut file = fs::File::create(&file_path)
            .await
            .map_err(|e| StackhouseError::Storage(format!("Failed to create file: {}", e)))?;

        file.write_all(&data)
            .await
            .map_err(|e| StackhouseError::Storage(format!("Failed to write file: {}", e)))?;

        // Upsert metadata
        let size = data.len() as i64;
        self.store
            .execute(
                r#"
            INSERT INTO stackhouse_objects (bucket_name, path, size, mime_type, owner_id)
            VALUES (?, ?, ?, ?, ?::bigint)
            ON CONFLICT(bucket_name, path) DO UPDATE SET
                size = excluded.size,
                mime_type = excluded.mime_type,
                updated_at = CURRENT_TIMESTAMP
            "#
                .to_string(),
                vec![
                    SqlValue::Text(bucket.to_string()),
                    SqlValue::Text(path.to_string()),
                    SqlValue::Integer(size),
                    SqlValue::Text(mime_type.to_string()),
                    owner_id.map(SqlValue::Integer).unwrap_or(SqlValue::Null),
                ],
            )
            .await?;

        info!("Uploaded object: {}/{} ({} bytes)", bucket, path, size);
        self.get_object(bucket, path).await
    }

    /// Get object metadata
    pub async fn get_object(&self, bucket: &str, path: &str) -> StackhouseResult<StorageObject> {
        let rows = self
            .store
            .query(
                r#"
            SELECT id, bucket_name, path, size, mime_type, owner_id, created_at, updated_at
            FROM stackhouse_objects WHERE bucket_name = ? AND path = ?
            "#
                .to_string(),
                vec![
                    SqlValue::Text(bucket.to_string()),
                    SqlValue::Text(path.to_string()),
                ],
            )
            .await?;

        if rows.is_empty() {
            return Err(StackhouseError::NotFound("Object not found".to_string()));
        }

        self.row_to_object(&rows[0])
    }

    /// Download a file
    pub async fn download_object(
        &self,
        bucket: &str,
        path: &str,
    ) -> StackhouseResult<(Vec<u8>, String)> {
        let object = self.get_object(bucket, path).await?;
        let file_path = self.get_file_path(bucket, path);

        let data = fs::read(&file_path)
            .await
            .map_err(|e| StackhouseError::Storage(format!("Failed to read file: {}", e)))?;

        Ok((data, object.mime_type))
    }

    /// Delete an object
    pub async fn delete_object(&self, bucket: &str, path: &str) -> StackhouseResult<()> {
        let _ = self.get_object(bucket, path).await?;
        let file_path = self.get_file_path(bucket, path);

        // Delete file
        if file_path.exists() {
            fs::remove_file(&file_path)
                .await
                .map_err(|e| StackhouseError::Storage(format!("Failed to delete file: {}", e)))?;
        }

        // Delete from database
        self.store
            .execute(
                "DELETE FROM stackhouse_objects WHERE bucket_name = ? AND path = ?".to_string(),
                vec![
                    SqlValue::Text(bucket.to_string()),
                    SqlValue::Text(path.to_string()),
                ],
            )
            .await?;

        info!("Deleted object: {}/{}", bucket, path);
        Ok(())
    }

    /// List objects in a bucket
    pub async fn list_objects(
        &self,
        bucket: &str,
        query: ListObjectsQuery,
    ) -> StackhouseResult<Vec<StorageObject>> {
        let _ = self.get_bucket(bucket).await?;

        let (sql, params) = if let Some(prefix) = query.prefix {
            (
                r#"
                SELECT id, bucket_name, path, size, mime_type, owner_id, created_at, updated_at
                FROM stackhouse_objects 
                WHERE bucket_name = ? AND path LIKE ?
                ORDER BY path
                LIMIT ? OFFSET ?
                "#
                .to_string(),
                vec![
                    SqlValue::Text(bucket.to_string()),
                    SqlValue::Text(format!("{}%", prefix)),
                    SqlValue::Integer(query.limit),
                    SqlValue::Integer(query.offset),
                ],
            )
        } else {
            (
                r#"
                SELECT id, bucket_name, path, size, mime_type, owner_id, created_at, updated_at
                FROM stackhouse_objects 
                WHERE bucket_name = ?
                ORDER BY path
                LIMIT ? OFFSET ?
                "#
                .to_string(),
                vec![
                    SqlValue::Text(bucket.to_string()),
                    SqlValue::Integer(query.limit),
                    SqlValue::Integer(query.offset),
                ],
            )
        };

        let rows = self.store.query(sql, params).await?;
        rows.iter().map(|row| self.row_to_object(row)).collect()
    }

    // ========================================================================
    // Helpers
    // ========================================================================

    fn row_to_bucket(&self, row: &[(String, Value)]) -> StackhouseResult<Bucket> {
        let get_str = |key: &str| -> StackhouseResult<String> {
            row.iter()
                .find(|(k, _)| k == key)
                .and_then(|(_, v)| v.as_str().map(String::from))
                .ok_or_else(|| StackhouseError::Internal(anyhow::anyhow!("Missing field: {}", key)))
        };

        let get_i64 = |key: &str| -> StackhouseResult<i64> {
            row.iter()
                .find(|(k, _)| k == key)
                .and_then(|(_, v)| v.as_i64())
                .ok_or_else(|| StackhouseError::Internal(anyhow::anyhow!("Missing field: {}", key)))
        };

        let owner_id = row
            .iter()
            .find(|(k, _)| k == "owner_id")
            .and_then(|(_, v)| v.as_i64());

        Ok(Bucket {
            id: get_i64("id")?,
            name: get_str("name")?,
            public: get_i64("public")? == 1,
            created_at: get_str("created_at")?,
            owner_id,
        })
    }

    fn row_to_object(&self, row: &[(String, Value)]) -> StackhouseResult<StorageObject> {
        let get_str = |key: &str| -> StackhouseResult<String> {
            row.iter()
                .find(|(k, _)| k == key)
                .and_then(|(_, v)| v.as_str().map(String::from))
                .ok_or_else(|| StackhouseError::Internal(anyhow::anyhow!("Missing field: {}", key)))
        };

        let get_i64 = |key: &str| -> StackhouseResult<i64> {
            row.iter()
                .find(|(k, _)| k == key)
                .and_then(|(_, v)| v.as_i64())
                .ok_or_else(|| StackhouseError::Internal(anyhow::anyhow!("Missing field: {}", key)))
        };

        let owner_id = row
            .iter()
            .find(|(k, _)| k == "owner_id")
            .and_then(|(_, v)| v.as_i64());

        Ok(StorageObject {
            id: get_i64("id")?,
            bucket_name: get_str("bucket_name")?,
            path: get_str("path")?,
            size: get_i64("size")?,
            mime_type: get_str("mime_type")?,
            created_at: get_str("created_at")?,
            updated_at: get_str("updated_at")?,
            owner_id,
        })
    }
}

// ============================================================================
// API Handlers
// ============================================================================

/// Storage state for handlers
#[derive(Clone)]
pub struct StorageState {
    pub storage: StorageService,
}

/// POST /v1/storage/buckets - Create bucket
async fn create_bucket_handler(
    State(state): State<StorageState>,
    Json(req): Json<CreateBucketRequest>,
) -> Result<impl IntoResponse, StackhouseError> {
    let bucket = state.storage.create_bucket(req, None).await?;
    Ok((
        StatusCode::CREATED,
        Json(json!({
            "success": true,
            "data": bucket
        })),
    ))
}

/// GET /v1/storage/buckets - List buckets
async fn list_buckets_handler(
    State(state): State<StorageState>,
) -> Result<impl IntoResponse, StackhouseError> {
    let buckets = state.storage.list_buckets().await?;
    Ok(Json(json!({
        "success": true,
        "data": buckets
    })))
}

/// GET /v1/storage/buckets/:name - Get bucket info
async fn get_bucket_handler(
    State(state): State<StorageState>,
    Path(name): Path<String>,
) -> Result<impl IntoResponse, StackhouseError> {
    let bucket = state.storage.get_bucket(&name).await?;
    Ok(Json(json!({
        "success": true,
        "data": bucket
    })))
}

/// DELETE /v1/storage/buckets/:name - Delete bucket
async fn delete_bucket_handler(
    State(state): State<StorageState>,
    Path(name): Path<String>,
) -> Result<impl IntoResponse, StackhouseError> {
    state.storage.delete_bucket(&name).await?;
    Ok(Json(json!({
        "success": true,
        "message": "Bucket deleted"
    })))
}

/// POST /v1/storage/object/:bucket/*path - Upload file
async fn upload_handler(
    State(state): State<StorageState>,
    Path((bucket, path)): Path<(String, String)>,
    mut multipart: Multipart,
) -> Result<impl IntoResponse, StackhouseError> {
    // Get the file from multipart
    let mut file_data: Option<(Vec<u8>, String)> = None;

    while let Some(field) = multipart
        .next_field()
        .await
        .map_err(|e| StackhouseError::InvalidPayload(format!("Multipart error: {}", e)))?
    {
        if field.name() == Some("file") {
            let mime_type = field
                .content_type()
                .map(|s| s.to_string())
                .unwrap_or_else(|| "application/octet-stream".to_string());

            let data = field.bytes().await.map_err(|e| {
                StackhouseError::InvalidPayload(format!("Failed to read file: {}", e))
            })?;

            file_data = Some((data.to_vec(), mime_type));
            break;
        }
    }

    let (data, mime_type) =
        file_data.ok_or_else(|| StackhouseError::InvalidPayload("No file provided".to_string()))?;

    let object = state
        .storage
        .upload_object(&bucket, &path, data, &mime_type, None)
        .await?;

    Ok((
        StatusCode::CREATED,
        Json(json!({
            "success": true,
            "data": object
        })),
    ))
}

/// GET /v1/storage/object/:bucket/*path - Download file
async fn download_handler(
    State(state): State<StorageState>,
    Path((bucket, path)): Path<(String, String)>,
) -> Result<impl IntoResponse, StackhouseError> {
    let (data, mime_type) = state.storage.download_object(&bucket, &path).await?;

    Ok((
        StatusCode::OK,
        [
            (header::CONTENT_TYPE, mime_type),
            (
                header::CONTENT_DISPOSITION,
                format!(
                    "inline; filename=\"{}\"",
                    path.split('/').last().unwrap_or(&path)
                ),
            ),
        ],
        data,
    ))
}

/// DELETE /v1/storage/object/:bucket/*path - Delete file
async fn delete_object_handler(
    State(state): State<StorageState>,
    Path((bucket, path)): Path<(String, String)>,
) -> Result<impl IntoResponse, StackhouseError> {
    state.storage.delete_object(&bucket, &path).await?;
    Ok(Json(json!({
        "success": true,
        "message": "Object deleted"
    })))
}

/// GET /v1/storage/list/:bucket - List objects
async fn list_objects_handler(
    State(state): State<StorageState>,
    Path(bucket): Path<String>,
    Query(query): Query<ListObjectsQuery>,
) -> Result<impl IntoResponse, StackhouseError> {
    let objects = state.storage.list_objects(&bucket, query).await?;
    Ok(Json(json!({
        "success": true,
        "data": objects
    })))
}

// ============================================================================
// Router
// ============================================================================

/// Creates the storage router with all storage endpoints
pub fn create_storage_router(storage_state: StorageState) -> Router {
    Router::new()
        // Bucket operations
        .route("/buckets", post(create_bucket_handler))
        .route("/buckets", get(list_buckets_handler))
        .route("/buckets/:name", get(get_bucket_handler))
        .route("/buckets/:name", delete(delete_bucket_handler))
        // Object operations
        .route("/object/:bucket/*path", post(upload_handler))
        .route("/object/:bucket/*path", get(download_handler))
        .route("/object/:bucket/*path", delete(delete_object_handler))
        .route("/list/:bucket", get(list_objects_handler))
        .with_state(storage_state)
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    async fn create_test_service() -> StorageService {
        let store = Arc::new(StackhouseStore::in_memory().await.unwrap());

        // Create the stackhouse_users table first to satisfy foreign key constraints
        // This table is normally created by the auth module but we need it for test isolation
        store
            .execute_batch(
                r#"
            CREATE TABLE IF NOT EXISTS stackhouse_users (
                id SERIAL PRIMARY KEY,
                email TEXT UNIQUE NOT NULL,
                password_hash TEXT NOT NULL,
                metadata TEXT DEFAULT '{}',
                created_at TIMESTAMP WITH TIME ZONE DEFAULT CURRENT_TIMESTAMP,
                updated_at TIMESTAMP WITH TIME ZONE DEFAULT CURRENT_TIMESTAMP
            );
            "#
                .to_string(),
            )
            .await
            .unwrap();

        let temp_dir = tempdir().unwrap();
        StorageService::new(store, Some(temp_dir.into_path()))
            .await
            .unwrap()
    }

    #[tokio::test]
    async fn test_bucket_creation() {
        let service = create_test_service().await;

        let bucket = service
            .create_bucket(
                CreateBucketRequest {
                    name: "test-bucket".to_string(),
                    public: false,
                },
                None,
            )
            .await
            .unwrap();

        assert_eq!(bucket.name, "test-bucket");
        assert!(!bucket.public);
    }

    #[tokio::test]
    async fn test_invalid_bucket_name() {
        let service = create_test_service().await;

        let result = service
            .create_bucket(
                CreateBucketRequest {
                    name: "Invalid_Name".to_string(),
                    public: false,
                },
                None,
            )
            .await;

        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_file_upload_download() {
        let service = create_test_service().await;

        // Create bucket
        service
            .create_bucket(
                CreateBucketRequest {
                    name: "files".to_string(),
                    public: true,
                },
                None,
            )
            .await
            .unwrap();

        // Upload file
        let data = b"Hello, Stackhouse!".to_vec();
        let object = service
            .upload_object("files", "hello.txt", data.clone(), "text/plain", None)
            .await
            .unwrap();

        assert_eq!(object.bucket_name, "files");
        assert_eq!(object.path, "hello.txt");
        assert_eq!(object.size, 14);

        // Download file
        let (downloaded, mime) = service.download_object("files", "hello.txt").await.unwrap();
        assert_eq!(downloaded, data);
        assert_eq!(mime, "text/plain");
    }

    #[tokio::test]
    async fn test_list_objects() {
        let service = create_test_service().await;

        service
            .create_bucket(
                CreateBucketRequest {
                    name: "test".to_string(),
                    public: false,
                },
                None,
            )
            .await
            .unwrap();

        // Upload multiple files
        for i in 0..3 {
            service
                .upload_object(
                    "test",
                    &format!("file{}.txt", i),
                    format!("content {}", i).into_bytes(),
                    "text/plain",
                    None,
                )
                .await
                .unwrap();
        }

        let objects = service
            .list_objects(
                "test",
                ListObjectsQuery {
                    prefix: None,
                    limit: 100,
                    offset: 0,
                },
            )
            .await
            .unwrap();

        assert_eq!(objects.len(), 3);
    }

    #[tokio::test]
    async fn test_delete_object() {
        let service = create_test_service().await;

        service
            .create_bucket(
                CreateBucketRequest {
                    name: "delete-test".to_string(),
                    public: false,
                },
                None,
            )
            .await
            .unwrap();

        service
            .upload_object(
                "delete-test",
                "to-delete.txt",
                b"delete me".to_vec(),
                "text/plain",
                None,
            )
            .await
            .unwrap();

        service
            .delete_object("delete-test", "to-delete.txt")
            .await
            .unwrap();

        let result = service.get_object("delete-test", "to-delete.txt").await;
        assert!(result.is_err());
    }
}
