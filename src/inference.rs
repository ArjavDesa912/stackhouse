//! # Inference Engine (Schema-Brain)
//!
//! Inspects JSON payloads and maps them to PostgreSQL-compatible types.
//! This module is responsible for ensuring no data is lost during type inference.
//!
//! ## Type Mapping
//!
//! | JSON Type       | PostgreSQL Type | Logic/Constraint              |
//! |----------------|-----------------|------------------------------|
//! | Number (Int)   | BIGINT          | Check if `is_i64()`          |
//! | Number (Float) | DOUBLE PRECISION| Default for any decimal      |
//! | Boolean        | BOOLEAN         | Direct mapping               |
//! | String         | TEXT            | Standard UTF-8               |
//! | Object / Array | JSONB           | Native JSON support          |
//! | Null           | NULL            | Ignored during column creation |

use crate::error::{StackhouseError, StackhouseResult};
use lazy_static::lazy_static;
use regex::Regex;
use serde::{Deserialize, Serialize};
use serde_json::Value;

/// PostgreSQL type affinity for column definitions
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum PgType {
    BigInt,
    DoublePrecision,
    Boolean,
    Text,
    Jsonb,
    Bytea,
    Null,
    Date,
    TimestampTz,
    Uuid,
}

lazy_static! {
    /// ISO 8601 full timestamp (with optional fractional seconds and offset).
    static ref TIMESTAMP_RE: Regex = Regex::new(r"^\d{4}-\d{2}-\d{2}[T ]\d{2}:\d{2}(:\d{2}(\.\d+)?)?(Z|[+-]\d{2}:?\d{2})?$").unwrap();

    /// ISO 8601 date (YYYY-MM-DD).
    static ref DATE_RE: Regex = Regex::new(r"^\d{4}-\d{2}-\d{2}$").unwrap();

    /// UUID (8-4-4-4-12 hex digits).
    static ref UUID_RE: Regex = Regex::new(r"^[0-9a-fA-F]{8}-[0-9a-fA-F]{4}-[0-9a-fA-F]{4}-[0-9a-fA-F]{4}-[0-9a-fA-F]{12}$").unwrap();
}

impl PgType {
    /// Returns the SQL type name for column creation
    pub fn as_sql(&self) -> &'static str {
        match self {
            PgType::BigInt => "BIGINT",
            PgType::DoublePrecision => "DOUBLE PRECISION",
            PgType::Boolean => "BOOLEAN",
            PgType::Text => "TEXT",
            PgType::Jsonb => "JSONB",
            PgType::Bytea => "BYTEA",
            PgType::Null => "NULL",
            PgType::Date => "DATE",
            PgType::TimestampTz => "TIMESTAMPTZ",
            PgType::Uuid => "UUID",
        }
    }

    /// Parse a PostgreSQL `data_type` value (from `information_schema.columns`)
    /// into the closest `PgType` affinity.
    pub fn from_data_type(data_type: &str) -> Option<PgType> {
        let normalized = data_type.to_lowercase();
        match normalized.as_str() {
            "bigint" | "integer" | "smallint" | "int" | "int2" | "int4" | "int8" | "serial"
            | "bigserial" => Some(PgType::BigInt),
            "double precision" | "float8" | "real" | "float4" | "numeric" | "decimal" | "money" => {
                Some(PgType::DoublePrecision)
            }
            "boolean" | "bool" => Some(PgType::Boolean),
            "text" | "character varying" | "varchar" | "character" | "char" | "bpchar" | "name"
            | "citext" => Some(PgType::Text),
            "jsonb" | "json" => Some(PgType::Jsonb),
            "bytea" => Some(PgType::Bytea),
            "date" => Some(PgType::Date),
            "timestamp with time zone"
            | "timestamp without time zone"
            | "timestamp"
            | "timestamptz" => Some(PgType::TimestampTz),
            "uuid" => Some(PgType::Uuid),
            _ => None,
        }
    }

    /// Determines if this type can be promoted to another type
    /// Used for schema evolution when types conflict
    pub fn can_promote_to(&self, other: &PgType) -> bool {
        match (self, other) {
            // Same type - no promotion needed
            (a, b) if a == b => true,
            // BigInt can be promoted to DoublePrecision
            (PgType::BigInt, PgType::DoublePrecision) => true,
            // DATE can be widened to a timestamp (time is set to 00:00:00)
            (PgType::Date, PgType::TimestampTz) => true,
            // Anything can be promoted to JSONB
            (_, PgType::Jsonb) => true,
            // TEXT can hold anything as string
            (_, PgType::Text) => true,
            // NULL can be promoted to anything
            (PgType::Null, _) => true,
            _ => false,
        }
    }

    /// Returns the more general type between two types
    pub fn common_type(a: &PgType, b: &PgType) -> PgType {
        if a == b {
            return a.clone();
        }

        match (a, b) {
            (PgType::Null, other) | (other, PgType::Null) => other.clone(),
            (PgType::BigInt, PgType::DoublePrecision)
            | (PgType::DoublePrecision, PgType::BigInt) => PgType::DoublePrecision,
            // DATE + TIMESTAMP -> TIMESTAMP
            (PgType::Date, PgType::TimestampTz) | (PgType::TimestampTz, PgType::Date) => {
                PgType::TimestampTz
            }
            // For any other conflict, JSONB is the most permissive PostgreSQL
            // type and can safely hold every supported affinity.
            _ => PgType::Jsonb,
        }
    }

    /// Returns a safe PostgreSQL `USING` expression to cast a column of `source`
    /// type to `self` inside `ALTER TABLE ... ALTER COLUMN ... TYPE`.
    ///
    /// This must be data-loss-safe for every promotion path `can_promote_to`
    /// allows, including `TEXT -> JSONB` (which would fail with a bare `::jsonb`
    /// cast on non-JSON text) and `JSONB -> TEXT` (where a naive `::text` cast
    /// would leave JSON scalar strings wrapped in quotes).
    pub fn using_cast_expr(&self, source: &PgType, column: &str) -> String {
        if source == self {
            return column.to_string();
        }

        match (source, self) {
            // JSONB -> TEXT: extract the JSON text without quoting scalar strings.
            (PgType::Jsonb, PgType::Text) => {
                format!("{} #>> ARRAY[]::text[]", column)
            }
            // Any type -> JSONB: `to_jsonb` is safe for text, numbers, booleans,
            // bytea, timestamps, UUIDs, and existing JSONB values.
            (_, PgType::Jsonb) => format!("to_jsonb({})", column),
            // Any type -> TEXT: `::text` is a safe, lossless string representation.
            (_, PgType::Text) => format!("{}::text", column),
            // BIGINT -> DOUBLE PRECISION
            (PgType::BigInt, PgType::DoublePrecision) => {
                format!("{}::double precision", column)
            }
            // JSONB -> temporal/scalar: extract the JSON text first, then cast.
            (PgType::Jsonb, PgType::Date)
            | (PgType::Jsonb, PgType::TimestampTz)
            | (PgType::Jsonb, PgType::Uuid) => {
                format!(
                    "({} #>> ARRAY[]::text[])::{}",
                    column,
                    self.as_sql().to_lowercase()
                )
            }
            // Fallback: try an assignment cast. This is only reached for
            // promotion paths that are explicitly allowed by `can_promote_to`.
            _ => format!("{}::{}", column, self.as_sql().to_lowercase()),
        }
    }
}

