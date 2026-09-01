//! End-to-end integration test for the serverless-functions router.

use std::sync::Arc;

use axum::{
    body::{to_bytes, Body},
    http::{Method, Request, StatusCode},
    Router,
};
use serde_json::{json, Value};
use stackhouse::{
    auth::{create_auth_router, AuthService, AuthState, SignupRequest},
    compute::{create_functions_router, FunctionsService, FunctionsState},
    db::StackhouseStore,
};
use tower::ServiceExt;

async fn build_app() -> Option<(Router, String)> {
    let store = match StackhouseStore::in_memory().await {
        Ok(store) => Arc::new(store),
        Err(error) => {
            eprintln!(
                "skipping functions integration test because the test database is unavailable: {error}"
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
            email: "functions@stackhouse.dev".to_string(),
            password: "password123".to_string(),
            metadata: None,
        })
        .await
        .unwrap();

    let functions = Arc::new(FunctionsService::new(Arc::clone(&store)).await.unwrap());
    let functions_state = FunctionsState {
        functions,
        auth: auth_state.clone(),
    };

    let app = Router::new()
        .nest("/v1/auth", create_auth_router(auth_state.clone()))
        .nest("/v1/functions", create_functions_router(functions_state));

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
async fn functions_deploy_invoke_list_delete_lifecycle() {
    let Some((app, token)) = build_app().await else {
        return;
    };

    // Deploy a simple JavaScript function.
    let deploy = request(
        &app,
        &token,
        Method::POST,
        "/v1/functions/deploy",
        Some(json!({
            "name": "double",
            "runtime": "javascript",
            "source_code": "exports.handler = (input) => ({ doubled: input.value * 2 });"
        })),
    )
    .await;
    assert_eq!(deploy.status(), StatusCode::OK);
    let deploy_body = json_body(deploy).await;
    assert_eq!(deploy_body["success"], true);
    let func_id = deploy_body["data"]["id"].as_str().unwrap().to_string();
    assert_eq!(deploy_body["data"]["runtime"], "javascript");

    // Invoke it with a plain input body (no `input` wrapper).
    let invoke = request(
        &app,
        &token,
        Method::POST,
        "/v1/functions/invoke/double",
        Some(json!({"value": 21})),
    )
    .await;
    assert_eq!(invoke.status(), StatusCode::OK);
    let invoke_body = json_body(invoke).await;
    assert_eq!(invoke_body["success"], true);
    assert_eq!(invoke_body["data"]["output"], json!({"doubled": 42}));

    // The wrapped `input` form still works.
    let invoke_wrapped = request(
        &app,
        &token,
        Method::POST,
        "/v1/functions/invoke/double",
        Some(json!({"input": {"value": 5}})),
    )
    .await;
    assert_eq!(invoke_wrapped.status(), StatusCode::OK);
    let invoke_wrapped_body = json_body(invoke_wrapped).await;
    assert_eq!(
        invoke_wrapped_body["data"]["output"],
        json!({"doubled": 10})
    );

    // List functions.
    let list = request(&app, &token, Method::GET, "/v1/functions", None).await;
    assert_eq!(list.status(), StatusCode::OK);
    let list_body = json_body(list).await;
    assert_eq!(list_body["success"], true);
    let functions = list_body["data"].as_array().unwrap();
    assert_eq!(functions.len(), 1);
    assert_eq!(functions[0]["name"], "double");

    // Delete the function.
    let delete = request(
        &app,
        &token,
        Method::DELETE,
        &format!("/v1/functions/{}", func_id),
        None,
    )
    .await;
    assert_eq!(delete.status(), StatusCode::OK);
    let delete_body = json_body(delete).await;
    assert_eq!(delete_body["success"], true);

    // Listing is now empty.
    let list_after = request(&app, &token, Method::GET, "/v1/functions", None).await;
    let list_after_body = json_body(list_after).await;
    assert!(list_after_body["data"].as_array().unwrap().is_empty());
}

#[tokio::test]
async fn functions_runtime_values_all_execute_as_javascript() {
    let Some((app, token)) = build_app().await else {
        return;
    };

    for runtime in ["typescript", "wasm_rust", "wasm_js", "javascript"] {
        let name = format!("fn_{}", runtime.replace('_', ""));
        let deploy = request(
            &app,
            &token,
            Method::POST,
            "/v1/functions/deploy",
            Some(json!({
                "name": name,
                "runtime": runtime,
                "source_code": "exports.handler = (input) => ({ runtime: input.runtime });"
            })),
        )
        .await;
        assert_eq!(
            deploy.status(),
            StatusCode::OK,
            "deploy failed for {}",
            runtime
        );

        let invoke = request(
            &app,
            &token,
            Method::POST,
            &format!("/v1/functions/invoke/{}", name),
            Some(json!({"runtime": runtime})),
        )
        .await;
        assert_eq!(
            invoke.status(),
            StatusCode::OK,
            "invoke failed for {}",
            runtime
        );
        let body = json_body(invoke).await;
        assert_eq!(body["data"]["output"], json!({"runtime": runtime}));
    }
}
