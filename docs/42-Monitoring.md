# 42 - Monitoring

## 📊 Monitoring & Observability

### Metrics Endpoint

```bash
curl http://localhost:3000/v1/metrics
```

**Response:**
```json
{
  "uptime_seconds": 3600,
  "connections": 42,
  "queries_per_second": 1250,
  "latency_p50_ms": 2.5,
  "latency_p99_ms": 15.2,
  "cache_hit_rate": 0.95,
  "compaction_queue_size": 0
}
```

### Key Metrics to Monitor

```
┌─────────────────────────────────────────────────────────────┐
│              KEY PERFORMANCE INDICATORS                        │
├─────────────────────────────────────────────────────────────┤
│                                                              │
│  Red Metrics (Alert on threshold):                          │
│  • p99 latency > 100ms                                       │
│  • Error rate > 1%                                           │
│  • Cache hit rate < 80%                                       │
│  • Compaction queue > 10                                     │
│                                                              │
│  Green Metrics (Track trends):                               │
│  • Query throughput                                          │
│  • Active connections                                        │
│  • Storage size                                              │
│  • Memory usage                                              │
│                                                              │
└─────────────────────────────────────────────────────────────┘
```

### Logging

```toml
[logging]
level = "info"  # debug, info, warn, error
format = "json"
output = "/var/log/stackhouse/app.log"
```

### Dashboard Integration

**Prometheus:**
```yaml
scrape_configs:
  - job_name: 'stackhouse'
    metrics_path: '/v1/metrics'
    static_configs:
      - targets: ['localhost:3000']
```

**Grafana:**
- Import dashboard from `contrib/grafana/`
- Pre-configured panels for all metrics

### Alerts

```yaml
# AlertManager rules
groups:
  - name: stackhouse
    rules:
      - alert: HighLatency
        expr: latency_p99_ms > 100
        for: 5m

      - alert: LowCacheHitRate
        expr: cache_hit_rate < 0.8
        for: 10m
```

---

**Next:** [Backup & Recovery](./43-Backup-Recovery.md)
