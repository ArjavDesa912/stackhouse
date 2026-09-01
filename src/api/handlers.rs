use crate::api::admin;
use crate::auth::{extract_auth_user, AuthState, AuthUser};
use crate::authorization::AuthorizationService;
use crate::data_processing::{CreateDatasetRequest, DatasetService};
use crate::db::{json_to_sql_value, json_to_sql_value_for_type, SqlValue, StackhouseStore};
use crate::error::StackhouseError;
use crate::guard::SchemaGuard;
use crate::rls::RlsService;
use axum::{
    body::{to_bytes, Body},
    extract::{Path, Query, State},
    http::{HeaderMap, Request, StatusCode},
    response::{sse::Event, IntoResponse, Sse},
    Json,
};
use futures::stream::Stream;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::HashMap;
use std::convert::Infallible;
use std::env;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::broadcast;
use tracing::{debug, info};

/// Application state shared across all handlers
#[derive(Clone)]
pub struct AppState {
    pub store: Arc<StackhouseStore>,
    pub guard: Arc<SchemaGuard>,
    /// Broadcast channel for real-time updates per table
    pub broadcasters: Arc<dashmap::DashMap<String, broadcast::Sender<Value>>>,
    pub auth: Option<AuthState>,
    pub authorization: AuthorizationService,
    pub raw_sql_enabled: bool,
    pub destructive_admin_enabled: bool,
    pub raw_sql_query_allowlist: Vec<String>,
    pub raw_sql_execute_blocklist: Vec<String>,
    pub admin_audit: Option<Arc<admin::AdminAuditService>>,
    pub rls: Option<Arc<RlsService>>,
    pub datasets: Arc<DatasetService>,
}

impl AppState {
    pub fn new(store: Arc<StackhouseStore>) -> Self {
        let guard = Arc::new(SchemaGuard::new(Arc::clone(&store)));
        Self {
            store: Arc::clone(&store),
            guard,
            broadcasters: Arc::new(dashmap::DashMap::new()),
            auth: None,
            authorization: AuthorizationService::new(
                crate::authorization::SecurityConfig::default(),
            ),
            raw_sql_enabled: false,
            destructive_admin_enabled: false,
            raw_sql_query_allowlist: raw_sql_query_allowlist_from_env(),
            raw_sql_execute_blocklist: raw_sql_execute_blocklist_from_env(),
            admin_audit: None,
            rls: None,
            datasets: Arc::new(DatasetService::new(store)),
        }
    }

    pub fn with_security(
        store: Arc<StackhouseStore>,
        auth: AuthState,
        authorization: AuthorizationService,
        raw_sql_enabled: bool,
        destructive_admin_enabled: bool,
        admin_audit: Option<Arc<admin::AdminAuditService>>,
    ) -> Self {
        let guard = Arc::new(SchemaGuard::new(Arc::clone(&store)));
        Self {
            store: Arc::clone(&store),
            guard,
            broadcasters: Arc::new(dashmap::DashMap::new()),
            auth: Some(auth),
            authorization,
            raw_sql_enabled,
            destructive_admin_enabled,
            raw_sql_query_allowlist: raw_sql_query_allowlist_from_env(),
            raw_sql_execute_blocklist: raw_sql_execute_blocklist_from_env(),
            admin_audit,
            rls: None,
            datasets: Arc::new(DatasetService::new(store)),
        }
    }

    /// Attach an RlsService so REST handlers can inject JWT context for RLS
    pub fn with_rls(mut self, rls: Arc<RlsService>) -> Self {
        self.rls = Some(rls);
        self
    }

    /// Extract JWT from Authorization header and inject claims into PG session for RLS.
    /// Best-effort: if no RLS service or no auth header, silently skips.
    async fn maybe_inject_rls_context(&self, headers: &HeaderMap) {
        if let Some(rls) = &self.rls {
            if let Some(auth_val) = headers.get("authorization") {
                if let Ok(auth_str) = auth_val.to_str() {
                    if let Some(token) = auth_str.strip_prefix("Bearer ") {
                        // Decode JWT payload (middle segment) without verifying —
                        // verification already happened in auth middleware.
                        // RLS policies in PG read from request.jwt.claims GUC.
                        let claims_json = decode_jwt_payload(token).unwrap_or_default();
                        if !claims_json.is_empty() {
                            let _ = rls.inject_jwt_context(&claims_json).await;
                        }
                    }
                }
            }
        }
    }

    /// Get or create a broadcaster for a collection
    fn get_broadcaster(&self, collection: &str) -> broadcast::Sender<Value> {
        self.broadcasters
            .entry(collection.to_string())
            .or_insert_with(|| {
                let (tx, _) = broadcast::channel(100);
                tx
            })
            .clone()
    }
}

/// Standard API response
#[derive(Debug, Serialize)]
pub struct ApiResponse<T: Serialize> {
    pub success: bool,
    pub data: Option<T>,
    pub message: Option<String>,
}

impl<T: Serialize> ApiResponse<T> {
    pub fn success(data: T) -> Self {
        Self {
            success: true,
            data: Some(data),
            message: None,
        }
    }

    pub fn success_with_message(data: T, message: impl Into<String>) -> Self {
        Self {
            success: true,
            data: Some(data),
            message: Some(message.into()),
        }
    }
}

/// Push response data
#[derive(Debug, Serialize)]
pub struct PushResponse {
    pub id: i64,
    pub collection: String,
    pub columns_added: Vec<String>,
}

