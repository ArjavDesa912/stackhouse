//! # 🛸 Stackhouse
//!
//! A high-performance, "Schema-Later" database that dynamically evolves
//! its schema based on incoming JSON payloads. Better than Supabase.
//!
//! ## Core Components
//!
//! - **Stackhouse-API**: HTTP/WebSocket API layer powered by Axum
//! - **Schema-Brain (Inference)**: Inspects JSON and generates delta-migrations
//! - **Migration-Automaton (Guard)**: Executes ALTER TABLE statements safely
//! - **Stackhouse-Store**: PostgreSQL connection pool and query utilities
//! - **Stackhouse-Explorer**: Embedded dashboard for real-time visualization
//! - **Stackhouse-Auth**: JWT-based authentication with Argon2 password hashing
//! - **Stackhouse-OAuth**: OAuth2 social login (Google, GitHub, Apple, Discord)
//! - **Stackhouse-MagicLink**: Passwordless authentication via email magic links
//! - **Stackhouse-MFA**: Multi-factor authentication with TOTP and recovery codes
//! - **Stackhouse-PhoneOTP**: SMS-based phone verification (Twilio)
//! - **Stackhouse-Captcha**: hCaptcha, reCAPTCHA, Turnstile verification
//! - **Stackhouse-Storage**: Bucket-based file storage with PostgreSQL metadata
//! - **Stackhouse-Vectors**: Qdrant-powered similarity search with HNSW indexing
//! - **Stackhouse-RLS**: Row Level Security with JWT context injection
//! - **Stackhouse-Realtime**: WebSocket-based realtime subscriptions
//! - **Stackhouse-Presence**: User presence tracking for collaborative apps
//! - **Stackhouse-Broadcast**: Pub/sub broadcast channels
//! - **Stackhouse-GraphQL**: Auto-generated GraphQL API from database schema
//! - **Stackhouse-Metrics**: Prometheus metrics and observability
//! - **Stackhouse-LogDrain**: Structured logging with webhook drains
//! - **Stackhouse-ImageTransform**: On-the-fly image transformations
//! - **Stackhouse-Extensions**: Postgres extension management
//! - **Stackhouse-Branching**: Database branching for dev/staging
//! - **Stackhouse-Teams**: Organization & role-based access control
//! - **Stackhouse-Network**: IP allowlisting & network security
//! - **Stackhouse-Backup**: Database backup & point-in-time recovery

pub mod api;
pub mod auth;
#[path = "security/authorization.rs"]
pub mod authorization;
#[path = "storage/backups.rs"]
pub mod backup;
pub mod branching;
#[path = "realtime/broadcast.rs"]
pub mod broadcast;
#[path = "platform/db.rs"]
pub mod db;
#[path = "platform/error.rs"]
pub mod error;
#[path = "storage/explorer.rs"]
pub mod explorer;
pub mod extensions;
#[path = "security/guard.rs"]
pub mod guard;
pub mod image_transform;
pub mod inference;
#[path = "platform/logging.rs"]
pub mod log_drain;
#[path = "platform/metrics.rs"]
pub mod metrics;
#[path = "security/network.rs"]
pub mod network;
pub mod platform;
#[path = "realtime/presence.rs"]
pub mod presence;
pub mod realtime;
#[path = "security/rls.rs"]
pub mod rls;
pub mod security;
pub mod storage;
pub mod teams;
#[path = "storage/vectors.rs"]
pub mod vector;

// === Data Processing (datasets) ===
pub mod data_processing;

// === Billing / Subscriptions (RevenueCat-style) ===
pub mod billing;

// === Edge Functions & Compute ===
pub mod compute;

// === Local Dev CLI ===
pub mod cli;

pub use api::admin as admin_audit;
pub use auth::mfa;
pub use authorization::{AuthorizationService, SecurityConfig};
pub use error::{StackhouseError, StackhouseResult};
