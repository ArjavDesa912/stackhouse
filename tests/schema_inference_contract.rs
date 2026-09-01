use std::collections::HashMap;

use serde_json::json;
use stackhouse::{
    db::{json_to_sql_value, SqlValue},
    inference::{infer_batch_schema, infer_schema, infer_type, PgType},
};

#[test]
fn infer_type_maps_every_json_shape_to_postgres_affinity() {
    assert_eq!(infer_type(&json!(42)), PgType::BigInt);
    assert_eq!(infer_type(&json!(42_u64)), PgType::BigInt);
    assert_eq!(infer_type(&json!(3.14)), PgType::DoublePrecision);
    assert_eq!(infer_type(&json!(true)), PgType::Boolean);
    assert_eq!(infer_type(&json!("hello")), PgType::Text);
    assert_eq!(infer_type(&json!({"nested": true})), PgType::Jsonb);
    assert_eq!(infer_type(&json!(["a", "b"])), PgType::Jsonb);
    assert_eq!(infer_type(&json!(null)), PgType::Null);
}

#[test]
fn infer_schema_skips_null_columns_and_marks_nested_values() {
    let schema = infer_schema(&json!({
        "name": "Ada",
        "age": 37,
        "profile": {"tier": "pro"},
        "events": ["signup"],
        "ignored": null
    }))
    .unwrap();

    let columns: HashMap<_, _> = schema
        .iter()
        .map(|column| (column.name.as_str(), column))
        .collect();

    assert_eq!(columns.len(), 4);
    assert_eq!(columns["name"].pg_type, PgType::Text);
    assert_eq!(columns["age"].pg_type, PgType::BigInt);
    assert_eq!(columns["profile"].pg_type, PgType::Jsonb);
    assert!(columns["profile"].is_nested);
    assert!(columns["events"].is_nested);
    assert!(!columns.contains_key("ignored"));
}

#[test]
fn infer_batch_schema_unifies_columns_and_promotes_conflicting_types() {
    let schema = infer_batch_schema(&[
        json!({"amount": 12, "metadata": null}),
        json!({"amount": 12.5, "metadata": {"region": "west"}}),
        json!({"amount": 99, "active": true}),
    ])
    .unwrap();

    let columns: HashMap<_, _> = schema
        .iter()
        .map(|column| (column.name.as_str(), column))
        .collect();

    assert_eq!(columns["amount"].pg_type, PgType::DoublePrecision);
    assert_eq!(columns["metadata"].pg_type, PgType::Jsonb);
    assert!(columns["metadata"].is_nested);
    assert_eq!(columns["active"].pg_type, PgType::Boolean);
}

#[test]
fn infer_schema_rejects_non_object_payloads() {
    let error = infer_schema(&json!(["not", "an", "object"])).unwrap_err();
    assert_eq!(error.error_code(), "INVALID_PAYLOAD");
}

#[test]
fn postgres_type_promotion_is_predictable() {
    assert!(PgType::BigInt.can_promote_to(&PgType::DoublePrecision));
    assert!(PgType::BigInt.can_promote_to(&PgType::Text));
    assert!(PgType::Null.can_promote_to(&PgType::Boolean));
    assert!(!PgType::Text.can_promote_to(&PgType::BigInt));
    assert_eq!(
        PgType::common_type(&PgType::BigInt, &PgType::DoublePrecision),
        PgType::DoublePrecision
    );
    assert_eq!(
        PgType::common_type(&PgType::Text, &PgType::Boolean),
        PgType::Jsonb
    );
}

#[test]
fn json_to_sql_value_preserves_every_json_value_family() {
    assert!(matches!(json_to_sql_value(&json!(null)), SqlValue::Null));
    assert!(matches!(
        json_to_sql_value(&json!(true)),
        SqlValue::Boolean(true)
    ));
    assert!(matches!(
        json_to_sql_value(&json!(42)),
        SqlValue::Integer(42)
    ));
    assert!(matches!(
        json_to_sql_value(&json!(3.5)),
        SqlValue::Real(value) if value == 3.5
    ));
    assert!(matches!(
        json_to_sql_value(&json!("hello")),
        SqlValue::Text(value) if value == "hello"
    ));
    assert!(matches!(
        json_to_sql_value(&json!({"nested": true})),
        SqlValue::Json(value) if value == json!({"nested": true})
    ));
}
