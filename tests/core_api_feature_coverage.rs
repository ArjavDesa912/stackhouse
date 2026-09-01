use std::{collections::HashSet, sync::Arc};

use axum::{
    body::{to_bytes, Body},
    http::{Method, StatusCode},
    Router,
};
use serde_json::{json, Value};
use stackhouse::{
    api::{create_router, AppState},
    db::StackhouseStore,
    guard::SchemaGuard,
};
use tower::ServiceExt;

async fn build_app() -> Option<Router> {
    match StackhouseStore::in_memory().await {
        Ok(store) => Some(create_router(AppState::new(Arc::new(store)))),
        Err(error) => {
            eprintln!(
                "skipping DB-backed public API feature coverage because the test database is unavailable: {error}"
            );
            None
        }
    }
}

fn unique_collection(prefix: &str) -> String {
    format!("{}_{}", prefix, uuid::Uuid::new_v4().simple())
}

async fn request(
    app: Router,
    method: Method,
    uri: &str,
    body: Option<Value>,
) -> axum::response::Response {
    let mut builder = axum::http::Request::builder().method(method).uri(uri);
    let body = match body {
        Some(value) => {
            builder = builder.header("content-type", "application/json");
            Body::from(serde_json::to_vec(&value).unwrap())
        }
        None => Body::empty(),
    };

    app.oneshot(builder.body(body).unwrap()).await.unwrap()
}

async fn json_body(response: axum::response::Response) -> Value {
    let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    serde_json::from_slice(&body).unwrap()
}

fn array_values(body: &Value, key: &str) -> Vec<Value> {
    body[key].as_array().unwrap().clone()
}

fn string_set(values: &[Value], key: &str) -> HashSet<String> {
    values
        .iter()
        .filter_map(|entry| entry[key].as_str().map(str::to_string))
        .collect()
}

#[tokio::test]
async fn root_and_health_expose_public_api_contracts() {
    let Some(app) = build_app().await else {
        return;
    };

    let root = request(app.clone(), Method::GET, "/", None).await;
    assert_eq!(root.status(), StatusCode::OK);
    let root_body = json_body(root).await;
    assert_eq!(root_body["name"], "Stackhouse");
    for endpoint in [
        "push",
        "batch_push",
        "preview",
        "query",
        "get_by_id",
        "update",
        "bulk_update",
        "delete",
        "bulk_delete",
        "tables",
        "table_stats",
        "stream",
        "health",
    ] {
        assert!(
            root_body["endpoints"][endpoint].is_string(),
            "root response should describe {endpoint}"
        );
    }

    let health = request(app, Method::GET, "/health", None).await;
    assert_eq!(health.status(), StatusCode::OK);
    let health_body = json_body(health).await;
    assert_eq!(health_body["status"], "healthy");
    assert_eq!(health_body["database"], "connected");
}