/// Batch push response
#[derive(Debug, Serialize)]
pub struct BatchPushResponse {
    pub inserted: u64,
    pub collection: String,
    pub columns_added: Vec<String>,
}

/// Query parameters for GET requests
#[derive(Debug, Deserialize)]
pub struct QueryParams {
    #[serde(default)]
    pub limit: Option<u32>,
    #[serde(default)]
    pub offset: Option<u32>,
    #[serde(default)]
    pub order_by: Option<String>,
    #[serde(default)]
    pub order_dir: Option<String>,
    #[serde(flatten)]
    pub filters: HashMap<String, String>,
}

/// Table stats response
#[derive(Debug, Serialize)]
pub struct TableStatsResponse {
    pub name: String,
    pub column_count: usize,
    pub row_count: u64,
    pub columns: Vec<ColumnResponse>,
}

#[derive(Debug, Serialize)]
pub struct ColumnResponse {
    pub name: String,
    pub col_type: String,
    pub nullable: bool,
    pub primary_key: bool,
}

/// Root handler - API info
pub(crate) async fn root_handler() -> impl IntoResponse {
    Json(json!({
        "name": "Stackhouse",
        "version": "1.0.0",
        "description": "🛸 Schema-Later Database with Automatic Evolution",
        "endpoints": {
            "push": "POST /v1/push/:collection",
            "batch_push": "POST /v1/push/:collection/batch",
            "preview": "POST /v1/preview/:collection",
            "query": "GET /v1/query/:collection",
            "get_by_id": "GET /v1/query/:collection/:id",
            "update": "POST /v1/update/:collection/:id",
            "bulk_update": "POST /v1/update/:collection",
            "delete": "POST /v1/delete/:collection/:id",
            "bulk_delete": "POST /v1/delete/:collection",
            "tables": "GET /v1/tables",
            "table_stats": "GET /v1/tables/:collection",
            "drop_table": "DELETE /v1/tables/:collection",
            "stream": "GET /v1/stream/:collection",
            "health": "GET /health",
            "explorer": "GET /explore"
        }
    }))
}

/// Health check endpoint
pub(crate) async fn health_handler(State(state): State<AppState>) -> impl IntoResponse {
    match state.store.query_simple("SELECT 1".to_string()).await {
        Ok(_) => Json(json!({
            "status": "healthy",
            "database": "connected"
        })),
        Err(e) => Json(json!({
            "status": "unhealthy",
            "database": "disconnected",
            "error": e.to_string()
        })),
    }
}

/// POST /v1/push/:collection - Insert a single document
pub(crate) async fn push_handler(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(collection): Path<String>,
    Json(payload): Json<Value>,
) -> Result<impl IntoResponse, StackhouseError> {
    info!("📥 Pushing to collection: {}", collection);

    state.maybe_inject_rls_context(&headers).await;

    // Ensure table exists
    state.guard.ensure_table(&collection).await?;

    // Ensure columns exist and get insertable column names with their target types
    let columns = state.guard.ensure_columns(&collection, &payload).await?;

    let id = if columns.is_empty() {
        // Insert with only default values
        let sql = format!("INSERT INTO {} DEFAULT VALUES", collection);
        state.store.insert_returning_id(sql, vec![]).await?
    } else {
        // Build INSERT statement
        let column_names: Vec<&str> = columns.iter().map(|(name, _)| name.as_str()).collect();
        let placeholders: Vec<&str> = columns.iter().map(|_| "?").collect();
        let sql = format!(
            "INSERT INTO {} ({}) VALUES ({})",
            collection,
            column_names.join(", "),
            placeholders.join(", ")
        );

        // Convert JSON values to SQL values
        let obj = payload.as_object().ok_or_else(|| {
            StackhouseError::InvalidPayload("Payload must be a JSON object".to_string())
        })?;

        let params: Vec<SqlValue> = columns
            .iter()
            .map(|(name, pg_type)| {
                obj.get(name)
                    .map(|v| json_to_sql_value_for_type(v, pg_type))
                    .unwrap_or(SqlValue::Null)
            })
            .collect();

        debug!("Executing: {} with {} params", sql, params.len());
        state.store.insert_returning_id(sql, params).await?
    };

    // Broadcast the new data
    let tx = state.get_broadcaster(&collection);
    let _ = tx.send(json!({
        "event": "insert",
        "id": id,
        "data": payload
    }));

    let response = ApiResponse::success_with_message(
        PushResponse {
            id,
            collection: collection.clone(),
            columns_added: columns.iter().map(|(name, _)| name.clone()).collect(),
        },
        "Data pushed successfully",
    );

    Ok((StatusCode::CREATED, Json(response)))
}

