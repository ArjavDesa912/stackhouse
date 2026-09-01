//! # Vector Search Module (Stackhouse-Vectors)
//!
//! Provides first-class Qdrant integration for Stackhouse, offering a clean REST API
//! for similarity search with a dedicated high-performance vector database backend.
//!
//! ## Features
//! - Automatic Qdrant collection creation and management
//! - Dynamic vector dimension detection from first insert
//! - Cosine, Euclidean (L2), and Inner Product (Dot) similarity search
//! - Rich metadata filtering via Qdrant's payload system
//! - Combined vector + metadata upserts
//!
//! ## Endpoints
//!
//! - `POST /v1/vectors/:collection/search` - Similarity search
//! - `POST /v1/vectors/:collection/upsert` - Insert/update with embeddings
//! - `GET /v1/vectors/:collection/info` - Collection metadata
//! - `POST /v1/vectors/:collection/batch` - Batch upsert

use crate::error::{StackhouseError, StackhouseResult};

use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::IntoResponse,
    routing::{get, post},
    Json, Router,
};
use reqwest::Client;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use tracing::{debug, info, warn};

// ============================================================================
// Core Types
// ============================================================================

/// Distance metrics for similarity search
#[derive(Debug, Clone, Deserialize, Serialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum DistanceMetric {
    Cosine,
    L2,
    #[serde(alias = "inner_product", alias = "dot")]
    InnerProduct,
}

impl DistanceMetric {
    /// Returns the Qdrant distance type string
    pub fn qdrant_distance(&self) -> &'static str {
        match self {
            DistanceMetric::Cosine => "Cosine",
            DistanceMetric::L2 => "Euclid",
            DistanceMetric::InnerProduct => "Dot",
        }
    }
}

/// Result of a similarity search
#[derive(Debug, Serialize)]
pub struct SearchResult {
    pub id: String,
    pub similarity: f64,
    pub data: Value,
}

/// Vector collection metadata
#[derive(Debug, Serialize)]
pub struct VectorColumnInfo {
    pub table: String,
    pub column: String,
    pub dimensions: usize,
    pub index_type: String,
    pub row_count: u64,
}

// ============================================================================
// Request DTOs
// ============================================================================

/// Search request body
#[derive(Debug, Deserialize)]
pub struct VectorSearchRequest {
    /// Query vector for similarity search
    pub vector: Vec<f64>,
    /// Number of results to return (default: 10)
    #[serde(default = "default_top_k")]
    pub top_k: usize,
    /// Distance metric to use (default: cosine)
    #[serde(default = "default_metric")]
    pub metric: DistanceMetric,
    /// Optional metadata filters as key-value pairs
    #[serde(default)]
    pub filters: Option<Value>,
    /// Name of the vector column (default: "embedding")  
    /// Note: In Qdrant mode this selects the named vector if multiple vectors exist.
    #[serde(default = "default_embedding_column")]
    pub column: String,
}

/// Upsert request body
#[derive(Debug, Deserialize)]
pub struct VectorUpsertRequest {
    /// Optional string ID for update (omit for auto-generated UUID)
    pub id: Option<String>,
    /// The embedding vector
    pub embedding: Vec<f64>,
    /// Additional payload data to store alongside the vector
    #[serde(default)]
    pub data: Option<Value>,
    /// Name of the vector column (default: "embedding")
    #[serde(default = "default_embedding_column")]
    pub column: String,
}

/// Batch upsert request
#[derive(Debug, Deserialize)]
pub struct VectorBatchUpsertRequest {
    /// List of records to upsert
    pub records: Vec<VectorUpsertRequest>,
}

fn default_top_k() -> usize {
    10
}
fn default_metric() -> DistanceMetric {
    DistanceMetric::Cosine
}
fn default_embedding_column() -> String {
    "embedding".to_string()
}

// ============================================================================
// Vector Service Implementation (Qdrant Backend)
// ============================================================================

/// Vector search service powered by Qdrant
#[derive(Clone)]
pub struct VectorService {
    http: Client,
    qdrant_url: String,
}