#[tokio::test]
async fn push_query_update_delete_and_table_metadata_cover_schema_lifecycle() {
    let Some(app) = build_app().await else {
        return;
    };
    let collection = unique_collection("feature_items");

    let first_push = request(
        app.clone(),
        Method::POST,
        &format!("/v1/push/{collection}"),
        Some(json!({
            "name": "alpha",
            "amount": 12,
            "ratio": 1.25,
            "active": true,
            "metadata": {"region": "north"},
            "tags": ["seed", "core"],
            "ignored_null": null
        })),
    )
    .await;
    assert_eq!(first_push.status(), StatusCode::CREATED);
    let first_body = json_body(first_push).await;
    let first_id = first_body["data"]["id"].as_i64().unwrap();
    assert_eq!(first_body["data"]["collection"], collection);
    let inserted_columns: HashSet<_> = first_body["data"]["columns_added"]
        .as_array()
        .unwrap()
        .iter()
        .map(|value| value.as_str().unwrap().to_string())
        .collect();
    assert!(inserted_columns.contains("name"));
    assert!(inserted_columns.contains("amount"));
    assert!(inserted_columns.contains("ratio"));
    assert!(inserted_columns.contains("active"));
    assert!(inserted_columns.contains("metadata"));
    assert!(inserted_columns.contains("tags"));
    assert!(!inserted_columns.contains("ignored_null"));

    let second_push = request(
        app.clone(),
        Method::POST,
        &format!("/v1/push/{collection}"),
        Some(json!({
            "name": "beta",
            "amount": 3,
            "status": "new"
        })),
    )
    .await;
    assert_eq!(second_push.status(), StatusCode::CREATED);

    let query = request(
        app.clone(),
        Method::GET,
        &format!("/v1/query/{collection}?name=alpha&order_by=amount&order_dir=DESC"),
        None,
    )
    .await;
    assert_eq!(query.status(), StatusCode::OK);
    let query_body = json_body(query).await;
    assert_eq!(query_body["success"], true);
    assert_eq!(query_body["count"], 1);
    let alpha = &query_body["data"].as_array().unwrap()[0];
    assert_eq!(alpha["name"], "alpha");
    assert_eq!(alpha["amount"], 12);
    assert_eq!(alpha["ratio"], 1.25);
    assert_eq!(alpha["active"], true);
    assert_eq!(alpha["metadata"]["region"], "north");
    assert_eq!(alpha["tags"], json!(["seed", "core"]));
    assert!(alpha.get("ignored_null").is_none());

    let stats = request(
        app.clone(),
        Method::GET,
        &format!("/v1/tables/{collection}"),
        None,
    )
    .await;
    assert_eq!(stats.status(), StatusCode::OK);
    let stats_body = json_body(stats).await;
    assert_eq!(stats_body["data"]["name"], collection);
    assert_eq!(stats_body["data"]["row_count"], 2);
    let columns = array_values(&stats_body["data"], "columns");
    let column_names = string_set(&columns, "name");
    for column in [
        "id",
        "created_at",
        "updated_at",
        "name",
        "amount",
        "ratio",
        "active",
        "metadata",
        "tags",
        "status",
    ] {
        assert!(column_names.contains(column), "{column} should exist");
    }

    let tables = request(app.clone(), Method::GET, "/v1/tables", None).await;
    assert_eq!(tables.status(), StatusCode::OK);
    let tables_body = json_body(tables).await;
    let table_names: HashSet<_> = tables_body["tables"]
        .as_array()
        .unwrap()
        .iter()
        .map(|name| name.as_str().unwrap().to_string())
        .collect();
    assert!(table_names.contains(&collection));

    let update = request(
        app.clone(),
        Method::POST,
        &format!("/v1/update/{collection}/{first_id}"),
        Some(json!({
            "status": "reviewed",
            "reviewed_by": "qa"
        })),
    )
    .await;
    assert_eq!(update.status(), StatusCode::OK);
    let update_body = json_body(update).await;
    assert_eq!(update_body["affected"], 1);

    let fetched = request(
        app.clone(),
        Method::GET,
        &format!("/v1/query/{collection}/{first_id}"),
        None,
    )
    .await;
    assert_eq!(fetched.status(), StatusCode::OK);
    let fetched_body = json_body(fetched).await;
    assert_eq!(fetched_body["data"]["status"], "reviewed");
    assert_eq!(fetched_body["data"]["reviewed_by"], "qa");

    let bulk_update = request(
        app.clone(),
        Method::POST,
        &format!("/v1/update/{collection}"),
        Some(json!({
            "filters": {"status": "new"},
            "data": {"status": "archived", "archived_by": "job"}
        })),
    )
    .await;
    assert_eq!(bulk_update.status(), StatusCode::OK);
    let bulk_update_body = json_body(bulk_update).await;
    assert_eq!(bulk_update_body["affected"], 1);

    let bulk_delete = request(
        app.clone(),
        Method::POST,
        &format!("/v1/delete/{collection}"),
        Some(json!({
            "filters": {"status": "archived"}
        })),
    )
    .await;
    assert_eq!(bulk_delete.status(), StatusCode::OK);
    let bulk_delete_body = json_body(bulk_delete).await;
    assert_eq!(bulk_delete_body["affected"], 1);

    let delete = request(
        app.clone(),
        Method::POST,
        &format!("/v1/delete/{collection}/{first_id}"),
        None,
    )
    .await;
    assert_eq!(delete.status(), StatusCode::OK);
    let delete_body = json_body(delete).await;
    assert_eq!(delete_body["affected"], 1);

    let final_query = request(app, Method::GET, &format!("/v1/query/{collection}"), None).await;
    assert_eq!(final_query.status(), StatusCode::OK);
    let final_body = json_body(final_query).await;
    assert_eq!(final_body["count"], 0);
    assert_eq!(final_body["data"], json!([]));
}

