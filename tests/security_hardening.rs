use std::sync::Arc;
use std::sync::Once;
use std::time::Duration;

use axum::{
    body::{to_bytes, Body},
    extract::ConnectInfo,
    http::{Request, StatusCode},
    middleware::from_fn_with_state,
    routing::get,
    Router,
};
use serde_json::Value;
use stackhouse::{
    admin_audit::AdminAuditService,
    api::{create_router, AppState},
    auth::{create_auth_router, AuthService, AuthState, SignupRequest},
    authorization::{AuthorizationService, SecurityConfig},
    backup::{create_backup_router, BackupService, BackupState, PitrService},
    branching::{create_branching_router, BranchingService, BranchingState},
    db::StackhouseStore,
    error::StackhouseError,
    extensions::{create_extensions_router, ExtensionsService, ExtensionsState},
    log_drain::{create_log_drain_router, LogDrainService, LogDrainState},
    mfa::MfaService,
    network::{
        create_network_router, network_middleware, NetworkRule, NetworkService, NetworkState,
    },
    teams::{create_teams_router, TeamsService, TeamsState},
};
use std::net::SocketAddr;
use std::path::PathBuf;
use tokio::process::Command;
use tokio::time::sleep;
use tower::ServiceExt;

async fn build_auth_context() -> (Arc<StackhouseStore>, AuthService, String, String, i64, i64) {
    let store = Arc::new(StackhouseStore::in_memory().await.unwrap());
    let auth = AuthService::new(Arc::clone(&store), AuthService::generate_secret())
        .await
        .unwrap();

    let admin_tokens = auth
        .signup(SignupRequest {
            email: "admin@stackhouse.dev".to_string(),
            password: "password123".to_string(),
            metadata: Some(serde_json::json!({"service_admin": true})),
        })
        .await
        .unwrap();

    let user_tokens = auth
        .signup(SignupRequest {
            email: "user@stackhouse.dev".to_string(),
            password: "password123".to_string(),
            metadata: Some(serde_json::json!({"service_admin": false})),
        })
        .await
        .unwrap();

    (
        store,
        auth,
        admin_tokens.access_token,
        user_tokens.access_token,
        admin_tokens.user.id,
        user_tokens.user.id,
    )
}

async fn build_log_router() -> (Router, String, String) {
    let (app, _, admin_token, user_token) =
        build_log_router_with_authorization(AuthorizationService::new(SecurityConfig::default()))
            .await;
    (app, admin_token, user_token)
}

async fn build_log_router_with_authorization(
    log_authorization: AuthorizationService,
) -> (Router, Arc<StackhouseStore>, String, String) {
    let (store, auth, admin_token, user_token, _, _) = build_auth_context().await;
    let log_drain = LogDrainService::new(Arc::clone(&store), vec![])
        .await
        .unwrap();
    let admin_audit = Arc::new(AdminAuditService::new(Arc::clone(&store)).await.unwrap());

    let state = LogDrainState {
        log_drain,
        auth: AuthState { auth },
        authorization: log_authorization,
        admin_audit,
    };

    let app = Router::new()
        .nest("/v1/auth", create_auth_router(state.auth.clone()))
        .nest("/v1/admin", create_log_drain_router(state));

    (app, store, admin_token, user_token)
}

async fn build_admin_context() -> (
    Arc<StackhouseStore>,
    AuthState,
    AuthorizationService,
    String,
    String,
) {
    let (store, auth, admin_token, user_token, _, _) = build_auth_context().await;
    (
        store,
        AuthState { auth },
        AuthorizationService::new(SecurityConfig::default()),
        admin_token,
        user_token,
    )
}

fn install_test_data_encryption_key() {
    static INIT: Once = Once::new();
    INIT.call_once(|| {
        std::env::set_var(
            "STACKHOUSE_DATA_ENCRYPTION_KEY",
            "000102030405060708090a0b0c0d0e0f101112131415161718191a1b1c1d1e1f",
        );
    });
}

fn free_port() -> u16 {
    std::net::TcpListener::bind(("127.0.0.1", 0))
        .unwrap()
        .local_addr()
        .unwrap()
        .port()
}

fn stackhouse_binary_path() -> PathBuf {
    if let Some(path) = std::env::var_os("CARGO_BIN_EXE_stackhouse") {
        return PathBuf::from(path);
    }

    let mut path = std::env::current_exe().expect("failed to resolve test executable path");
    path.pop();
    path.pop();
    path.push(if cfg!(windows) {
        "stackhouse.exe"
    } else {
        "stackhouse"
    });
    path
}

async fn spawn_stackhouse_server(port: u16, allowed_origins: &str) -> tokio::process::Child {
    let mut command = Command::new(stackhouse_binary_path());
    command
        .env("STACKHOUSE_MEMORY", "1")
        .env("STACKHOUSE_HOST", "127.0.0.1")
        .env("STACKHOUSE_PORT", port.to_string())
        .env("STACKHOUSE_CORS_ALLOWED_ORIGINS", allowed_origins)
        .env("RUST_LOG", "warn")
        .kill_on_drop(true);

    command.spawn().expect("failed to start stackhouse binary")
}

async fn wait_for_health(base_url: &str) {
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(1))
        .build()
        .unwrap();
    let deadline = tokio::time::Instant::now() + Duration::from_secs(30);

    loop {
        if let Ok(response) = client.get(format!("{}/health", base_url)).send().await {
            if response.status().is_success() {
                return;
            }
        }

        if tokio::time::Instant::now() >= deadline {
            panic!("timed out waiting for stackhouse server at {}", base_url);
        }

        sleep(Duration::from_millis(200)).await;
    }
}

async fn build_security_enabled_app() -> (Router, String, String) {
    let (app, _, admin_token, user_token) = build_security_enabled_app_with_authorization(
        AuthorizationService::new(SecurityConfig::default()),
    )
    .await;
    (app, admin_token, user_token)
}

