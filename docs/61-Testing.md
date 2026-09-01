# 61 - Testing

## 🧪 Testing Guide

### Running Tests

```bash
# Run all tests
cargo test

# Run specific test
cargo test test_wal_write_read

# Run with output
cargo test -- --nocapture

# Run tests in parallel
cargo test --release --test-threads=4
```

### Test Structure

```
stackhouse/
├── src/
│   └── stackhouse_core/
│       ├── wal.rs (contains #[cfg(test)] tests)
│       ├── memtable.rs (contains tests)
│       ├── sstable.rs (contains tests)
│       └── tests.rs (integration tests)
└── tests_final.rs
```

### Writing Tests

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn test_basic_operation() {
        let db = Stackhouse::new(tempdir().path()).unwrap();

        // Test write
        db.put(b"key", b"value").unwrap();

        // Test read
        let result = db.get(b"key").unwrap();
        assert_eq!(result, Some(b"value".to_vec()));
    }
}
```

### Integration Tests

```rust
#[tokio::test]
async fn test_api_endpoint() {
    let app = create_test_app().await;

    let response = app
        .oneshot(Request::builder()
            .uri("/v1/push/test")
            .body(Body::from(json!({"name": "Test"})))
            .unwrap())
        .await
        .unwrap();

    assert_eq!(response.status(), 200);
}
```

### Test Coverage

```bash
# Install tarpaulin
cargo install cargo-tarpaulin

# Generate coverage report
cargo tarpaulin --out Html

# View report
open tarpaulin-report/index.html
```

### Benchmarks

See [Benchmarks](./62-Benchmarks.md) for performance testing.

---

**Done!** 🎉
