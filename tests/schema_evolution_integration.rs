//! Integration tests for the schema-later/type-promotion write path.
//!
//! These tests exercise the actual PostgreSQL casts that `SchemaGuard` emits,
//! including `BIGINT -> DOUBLE PRECISION`, `anything -> JSONB`, and
//! `JSONB -> TEXT`.

use serde_json::{json, Value};
use stackhouse::db::StackhouseStore;
use stackhouse::guard::SchemaGuard;
use stackhouse::inference::PgType;
use std::sync::Arc;

fn with_random_suffix(prefix: &str) -> String {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    format!("{}_{}", prefix, nanos)
}

#[tokio::test]
async fn bigint_to_double_widening_on_second_push() {
    let store = Arc::new(StackhouseStore::in_memory().await.unwrap());
    let guard = SchemaGuard::new(Arc::clone(&store));
    let table = with_random_suffix("promote_test");

    // First push: integer -> BIGINT
    let payload1 = json!({"amount": 12});
    guard.ensure_table(&table).await.unwrap();
    let cols1 = guard.ensure_columns(&table, &payload1).await.unwrap();
    assert_eq!(cols1.len(), 1);
    assert_eq!(cols1[0].0, "amount");
    assert_eq!(cols1[0].1, PgType::BigInt);

    // Second push: decimal -> must widen to DOUBLE PRECISION and succeed
    let payload2 = json!({"amount": 12.5});
    let cols2 = guard.ensure_columns(&table, &payload2).await.unwrap();
    assert_eq!(cols2[0].1, PgType::DoublePrecision);

    let schema = guard.get_table_schema(&table).await.unwrap();
    let amount_type = schema
        .iter()
        .find(|c| c.name == "amount")
        .map(|c| c.col_type.clone())
        .unwrap();
    assert_eq!(amount_type, "double precision");
}

#[tokio::test]
async fn mixed_type_batch_unifies_to_double() {
    let store = Arc::new(StackhouseStore::in_memory().await.unwrap());
    let guard = SchemaGuard::new(Arc::clone(&store));
    let table = with_random_suffix("batch_promote");

    let payloads = vec![
        json!({"amount": 12}),
        json!({"amount": 12.5}),
        json!({"amount": 7}),
    ];

    guard.ensure_table(&table).await.unwrap();
    let columns = guard.ensure_batch_columns(&table, &payloads).await.unwrap();
    assert_eq!(columns.len(), 1);
    assert_eq!(columns[0].1, PgType::DoublePrecision);

    // All rows should now be insertable as DOUBLE PRECISION.
    for payload in &payloads {
        let obj = payload.as_object().unwrap();
        let params = vec![stackhouse::db::json_to_sql_value_for_type(
            obj.get("amount").unwrap(),
            &PgType::DoublePrecision,
        )];
        let sql = format!("INSERT INTO {} (amount) VALUES (?)", table);
        store.execute(sql, params).await.unwrap();
    }

    let rows = store
        .query(format!("SELECT amount FROM {} ORDER BY id", table), vec![])
        .await
        .unwrap();
    assert_eq!(rows.len(), 3);
}

#[tokio::test]
async fn jsonb_and_text_casts_are_safe() {
    let store = Arc::new(StackhouseStore::in_memory().await.unwrap());
    let guard = SchemaGuard::new(Arc::clone(&store));
    let table = with_random_suffix("jsonb_text_cast");

    // Create a TEXT column, then push an object. The column should be widened
    // to JSONB and the original text value must be preserved.
    let payload1 = json!({"note": "hello"});
    guard.ensure_table(&table).await.unwrap();
    guard.ensure_columns(&table, &payload1).await.unwrap();

    let payload2 = json!({"note": {"nested": "object"}});
    let cols = guard.ensure_columns(&table, &payload2).await.unwrap();
    assert_eq!(cols[0].1, PgType::Jsonb);

    // Insert the original text. For a JSONB column we must use SqlValue::Json.
    let text_json = json!("hello");
    let sql1 = format!("INSERT INTO {} (note) VALUES (?)", table);
    store
        .execute(
            sql1,
            vec![stackhouse::db::json_to_sql_value_for_type(
                &text_json,
                &PgType::Jsonb,
            )],
        )
        .await
        .unwrap();

    // Insert the object.
    let obj_json = json!({"nested": "object"});
    let sql2 = format!("INSERT INTO {} (note) VALUES (?)", table);
    store
        .execute(
            sql2,
            vec![stackhouse::db::json_to_sql_value_for_type(
                &obj_json,
                &PgType::Jsonb,
            )],
        )
        .await
        .unwrap();

    let schema = guard.get_table_schema(&table).await.unwrap();
    let note_type = schema
        .iter()
        .find(|c| c.name == "note")
        .map(|c| c.col_type.clone())
        .unwrap();
    assert_eq!(note_type, "jsonb");
}