/// POST /v1/push/:collection/batch - Insert multiple documents
pub(crate) async fn batch_push_handler(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(collection): Path<String>,
    Json(payloads): Json<Vec<Value>>,
) -> Result<impl IntoResponse, StackhouseError> {
    info!(
        "📥 Batch pushing {} items to collection: {}",
        payloads.len(),
        collection
    );

    state.maybe_inject_rls_context(&headers).await;

    if payloads.is_empty() {
        return Err(StackhouseError::InvalidPayload("Empty batch".to_string()));
    }

    // Ensure table exists
    state.guard.ensure_table(&collection).await?;

    // Compute the unified batch schema once, then ensure it and insert every
    // document with the correct column types.
    let columns = state
        .guard
        .ensure_batch_columns(&collection, &payloads)
        .await?;
    let mut inserted = 0u64;

    if columns.is_empty() {
        // Insert with only default values
        for _ in &payloads {
            let sql = format!("INSERT INTO {} DEFAULT VALUES", collection);
            state.store.execute_simple(sql).await?;
            inserted += 1;
        }
    } else {
        let column_names: Vec<&str> = columns.iter().map(|(name, _)| name.as_str()).collect();
        let placeholders: Vec<&str> = columns.iter().map(|_| "?").collect();
        let sql = format!(
            "INSERT INTO {} ({}) VALUES ({})",
            collection,
            column_names.join(", "),
            placeholders.join(", ")
        );

        for payload in &payloads {
            let obj = payload.as_object().ok_or_else(|| {
                StackhouseError::InvalidPayload("Each item must be a JSON object".to_string())
            })?;

            let params: Vec<SqlValue> = columns
                .iter()
                .map(|(name, pg_type)| {
                    obj.get(name)
                        .map(|v| json_to_sql_value_for_type(v, pg_type))
                        .unwrap_or(SqlValue::Null)
                })
                .collect();

            state.store.execute(sql.clone(), params).await?;
            inserted += 1;
        }
    }

    // Broadcast batch insert
    let tx = state.get_broadcaster(&collection);
    let _ = tx.send(json!({
        "event": "batch_insert",
        "count": inserted
    }));

    let response = ApiResponse::success(BatchPushResponse {
        inserted,
        collection,
        columns_added: columns.iter().map(|(name, _)| name.clone()).collect(),
    });

    Ok((StatusCode::CREATED, Json(response)))
}

/// POST /v1/preview/:collection - Dry-run schema changes for a payload
pub(crate) async fn preview_handler(
    State(state): State<AppState>,
    Path(collection): Path<String>,
    Json(payload): Json<Value>,
) -> Result<impl IntoResponse, StackhouseError> {
    info!(
        "🔮 Previewing schema changes for collection: {}",
        collection
    );

    let preview = state
        .guard
        .preview_schema_changes(&collection, &payload)
        .await?;
    let response = ApiResponse::success(preview);

    Ok((StatusCode::OK, Json(response)))
}

/// GET /v1/query/:collection - Query documents with filters
pub(crate) async fn query_handler(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(collection): Path<String>,
    Query(params): Query<QueryParams>,
) -> Result<impl IntoResponse, StackhouseError> {
    debug!("🔍 Querying collection: {}", collection);

    state.maybe_inject_rls_context(&headers).await;

    // Check if table exists
    let _stats = state.guard.get_table_stats(&collection).await?;

    // Build query
    let mut sql = format!("SELECT * FROM {}", collection);
    let mut query_params: Vec<SqlValue> = Vec::new();

    // Add WHERE clauses from filters (excluding reserved params)
    let reserved = ["limit", "offset", "order_by", "order_dir"];
    let filters: Vec<_> = params
        .filters
        .iter()
        .filter(|(k, _)| !reserved.contains(&k.as_str()))
        .collect();

    if !filters.is_empty() {
        let mut conditions = Vec::with_capacity(filters.len());
        for (k, _) in &filters {
            SchemaGuard::validate_identifier(k)?;
            conditions.push(format!("{} = ?", k));
        }
        sql.push_str(" WHERE ");
        sql.push_str(&conditions.join(" AND "));

        for (_, v) in filters {
            query_params.push(SqlValue::Text(v.clone()));
        }
    }

    // Add ORDER BY
    if let Some(order_by) = &params.order_by {
        SchemaGuard::validate_identifier(order_by)?;
        let dir = params.order_dir.as_deref().unwrap_or("ASC").to_uppercase();
        if dir != "ASC" && dir != "DESC" {
            return Err(StackhouseError::InvalidPayload(
                "order_dir must be ASC or DESC".to_string(),
            ));
        }
        sql.push_str(&format!(" ORDER BY {} {}", order_by, dir));
    }

    // Add LIMIT and OFFSET
    let limit = params.limit.unwrap_or(100).min(1000);
    sql.push_str(&format!(" LIMIT {}", limit));
    if let Some(offset) = params.offset {
        sql.push_str(&format!(" OFFSET {}", offset));
    }

    // Execute query
    let rows = state.store.query(sql, query_params).await?;

    let results: Vec<Value> = rows
        .into_iter()
        .map(|row| {
            let mut obj = serde_json::Map::new();
            for (key, value) in row {
                obj.insert(key, value);
            }
            Value::Object(obj)
        })
        .collect();

    Ok(Json(json!({
        "success": true,
        "data": results,
        "count": results.len(),
        "collection": collection
    })))
}

/// GET /v1/query/:collection/:id - Get single document by ID
pub(crate) async fn get_by_id_handler(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path((collection, id)): Path<(String, i64)>,
) -> Result<impl IntoResponse, StackhouseError> {
    debug!("🔍 Getting {} from {}", id, collection);

    state.maybe_inject_rls_context(&headers).await;

    let _stats = state.guard.get_table_stats(&collection).await?;

    let sql = format!("SELECT * FROM {} WHERE id = ?", collection);
    let rows = state.store.query(sql, vec![SqlValue::Integer(id)]).await?;

    if let Some(row) = rows.into_iter().next() {
        let mut obj = serde_json::Map::new();
        for (key, value) in row {
            obj.insert(key, value);
        }

        Ok(Json(json!({
            "success": true,
            "data": Value::Object(obj)
        })))
    } else {
        Err(StackhouseError::TableNotFound(format!(
            "Document with id {} not found in {}",
            id, collection
        )))
    }
}