#[tokio::test]
async fn batch_push_query_pagination_and_validation_cover_query_features() {
    let Some(app) = build_app().await else {
        return;
    };
    let collection = unique_collection("batch_items");

    let empty_batch = request(
        app.clone(),
        Method::POST,
        &format!("/v1/push/{collection}/batch"),
        Some(json!([])),
    )
    .await;
    assert_eq!(empty_batch.status(), StatusCode::BAD_REQUEST);
    let empty_body = json_body(empty_batch).await;
    assert_eq!(empty_body["error"]["code"], "INVALID_PAYLOAD");

    let invalid_batch = request(
        app.clone(),
        Method::POST,
        &format!("/v1/push/{collection}/batch"),
        Some(json!([1])),
    )
    .await;
    assert_eq!(invalid_batch.status(), StatusCode::BAD_REQUEST);
    let invalid_body = json_body(invalid_batch).await;
    assert_eq!(invalid_body["error"]["code"], "INVALID_PAYLOAD");

    let batch = request(
        app.clone(),
        Method::POST,
        &format!("/v1/push/{collection}/batch"),
        Some(json!([
            {"tenant": "acme", "amount": 10},
            {"tenant": "globex", "amount": 20, "notes": "priority"},
            {"tenant": "acme", "amount": 30}
        ])),
    )
    .await;
    assert_eq!(batch.status(), StatusCode::CREATED);
    let batch_body = json_body(batch).await;
    assert_eq!(batch_body["data"]["inserted"], 3);

    let filtered = request(
        app.clone(),
        Method::GET,
        &format!("/v1/query/{collection}?tenant=acme&order_by=amount&order_dir=DESC"),
        None,
    )
    .await;
    assert_eq!(filtered.status(), StatusCode::OK);
    let filtered_body = json_body(filtered).await;
    let filtered_rows = filtered_body["data"].as_array().unwrap();
    assert_eq!(filtered_rows.len(), 2);
    assert_eq!(filtered_rows[0]["amount"], 30);
    assert_eq!(filtered_rows[1]["amount"], 10);

    let paged = request(
        app.clone(),
        Method::GET,
        &format!("/v1/query/{collection}?order_by=amount&order_dir=ASC&limit=2&offset=1"),
        None,
    )
    .await;
    assert_eq!(paged.status(), StatusCode::OK);
    let paged_body = json_body(paged).await;
    let paged_rows = paged_body["data"].as_array().unwrap();
    assert_eq!(paged_rows.len(), 2);
    assert_eq!(paged_rows[0]["amount"], 20);
    assert_eq!(paged_rows[1]["amount"], 30);

    let bad_order_dir = request(
        app.clone(),
        Method::GET,
        &format!("/v1/query/{collection}?order_by=amount&order_dir=SIDEWAYS"),
        None,
    )
    .await;
    assert_eq!(bad_order_dir.status(), StatusCode::BAD_REQUEST);
    let bad_order_dir_body = json_body(bad_order_dir).await;
    assert_eq!(bad_order_dir_body["error"]["code"], "INVALID_PAYLOAD");

    let bad_order_by = request(
        app,
        Method::GET,
        &format!("/v1/query/{collection}?order_by=amount%3Bdrop"),
        None,
    )
    .await;
    assert_eq!(bad_order_by.status(), StatusCode::BAD_REQUEST);
    let bad_order_by_body = json_body(bad_order_by).await;
    assert_eq!(bad_order_by_body["error"]["code"], "INVALID_IDENTIFIER");
}

#[tokio::test]
async fn identifier_validation_is_enforced_across_collection_and_payload_inputs() {
    for valid in ["valid_name", "_internal", "Name123"] {
        SchemaGuard::validate_identifier(valid).unwrap();
    }

    let too_long = "x".repeat(129);
    for invalid in ["", "1bad", "has-dash", "DROP", "SELECT", too_long.as_str()] {
        assert!(
            SchemaGuard::validate_identifier(invalid).is_err(),
            "{invalid:?} should be rejected"
        );
    }

    let Some(app) = build_app().await else {
        return;
    };
    let collection = unique_collection("identifier_items");

    let bad_collection = request(
        app.clone(),
        Method::POST,
        "/v1/push/SELECT",
        Some(json!({"name": "bad"})),
    )
    .await;
    assert_eq!(bad_collection.status(), StatusCode::BAD_REQUEST);
    let bad_collection_body = json_body(bad_collection).await;
    assert_eq!(bad_collection_body["error"]["code"], "INVALID_IDENTIFIER");

    let bad_payload_key = request(
        app,
        Method::POST,
        &format!("/v1/push/{collection}"),
        Some(json!({"name": "ok", "DROP": true})),
    )
    .await;
    assert_eq!(bad_payload_key.status(), StatusCode::BAD_REQUEST);
    let bad_payload_body = json_body(bad_payload_key).await;
    assert_eq!(bad_payload_body["error"]["code"], "INVALID_IDENTIFIER");
}