/// Shared state for vector routes
#[derive(Clone)]
pub struct VectorState {
    pub vector: VectorService,
}

impl VectorService {
    /// Creates a new VectorService pointing at a Qdrant instance.
    /// `qdrant_url` should be like `http://qdrant:6333`.
    pub async fn new(qdrant_url: String) -> StackhouseResult<Self> {
        let http = Client::builder()
            .timeout(std::time::Duration::from_secs(30))
            .build()
            .map_err(|e| StackhouseError::Brain(format!("Failed to create HTTP client: {}", e)))?;

        let service = Self { http, qdrant_url };
        service.health_check().await?;
        Ok(service)
    }

    /// Health check — verify Qdrant is reachable
    async fn health_check(&self) -> StackhouseResult<()> {
        info!("🧠 Connecting to Qdrant at {}...", self.qdrant_url);
        match self
            .http
            .get(format!("{}/healthz", self.qdrant_url))
            .send()
            .await
        {
            Ok(resp) if resp.status().is_success() => {
                info!("✅ Qdrant is healthy and reachable");
                Ok(())
            }
            Ok(resp) => {
                warn!(
                    "⚠️ Qdrant returned status {}: vector features may be limited",
                    resp.status()
                );
                Ok(())
            }
            Err(e) => {
                warn!(
                    "⚠️ Qdrant not reachable at {}: {}. Vector features will be limited.",
                    self.qdrant_url, e
                );
                Ok(())
            }
        }
    }

    /// Ensure a Qdrant collection exists with the correct dimensions and distance metric.
    /// Qdrant collections are created lazily on first insert.
    pub async fn ensure_collection(
        &self,
        collection: &str,
        dimensions: usize,
        metric: &DistanceMetric,
    ) -> StackhouseResult<()> {
        if dimensions == 0 || dimensions > 65536 {
            return Err(StackhouseError::InvalidPayload(format!(
                "Vector dimensions must be between 1 and 65536, got {}",
                dimensions
            )));
        }

        // Check if collection already exists
        let url = format!("{}/collections/{}", self.qdrant_url, collection);
        let resp = self
            .http
            .get(&url)
            .send()
            .await
            .map_err(|e| StackhouseError::Brain(format!("Qdrant request failed: {}", e)))?;

        if resp.status().is_success() {
            debug!("Collection '{}' already exists", collection);
            return Ok(());
        }

        // Create collection
        let body = json!({
            "vectors": {
                "size": dimensions,
                "distance": metric.qdrant_distance()
            },
            "optimizers_config": {
                "indexing_threshold": 20000
            },
            "hnsw_config": {
                "m": 16,
                "ef_construct": 100
            }
        });

        let resp = self.http.put(&url).json(&body).send().await.map_err(|e| {
            StackhouseError::Brain(format!("Qdrant create collection failed: {}", e))
        })?;

        if resp.status().is_success() {
            info!(
                "📐 Created Qdrant collection '{}' (dims={}, metric={:?})",
                collection, dimensions, metric
            );
        } else {
            let err_text = resp.text().await.unwrap_or_default();
            return Err(StackhouseError::Brain(format!(
                "Failed to create Qdrant collection '{}': {}",
                collection, err_text
            )));
        }

        Ok(())
    }