/// POST /v1/update/:collection/:id - Update a document
pub(crate) async fn update_handler(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path((collection, id)): Path<(String, i64)>,
    Json(payload): Json<Value>,
) -> Result<impl IntoResponse, StackhouseError> {
    state.maybe_inject_rls_context(&headers).await;

    info!("📝 Updating {} in {}", id, collection);

    // Ensure columns exist
    let columns = state.guard.ensure_columns(&collection, &payload).await?;

    if columns.is_empty() {
        return Ok(Json(json!({
            "success": true,
            "message": "No updates provided"
        })));
    }

    let obj = payload.as_object().ok_or_else(|| {
        StackhouseError::InvalidPayload("Payload must be a JSON object".to_string())
    })?;

    // Build UPDATE statement
    let column_names: Vec<&str> = columns.iter().map(|(name, _)| name.as_str()).collect();
    let set_clauses: Vec<String> = column_names.iter().map(|c| format!("{} = ?", c)).collect();
    let sql = format!(
        "UPDATE {} SET {}, updated_at = CURRENT_TIMESTAMP WHERE id = ?",
        collection,
        set_clauses.join(", ")
    );

    let mut params: Vec<SqlValue> = columns
        .iter()
        .map(|(name, pg_type)| {
            obj.get(name)
                .map(|v| json_to_sql_value_for_type(v, pg_type))
                .unwrap_or(SqlValue::Null)
        })
        .collect();
    params.push(SqlValue::Integer(id));

    let affected = state.store.execute(sql, params).await?;

    // Broadcast update
    let tx = state.get_broadcaster(&collection);
    let _ = tx.send(json!({
        "event": "update",
        "id": id,
        "data": payload
    }));

    Ok(Json(json!({
        "success": true,
        "affected": affected,
        "id": id
    })))
}

/// POST /v1/delete/:collection/:id - Delete a document
pub(crate) async fn delete_handler(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path((collection, id)): Path<(String, i64)>,
) -> Result<impl IntoResponse, StackhouseError> {
    info!("🗑️ Deleting {} from {}", id, collection);

    state.maybe_inject_rls_context(&headers).await;

    SchemaGuard::validate_identifier(&collection)?;

    let sql = format!("DELETE FROM {} WHERE id = ?", collection);
    let affected = state
        .store
        .execute(sql, vec![SqlValue::Integer(id)])
        .await?;

    // Broadcast delete
    let tx = state.get_broadcaster(&collection);
    let _ = tx.send(json!({
        "event": "delete",
        "id": id
    }));

    Ok(Json(json!({
        "success": true,
        "affected": affected,
        "id": id
    })))
}

/// GET /v1/tables - List all tables
pub(crate) async fn list_tables_handler(
    State(state): State<AppState>,
) -> Result<impl IntoResponse, StackhouseError> {
    let tables = state.store.list_tables().await?;

    Ok(Json(json!({
        "success": true,
        "tables": tables,
        "count": tables.len()
    })))
}

/// GET /v1/tables/:collection - Get table stats
pub(crate) async fn table_stats_handler(
    State(state): State<AppState>,
    Path(collection): Path<String>,
) -> Result<impl IntoResponse, StackhouseError> {
    let stats = state.guard.get_table_stats(&collection).await?;

    let columns: Vec<ColumnResponse> = stats
        .columns
        .iter()
        .map(|c| ColumnResponse {
            name: c.name.clone(),
            col_type: c.col_type.clone(),
            nullable: !c.notnull,
            primary_key: c.pk,
        })
        .collect();

    Ok(Json(json!({
        "success": true,
        "data": TableStatsResponse {
            name: stats.name,
            column_count: stats.column_count,
            row_count: stats.row_count,
            columns,
        }
    })))
}

/// GET /v1/stream/:collection - Server-Sent Events stream
pub(crate) async fn stream_handler(
    State(state): State<AppState>,
    Path(collection): Path<String>,
) -> Sse<impl Stream<Item = Result<Event, Infallible>>> {
    info!("📡 New stream subscriber for: {}", collection);

    let tx = state.get_broadcaster(&collection);
    let mut rx = tx.subscribe();

    let stream = async_stream::stream! {
        // Send initial connection message
        yield Ok(Event::default().data(json!({
            "event": "connected",
            "collection": collection
        }).to_string()));

        // Stream updates
        loop {
            match rx.recv().await {
                Ok(value) => {
                    yield Ok(Event::default().data(value.to_string()));
                }
                Err(broadcast::error::RecvError::Closed) => break,
                Err(broadcast::error::RecvError::Lagged(n)) => {
                    yield Ok(Event::default().data(json!({
                        "event": "warning",
                        "message": format!("Missed {} messages", n)
                    }).to_string()));
                }
            }
        }
    };

    Sse::new(stream).keep_alive(
        axum::response::sse::KeepAlive::new()
            .interval(Duration::from_secs(30))
            .text("ping"),
    )
}

