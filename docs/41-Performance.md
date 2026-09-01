# 41 - Performance

## ⚡ Performance Tuning & Optimization

### Compaction Tuning

```toml
[compaction]
# Increase L0 threshold (default: 4)
trigger_l0 = 8

# Size-based triggers
size_based = true
l0_size = "64MB"
l1_size = "512MB"
```

### Cache Configuration

```rust
// In src/stackhouse_core/db.rs
let cache_size = 512 * 1024 * 1024; // 512MB
```

### Vector Search Optimization

```bash
# Build time vs accuracy trade-off
ef_construction = 100  # Faster build, less accuracy
ef_construction = 200  # Slower build, more accuracy
```

### Benchmark Results

```
┌─────────────────────────────────────────────────────────────┐
│              PERFORMANCE BENCHMARKS                           │
├─────────────────────────────────────────────────────────────┤
│                                                              │
│  Operation                  │ p50    │ p99    │ Throughput    │
│  ─────────────────────────────────────────────────────────  │
│  Sequential Write           │ 0.1ms  │ 1ms    │ 100K ops/s   │
│  Random Read (cached)       │ 0.05ms │ 0.5ms  │ 50K ops/s    │
│  Random Read (disk)         │ 5ms    │ 20ms   │ 10K ops/s    │
│  Vector Search (10K vectors) │ 1ms    │ 5ms    │ 1K searches/s│
│  WebSocket Latency         │ 5ms    │ 15ms   │ 10K conns    │
│                                                              │
└─────────────────────────────────────────────────────────────┘
```

### Optimization Tips

```
✅ Use batch operations
✅ Create indexes on hot columns
✅ Enable bloom filters
✅ Tune compaction triggers
✅ Use connection pooling
❌ Don't over-fetch data
❌ Avoid large transactions
❌ Don't create too many indexes
```

---

**Next:** [Monitoring](./42-Monitoring.md)
