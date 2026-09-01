//! # Stackhouse-API
//!
//! The HTTP/WebSocket API layer powered by Axum.
//! Provides idempotent endpoints for data ingestion and querying.

mod handlers;
mod routes;

pub mod admin;

pub mod auto_rest;
pub mod dashboard;
pub mod graphql;
pub mod mcp_server;
pub mod openapi;
pub mod platform;
pub mod versioned_api;

pub use admin::*;

pub use auto_rest::*;
pub use dashboard::*;
pub use graphql::*;
pub use handlers::*;
pub use mcp_server::*;
pub use openapi::*;
pub use platform::*;
pub use routes::create_router;
pub use versioned_api::*;