/// Bulk delete request body
#[derive(Debug, Deserialize)]
pub struct BulkDeleteRequest {
    #[serde(default)]
    pub filters: HashMap<String, Value>,
}

/// POST /v1/delete/:collection - Bulk delete with filters
pub(crate) async fn bulk_delete_handler(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(collection): Path<String>,
    Json(payload): Json<BulkDeleteRequest>,
) -> Result<impl IntoResponse, StackhouseError> {
    info!("🗑️ Bulk deleting from {}", collection);

    state.maybe_inject_rls_context(&headers).await;

    let _stats = state.guard.get_table_stats(&collection).await?;

    let mut sql = format!("DELETE FROM {}", collection);
    let mut params: Vec<SqlValue> = Vec::new();

    if !payload.filters.is_empty() {
        let mut conditions = Vec::with_capacity(payload.filters.len());
        for key in payload.filters.keys() {
            SchemaGuard::validate_identifier(key)?;
            conditions.push(format!("{} = ?", key));
        }
        sql.push_str(" WHERE ");
        sql.push_str(&conditions.join(" AND "));

        for v in payload.filters.values() {
            params.push(json_to_sql_value(v));
        }
    }

    let affected = state.store.execute(sql, params).await?;

    // Broadcast bulk delete
    let tx = state.get_broadcaster(&collection);
    let _ = tx.send(json!({
        "event": "bulk_delete",
        "affected": affected
    }));

    Ok(Json(json!({
        "success": true,
        "affected": affected
    })))
}

/// Bulk update request body
#[derive(Debug, Deserialize)]
pub struct BulkUpdateRequest {
    #[serde(default)]
    pub filters: HashMap<String, Value>,
    pub data: Value,
}

/// POST /v1/update/:collection - Bulk update with filters
pub(crate) async fn bulk_update_handler(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(collection): Path<String>,
    Json(payload): Json<BulkUpdateRequest>,
) -> Result<impl IntoResponse, StackhouseError> {
    info!("📝 Bulk updating {}", collection);

    state.maybe_inject_rls_context(&headers).await;

    let _stats = state.guard.get_table_stats(&collection).await?;

    let obj = payload
        .data
        .as_object()
        .ok_or_else(|| StackhouseError::InvalidPayload("data must be a JSON object".to_string()))?;

    if obj.is_empty() {
        return Err(StackhouseError::InvalidPayload(
            "data must not be empty".to_string(),
        ));
    }

    // Ensure columns exist
    let columns = state
        .guard
        .ensure_columns(&collection, &payload.data)
        .await?;

    // Build UPDATE SET clauses
    let column_names: Vec<&str> = columns.iter().map(|(name, _)| name.as_str()).collect();
    let set_clauses: Vec<String> = column_names.iter().map(|c| format!("{} = ?", c)).collect();
    let mut sql = format!(
        "UPDATE {} SET {}, updated_at = CURRENT_TIMESTAMP",
        collection,
        set_clauses.join(", ")
    );

    let mut params: Vec<SqlValue> = columns
        .iter()
        .map(|(name, pg_type)| {
            obj.get(name)
                .map(|v| json_to_sql_value_for_type(v, pg_type))
                .unwrap_or(SqlValue::Null)
        })
        .collect();

    // Add WHERE clauses from filters
    if !payload.filters.is_empty() {
        let mut conditions = Vec::with_capacity(payload.filters.len());
        for key in payload.filters.keys() {
            SchemaGuard::validate_identifier(key)?;
            conditions.push(format!("{} = ?", key));
        }
        sql.push_str(" WHERE ");
        sql.push_str(&conditions.join(" AND "));

        for v in payload.filters.values() {
            params.push(json_to_sql_value(v));
        }
    }

    let affected = state.store.execute(sql, params).await?;

    // Broadcast bulk update
    let tx = state.get_broadcaster(&collection);
    let _ = tx.send(json!({
        "event": "bulk_update",
        "affected": affected
    }));

    Ok(Json(json!({
        "success": true,
        "affected": affected
    })))
}

/// DELETE /v1/tables/:collection - Drop a table
pub(crate) async fn drop_table_handler(
    State(state): State<AppState>,
    Path(collection): Path<String>,
    headers: HeaderMap,
) -> Result<impl IntoResponse, StackhouseError> {
    if !state.destructive_admin_enabled {
        return Err(StackhouseError::Forbidden(
            "Destructive admin actions are disabled by default".to_string(),
        ));
    }

    let auth_user = require_service_admin(&state, &headers).await?;
    info!("💥 Dropping table: {}", collection);

    SchemaGuard::validate_identifier(&collection)?;

    let sql = format!("DROP TABLE IF EXISTS {}", collection);
    state.store.execute_simple(sql).await?;
    record_admin_audit(
        &state,
        auth_user.id,
        "api.drop_table",
        "table",
        Some(collection.clone()),
        json!({"route": "/v1/tables/:collection"}),
    )
    .await?;

    Ok(Json(json!({
        "success": true,
        "message": format!("Table '{}' dropped successfully", collection)
    })))
}

/// SQL Request
#[derive(Debug, Deserialize)]
pub struct SqlRequest {
    pub query: String,
}