/// Infers the PostgreSQL type from a JSON value
///
/// # Arguments
/// * `value` - The JSON value to infer type from
///
/// # Returns
/// The corresponding PostgreSQL type affinity
pub fn infer_type(value: &Value) -> PgType {
    match value {
        Value::Null => PgType::Null,
        Value::Bool(_) => PgType::Boolean,
        Value::Number(n) => {
            if n.is_i64() || n.is_u64() {
                PgType::BigInt
            } else {
                PgType::DoublePrecision
            }
        }
        Value::String(s) => {
            if TIMESTAMP_RE.is_match(s) {
                PgType::TimestampTz
            } else if DATE_RE.is_match(s) {
                PgType::Date
            } else if UUID_RE.is_match(s) {
                PgType::Uuid
            } else {
                PgType::Text
            }
        }
        // Objects and Arrays are stored as JSONB natively
        Value::Object(_) | Value::Array(_) => PgType::Jsonb,
    }
}

/// Represents a column schema derived from JSON
#[derive(Debug, Clone)]
pub struct InferredColumn {
    pub name: String,
    pub pg_type: PgType,
    pub is_nested: bool, // True if original value was Object/Array
    pub is_nullable: bool,
}

impl InferredColumn {
    pub fn new(name: String, pg_type: PgType, is_nested: bool) -> Self {
        Self {
            name,
            pg_type,
            is_nested,
            is_nullable: true, // All dynamically added columns are nullable
        }
    }
}

/// Infers the schema from a JSON object
///
/// # Arguments
/// * `value` - The JSON value (must be an object)
///
/// # Returns
/// A vector of inferred columns, or an error if the value is not an object
pub fn infer_schema(value: &Value) -> StackhouseResult<Vec<InferredColumn>> {
    let obj = value.as_object().ok_or_else(|| {
        StackhouseError::InvalidPayload("Payload must be a JSON object".to_string())
    })?;

    let columns: Vec<InferredColumn> = obj
        .iter()
        .filter(|(_, v)| !v.is_null()) // Skip null values for column creation
        .map(|(key, val)| {
            let is_nested = matches!(val, Value::Object(_) | Value::Array(_));
            InferredColumn::new(key.clone(), infer_type(val), is_nested)
        })
        .collect();

    Ok(columns)
}