async fn build_security_enabled_app_with_authorization(
    authorization: AuthorizationService,
) -> (Router, Arc<StackhouseStore>, String, String) {
    let (store, auth_state, _, admin_token, user_token) = build_admin_context().await;
    let log_drain = LogDrainService::new(Arc::clone(&store), vec![])
        .await
        .unwrap();
    let admin_audit = Arc::new(AdminAuditService::new(Arc::clone(&store)).await.unwrap());
    let app = create_router(AppState::with_security(
        Arc::clone(&store),
        auth_state.clone(),
        authorization.clone(),
        true,
        true,
        Some(Arc::clone(&admin_audit)),
    ))
    .nest("/v1/auth", create_auth_router(auth_state.clone()))
    .nest(
        "/v1/admin",
        create_log_drain_router(LogDrainState {
            log_drain,
            auth: auth_state,
            authorization,
            admin_audit,
        }),
    );

    (app, store, admin_token, user_token)
}

async fn build_branching_router() -> (Router, Arc<StackhouseStore>, String, String) {
    build_branching_router_with_authorization(AuthorizationService::new(SecurityConfig::default()))
        .await
}

async fn build_branching_router_with_authorization(
    branching_authorization: AuthorizationService,
) -> (Router, Arc<StackhouseStore>, String, String) {
    let (store, auth_state, _, admin_token, user_token) = build_admin_context().await;
    let branching = Arc::new(BranchingService::new(Arc::clone(&store)));
    let admin_audit = Arc::new(AdminAuditService::new(Arc::clone(&store)).await.unwrap());

    let app = Router::new()
        .nest("/v1/auth", create_auth_router(auth_state.clone()))
        .nest(
            "/v1/admin",
            create_branching_router(BranchingState {
                branching,
                auth: auth_state,
                authorization: branching_authorization,
                admin_audit,
            }),
        );

    (app, store, admin_token, user_token)
}

async fn build_extensions_router() -> (Router, Arc<StackhouseStore>, String, String) {
    let (store, auth_state, authorization, admin_token, user_token) = build_admin_context().await;
    let extensions = Arc::new(ExtensionsService::new(Arc::clone(&store)));
    let admin_audit = Arc::new(AdminAuditService::new(Arc::clone(&store)).await.unwrap());

    let app = Router::new()
        .nest("/v1/auth", create_auth_router(auth_state.clone()))
        .nest(
            "/v1/admin",
            create_extensions_router(ExtensionsState {
                extensions,
                auth: auth_state,
                authorization,
                admin_audit,
            }),
        );

    (app, store, admin_token, user_token)
}

async fn build_backup_router_with_authorization(
    backup_authorization: AuthorizationService,
) -> Option<(Router, Arc<StackhouseStore>, String, String)> {
    install_test_data_encryption_key();
    let store = match StackhouseStore::in_memory().await {
        Ok(store) => Arc::new(store),
        Err(error) => {
            eprintln!(
                "skipping backup/pitr integration test because the test database is unavailable: {error}"
            );
            return None;
        }
    };
    let auth = AuthService::new(Arc::clone(&store), AuthService::generate_secret())
        .await
        .ok()?;

    let admin_tokens = auth
        .signup(SignupRequest {
            email: "admin@stackhouse.dev".to_string(),
            password: "password123".to_string(),
            metadata: Some(serde_json::json!({"service_admin": true})),
        })
        .await
        .ok()?;

    let user_tokens = auth
        .signup(SignupRequest {
            email: "user@stackhouse.dev".to_string(),
            password: "password123".to_string(),
            metadata: Some(serde_json::json!({"service_admin": false})),
        })
        .await
        .ok()?;

    let auth_state = AuthState { auth: auth.clone() };
    let backup_service = BackupService::new(
        Arc::clone(&store),
        std::env::temp_dir().join("stackhouse-backups"),
        "postgres://postgres:postgres@localhost:5432/stackhouse_test".to_string(),
    )
    .await
    .ok()?;
    let admin_audit = Arc::new(AdminAuditService::new(Arc::clone(&store)).await.ok()?);
    let pitr_service = Arc::new(PitrService::new(Arc::clone(&store)).await.ok()?);
    let app = Router::new().nest(
        "/v1/admin",
        create_backup_router(BackupState {
            backup: Arc::new(backup_service),
            pitr: pitr_service,
            auth: auth_state,
            authorization: backup_authorization,
            admin_audit,
        }),
    );

    Some((
        app,
        store,
        admin_tokens.access_token,
        user_tokens.access_token,
    ))
}

async fn build_network_router_with_authorization(
    network_authorization: AuthorizationService,
) -> (Router, Arc<StackhouseStore>, String, String) {
    let (store, auth_state, _, admin_token, user_token) = build_admin_context().await;
    let network_service = Arc::new(NetworkService::new());
    let admin_audit = Arc::new(AdminAuditService::new(Arc::clone(&store)).await.unwrap());
    let app = Router::new().nest(
        "/v1/admin",
        create_network_router(NetworkState {
            network: network_service,
            auth: auth_state,
            authorization: network_authorization,
            admin_audit,
        }),
    );

    (app, store, admin_token, user_token)
}

async fn response_json(response: axum::response::Response) -> Value {
    let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    serde_json::from_slice(&body).unwrap()
}

fn bearer(token: &str) -> String {
    format!("Bearer {}", token)
}

#[tokio::test]
async fn admin_logs_require_authentication() {
    let (app, _, _) = build_log_router().await;

    let response = app
        .oneshot(
            Request::builder()
                .uri("/v1/admin/logs")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);

    let body = response_json(response).await;
    assert_eq!(body["error"]["code"], "UNAUTHORIZED");
}

