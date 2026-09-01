use std::fs;

#[test]
fn identifier_validation_results_are_not_discarded() {
    for path in ["src/api/handlers.rs", "src/security/guard.rs"] {
        let source = fs::read_to_string(path).unwrap();
        for (index, line) in source.lines().enumerate() {
            assert!(
                !(line.contains("validate_identifier(") && line.contains(".ok()")),
                "{path}:{} must propagate identifier validation errors instead of discarding them",
                index + 1
            );
        }
    }
}

#[test]
fn schema_lookup_uses_bound_table_parameter() {
    let source = fs::read_to_string("src/security/guard.rs").unwrap();

    assert!(
        !source.contains("WHERE table_name = '{}'"),
        "information_schema table lookups must not interpolate table names"
    );
    assert!(
        source.contains("WHERE table_name = $1 AND table_schema = current_schema()"),
        "information_schema table lookup should bind the table name as a parameter"
    );
}

#[test]
fn dangerous_admin_sql_surfaces_remain_feature_gated() {
    let main_source = fs::read_to_string("src/main.rs").unwrap();
    let handler_source = fs::read_to_string("src/api/handlers.rs").unwrap();

    assert!(main_source.contains("env_flag(\"STACKHOUSE_ENABLE_RAW_SQL\")"));
    assert!(main_source.contains("env_flag(\"STACKHOUSE_ENABLE_DESTRUCTIVE_ADMIN\")"));
    assert!(handler_source.contains("Raw SQL access is disabled by default"));
    assert!(handler_source.contains("Destructive admin actions are disabled by default"));
}

#[test]
fn schema_evolution_adds_columns_idempotently() {
    let source = fs::read_to_string("src/security/guard.rs").unwrap();

    assert!(
        source.contains("ADD COLUMN IF NOT EXISTS"),
        "automatic schema evolution must tolerate concurrent writers adding the same column"
    );
}
