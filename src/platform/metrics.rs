//! # Observability Module (Stackhouse-Metrics)
//!
//! Production-grade Prometheus metrics and structured observability.
//!
//! ## Metrics Exposed
//! - HTTP request duration histograms
//! - Request count by method, path, status
//! - Active connections gauge
//! - Database query duration
//! - Auth events (signup, login, oauth)
//! - Storage operations
//! - Realtime WebSocket connections
//! - System metrics (uptime, version)

use axum::{
    body::Body,
    extract::State,
    http::{Request, Response, StatusCode},
    middleware::Next,
    response::IntoResponse,
    routing::get,
    Router,
};
use prometheus::{
    Encoder, HistogramOpts, HistogramVec, IntCounter, IntCounterVec, IntGauge, Opts, Registry,
    TextEncoder,
};
use std::sync::Arc;
use std::time::Instant;

// ============================================================================
// Metrics Registry
// ============================================================================

/// Central metrics registry for all Stackhouse metrics
#[derive(Clone)]
pub struct StackhouseMetrics {
    pub registry: Arc<Registry>,

    // HTTP metrics
    pub http_requests_total: IntCounterVec,
    pub http_request_duration: HistogramVec,
    pub http_active_connections: IntGauge,

    // Database metrics
    pub db_query_duration: HistogramVec,
    pub db_queries_total: IntCounterVec,

    // Auth metrics
    pub auth_signups_total: IntCounter,
    pub auth_logins_total: IntCounter,
    pub auth_oauth_logins_total: IntCounterVec,
    pub auth_failed_logins_total: IntCounter,
    pub auth_token_refreshes_total: IntCounter,

    // Storage metrics
    pub storage_uploads_total: IntCounter,
    pub storage_downloads_total: IntCounter,
    pub storage_bytes_uploaded: IntCounter,
    pub storage_bytes_downloaded: IntCounter,

    // Realtime metrics
    pub realtime_connections: IntGauge,
    pub realtime_messages_sent: IntCounter,
    pub realtime_subscriptions: IntGauge,

    // System
    pub uptime_seconds: IntGauge,
    pub start_time: Instant,
}

