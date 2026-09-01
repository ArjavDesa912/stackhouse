use std::sync::Arc;

use axum::{
    body::{to_bytes, Body},
    http::{Method, Request, StatusCode},
    Router,
};
use futures::future::join_all;
use serde_json::{json, Value};
use stackhouse::{
    api::{create_router, AppState},
    db::StackhouseStore,
};
use tower::ServiceExt;

async fn build_app() -> Router {
    let store = Arc::new(StackhouseStore::in_memory().await.unwrap());
    create_router(AppState::new(store))
}

async fn json_body(response: axum::response::Response) -> Value {
    let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    serde_json::from_slice(&body).unwrap()
}

async fn request(
    app: Router,
    method: Method,
    uri: &str,
    body: Option<Value>,
) -> axum::response::Response {
    let mut builder = Request::builder().method(method).uri(uri);
    let body = match body {
        Some(value) => {
            builder = builder.header("content-type", "application/json");
            Body::from(serde_json::to_vec(&value).unwrap())
        }
        None => Body::empty(),
    };

    app.oneshot(builder.body(body).unwrap()).await.unwrap()
}

async fn seed_security_table(app: &Router) {
    let response = request(
        app.clone(),
        Method::POST,
        "/v1/push/security_items",
        Some(json!({"name": "Alice", "status": "active"})),
    )
    .await;
    assert_eq!(response.status(), StatusCode::CREATED);
}

async fn assert_invalid_identifier(response: axum::response::Response) {
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    let body = json_body(response).await;
    assert_eq!(body["error"]["code"], "INVALID_IDENTIFIER");
}

#[tokio::test]
async fn query_filter_keys_reject_identifier_injection() {
    let app = build_app().await;
    seed_security_table(&app).await;

    let response = request(
        app,
        Method::GET,
        "/v1/query/security_items?name%20OR%201%3D1=Alice",
        None,
    )
    .await;

    assert_invalid_identifier(response).await;
}

#[tokio::test]
async fn bulk_mutation_filters_reject_identifier_injection() {
    let app = build_app().await;
    seed_security_table(&app).await;

    let malicious_filter = "name; DROP TABLE security_items;--";

    let update = request(
        app.clone(),
        Method::POST,
        "/v1/update/security_items",
        Some(json!({
            "filters": { malicious_filter: "Alice" },
            "data": { "status": "inactive" }
        })),
    )
    .await;
    assert_invalid_identifier(update).await;

    let delete = request(
        app,
        Method::POST,
        "/v1/delete/security_items",
        Some(json!({
            "filters": { malicious_filter: "Alice" }
        })),
    )
    .await;
    assert_invalid_identifier(delete).await;
}

#[tokio::test]
async fn collection_paths_reject_identifier_injection_before_sql_execution() {
    let app = build_app().await;
    seed_security_table(&app).await;

    for (method, uri, body) in [
        (
            Method::GET,
            "/v1/query/security_items%27%20OR%20%271%27%3D%271",
            None,
        ),
        (
            Method::POST,
            "/v1/delete/security_items%3Bdrop%20table%20security_items/1",
            None,
        ),
        (
            Method::POST,
            "/v1/update/security_items%3Bdrop%20table%20security_items/1",
            Some(json!({"status": "inactive"})),
        ),
        (
            Method::GET,
            "/v1/tables/security_items%27%20OR%20%271%27%3D%271",
            None,
        ),
    ] {
        let response = request(app.clone(), method, uri, body).await;
        assert_invalid_identifier(response).await;
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn concurrent_push_stress_keeps_collection_queryable() {
    let app = build_app().await;
    let writes = (0..64).map(|index| {
        request(
            app.clone(),
            Method::POST,
            "/v1/push/stress_items",
            Some(json!({
                "name": format!("item-{index}"),
                "batch": "stress",
            })),
        )
    });

    let responses = join_all(writes).await;
    let mut failures = Vec::new();
    for response in responses {
        let status = response.status();
        if status != StatusCode::CREATED {
            failures.push((status, json_body(response).await));
        }
    }
    assert!(
        failures.is_empty(),
        "concurrent write failures: {failures:?}"
    );

    let response = request(
        app,
        Method::GET,
        "/v1/query/stress_items?limit=1000&batch=stress",
        None,
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK);

    let body = json_body(response).await;
    assert_eq!(body["count"], 64);
}
