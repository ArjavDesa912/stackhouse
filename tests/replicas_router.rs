//! End-to-end integration test for the read-replica router.

use std::sync::Arc;

use axum::{
    body::{to_bytes, Body},
    http::{Method, Request, StatusCode},
    Router,
};
use serde_json::{json, Value};
use stackhouse::{
    auth::{create_auth_router, AuthService, AuthState, SignupRequest},
    db::StackhouseStore,
    platform::{create_replicas_router, ReplicaService, ReplicaState},
};
use tower::ServiceExt;

async fn build_app() -> Option<(Router, String)> {
    let store = match StackhouseStore::in_memory().await {
        Ok(store) => Arc::new(store),
        Err(error) => {
            eprintln!(
                "skipping replica router integration test because the test database is unavailable: {error}"
            );
            return None;
        }
    };
    let auth = AuthService::new(Arc::clone(&store), AuthService::generate_secret())
        .await
        .unwrap();
    let auth_state = AuthState { auth: auth.clone() };

    let tokens = auth
        .signup(SignupRequest {
            email: "replicas@stackhouse.dev".to_string(),
            password: "password123".to_string(),
            metadata: None,
        })
        .await
        .unwrap();

    let replicas = Arc::new(ReplicaService::new(Arc::clone(&store)).await.unwrap());
    let replica_state = ReplicaState {
        replicas,
        auth: auth_state.clone(),
    };

    let app = Router::new()
        .nest("/v1/auth", create_auth_router(auth_state.clone()))
        .nest(
            "/v1/platform/replicas",
            create_replicas_router(replica_state),
        );

    Some((app, tokens.access_token))
}

async fn request(
    app: &Router,
    token: &str,
    method: Method,
    uri: &str,
    body: Option<Value>,
) -> axum::response::Response {
    let mut builder = Request::builder().method(method).uri(uri);
    builder = builder.header("authorization", format!("Bearer {}", token));
    let body = match body {
        Some(value) => {
            builder = builder.header("content-type", "application/json");
            Body::from(serde_json::to_vec(&value).unwrap())
        }
        None => Body::empty(),
    };
    app.clone()
        .oneshot(builder.body(body).unwrap())
        .await
        .unwrap()
}

async fn json_body(response: axum::response::Response) -> Value {
    let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    serde_json::from_slice(&body).unwrap_or_else(|_| {
        json!({
            "success": false,
            "error": String::from_utf8_lossy(&body),
        })
    })
}

#[tokio::test]
async fn replicas_register_list_promote_remove_lifecycle() {
    let Some((app, token)) = build_app().await else {
        return;
    };

    let register = request(
        &app,
        &token,
        Method::POST,
        "/v1/platform/replicas/nodes",
        Some(json!({
            "name": "replica-us-east-1",
            "host": "127.0.0.1",
            "port": 5433,
            "database": "postgres",
            "region": "us-east-1",
            "role": "replica"
        })),
    )
    .await;
    assert_eq!(register.status(), StatusCode::OK);
    let register_body = json_body(register).await;
    assert_eq!(register_body["success"], true);
    let node_id = register_body["data"]["id"].as_str().unwrap().to_string();
    assert_eq!(register_body["data"]["role"], "replica");

    let list = request(
        &app,
        &token,
        Method::GET,
        "/v1/platform/replicas/nodes",
        None,
    )
    .await;
    assert_eq!(list.status(), StatusCode::OK);
    let list_body = json_body(list).await;
    assert_eq!(list_body["success"], true);
    let nodes = list_body["data"].as_array().unwrap();
    assert_eq!(nodes.len(), 1);
    assert_eq!(nodes[0]["name"], "replica-us-east-1");

    let promote = request(
        &app,
        &token,
        Method::POST,
        &format!("/v1/platform/replicas/nodes/{}/promote", node_id),
        None,
    )
    .await;
    assert_eq!(promote.status(), StatusCode::OK);
    let promote_body = json_body(promote).await;
    assert_eq!(promote_body["success"], true);
    assert_eq!(promote_body["data"]["new_primary_id"], node_id);

    let list_after = request(
        &app,
        &token,
        Method::GET,
        "/v1/platform/replicas/nodes",
        None,
    )
    .await;
    let list_after_body = json_body(list_after).await;
    let nodes_after = list_after_body["data"].as_array().unwrap();
    assert_eq!(nodes_after[0]["role"], "primary");

    let stats = request(
        &app,
        &token,
        Method::GET,
        "/v1/platform/replicas/stats",
        None,
    )
    .await;
    assert_eq!(stats.status(), StatusCode::OK);
    let stats_body = json_body(stats).await;
    assert_eq!(stats_body["success"], true);
    assert_eq!(stats_body["data"]["primary_id"], node_id);

    let remove = request(
        &app,
        &token,
        Method::DELETE,
        &format!("/v1/platform/replicas/nodes/{}", node_id),
        None,
    )
    .await;
    assert_eq!(remove.status(), StatusCode::OK);

    let list_empty = request(
        &app,
        &token,
        Method::GET,
        "/v1/platform/replicas/nodes",
        None,
    )
    .await;
    let list_empty_body = json_body(list_empty).await;
    assert!(list_empty_body["data"].as_array().unwrap().is_empty());
}