impl StackhouseMetrics {
    pub fn new() -> Self {
        let registry = Registry::new();

        // HTTP
        let http_requests_total = IntCounterVec::new(
            Opts::new("stackhouse_http_requests_total", "Total HTTP requests"),
            &["method", "path", "status"],
        )
        .unwrap();

        let http_request_duration = HistogramVec::new(
            HistogramOpts::new(
                "stackhouse_http_request_duration_seconds",
                "HTTP request duration",
            )
            .buckets(vec![
                0.001, 0.005, 0.01, 0.025, 0.05, 0.1, 0.25, 0.5, 1.0, 5.0, 10.0,
            ]),
            &["method", "path"],
        )
        .unwrap();

        let http_active_connections = IntGauge::new(
            "stackhouse_http_active_connections",
            "Active HTTP connections",
        )
        .unwrap();

        // Database
        let db_query_duration = HistogramVec::new(
            HistogramOpts::new(
                "stackhouse_db_query_duration_seconds",
                "Database query duration",
            )
            .buckets(vec![0.0005, 0.001, 0.005, 0.01, 0.025, 0.05, 0.1, 0.5, 1.0]),
            &["operation"],
        )
        .unwrap();

        let db_queries_total = IntCounterVec::new(
            Opts::new("stackhouse_db_queries_total", "Total database queries"),
            &["operation", "status"],
        )
        .unwrap();

        // Auth
        let auth_signups_total =
            IntCounter::new("stackhouse_auth_signups_total", "Total user signups").unwrap();
        let auth_logins_total =
            IntCounter::new("stackhouse_auth_logins_total", "Total successful logins").unwrap();
        let auth_oauth_logins_total = IntCounterVec::new(
            Opts::new(
                "stackhouse_auth_oauth_logins_total",
                "OAuth logins by provider",
            ),
            &["provider"],
        )
        .unwrap();
        let auth_failed_logins_total = IntCounter::new(
            "stackhouse_auth_failed_logins_total",
            "Total failed login attempts",
        )
        .unwrap();
        let auth_token_refreshes_total = IntCounter::new(
            "stackhouse_auth_token_refreshes_total",
            "Total token refreshes",
        )
        .unwrap();

        // Storage
        let storage_uploads_total =
            IntCounter::new("stackhouse_storage_uploads_total", "Total file uploads").unwrap();
        let storage_downloads_total =
            IntCounter::new("stackhouse_storage_downloads_total", "Total file downloads").unwrap();
        let storage_bytes_uploaded = IntCounter::new(
            "stackhouse_storage_bytes_uploaded_total",
            "Total bytes uploaded",
        )
        .unwrap();
        let storage_bytes_downloaded = IntCounter::new(
            "stackhouse_storage_bytes_downloaded_total",
            "Total bytes downloaded",
        )
        .unwrap();

        // Realtime
        let realtime_connections = IntGauge::new(
            "stackhouse_realtime_connections",
            "Active WebSocket connections",
        )
        .unwrap();
        let realtime_messages_sent = IntCounter::new(
            "stackhouse_realtime_messages_sent_total",
            "Total realtime messages sent",
        )
        .unwrap();
        let realtime_subscriptions = IntGauge::new(
            "stackhouse_realtime_subscriptions",
            "Active realtime subscriptions",
        )
        .unwrap();

        // System
        let uptime_seconds =
            IntGauge::new("stackhouse_uptime_seconds", "Server uptime in seconds").unwrap();

        // Register all metrics
        let _ = registry.register(Box::new(http_requests_total.clone()));
        let _ = registry.register(Box::new(http_request_duration.clone()));
        let _ = registry.register(Box::new(http_active_connections.clone()));
        let _ = registry.register(Box::new(db_query_duration.clone()));
        let _ = registry.register(Box::new(db_queries_total.clone()));
        let _ = registry.register(Box::new(auth_signups_total.clone()));
        let _ = registry.register(Box::new(auth_logins_total.clone()));
        let _ = registry.register(Box::new(auth_oauth_logins_total.clone()));
        let _ = registry.register(Box::new(auth_failed_logins_total.clone()));
        let _ = registry.register(Box::new(auth_token_refreshes_total.clone()));
        let _ = registry.register(Box::new(storage_uploads_total.clone()));
        let _ = registry.register(Box::new(storage_downloads_total.clone()));
        let _ = registry.register(Box::new(storage_bytes_uploaded.clone()));
        let _ = registry.register(Box::new(storage_bytes_downloaded.clone()));
        let _ = registry.register(Box::new(realtime_connections.clone()));
        let _ = registry.register(Box::new(realtime_messages_sent.clone()));
        let _ = registry.register(Box::new(realtime_subscriptions.clone()));
        let _ = registry.register(Box::new(uptime_seconds.clone()));

        Self {
            registry: Arc::new(registry),
            http_requests_total,
            http_request_duration,
            http_active_connections,
            db_query_duration,
            db_queries_total,
            auth_signups_total,
            auth_logins_total,
            auth_oauth_logins_total,
            auth_failed_logins_total,
            auth_token_refreshes_total,
            storage_uploads_total,
            storage_downloads_total,
            storage_bytes_uploaded,
            storage_bytes_downloaded,
            realtime_connections,
            realtime_messages_sent,
            realtime_subscriptions,
            uptime_seconds,
            start_time: Instant::now(),
        }
    }

    /// Record an HTTP request
    pub fn record_request(&self, method: &str, path: &str, status: u16, duration: f64) {
        self.http_requests_total
            .with_label_values(&[method, path, &status.to_string()])
            .inc();
        self.http_request_duration
            .with_label_values(&[method, path])
            .observe(duration);
    }