/// POST /v1/sql/query - Execute a SQL query and return rows
pub(crate) async fn sql_query_handler(
    State(state): State<AppState>,
    request: Request<Body>,
) -> Result<impl IntoResponse, StackhouseError> {
    if !state.raw_sql_enabled {
        return Err(StackhouseError::Forbidden(
            "Raw SQL access is disabled by default".to_string(),
        ));
    }

    let auth_user = require_service_admin(&state, request.headers()).await?;
    let body = to_bytes(request.into_body(), usize::MAX)
        .await
        .map_err(|e| StackhouseError::InvalidPayload(format!("Invalid request body: {}", e)))?;
    let payload: SqlRequest = serde_json::from_slice(&body)?;
    let query_length = payload.query.len();
    info!("🔍 Executing Raw SQL Query: {}", payload.query);

    validate_raw_sql(
        "query",
        &payload.query,
        Some(&state.raw_sql_query_allowlist),
        None,
        state.destructive_admin_enabled,
    )?;

    let rows = state.store.query_simple(payload.query).await?;

    // Transform specifically to look generic
    let results: Vec<Value> = rows
        .into_iter()
        .map(|row| {
            let mut obj = serde_json::Map::new();
            for (key, value) in row {
                obj.insert(key, value);
            }
            Value::Object(obj)
        })
        .collect();
    record_admin_audit(
        &state,
        auth_user.id,
        "api.sql_query",
        "sql",
        None,
        json!({
            "route": "/v1/sql/query",
            "query_length": query_length,
            "result_count": results.len(),
        }),
    )
    .await?;

    Ok(Json(json!({
        "success": true,
        "data": results,
        "count": results.len()
    })))
}

/// POST /v1/sql/execute - Execute a SQL statement (DDL/DML)
pub(crate) async fn sql_execute_handler(
    State(state): State<AppState>,
    request: Request<Body>,
) -> Result<impl IntoResponse, StackhouseError> {
    if !state.raw_sql_enabled {
        return Err(StackhouseError::Forbidden(
            "Raw SQL access is disabled by default".to_string(),
        ));
    }

    let auth_user = require_service_admin(&state, request.headers()).await?;
    let body = to_bytes(request.into_body(), usize::MAX)
        .await
        .map_err(|e| StackhouseError::InvalidPayload(format!("Invalid request body: {}", e)))?;
    let payload: SqlRequest = serde_json::from_slice(&body)?;
    let query_length = payload.query.len();
    info!("⚡ Executing Raw SQL Statement: {}", payload.query);

    validate_raw_sql(
        "execute",
        &payload.query,
        None,
        Some(&state.raw_sql_execute_blocklist),
        state.destructive_admin_enabled,
    )?;

    let affected = state.store.execute_simple(payload.query).await?;
    record_admin_audit(
        &state,
        auth_user.id,
        "api.sql_execute",
        "sql",
        None,
        json!({
            "route": "/v1/sql/execute",
            "query_length": query_length,
            "affected": affected,
        }),
    )
    .await?;

    Ok(Json(json!({
        "success": true,
        "affected": affected
    })))
}

async fn require_service_admin(
    state: &AppState,
    headers: &HeaderMap,
) -> Result<AuthUser, StackhouseError> {
    let auth_state = state
        .auth
        .as_ref()
        .ok_or_else(|| StackhouseError::Unauthorized("Authentication required".to_string()))?;
    let auth_user = extract_auth_user(auth_state, headers)?;
    let user = auth_state.auth.get_user_by_id(auth_user.id).await?;
    state
        .authorization
        .require_service_admin_unconditional(&user)?;
    Ok(auth_user)
}

async fn record_admin_audit(
    state: &AppState,
    actor_user_id: i64,
    action: &str,
    resource_type: &str,
    resource_id: Option<String>,
    details: Value,
) -> Result<(), StackhouseError> {
    let admin_audit = state.admin_audit.as_ref().ok_or_else(|| {
        StackhouseError::Internal(anyhow::anyhow!("Admin audit service unavailable"))
    })?;
    admin_audit
        .record(
            actor_user_id,
            action,
            resource_type,
            resource_id,
            "success",
            details,
        )
        .await
}

/// Decode the payload (middle segment) of a JWT and return it as a JSON string.
/// This does NOT verify the signature — verification is handled by auth middleware.
/// Used only for extracting claims to inject into PostgreSQL's RLS session context.
fn decode_jwt_payload(token: &str) -> Option<String> {
    let parts: Vec<&str> = token.split('.').collect();
    if parts.len() != 3 {
        return None;
    }
    use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine as _};
    let decoded = URL_SAFE_NO_PAD.decode(parts[1]).ok()?;
    String::from_utf8(decoded).ok()
}

// ============================================================================
// Raw SQL statement filtering
// ============================================================================

fn raw_sql_query_allowlist_from_env() -> Vec<String> {
    env::var("STACKHOUSE_RAW_SQL_QUERY_ALLOWLIST")
        .map(|s| {
            s.split(',')
                .map(|t| t.trim().to_uppercase())
                .filter(|t| !t.is_empty())
                .collect()
        })
        .unwrap_or_else(|_| {
            vec![
                "SELECT".into(),
                "WITH".into(),
                "EXPLAIN".into(),
                "SHOW".into(),
                "DESCRIBE".into(),
                "VALUES".into(),
            ]
        })
}

