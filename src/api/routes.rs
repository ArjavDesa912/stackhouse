use super::handlers::{
    batch_push_handler, bulk_delete_handler, bulk_update_handler, create_dataset_handler,
    delete_handler, drop_table_handler, get_by_id_handler, get_dataset_handler, health_handler,
    list_datasets_handler, list_tables_handler, preview_dataset_handler, preview_handler,
    push_handler, query_handler, root_handler, sql_execute_handler, sql_query_handler,
    stream_handler, table_stats_handler, update_handler, AppState,
};
use axum::{
    routing::{get, post},
    Router,
};

/// Creates the Axum router with all endpoints.
pub fn create_router(state: AppState) -> Router {
    Router::new()
        .route("/v1/push/:collection", post(push_handler))
        .route("/v1/push/:collection/batch", post(batch_push_handler))
        .route("/v1/preview/:collection", post(preview_handler))
        .route("/v1/query/:collection", get(query_handler))
        .route("/v1/query/:collection/:id", get(get_by_id_handler))
        .route("/v1/update/:collection/:id", post(update_handler))
        .route("/v1/update/:collection", post(bulk_update_handler))
        .route("/v1/delete/:collection/:id", post(delete_handler))
        .route("/v1/delete/:collection", post(bulk_delete_handler))
        .route("/v1/sql/query", post(sql_query_handler))
        .route("/v1/sql/execute", post(sql_execute_handler))
        .route("/v1/tables", get(list_tables_handler))
        .route(
            "/v1/tables/:collection",
            get(table_stats_handler).delete(drop_table_handler),
        )
        .route(
            "/v1/datasets",
            get(list_datasets_handler).post(create_dataset_handler),
        )
        .route("/v1/datasets/:id", get(get_dataset_handler))
        .route("/v1/datasets/:id/preview", get(preview_dataset_handler))
        .route("/v1/stream/:collection", get(stream_handler))
        .route("/health", get(health_handler))
        .route("/", get(root_handler))
        .with_state(state)
}