    /// Record a database query
    pub fn record_db_query(&self, operation: &str, duration: f64, success: bool) {
        self.db_query_duration
            .with_label_values(&[operation])
            .observe(duration);
        self.db_queries_total
            .with_label_values(&[operation, if success { "ok" } else { "error" }])
            .inc();
    }

    /// Update uptime
    pub fn update_uptime(&self) {
        self.uptime_seconds
            .set(self.start_time.elapsed().as_secs() as i64);
    }

    /// Gather all metrics as Prometheus text
    pub fn gather(&self) -> String {
        self.update_uptime();
        let encoder = TextEncoder::new();
        let metric_families = self.registry.gather();
        let mut buffer = Vec::new();
        encoder.encode(&metric_families, &mut buffer).unwrap();
        String::from_utf8(buffer).unwrap()
    }
}

// ============================================================================
// HTTP Metrics Middleware
// ============================================================================

/// Axum middleware layer that records HTTP request metrics
pub async fn metrics_middleware(
    State(metrics): State<Arc<StackhouseMetrics>>,
    request: Request<Body>,
    next: Next,
) -> Response<Body> {
    let method = request.method().to_string();
    let path = request.uri().path().to_string();

    // Normalize path for cardinality control
    let normalized_path = normalize_path(&path);

    metrics.http_active_connections.inc();
    let start = Instant::now();

    let response = next.run(request).await;

    let duration = start.elapsed().as_secs_f64();
    let status = response.status().as_u16();
    metrics.http_active_connections.dec();
    metrics.record_request(&method, &normalized_path, status, duration);

    response
}

/// Normalize paths to prevent high-cardinality label explosion
fn normalize_path(path: &str) -> String {
    let parts: Vec<&str> = path.split('/').collect();
    let mut normalized = Vec::new();

    for (_i, part) in parts.iter().enumerate() {
        if part.is_empty() {
            continue;
        }

        // Replace IDs and dynamic segments with placeholders
        if part.parse::<i64>().is_ok() || part.len() > 20 {
            normalized.push(":id");
        } else {
            normalized.push(part);
        }
    }

    format!("/{}", normalized.join("/"))
}

// ============================================================================
// Metrics State & Handlers
// ============================================================================

#[derive(Clone)]
pub struct MetricsState {
    pub metrics: Arc<StackhouseMetrics>,
}

/// GET /metrics - Prometheus metrics endpoint  
async fn metrics_handler(State(state): State<MetricsState>) -> impl IntoResponse {
    let body = state.metrics.gather();
    (
        StatusCode::OK,
        [("content-type", "text/plain; version=0.0.4; charset=utf-8")],
        body,
    )
}

/// GET /v1/metrics/summary - Human-readable metrics summary
async fn metrics_summary_handler(State(state): State<MetricsState>) -> impl IntoResponse {
    state.metrics.update_uptime();

    axum::Json(serde_json::json!({
        "uptime_seconds": state.metrics.start_time.elapsed().as_secs(),
        "http": {
            "active_connections": state.metrics.http_active_connections.get(),
        },
        "auth": {
            "total_signups": state.metrics.auth_signups_total.get(),
            "total_logins": state.metrics.auth_logins_total.get(),
            "failed_logins": state.metrics.auth_failed_logins_total.get(),
        },
        "storage": {
            "total_uploads": state.metrics.storage_uploads_total.get(),
            "total_downloads": state.metrics.storage_downloads_total.get(),
        },
        "realtime": {
            "active_connections": state.metrics.realtime_connections.get(),
            "active_subscriptions": state.metrics.realtime_subscriptions.get(),
            "total_messages": state.metrics.realtime_messages_sent.get(),
        }
    }))
}

// ============================================================================
// Router
// ============================================================================

pub fn create_metrics_router(state: MetricsState) -> Router {
    Router::new()
        .route("/metrics", get(metrics_handler))
        .route("/v1/metrics/summary", get(metrics_summary_handler))
        .with_state(state)
}