fn raw_sql_execute_blocklist_from_env() -> Vec<String> {
    env::var("STACKHOUSE_RAW_SQL_EXECUTE_BLOCKLIST")
        .map(|s| {
            s.split(',')
                .map(|t| t.trim().to_uppercase())
                .filter(|t| !t.is_empty())
                .collect()
        })
        .unwrap_or_else(|_| {
            // Default destructive-only blocklist. These are also gated by the
            // destructive_admin_enabled flag; the flag must be set to run them.
            vec!["DROP".into(), "TRUNCATE".into(), "ALTER".into()]
        })
}

/// Walk past SQL comments and string literals and return the first non-whitespace
/// keyword (e.g. SELECT, INSERT, DROP). Returns None for empty/comment-only input.
fn first_sql_keyword(sql: &str) -> Option<String> {
    let mut in_block_comment = false;
    let mut in_line_comment = false;
    let mut in_string = false;
    let mut string_quote = '\0';
    let mut chars = sql.chars().peekable();
    let mut token = String::new();

    while let Some(c) = chars.next() {
        if in_block_comment {
            if c == '*' && chars.peek() == Some(&'/') {
                chars.next();
                in_block_comment = false;
            }
            continue;
        }
        if in_line_comment {
            if c == '\n' {
                in_line_comment = false;
            }
            continue;
        }
        if in_string {
            if c == string_quote {
                in_string = false;
            } else if c == '\\' && string_quote == '\'' {
                // skip escaped quote; we don't need the exact string content
                let _ = chars.next();
            }
            continue;
        }
        if c == '-' && chars.peek() == Some(&'-') {
            chars.next();
            in_line_comment = true;
            continue;
        }
        if c == '/' && chars.peek() == Some(&'*') {
            chars.next();
            in_block_comment = true;
            continue;
        }
        if c == '\'' || c == '"' {
            in_string = true;
            string_quote = c;
            continue;
        }
        if c.is_alphanumeric() || c == '_' {
            token.push(c);
        } else if !token.is_empty() {
            break;
        }
    }

    if token.is_empty() {
        None
    } else {
        Some(token.to_uppercase())
    }
}

/// Returns true if the SQL contains more than one statement, i.e. a
/// semicolon with a subsequent SQL token outside of strings or comments.
/// Allows a trailing semicolon on an otherwise single statement.
fn has_multiple_statements(sql: &str) -> bool {
    let mut in_block_comment = false;
    let mut in_line_comment = false;
    let mut in_string = false;
    let mut string_quote = '\0';
    let mut chars = sql.chars().peekable();
    let mut saw_semicolon = false;

    while let Some(c) = chars.next() {
        if in_block_comment {
            if c == '*' && chars.peek() == Some(&'/') {
                chars.next();
                in_block_comment = false;
            }
            continue;
        }
        if in_line_comment {
            if c == '\n' {
                in_line_comment = false;
            }
            continue;
        }
        if in_string {
            if c == string_quote {
                in_string = false;
            } else if c == '\\' && string_quote == '\'' {
                let _ = chars.next();
            }
            continue;
        }
        if c == '-' && chars.peek() == Some(&'-') {
            chars.next();
            in_line_comment = true;
            continue;
        }
        if c == '/' && chars.peek() == Some(&'*') {
            chars.next();
            in_block_comment = true;
            continue;
        }
        if c == '\'' || c == '"' {
            if saw_semicolon {
                return true;
            }
            in_string = true;
            string_quote = c;
            continue;
        }
        if saw_semicolon {
            if !c.is_whitespace() {
                return true;
            }
            continue;
        }
        if c == ';' {
            saw_semicolon = true;
        }
    }

    false
}

const DESTRUCTIVE_KEYWORDS: &[&str] = &["DROP", "TRUNCATE", "ALTER"];

/// Validate a raw SQL statement against an allowlist and/or blocklist.
/// `mode` is used to return a contextual error message.
/// When `allow_destructive` is `false`, destructive statement types
/// (DROP, TRUNCATE, ALTER) and statement chaining are rejected
/// regardless of the allowlist/blocklist.
fn validate_raw_sql(
    mode: &str,
    sql: &str,
    allowlist: Option<&[String]>,
    blocklist: Option<&[String]>,
    allow_destructive: bool,
) -> Result<(), StackhouseError> {
    if sql.trim().is_empty() {
        return Err(StackhouseError::InvalidPayload(
            "Empty SQL statement".into(),
        ));
    }

    // Reject statement chaining unless destructive admin is enabled.
    if !allow_destructive && has_multiple_statements(sql) {
        return Err(StackhouseError::InvalidPayload(
            "Multiple SQL statements are not allowed".into(),
        ));
    }

    let keyword = first_sql_keyword(sql).ok_or_else(|| {
        StackhouseError::InvalidPayload("Could not determine SQL statement type".into())
    })?;
    let keyword_upper = keyword.to_uppercase();

    // Enforce destructive-statement restrictions.
    if !allow_destructive && DESTRUCTIVE_KEYWORDS.contains(&keyword_upper.as_str()) {
        return Err(StackhouseError::InvalidPayload(format!(
            "Destructive SQL statement type '{}' is not allowed unless destructive admin is enabled",
            keyword
        )));
    }

    // Skip allowlist/blocklist for destructive statements when destructive admin
    // is explicitly enabled, since they are permitted in that mode.
    if allow_destructive && DESTRUCTIVE_KEYWORDS.contains(&keyword_upper.as_str()) {
        return Ok(());
    }

    if let Some(list) = allowlist {
        if !list
            .iter()
            .any(|allowed| allowed.eq_ignore_ascii_case(&keyword))
        {
            return Err(StackhouseError::InvalidPayload(format!(
                "SQL statement type '{}' is not allowed for {}. Allowed: {:?}",
                keyword, mode, list
            )));
        }
    }

    if let Some(list) = blocklist {
        if list
            .iter()
            .any(|blocked| blocked.eq_ignore_ascii_case(&keyword))
        {
            return Err(StackhouseError::InvalidPayload(format!(
                "SQL statement type '{}' is blocked for {}",
                keyword, mode
            )));
        }
    }

    Ok(())
}