    /// Perform a similarity search using Qdrant
    pub async fn search(
        &self,
        collection: &str,
        _column: &str,
        query_vector: &[f64],
        top_k: usize,
        _metric: &DistanceMetric,
        filters: Option<&Value>,
    ) -> StackhouseResult<Vec<SearchResult>> {
        let url = format!(
            "{}/collections/{}/points/search",
            self.qdrant_url, collection
        );

        // Build Qdrant filter from key-value pairs
        let qdrant_filter = if let Some(filter_obj) = filters {
            if let Some(obj) = filter_obj.as_object() {
                if !obj.is_empty() {
                    let must_conditions: Vec<Value> = obj
                        .iter()
                        .map(|(key, value)| {
                            json!({
                                "key": key,
                                "match": { "value": value }
                            })
                        })
                        .collect();
                    Some(json!({ "must": must_conditions }))
                } else {
                    None
                }
            } else {
                None
            }
        } else {
            None
        };

        // Convert f64 to f32 for Qdrant (Qdrant uses f32 vectors)
        let vector_f32: Vec<f32> = query_vector.iter().map(|&v| v as f32).collect();

        let mut body = json!({
            "vector": vector_f32,
            "limit": top_k,
            "with_payload": true
        });

        if let Some(filter) = qdrant_filter {
            body["filter"] = filter;
        }

        debug!("Qdrant search on '{}': top_k={}", collection, top_k);

        let resp = self
            .http
            .post(&url)
            .json(&body)
            .send()
            .await
            .map_err(|e| StackhouseError::Brain(format!("Qdrant search failed: {}", e)))?;

        if !resp.status().is_success() {
            let err_text = resp.text().await.unwrap_or_default();
            return Err(StackhouseError::Brain(format!(
                "Qdrant search error on '{}': {}",
                collection, err_text
            )));
        }

        let resp_json: Value = resp.json().await.map_err(|e| {
            StackhouseError::Brain(format!("Failed to parse Qdrant response: {}", e))
        })?;

        let results = resp_json["result"]
            .as_array()
            .unwrap_or(&vec![])
            .iter()
            .map(|hit| {
                let id = hit["id"]
                    .as_str()
                    .map(|s| s.to_string())
                    .or_else(|| hit["id"].as_u64().map(|n| n.to_string()))
                    .unwrap_or_default();
                let similarity = hit["score"].as_f64().unwrap_or(0.0);
                let payload = hit["payload"].clone();

                SearchResult {
                    id,
                    similarity,
                    data: payload,
                }
            })
            .collect();

        Ok(results)
    }

    /// Upsert a record with vector embedding into Qdrant
    pub async fn upsert(
        &self,
        collection: &str,
        _column: &str,
        id: Option<String>,
        embedding: &[f64],
        data: Option<&Value>,
        metric: &DistanceMetric,
    ) -> StackhouseResult<String> {
        let dimensions = embedding.len();
        self.ensure_collection(collection, dimensions, metric)
            .await?;

        // Generate a UUID if no ID provided
        let point_id = id.unwrap_or_else(|| uuid::Uuid::new_v4().to_string());

        // Convert f64 to f32
        let vector_f32: Vec<f32> = embedding.iter().map(|&v| v as f32).collect();

        // Build payload from data
        let payload = data.cloned().unwrap_or(json!({}));

        let body = json!({
            "points": [
                {
                    "id": point_id,
                    "vector": vector_f32,
                    "payload": payload
                }
            ]
        });

        let url = format!(
            "{}/collections/{}/points?wait=true",
            self.qdrant_url, collection
        );

        let resp = self
            .http
            .put(&url)
            .json(&body)
            .send()
            .await
            .map_err(|e| StackhouseError::Brain(format!("Qdrant upsert failed: {}", e)))?;

        if !resp.status().is_success() {
            let err_text = resp.text().await.unwrap_or_default();
            return Err(StackhouseError::Brain(format!(
                "Qdrant upsert error on '{}': {}",
                collection, err_text
            )));
        }

        Ok(point_id)
    }

    /// Get collection information from Qdrant
    pub async fn get_info(&self, collection: &str) -> StackhouseResult<Vec<VectorColumnInfo>> {
        let url = format!("{}/collections/{}", self.qdrant_url, collection);

        let resp =
            self.http.get(&url).send().await.map_err(|e| {
                StackhouseError::Brain(format!("Qdrant info request failed: {}", e))
            })?;

        if !resp.status().is_success() {
            return Ok(vec![]);
        }

        let resp_json: Value = resp.json().await.map_err(|e| {
            StackhouseError::Brain(format!("Failed to parse Qdrant response: {}", e))
        })?;

        let result = &resp_json["result"];
        let config = &result["config"]["params"]["vectors"];

        let dimensions = config["size"].as_u64().unwrap_or(0) as usize;
        let distance = config["distance"].as_str().unwrap_or("unknown");
        let row_count = result["points_count"].as_u64().unwrap_or(0);

        let index_type = format!("hnsw ({})", distance);

        Ok(vec![VectorColumnInfo {
            table: collection.to_string(),
            column: "default".to_string(),
            dimensions,
            index_type,
            row_count,
        }])
    }
}