#[tokio::test]
async fn using_cast_expressions_work_in_postgres() {
    // Run a few sample `ALTER TABLE ... ALTER COLUMN ... TYPE ... USING ...`
    // statements to make sure the generated expressions are valid.
    let store = Arc::new(StackhouseStore::in_memory().await.unwrap());
    let table = with_random_suffix("cast_sanity");

    store
        .execute_batch(format!(
            "CREATE TABLE {} (id BIGSERIAL PRIMARY KEY, a BIGINT, b TEXT, c BOOLEAN)",
            table
        ))
        .await
        .unwrap();

    // BIGINT -> DOUBLE PRECISION
    let sql = format!(
        "ALTER TABLE {} ALTER COLUMN a TYPE DOUBLE PRECISION USING a::double precision",
        table
    );
    store.execute_batch(sql).await.unwrap();

    // BIGINT -> TEXT
    let sql = format!(
        "ALTER TABLE {} ALTER COLUMN a TYPE TEXT USING a::text",
        table
    );
    store.execute_batch(sql).await.unwrap();

    // TEXT -> JSONB
    let sql = format!(
        "ALTER TABLE {} ALTER COLUMN b TYPE JSONB USING to_jsonb(b)",
        table
    );
    store.execute_batch(sql).await.unwrap();

    // JSONB -> TEXT
    let sql = format!(
        "ALTER TABLE {} ALTER COLUMN b TYPE TEXT USING b #>> ARRAY[]::text[]",
        table
    );
    store.execute_batch(sql).await.unwrap();

    // BOOLEAN -> JSONB
    let sql = format!(
        "ALTER TABLE {} ALTER COLUMN c TYPE JSONB USING to_jsonb(c)",
        table
    );
    store.execute_batch(sql).await.unwrap();
}

#[tokio::test]
async fn schema_churn_rate_limit_is_distinct_from_column_cap() {
    let store = Arc::new(StackhouseStore::in_memory().await.unwrap());
    let guard = SchemaGuard::new(Arc::clone(&store));
    let table = with_random_suffix("churn");

    guard.ensure_table(&table).await.unwrap();

    // Saturate the default 20-column-per-minute churn limit.
    for i in 0..20 {
        let mut map = serde_json::Map::new();
        map.insert(format!("col_{}", i), json!(i));
        let payload = Value::Object(map);
        guard.ensure_columns(&table, &payload).await.unwrap();
    }

    // The next new column should hit the churn rate limit, not the column cap.
    let payload = json!({"one_too_many": 1});
    let err = guard.ensure_columns(&table, &payload).await.unwrap_err();
    let msg = format!("{}", err);
    assert!(
        msg.contains("schema churn"),
        "Expected churn rate limit, got: {}",
        msg
    );
    assert!(
        msg.contains("distinct from the hard"),
        "Expected distinct error message, got: {}",
        msg
    );
}

#[tokio::test]
async fn preview_endpoint_does_not_mutate_schema() {
    let store = Arc::new(StackhouseStore::in_memory().await.unwrap());
    let guard = SchemaGuard::new(Arc::clone(&store));
    let table = with_random_suffix("preview");

    let sample = json!({"amount": 12, "active": true});
    let preview = guard.preview_schema_changes(&table, &sample).await.unwrap();

    assert!(!preview.table_exists);
    assert!(preview.create_table_sql.is_some());
    assert_eq!(preview.additions.len(), 2);
    assert!(preview.widenings.is_empty());

    // The table should still not exist.
    let schema = guard.get_table_schema(&table).await.unwrap();
    assert!(schema.is_empty());
}

#[tokio::test]
async fn automatic_schema_changes_are_recorded_in_migration_history() {
    let store = Arc::new(StackhouseStore::in_memory().await.unwrap());
    let guard = SchemaGuard::new(Arc::clone(&store));
    let table = with_random_suffix("migrations");

    // Create the table and add a column.
    let payload1 = json!({"amount": 12});
    guard.ensure_table(&table).await.unwrap();
    guard.ensure_columns(&table, &payload1).await.unwrap();

    // Widen the column.
    let payload2 = json!({"amount": 12.5});
    guard.ensure_columns(&table, &payload2).await.unwrap();

    // The migration history table should have been created and contain rows.
    let rows = store
        .query(
            "SELECT version, name, up_sql, down_sql, status FROM stackhouse_schema_migrations ORDER BY version"
                .to_string(),
            vec![],
        )
        .await
        .unwrap();

    assert!(
        !rows.is_empty(),
        "Expected migration history to contain automatic schema changes"
    );

    let last = rows.last().unwrap();
    let status = last
        .iter()
        .find(|(k, _)| k == "status")
        .unwrap()
        .1
        .as_str()
        .unwrap();
    assert_eq!(status, "applied");

    let up = last
        .iter()
        .find(|(k, _)| k == "up_sql")
        .unwrap()
        .1
        .as_str()
        .unwrap();
    assert!(up.contains("ALTER TABLE"));
}