// === Dataset (data-processing layer) handlers ===

#[derive(Deserialize)]
pub struct DatasetQueryParams {
    pub limit: Option<i64>,
}

pub(crate) async fn list_datasets_handler(
    State(state): State<AppState>,
    request: Request<Body>,
) -> Result<impl IntoResponse, StackhouseError> {
    let tenant_id = if let Some(auth) = &state.auth {
        extract_auth_user(auth, request.headers())
            .map(|u| u.id)
            .unwrap_or(0)
    } else {
        0
    };
    let datasets = state.datasets.list(tenant_id).await?;
    Ok(Json(json!({
        "success": true,
        "datasets": datasets,
        "count": datasets.len()
    })))
}

pub(crate) async fn create_dataset_handler(
    State(state): State<AppState>,
    request: Request<Body>,
) -> Result<impl IntoResponse, StackhouseError> {
    let tenant_id = if let Some(auth) = &state.auth {
        extract_auth_user(auth, request.headers())
            .map(|u| u.id)
            .unwrap_or(0)
    } else {
        0
    };
    let body = to_bytes(request.into_body(), usize::MAX)
        .await
        .map_err(|e| StackhouseError::InvalidPayload(format!("Invalid request body: {}", e)))?;
    let payload: CreateDatasetRequest = serde_json::from_slice(&body)?;
    let dataset = state.datasets.create(tenant_id, payload).await?;
    Ok(Json(json!({
        "success": true,
        "dataset": dataset
    })))
}

pub(crate) async fn get_dataset_handler(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<impl IntoResponse, StackhouseError> {
    let dataset = state.datasets.get(&id).await?;
    Ok(Json(json!({
        "success": true,
        "dataset": dataset
    })))
}

pub(crate) async fn preview_dataset_handler(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Query(params): Query<DatasetQueryParams>,
) -> Result<impl IntoResponse, StackhouseError> {
    let rows = state.datasets.query(&id, params.limit).await?;
    Ok(Json(json!({
        "success": true,
        "data": rows,
        "count": rows.len()
    })))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::api::create_router;
    use axum::body::Body;
    use axum::http::{Request, StatusCode};
    use tower::util::ServiceExt;

    async fn create_test_app() -> axum::Router {
        let store = Arc::new(StackhouseStore::in_memory().await.unwrap());
        let state = AppState::new(store);
        create_router(state)
    }

    #[tokio::test]
    async fn test_health_endpoint() {
        let app = create_test_app().await;

        let response = app
            .oneshot(
                Request::builder()
                    .uri("/health")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn test_push_and_query() {
        let store = Arc::new(StackhouseStore::in_memory().await.unwrap());
        let state = AppState::new(store);
        let app = create_router(state);

        // Push data
        let response = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/v1/push/users")
                    .header("content-type", "application/json")
                    .body(Body::from(r#"{"name": "Alice", "age": 30}"#))
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::CREATED);

        // Query data
        let response = app
            .oneshot(
                Request::builder()
                    .uri("/v1/query/users")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);
    }

    #[test]
    fn validate_raw_sql_rejects_destructive_statements_by_default() {
        let allowlist = vec!["SELECT".into(), "WITH".into()];
        assert!(
            validate_raw_sql("query", "DROP TABLE users", Some(&allowlist), None, false).is_err()
        );
        assert!(validate_raw_sql("execute", "TRUNCATE TABLE users", None, None, false).is_err());
        assert!(validate_raw_sql(
            "execute",
            "ALTER TABLE users ADD COLUMN age INT",
            None,
            None,
            false
        )
        .is_err());
    }

    #[test]
    fn validate_raw_sql_allows_destructive_statements_when_enabled() {
        let allowlist = vec!["SELECT".into()];
        let blocklist = vec!["DROP".into(), "TRUNCATE".into(), "ALTER".into()];
        assert!(
            validate_raw_sql("query", "DROP TABLE users", Some(&allowlist), None, true).is_ok()
        );
        assert!(validate_raw_sql(
            "execute",
            "TRUNCATE TABLE users",
            None,
            Some(&blocklist),
            true
        )
        .is_ok());
        assert!(validate_raw_sql(
            "execute",
            "ALTER TABLE users ADD COLUMN age INT",
            None,
            Some(&blocklist),
            true
        )
        .is_ok());
    }

    #[test]
    fn validate_raw_sql_rejects_multiple_statements_unless_allowed() {
        assert!(validate_raw_sql("query", "SELECT 1; SELECT 2", None, None, false).is_err());
        assert!(validate_raw_sql("execute", "SELECT 1; SELECT 2", None, None, false).is_err());
        assert!(validate_raw_sql("query", "SELECT 1; SELECT 2", None, None, true).is_ok());
        assert!(validate_raw_sql("execute", "SELECT 1; SELECT 2", None, None, true).is_ok());
    }
}