// ============================================================================
// HTTP Handlers
// ============================================================================

/// POST /v1/vectors/:collection/search — Similarity search
async fn vector_search_handler(
    State(state): State<VectorState>,
    Path(collection): Path<String>,
    Json(req): Json<VectorSearchRequest>,
) -> Result<impl IntoResponse, StackhouseError> {
    info!(
        "🔍 Vector search on '{}' (top_k={}, metric={:?})",
        collection, req.top_k, req.metric
    );

    let results = state
        .vector
        .search(
            &collection,
            &req.column,
            &req.vector,
            req.top_k,
            &req.metric,
            req.filters.as_ref(),
        )
        .await?;

    let count = results.len();
    Ok(Json(json!({
        "success": true,
        "data": results,
        "count": count,
        "collection": collection,
        "metric": req.metric,
    })))
}

/// POST /v1/vectors/:collection/upsert — Insert/update with embedding
async fn vector_upsert_handler(
    State(state): State<VectorState>,
    Path(collection): Path<String>,
    Json(req): Json<VectorUpsertRequest>,
) -> Result<impl IntoResponse, StackhouseError> {
    info!(
        "📥 Vector upsert to '{}' (dims={})",
        collection,
        req.embedding.len()
    );

    let id = state
        .vector
        .upsert(
            &collection,
            &req.column,
            req.id,
            &req.embedding,
            req.data.as_ref(),
            &default_metric(),
        )
        .await?;

    Ok((
        StatusCode::CREATED,
        Json(json!({
            "success": true,
            "data": {
                "id": id,
                "collection": collection,
                "dimensions": req.embedding.len(),
            },
            "message": "Vector upserted successfully"
        })),
    ))
}

/// POST /v1/vectors/:collection/batch — Batch upsert
async fn vector_batch_upsert_handler(
    State(state): State<VectorState>,
    Path(collection): Path<String>,
    Json(req): Json<VectorBatchUpsertRequest>,
) -> Result<impl IntoResponse, StackhouseError> {
    info!(
        "📥 Vector batch upsert to '{}' ({} records)",
        collection,
        req.records.len()
    );

    if req.records.is_empty() {
        return Err(StackhouseError::InvalidPayload("Empty batch".to_string()));
    }

    let mut ids = Vec::new();
    for record in &req.records {
        let id = state
            .vector
            .upsert(
                &collection,
                &record.column,
                record.id.clone(),
                &record.embedding,
                record.data.as_ref(),
                &default_metric(),
            )
            .await?;
        ids.push(id);
    }

    Ok((
        StatusCode::CREATED,
        Json(json!({
            "success": true,
            "data": {
                "ids": ids,
                "collection": collection,
                "count": ids.len(),
            },
            "message": "Vectors batch upserted successfully"
        })),
    ))
}

/// GET /v1/vectors/:collection/info — Collection metadata
async fn vector_info_handler(
    State(state): State<VectorState>,
    Path(collection): Path<String>,
) -> Result<impl IntoResponse, StackhouseError> {
    let infos = state.vector.get_info(&collection).await?;

    Ok(Json(json!({
        "success": true,
        "data": infos,
        "collection": collection,
    })))
}

// ============================================================================
// Router
// ============================================================================

/// Creates the vector search router
pub fn create_vector_router(state: VectorState) -> Router {
    Router::new()
        .route("/:collection/search", post(vector_search_handler))
        .route("/:collection/upsert", post(vector_upsert_handler))
        .route("/:collection/batch", post(vector_batch_upsert_handler))
        .route("/:collection/info", get(vector_info_handler))
        .with_state(state)
}