/// Validates a batch of JSON values and infers a unified schema
///
/// # Arguments
/// * `values` - A vector of JSON values (all must be objects)
///
/// # Returns
/// A unified schema that can accommodate all values
pub fn infer_batch_schema(values: &[Value]) -> StackhouseResult<Vec<InferredColumn>> {
    if values.is_empty() {
        return Ok(vec![]);
    }

    let mut unified_columns: std::collections::HashMap<String, InferredColumn> =
        std::collections::HashMap::new();

    for value in values {
        let columns = infer_schema(value)?;
        for col in columns {
            unified_columns
                .entry(col.name.clone())
                .and_modify(|existing| {
                    // Promote type if needed
                    existing.pg_type = PgType::common_type(&existing.pg_type, &col.pg_type);
                    existing.is_nested = existing.is_nested || col.is_nested;
                })
                .or_insert(col);
        }
    }

    Ok(unified_columns.into_values().collect())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_type_inference() {
        assert_eq!(infer_type(&json!(42)), PgType::BigInt);
        assert_eq!(infer_type(&json!(3.14)), PgType::DoublePrecision);
        assert_eq!(infer_type(&json!("hello")), PgType::Text);
        assert_eq!(infer_type(&json!("2024-12-25")), PgType::Date);
        assert_eq!(
            infer_type(&json!("2024-12-25T10:30:00Z")),
            PgType::TimestampTz
        );
        assert_eq!(
            infer_type(&json!("550e8400-e29b-41d4-a716-446655440000")),
            PgType::Uuid
        );
        assert_eq!(infer_type(&json!(true)), PgType::Boolean);
        assert_eq!(infer_type(&json!(null)), PgType::Null);
        assert_eq!(infer_type(&json!({"nested": "object"})), PgType::Jsonb);
        assert_eq!(infer_type(&json!([1, 2, 3])), PgType::Jsonb);
    }

    #[test]
    fn test_schema_inference() {
        let payload = json!({
            "name": "Stackhouse",
            "version": 1,
            "rating": 9.5,
            "is_awesome": true,
            "metadata": {"key": "value"}
        });

        let schema = infer_schema(&payload).unwrap();
        assert_eq!(schema.len(), 5);
    }

    #[test]
    fn test_type_promotion() {
        assert!(PgType::BigInt.can_promote_to(&PgType::DoublePrecision));
        assert!(PgType::BigInt.can_promote_to(&PgType::Text));
        assert!(!PgType::Text.can_promote_to(&PgType::BigInt));
        assert!(PgType::Null.can_promote_to(&PgType::BigInt));

        assert!(PgType::Date.can_promote_to(&PgType::TimestampTz));
        assert!(PgType::Uuid.can_promote_to(&PgType::Jsonb));
        assert!(PgType::TimestampTz.can_promote_to(&PgType::Text));
        assert!(!PgType::Text.can_promote_to(&PgType::Uuid));
    }

    #[test]
    fn test_using_cast_expressions_are_safe() {
        assert_eq!(
            PgType::DoublePrecision.using_cast_expr(&PgType::BigInt, "amount"),
            "amount::double precision"
        );
        assert_eq!(
            PgType::Text.using_cast_expr(&PgType::BigInt, "amount"),
            "amount::text"
        );
        assert_eq!(
            PgType::Jsonb.using_cast_expr(&PgType::BigInt, "amount"),
            "to_jsonb(amount)"
        );
        assert_eq!(
            PgType::Jsonb.using_cast_expr(&PgType::Text, "note"),
            "to_jsonb(note)"
        );
        assert_eq!(
            PgType::Text.using_cast_expr(&PgType::Jsonb, "note"),
            "note #>> ARRAY[]::text[]"
        );
        assert_eq!(
            PgType::Jsonb.using_cast_expr(&PgType::Jsonb, "note"),
            "note"
        );
        assert_eq!(
            PgType::TimestampTz.using_cast_expr(&PgType::Date, "d"),
            "d::timestamptz"
        );
        assert_eq!(
            PgType::Uuid.using_cast_expr(&PgType::Text, "id"),
            "id::uuid"
        );
        assert_eq!(
            PgType::Text.using_cast_expr(&PgType::TimestampTz, "d"),
            "d::text"
        );
    }

    #[test]
    fn test_from_data_type_mapping() {
        assert_eq!(PgType::from_data_type("bigint"), Some(PgType::BigInt));
        assert_eq!(
            PgType::from_data_type("double precision"),
            Some(PgType::DoublePrecision)
        );
        assert_eq!(PgType::from_data_type("text"), Some(PgType::Text));
        assert_eq!(PgType::from_data_type("jsonb"), Some(PgType::Jsonb));
        assert_eq!(PgType::from_data_type("date"), Some(PgType::Date));
        assert_eq!(
            PgType::from_data_type("timestamp with time zone"),
            Some(PgType::TimestampTz)
        );
        assert_eq!(PgType::from_data_type("uuid"), Some(PgType::Uuid));
        assert_eq!(PgType::from_data_type("unknown_type"), None);
    }
}