#[tokio::test]
async fn admin_logs_reject_non_admin_authenticated_users() {
    let (app, _, user_token) = build_log_router().await;

    let response = app
        .oneshot(
            Request::builder()
                .uri("/v1/admin/logs")
                .header("authorization", format!("Bearer {}", user_token))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::FORBIDDEN);

    let body = response_json(response).await;
    assert_eq!(body["error"]["code"], "FORBIDDEN");
}

#[tokio::test]
async fn admin_logs_allow_service_admins() {
    let (app, admin_token, _) = build_log_router().await;

    let response = app
        .oneshot(
            Request::builder()
                .uri("/v1/admin/logs")
                .header("authorization", format!("Bearer {}", admin_token))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);

    let body = response_json(response).await;
    assert_eq!(body["success"], true);
}

#[tokio::test]
async fn admin_logs_stay_service_admin_only_when_admin_log_config_is_disabled() {
    let (app, _, admin_token, user_token) =
        build_log_router_with_authorization(AuthorizationService::new(SecurityConfig::new(false)))
            .await;

    let non_admin = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/v1/admin/logs")
                .header("authorization", bearer(&user_token))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(non_admin.status(), StatusCode::FORBIDDEN);

    let admin = app
        .oneshot(
            Request::builder()
                .uri("/v1/admin/logs")
                .header("authorization", bearer(&admin_token))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(admin.status(), StatusCode::OK);
}

#[tokio::test]
async fn admin_logs_write_admin_audit_entries() {
    let (app, store, admin_token, _) =
        build_log_router_with_authorization(AuthorizationService::new(SecurityConfig::default()))
            .await;

    let response = app
        .oneshot(
            Request::builder()
                .uri("/v1/admin/logs")
                .header("authorization", bearer(&admin_token))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    let rows = store
        .query(
            "SELECT action, outcome FROM stackhouse_admin_audit_logs WHERE action = 'log_drain.query_logs' ORDER BY occurred_at DESC"
                .to_string(),
            vec![],
        )
        .await
        .unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(
        rows[0]
            .iter()
            .find(|(k, _)| k == "outcome")
            .and_then(|(_, v)| v.as_str()),
        Some("success")
    );
}

#[tokio::test]
async fn branching_routes_require_service_admin() {
    let (app, _, admin_token, user_token) = build_branching_router().await;

    let unauth = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/v1/admin/branches")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(unauth.status(), StatusCode::UNAUTHORIZED);

    let non_admin = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/v1/admin/branches")
                .header("authorization", bearer(&user_token))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(non_admin.status(), StatusCode::FORBIDDEN);

    let admin = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/v1/admin/branches")
                .header("authorization", bearer(&admin_token))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(admin.status(), StatusCode::OK);
}

#[tokio::test]
async fn branching_routes_require_service_admin_even_when_admin_log_config_is_disabled() {
    let (app, _, admin_token, user_token) = build_branching_router_with_authorization(
        AuthorizationService::new(SecurityConfig::new(false)),
    )
    .await;

    let non_admin = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/v1/admin/branches")
                .header("authorization", bearer(&user_token))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(non_admin.status(), StatusCode::FORBIDDEN);

    let admin = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/v1/admin/branches")
                .header("authorization", bearer(&admin_token))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(admin.status(), StatusCode::OK);

    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/v1/admin/branches")
                .header("authorization", bearer(&user_token))
                .header("content-type", "application/json")
                .body(Body::from(r#"{"name":"branch-from-disabled-config"}"#))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn branching_create_route_denies_before_parsing_body() {
    let (app, _, admin_token, user_token) = build_branching_router().await;

    let unauth = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/v1/admin/branches")
                .header("content-type", "application/json")
                .body(Body::from("not-json"))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(unauth.status(), StatusCode::UNAUTHORIZED);

    let non_admin = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/v1/admin/branches")
                .header("authorization", bearer(&user_token))
                .header("content-type", "application/json")
                .body(Body::from("not-json"))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(non_admin.status(), StatusCode::FORBIDDEN);

    let admin_bad_body = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/v1/admin/branches")
                .header("authorization", bearer(&admin_token))
                .header("content-type", "application/json")
                .body(Body::from("not-json"))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(admin_bad_body.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn branching_routes_write_admin_audit_entries() {
    let (app, store, admin_token, _) = build_branching_router().await;

    let response = app
        .oneshot(
            Request::builder()
                .uri("/v1/admin/branches")
                .header("authorization", bearer(&admin_token))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);

    let rows = store
        .query(
            "SELECT actor_user_id, action, resource_type, outcome FROM stackhouse_admin_audit_logs WHERE action = 'branching.list' ORDER BY occurred_at DESC".to_string(),
            vec![],
        )
        .await
        .unwrap();

    assert_eq!(rows.len(), 1);
    assert_eq!(
        rows[0]
            .iter()
            .find(|(k, _)| k == "resource_type")
            .and_then(|(_, v)| v.as_str()),
        Some("branch")
    );
    assert_eq!(
        rows[0]
            .iter()
            .find(|(k, _)| k == "outcome")
            .and_then(|(_, v)| v.as_str()),
        Some("success")
    );
}

#[tokio::test]
async fn admin_log_drains_require_authentication() {
    let (app, _, _) = build_log_router().await;

    let response = app
        .oneshot(
            Request::builder()
                .uri("/v1/admin/logs/drains")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);

    let body = response_json(response).await;
    assert_eq!(body["error"]["code"], "UNAUTHORIZED");
}

#[tokio::test]
async fn admin_log_drains_reject_non_admin_authenticated_users() {
    let (app, _, user_token) = build_log_router().await;

    let response = app
        .oneshot(
            Request::builder()
                .uri("/v1/admin/logs/drains")
                .header("authorization", format!("Bearer {}", user_token))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::FORBIDDEN);

    let body = response_json(response).await;
    assert_eq!(body["error"]["code"], "FORBIDDEN");
}

#[tokio::test]
async fn admin_log_drains_allow_service_admins() {
    let (app, admin_token, _) = build_log_router().await;

    let response = app
        .oneshot(
            Request::builder()
                .uri("/v1/admin/logs/drains")
                .header("authorization", format!("Bearer {}", admin_token))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);

    let body = response_json(response).await;
    assert_eq!(body["success"], true);
}

#[tokio::test]
async fn raw_sql_endpoints_are_disabled_by_default() {
    let store = Arc::new(StackhouseStore::in_memory().await.unwrap());
    let app = create_router(AppState::new(store));

    for (method, uri, body) in [
        ("POST", "/v1/sql/query", r#"{"query":"SELECT 1"}"#),
        ("POST", "/v1/sql/execute", r#"{"query":"SELECT 1"}"#),
    ] {
        let response = app
            .clone()
            .oneshot(
                Request::builder()
                    .method(method)
                    .uri(uri)
                    .header("content-type", "application/json")
                    .body(Body::from(body))
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::FORBIDDEN);
    }
}

#[tokio::test]
async fn admin_audit_service_persists_entries() {
    let store = Arc::new(StackhouseStore::in_memory().await.unwrap());
    let audit = AdminAuditService::new(Arc::clone(&store)).await.unwrap();

    audit
        .record(
            42,
            "branching.list",
            "branch",
            None,
            "success",
            serde_json::json!({"route": "/v1/admin/branches"}),
        )
        .await
        .unwrap();

    let rows = store
        .query(
            "SELECT actor_user_id, action, resource_type, outcome FROM stackhouse_admin_audit_logs"
                .to_string(),
            vec![],
        )
        .await
        .unwrap();

    assert_eq!(rows.len(), 1);
    assert_eq!(
        rows[0]
            .iter()
            .find(|(k, _)| k == "action")
            .and_then(|(_, v)| v.as_str()),
        Some("branching.list")
    );
}

#[tokio::test]
async fn extensions_routes_require_service_admin() {
    let (app, _, admin_token, user_token) = build_extensions_router().await;

    let unauth = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/v1/admin/extensions")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(unauth.status(), StatusCode::UNAUTHORIZED);

    let non_admin = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/v1/admin/extensions")
                .header("authorization", bearer(&user_token))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(non_admin.status(), StatusCode::FORBIDDEN);

    let admin = app
        .oneshot(
            Request::builder()
                .uri("/v1/admin/extensions")
                .header("authorization", bearer(&admin_token))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(admin.status(), StatusCode::OK);
}

#[tokio::test]
async fn extensions_post_routes_require_service_admin_before_json_parsing() {
    let (app, _, admin_token, user_token) = build_extensions_router().await;

    let unauthenticated = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/v1/admin/extensions/install")
                .header("content-type", "application/json")
                .body(Body::from("{not valid json"))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(unauthenticated.status(), StatusCode::UNAUTHORIZED);

    let non_admin = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/v1/admin/extensions/install")
                .header("authorization", bearer(&user_token))
                .header("content-type", "application/json")
                .body(Body::from("{not valid json"))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(non_admin.status(), StatusCode::FORBIDDEN);

    let admin = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/v1/admin/extensions/install")
                .header("authorization", bearer(&admin_token))
                .header("content-type", "application/json")
                .body(Body::from("{not valid json"))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(admin.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn extensions_routes_write_admin_audit_entries() {
    let (app, store, admin_token, _) = build_extensions_router().await;

    let response = app
        .oneshot(
            Request::builder()
                .uri("/v1/admin/extensions")
                .header("authorization", bearer(&admin_token))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    let rows = store
        .query(
            "SELECT action, resource_type, outcome FROM stackhouse_admin_audit_logs ORDER BY occurred_at DESC LIMIT 1"
                .to_string(),
            vec![],
        )
        .await
        .unwrap();
    assert_eq!(
        rows[0]
            .iter()
            .find(|(k, _)| k == "action")
            .and_then(|(_, v)| v.as_str()),
        Some("extensions.list_installed")
    );
}

#[tokio::test]
async fn preflight_from_unknown_origin_is_rejected() {
    let port = free_port();
    let base_url = format!("http://127.0.0.1:{}", port);
    let _server = spawn_stackhouse_server(port, "http://allowed.example").await;
    wait_for_health(&base_url).await;

    let client = reqwest::Client::new();

    let allowed = client
        .request(reqwest::Method::OPTIONS, format!("{}/health", base_url))
        .header(reqwest::header::ORIGIN, "http://allowed.example")
        .header(reqwest::header::ACCESS_CONTROL_REQUEST_METHOD, "GET")
        .send()
        .await
        .unwrap();
    assert!(allowed.status().is_success());
    assert_eq!(
        allowed
            .headers()
            .get(reqwest::header::ACCESS_CONTROL_ALLOW_ORIGIN)
            .and_then(|value| value.to_str().ok()),
        Some("http://allowed.example")
    );

    let rejected = client
        .request(reqwest::Method::OPTIONS, format!("{}/health", base_url))
        .header(reqwest::header::ORIGIN, "http://evil.example")
        .header(reqwest::header::ACCESS_CONTROL_REQUEST_METHOD, "GET")
        .send()
        .await
        .unwrap();
    assert!(rejected.status().is_success());
    assert!(rejected
        .headers()
        .get(reqwest::header::ACCESS_CONTROL_ALLOW_ORIGIN)
        .is_none());
}

#[tokio::test]
async fn table_drop_is_disabled_by_default() {
    let store = Arc::new(StackhouseStore::in_memory().await.unwrap());
    let app = create_router(AppState::new(store));

    let response = app
        .oneshot(
            Request::builder()
                .method("DELETE")
                .uri("/v1/tables/users")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn raw_sql_query_requires_service_admin_when_enabled() {
    let (app, admin_token, user_token) = build_security_enabled_app().await;

    let unauth = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/v1/sql/query")
                .header("content-type", "application/json")
                .body(Body::from(r#"{"query":"SELECT 1 AS value"}"#))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(unauth.status(), StatusCode::UNAUTHORIZED);

    let non_admin = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/v1/sql/query")
                .header("authorization", bearer(&user_token))
                .header("content-type", "application/json")
                .body(Body::from(r#"{"query":"SELECT 1 AS value"}"#))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(non_admin.status(), StatusCode::FORBIDDEN);

    let admin = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/v1/sql/query")
                .header("authorization", bearer(&admin_token))
                .header("content-type", "application/json")
                .body(Body::from(r#"{"query":"SELECT 1 AS value"}"#))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(admin.status(), StatusCode::OK);

    let body = response_json(admin).await;
    assert_eq!(body["success"], true);
}

#[tokio::test]
async fn raw_sql_execute_requires_service_admin_when_enabled() {
    let (app, admin_token, user_token) = build_security_enabled_app().await;

    let unauth = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/v1/sql/execute")
                .header("content-type", "application/json")
                .body(Body::from(
                    r#"{"query":"CREATE TABLE enabled_sql_execute_test (id INTEGER)"}"#,
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(unauth.status(), StatusCode::UNAUTHORIZED);

    let non_admin = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/v1/sql/execute")
                .header("authorization", bearer(&user_token))
                .header("content-type", "application/json")
                .body(Body::from(
                    r#"{"query":"CREATE TABLE enabled_sql_execute_test (id INTEGER)"}"#,
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(non_admin.status(), StatusCode::FORBIDDEN);

    let admin = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/v1/sql/execute")
                .header("authorization", bearer(&admin_token))
                .header("content-type", "application/json")
                .body(Body::from(
                    r#"{"query":"CREATE TABLE enabled_sql_execute_test (id INTEGER)"}"#,
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(admin.status(), StatusCode::OK);

    let body = response_json(admin).await;
    assert_eq!(body["success"], true);
}

#[tokio::test]
async fn raw_sql_routes_stay_service_admin_only_when_admin_log_config_is_disabled() {
    let (app, _, admin_token, user_token) = build_security_enabled_app_with_authorization(
        AuthorizationService::new(SecurityConfig::new(false)),
    )
    .await;

    let non_admin = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/v1/sql/query")
                .header("authorization", bearer(&user_token))
                .header("content-type", "application/json")
                .body(Body::from(r#"{"query":"SELECT 1"}"#))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(non_admin.status(), StatusCode::FORBIDDEN);

    let admin = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/v1/sql/query")
                .header("authorization", bearer(&admin_token))
                .header("content-type", "application/json")
                .body(Body::from(r#"{"query":"SELECT 1"}"#))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(admin.status(), StatusCode::OK);
}

#[tokio::test]
async fn raw_sql_query_denies_before_json_parsing() {
    let (app, _, admin_token, user_token) = build_security_enabled_app_with_authorization(
        AuthorizationService::new(SecurityConfig::default()),
    )
    .await;

    let unauth = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/v1/sql/query")
                .header("content-type", "application/json")
                .body(Body::from("not-json"))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(unauth.status(), StatusCode::UNAUTHORIZED);

    let non_admin = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/v1/sql/query")
                .header("authorization", bearer(&user_token))
                .header("content-type", "application/json")
                .body(Body::from("not-json"))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(non_admin.status(), StatusCode::FORBIDDEN);

    let admin = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/v1/sql/query")
                .header("authorization", bearer(&admin_token))
                .header("content-type", "application/json")
                .body(Body::from("not-json"))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(admin.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn raw_sql_query_writes_admin_audit_entries() {
    let (app, store, admin_token, _) = build_security_enabled_app_with_authorization(
        AuthorizationService::new(SecurityConfig::default()),
    )
    .await;

    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/v1/sql/query")
                .header("authorization", bearer(&admin_token))
                .header("content-type", "application/json")
                .body(Body::from(r#"{"query":"SELECT 1 AS value"}"#))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    let rows = store
        .query(
            "SELECT action, outcome FROM stackhouse_admin_audit_logs WHERE action = 'api.sql_query' ORDER BY occurred_at DESC"
                .to_string(),
            vec![],
        )
        .await
        .unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(
        rows[0]
            .iter()
            .find(|(k, _)| k == "outcome")
            .and_then(|(_, v)| v.as_str()),
        Some("success")
    );
}

#[tokio::test]
async fn signup_metadata_cannot_self_assign_service_admin() {
    let (app, _, _) = build_security_enabled_app().await;

    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/v1/auth/signup")
                .header("content-type", "application/json")
                .body(Body::from(
                    r#"{"email":"signup-metadata@stackhouse.dev","password":"password123","metadata":{"display_name":"Signup User","service_admin":true}}"#,
                ))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::CREATED);

    let body = response_json(response).await;
    assert_eq!(
        body["data"]["user"]["metadata"]["display_name"],
        "Signup User"
    );
    assert!(body["data"]["user"]["metadata"]["service_admin"].is_null());

    let admin_attempt = app
        .oneshot(
            Request::builder()
                .uri("/v1/admin/logs")
                .header(
                    "authorization",
                    bearer(
                        body["data"]["access_token"]
                            .as_str()
                            .expect("signup should return an access token"),
                    ),
                )
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(admin_attempt.status(), StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn update_user_metadata_cannot_self_assign_service_admin() {
    let (app, _, _) = build_security_enabled_app().await;

    let signup = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/v1/auth/signup")
                .header("content-type", "application/json")
                .body(Body::from(
                    r#"{"email":"update-metadata@stackhouse.dev","password":"password123","metadata":{"display_name":"Initial Name","theme":"light"}}"#,
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(signup.status(), StatusCode::CREATED);

    let signup_body = response_json(signup).await;
    let access_token = signup_body["data"]["access_token"]
        .as_str()
        .expect("signup should return an access token")
        .to_string();

    let update = app
        .clone()
        .oneshot(
            Request::builder()
                .method("PUT")
                .uri("/v1/auth/user")
                .header("authorization", bearer(&access_token))
                .header("content-type", "application/json")
                .body(Body::from(
                    r#"{"metadata":{"display_name":"Updated Name","theme":"dark","service_admin":true}}"#,
                ))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(update.status(), StatusCode::OK);

    let body = response_json(update).await;
    assert_eq!(body["data"]["metadata"]["display_name"], "Updated Name");
    assert_eq!(body["data"]["metadata"]["theme"], "dark");
    assert!(body["data"]["metadata"]["service_admin"].is_null());

    let admin_attempt = app
        .oneshot(
            Request::builder()
                .uri("/v1/admin/logs")
                .header("authorization", bearer(&access_token))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(admin_attempt.status(), StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn destructive_table_drop_requires_service_admin_when_enabled() {
    let (app, admin_token, user_token) = build_security_enabled_app().await;

    let unauth = app
        .clone()
        .oneshot(
            Request::builder()
                .method("DELETE")
                .uri("/v1/tables/enabled_drop_target")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(unauth.status(), StatusCode::UNAUTHORIZED);

    let non_admin = app
        .clone()
        .oneshot(
            Request::builder()
                .method("DELETE")
                .uri("/v1/tables/enabled_drop_target")
                .header("authorization", bearer(&user_token))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(non_admin.status(), StatusCode::FORBIDDEN);

    let create_table = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/v1/sql/execute")
                .header("authorization", bearer(&admin_token))
                .header("content-type", "application/json")
                .body(Body::from(
                    r#"{"query":"CREATE TABLE enabled_drop_target (id INTEGER)"}"#,
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(create_table.status(), StatusCode::OK);

    let admin = app
        .oneshot(
            Request::builder()
                .method("DELETE")
                .uri("/v1/tables/enabled_drop_target")
                .header("authorization", bearer(&admin_token))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(admin.status(), StatusCode::OK);

    let body = response_json(admin).await;
    assert_eq!(body["success"], true);
}

#[tokio::test]
async fn backup_route_requires_authentication_and_service_admin() {
    let Some((app, _, admin_token, user_token)) = build_backup_router_with_authorization(
        AuthorizationService::new(SecurityConfig::default()),
    )
    .await
    else {
        return;
    };

    let unauth = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/v1/admin/backups")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(unauth.status(), StatusCode::UNAUTHORIZED);

    let non_admin = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/v1/admin/backups")
                .header("authorization", bearer(&user_token))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(non_admin.status(), StatusCode::FORBIDDEN);

    let admin = app
        .oneshot(
            Request::builder()
                .uri("/v1/admin/backups")
                .header("authorization", bearer(&admin_token))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(admin.status(), StatusCode::OK);

    let body = response_json(admin).await;
    assert_eq!(body["success"], true);
}

#[tokio::test]
async fn backup_routes_stay_service_admin_only_when_admin_log_config_is_disabled() {
    let Some((app, _, admin_token, user_token)) = build_backup_router_with_authorization(
        AuthorizationService::new(SecurityConfig::new(false)),
    )
    .await
    else {
        return;
    };

    let non_admin = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/v1/admin/backups")
                .header("authorization", bearer(&user_token))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(non_admin.status(), StatusCode::FORBIDDEN);

    let admin = app
        .oneshot(
            Request::builder()
                .uri("/v1/admin/backups")
                .header("authorization", bearer(&admin_token))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(admin.status(), StatusCode::OK);
}

#[tokio::test]
async fn backup_create_route_denies_before_json_parsing() {
    let Some((app, _, admin_token, user_token)) = build_backup_router_with_authorization(
        AuthorizationService::new(SecurityConfig::default()),
    )
    .await
    else {
        return;
    };

    let unauth = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/v1/admin/backups")
                .header("content-type", "application/json")
                .body(Body::from("not-json"))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(unauth.status(), StatusCode::UNAUTHORIZED);

    let non_admin = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/v1/admin/backups")
                .header("authorization", bearer(&user_token))
                .header("content-type", "application/json")
                .body(Body::from("not-json"))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(non_admin.status(), StatusCode::FORBIDDEN);

    let admin = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/v1/admin/backups")
                .header("authorization", bearer(&admin_token))
                .header("content-type", "application/json")
                .body(Body::from("not-json"))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(admin.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn backup_routes_write_admin_audit_entries() {
    let Some((app, store, admin_token, _)) = build_backup_router_with_authorization(
        AuthorizationService::new(SecurityConfig::default()),
    )
    .await
    else {
        return;
    };

    let response = app
        .oneshot(
            Request::builder()
                .uri("/v1/admin/backups")
                .header("authorization", bearer(&admin_token))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    let rows = store
        .query(
            "SELECT action, outcome FROM stackhouse_admin_audit_logs WHERE action = 'backup.list' ORDER BY occurred_at DESC"
                .to_string(),
            vec![],
        )
        .await
        .unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(
        rows[0]
            .iter()
            .find(|(k, _)| k == "outcome")
            .and_then(|(_, v)| v.as_str()),
        Some("success")
    );
}

#[tokio::test]
async fn mfa_secret_is_encrypted_at_rest_and_still_verifies() {
    install_test_data_encryption_key();
    let (store, auth, _, _, user_id, _) = build_auth_context().await;
    let mfa = MfaService::new(Arc::clone(&store), auth, "Stackhouse".to_string(), false)
        .await
        .unwrap();

    let enrollment = mfa.enroll(user_id, "user@stackhouse.dev").await.unwrap();
    let rows = store
        .query(
            "SELECT totp_secret FROM stackhouse_mfa WHERE user_id = $1".to_string(),
            vec![stackhouse::db::SqlValue::Integer(user_id)],
        )
        .await
        .unwrap();

    let stored_secret = rows[0]
        .iter()
        .find(|(k, _)| k == "totp_secret")
        .and_then(|(_, v)| v.as_str())
        .unwrap()
        .to_string();
    assert_ne!(stored_secret, enrollment.secret);
    assert!(!stored_secret.contains(&enrollment.secret));
    assert!(stored_secret.starts_with("enc:v1:"));

    let secret_bytes = totp_rs::Secret::Encoded(enrollment.secret.clone())
        .to_bytes()
        .unwrap();
    let totp = totp_rs::TOTP::new(
        totp_rs::Algorithm::SHA1,
        6,
        1,
        30,
        secret_bytes,
        Some("Stackhouse".to_string()),
        "user@stackhouse.dev".to_string(),
    )
    .unwrap();
    let code = totp.generate_current().unwrap();

    mfa.verify_enrollment(user_id, &code).await.unwrap();
    assert!(mfa.verify_code(user_id, &code).await.unwrap());
}

#[tokio::test]
async fn backup_artifact_is_encrypted_and_restores_successfully() {
    install_test_data_encryption_key();
    let store = Arc::new(StackhouseStore::in_memory().await.unwrap());
    store
        .execute_batch(
            r#"
            CREATE TABLE backup_encryption_items (
                name TEXT NOT NULL
            );
            INSERT INTO backup_encryption_items (name) VALUES ('alpha');
            "#
            .to_string(),
        )
        .await
        .unwrap();

    let backup_service = BackupService::new(
        Arc::clone(&store),
        std::env::temp_dir().join("stackhouse-backups-encryption-test"),
        "postgres://postgres:postgres@localhost:5432/stackhouse_test".to_string(),
    )
    .await
    .unwrap();

    let backup = backup_service
        .create_backup("encrypted-backup-test")
        .await
        .unwrap();
    let rows = store
        .query(
            "SELECT file_path FROM stackhouse_backups WHERE id = $1".to_string(),
            vec![stackhouse::db::SqlValue::Text(backup.id.clone())],
        )
        .await
        .unwrap();
    let file_path = rows[0]
        .iter()
        .find(|(k, _)| k == "file_path")
        .and_then(|(_, v)| v.as_str())
        .unwrap();

    let file_contents = std::fs::read_to_string(file_path).unwrap();
    assert!(file_contents.starts_with("enc:v1:"));
    assert!(!file_contents.contains("CREATE TABLE backup_encryption_items"));
    assert!(!file_contents.contains("INSERT INTO backup_encryption_items"));

    let smoke_sql = b"BEGIN; CREATE TABLE restore_encrypted_artifact (name TEXT NOT NULL); INSERT INTO restore_encrypted_artifact (name) VALUES ('alpha'); COMMIT;";
    let encrypted_smoke_sql = stackhouse::authorization::data_protector()
        .unwrap()
        .encrypt_bytes(smoke_sql)
        .unwrap();
    std::fs::write(file_path, &encrypted_smoke_sql).unwrap();

    backup_service.restore_backup(&backup.id).await.unwrap();

    let restored = store
        .query(
            "SELECT name FROM restore_encrypted_artifact".to_string(),
            vec![],
        )
        .await
        .unwrap();
    assert_eq!(restored.len(), 1);
    assert_eq!(
        restored[0]
            .iter()
            .find(|(k, _)| k == "name")
            .and_then(|(_, v)| v.as_str()),
        Some("alpha")
    );
}

#[tokio::test]
async fn pitr_restore_route_requires_service_admin_and_reaches_handler() {
    let Some((app, _, admin_token, user_token)) = build_backup_router_with_authorization(
        AuthorizationService::new(SecurityConfig::default()),
    )
    .await
    else {
        return;
    };

    let unauth = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/v1/admin/backups/pitr/restore")
                .method("POST")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(unauth.status(), StatusCode::UNAUTHORIZED);

    let non_admin = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/v1/admin/backups/pitr/restore")
                .method("POST")
                .header("authorization", bearer(&user_token))
                .header("content-type", "application/json")
                .body(Body::from(r#"{"target_time":"2026-08-18T00:00:00Z"}"#))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(non_admin.status(), StatusCode::FORBIDDEN);

    let admin = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/v1/admin/backups/pitr/restore")
                .method("POST")
                .header("authorization", bearer(&admin_token))
                .header("content-type", "application/json")
                .body(Body::from(r#"{"target_time":"2026-08-18T00:00:00Z"}"#))
                .unwrap(),
        )
        .await
        .unwrap();
    // The request reaches the handler and is gated correctly; without a base backup
    // and WAL slot the actual PITR restore cannot succeed, so it returns NotFound.
    assert_eq!(admin.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn network_route_requires_authentication_and_service_admin() {
    let (app, _, admin_token, user_token) = build_network_router_with_authorization(
        AuthorizationService::new(SecurityConfig::default()),
    )
    .await;

    let unauth = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/v1/admin/network/rules")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(unauth.status(), StatusCode::UNAUTHORIZED);

    let non_admin = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/v1/admin/network/rules")
                .header("authorization", bearer(&user_token))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(non_admin.status(), StatusCode::FORBIDDEN);

    let admin = app
        .oneshot(
            Request::builder()
                .uri("/v1/admin/network/rules")
                .header("authorization", bearer(&admin_token))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(admin.status(), StatusCode::OK);
}

#[tokio::test]
async fn network_routes_stay_service_admin_only_when_admin_log_config_is_disabled() {
    let (app, _, admin_token, user_token) = build_network_router_with_authorization(
        AuthorizationService::new(SecurityConfig::new(false)),
    )
    .await;

    let non_admin = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/v1/admin/network/rules")
                .header("authorization", bearer(&user_token))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(non_admin.status(), StatusCode::FORBIDDEN);

    let admin = app
        .oneshot(
            Request::builder()
                .uri("/v1/admin/network/rules")
                .header("authorization", bearer(&admin_token))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(admin.status(), StatusCode::OK);
}

#[tokio::test]
async fn network_add_rule_denies_before_json_parsing() {
    let (app, _, admin_token, user_token) = build_network_router_with_authorization(
        AuthorizationService::new(SecurityConfig::default()),
    )
    .await;

    let unauth = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/v1/admin/network/rules")
                .header("content-type", "application/json")
                .body(Body::from("not-json"))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(unauth.status(), StatusCode::UNAUTHORIZED);

    let non_admin = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/v1/admin/network/rules")
                .header("authorization", bearer(&user_token))
                .header("content-type", "application/json")
                .body(Body::from("not-json"))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(non_admin.status(), StatusCode::FORBIDDEN);

    let admin = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/v1/admin/network/rules")
                .header("authorization", bearer(&admin_token))
                .header("content-type", "application/json")
                .body(Body::from("not-json"))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(admin.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn network_routes_write_admin_audit_entries() {
    let (app, store, admin_token, _) = build_network_router_with_authorization(
        AuthorizationService::new(SecurityConfig::default()),
    )
    .await;

    let response = app
        .oneshot(
            Request::builder()
                .uri("/v1/admin/network/rules")
                .header("authorization", bearer(&admin_token))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    let rows = store
        .query(
            "SELECT action, outcome FROM stackhouse_admin_audit_logs WHERE action = 'network.list_rules' ORDER BY occurred_at DESC"
                .to_string(),
            vec![],
        )
        .await
        .unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(
        rows[0]
            .iter()
            .find(|(k, _)| k == "outcome")
            .and_then(|(_, v)| v.as_str()),
        Some("success")
    );
}

#[tokio::test]
async fn network_allowlist_uses_connection_info_not_forwarded_headers() {
    let network_service = Arc::new(NetworkService::new());
    network_service.enable().await;
    network_service
        .add_rule(NetworkRule {
            ip: "198.51.100.23".to_string(),
            description: "allowed client".to_string(),
            enabled: true,
        })
        .await;

    let app = Router::new()
        .route("/", get(|| async { StatusCode::OK }))
        .layer(from_fn_with_state(
            Arc::clone(&network_service),
            network_middleware,
        ));

    let mut request = Request::builder()
        .uri("/")
        .header("x-forwarded-for", "198.51.100.23")
        .body(Body::empty())
        .unwrap();
    request
        .extensions_mut()
        .insert(ConnectInfo(SocketAddr::from(([203, 0, 113, 7], 44321))));

    let response = app.oneshot(request).await.unwrap();
    assert_eq!(response.status(), StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn network_allowlist_denies_when_enabled_but_empty() {
    let network_service = Arc::new(NetworkService::new());
    network_service.enable().await;

    let app = Router::new()
        .route("/", get(|| async { StatusCode::OK }))
        .layer(from_fn_with_state(
            Arc::clone(&network_service),
            network_middleware,
        ));

    let mut request = Request::builder().uri("/").body(Body::empty()).unwrap();
    request
        .extensions_mut()
        .insert(ConnectInfo(SocketAddr::from(([203, 0, 113, 7], 44321))));

    let response = app.oneshot(request).await.unwrap();
    assert_eq!(response.status(), StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn team_members_route_rejects_non_members() {
    let (store, auth, _admin_token, user_token, admin_user_id, _) = build_auth_context().await;
    let teams = Arc::new(TeamsService::new(Arc::clone(&store)).await.unwrap());
    let team_id = teams.create_team("Acme", admin_user_id).await.unwrap();
    let app = Router::new().nest(
        "/v1",
        create_teams_router(TeamsState {
            teams,
            auth: AuthState { auth },
        }),
    );

    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .uri(format!("/v1/teams/{}/members", team_id))
                .header("authorization", bearer(&user_token))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn team_members_route_allows_members() {
    let (store, auth, admin_token, _, admin_user_id, _) = build_auth_context().await;
    let teams = Arc::new(TeamsService::new(Arc::clone(&store)).await.unwrap());
    let team_id = teams.create_team("Acme", admin_user_id).await.unwrap();
    let app = Router::new().nest(
        "/v1",
        create_teams_router(TeamsState {
            teams,
            auth: AuthState { auth },
        }),
    );

    let response = app
        .oneshot(
            Request::builder()
                .uri(format!("/v1/teams/{}/members", team_id))
                .header("authorization", bearer(&admin_token))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
}

#[tokio::test]
async fn team_invites_require_team_scope() {
    let (store, auth, admin_token, user_token, admin_user_id, _) = build_auth_context().await;
    let teams = Arc::new(TeamsService::new(Arc::clone(&store)).await.unwrap());
    let team_id = teams.create_team("Acme", admin_user_id).await.unwrap();
    let app = Router::new().nest(
        "/v1",
        create_teams_router(TeamsState {
            teams,
            auth: AuthState { auth },
        }),
    );

    let non_member = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/v1/teams/invite")
                .header("authorization", bearer(&user_token))
                .header("content-type", "application/json")
                .body(Body::from(format!(
                    r#"{{"team_id":{},"email":"new@stackhouse.dev"}}"#,
                    team_id
                )))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(non_member.status(), StatusCode::FORBIDDEN);

    let owner = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/v1/teams/invite")
                .header("authorization", bearer(&admin_token))
                .header("content-type", "application/json")
                .body(Body::from(format!(
                    r#"{{"team_id":{},"email":"new@stackhouse.dev"}}"#,
                    team_id
                )))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(owner.status(), StatusCode::OK);
}

#[test]
fn forbidden_error_maps_to_403_and_forbidden_code() {
    let error = StackhouseError::Forbidden("Service admin access required".to_string());

    assert_eq!(error.status_code(), StatusCode::FORBIDDEN);
    assert_eq!(error.error_code(), "FORBIDDEN");
}
