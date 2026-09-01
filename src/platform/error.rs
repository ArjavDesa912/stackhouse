//! # Error Handling Module
//!
//! Provides structured error types for Stackhouse operations.
//! All errors are propagated with meaningful messages for API consumers.

use axum::{
    http::StatusCode,
    response::{IntoResponse, Response},
    Json,
};
use serde_json::json;
use thiserror::Error;

/// Result type alias for Stackhouse operations
pub type StackhouseResult<T> = Result<T, StackhouseError>;

/// Comprehensive error type for all Stackhouse operations
#[derive(Error, Debug)]
pub enum StackhouseError {
    /// Database connection or query errors
    #[error("Database error: {0}")]
    Database(String),

    /// JSON parsing or serialization errors
    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),

    /// Invalid table or column name
    #[error("Invalid identifier: {0}")]
    InvalidIdentifier(String),

    /// Schema validation errors
    #[error("Schema error: {0}")]
    Schema(String),

    /// Column limit exceeded (max 1000 per table)
    #[error("Column limit exceeded: {message}")]
    ColumnLimitExceeded { message: String },

    /// Table not found
    #[error("Table not found: {0}")]
    TableNotFound(String),

    /// Invalid payload structure
    #[error("Invalid payload: {0}")]
    InvalidPayload(String),

    /// Migration error
    #[error("Migration failed: {0}")]
    MigrationFailed(String),

    /// Internal server error
    #[error("Internal error: {0}")]
    Internal(#[from] anyhow::Error),

    // =========== Auth & Storage Errors ===========
    /// Authentication failed
    #[error("Unauthorized: {0}")]
    Unauthorized(String),

    /// Authenticated but not allowed to access the resource
    #[error("Forbidden: {0}")]
    Forbidden(String),

    /// Resource conflict (e.g., user already exists)
    #[error("Conflict: {0}")]
    Conflict(String),

    /// Resource not found
    #[error("Not found: {0}")]
    NotFound(String),

    /// Storage error
    #[error("Storage error: {0}")]
    Storage(String),

    /// Rate limited
    #[error("Rate limited: {0}")]
    RateLimited(String),

    /// Generic string-based error (used across modules for convenience)
    #[error("{0}")]
    Brain(String),
}

impl StackhouseError {
    /// Create an Internal error from a string
    pub fn internal(msg: impl Into<String>) -> Self {
        StackhouseError::Brain(msg.into())
    }
}

impl StackhouseError {
    /// Returns the appropriate HTTP status code for this error
    pub fn status_code(&self) -> StatusCode {
        match self {
            StackhouseError::Database(_) => StatusCode::SERVICE_UNAVAILABLE,
            StackhouseError::Json(_) => StatusCode::BAD_REQUEST,
            StackhouseError::InvalidIdentifier(_) => StatusCode::BAD_REQUEST,
            StackhouseError::Schema(_) => StatusCode::UNPROCESSABLE_ENTITY,
            StackhouseError::ColumnLimitExceeded { .. } => StatusCode::BAD_REQUEST,
            StackhouseError::TableNotFound(_) => StatusCode::NOT_FOUND,
            StackhouseError::InvalidPayload(_) => StatusCode::BAD_REQUEST,
            StackhouseError::MigrationFailed(_) => StatusCode::INTERNAL_SERVER_ERROR,
            StackhouseError::Internal(_) => StatusCode::INTERNAL_SERVER_ERROR,
            StackhouseError::Unauthorized(_) => StatusCode::UNAUTHORIZED,
            StackhouseError::Forbidden(_) => StatusCode::FORBIDDEN,
            StackhouseError::Conflict(_) => StatusCode::CONFLICT,
            StackhouseError::NotFound(_) => StatusCode::NOT_FOUND,
            StackhouseError::Storage(_) => StatusCode::INTERNAL_SERVER_ERROR,
            StackhouseError::RateLimited(_) => StatusCode::TOO_MANY_REQUESTS,
            StackhouseError::Brain(_) => StatusCode::INTERNAL_SERVER_ERROR,
        }
    }

    /// Returns a machine-readable error code
    pub fn error_code(&self) -> &'static str {
        match self {
            StackhouseError::Database(_) => "DATABASE_ERROR",
            StackhouseError::Json(_) => "JSON_ERROR",
            StackhouseError::InvalidIdentifier(_) => "INVALID_IDENTIFIER",
            StackhouseError::Schema(_) => "SCHEMA_ERROR",
            StackhouseError::ColumnLimitExceeded { .. } => "COLUMN_LIMIT_EXCEEDED",
            StackhouseError::TableNotFound(_) => "TABLE_NOT_FOUND",
            StackhouseError::InvalidPayload(_) => "INVALID_PAYLOAD",
            StackhouseError::MigrationFailed(_) => "MIGRATION_FAILED",
            StackhouseError::Internal(_) => "INTERNAL_ERROR",
            StackhouseError::Unauthorized(_) => "UNAUTHORIZED",
            StackhouseError::Forbidden(_) => "FORBIDDEN",
            StackhouseError::Conflict(_) => "CONFLICT",
            StackhouseError::NotFound(_) => "NOT_FOUND",
            StackhouseError::Storage(_) => "STORAGE_ERROR",
            StackhouseError::RateLimited(_) => "RATE_LIMITED",
            StackhouseError::Brain(_) => "BRAIN_ERROR",
        }
    }
}

/// Converts StackhouseError into an Axum HTTP response.
/// Internal errors are logged but return a generic message to the client.
impl IntoResponse for StackhouseError {
    fn into_response(self) -> Response {
        let status = self.status_code();
        let code = self.error_code();

        // Return generic messages for internal errors to prevent information leakage
        let message = match &self {
            StackhouseError::Internal(_) | StackhouseError::Brain(_) => {
                tracing::error!("Internal error: {}", self);
                "An internal error occurred. Please try again later.".to_string()
            }
            StackhouseError::Database(_) => {
                tracing::error!("Database error: {}", self);
                "Service temporarily unavailable. Please try again later.".to_string()
            }
            StackhouseError::MigrationFailed(_) => {
                tracing::error!("Migration error: {}", self);
                "Service temporarily unavailable. Please try again later.".to_string()
            }
            StackhouseError::Storage(_) => {
                tracing::error!("Storage error: {}", self);
                "Storage service error. Please try again later.".to_string()
            }
            _ => self.to_string(),
        };

        let body = Json(json!({
            "error": {
                "code": code,
                "message": message,
            },
            "success": false,
        }));

        (status, body).into_response()
    }
}

/// Convert sqlx errors to StackhouseError
impl From<sqlx::Error> for StackhouseError {
    fn from(err: sqlx::Error) -> Self {
        StackhouseError::Database(err.to_string())
    }
}
