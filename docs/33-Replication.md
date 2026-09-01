# 33 - Replication

## Overview

Replication in the current codebase is implemented as a **read-replica registry and failover-management layer** built on top of PostgreSQL (`stackhouse/src/platform/replicas.rs`), not a custom WAL-streaming leader/follower engine. Stackhouse relies on Postgres's own physical/streaming replication to keep replica nodes in sync; this layer tracks which nodes exist, routes reads across them, and orchestrates promoting a replica to primary on failover.

> The read-replica router is mounted by default under `/v1/platform/replicas` in `stackhouse/src/main.rs`. The endpoints below are reachable over HTTP.

## Data Model

```rust
pub struct ReplicaNode {
    pub id: String,
    pub tenant_id: i64,
    pub name: String,
    pub host: String,
    pub port: u16,
    pub database: String,
    pub region: String,
    pub role: NodeRole,              // Primary | Replica | Standby
    pub status: NodeStatus,          // Healthy | Degraded | Unhealthy | Offline | Promoting
    pub replication_lag_ms: u64,
    pub connections_active: u32,
    pub connections_max: u32,
    pub last_health_check: Option<String>,
    pub created_at: String,
}
```

## Routes

All routes require an authenticated user (`extract_auth_user`); nodes are scoped per-tenant by the caller's user ID.

```
POST   /nodes            # Register a replica/standby node
GET    /nodes            # List nodes for the current tenant
POST   /nodes/:id/promote  # Promote a replica to primary (failover)
DELETE /nodes/:id        # Remove a node
GET    /stats            # Aggregate replication stats
```

### Register a Node

```bash
curl -X POST http://localhost:3000/v1/platform/replicas/nodes \
  -H "Authorization: Bearer <jwt_token>" \
  -H "Content-Type: application/json" \
  -d '{
    "name": "replica-us-east-2",
    "host": "replica2.db.internal",
    "port": 5432,
    "database": "postgres",
    "region": "us-east-2",
    "role": "replica"
  }'
```

`port` defaults to `5432`, `database` to `"postgres"`, `region` to `"us-east-1"`, `role` to `"replica"` (accepted values: `primary`, `replica`, `standby`) if omitted.

### Promote to Primary (Failover)

```bash
curl -X POST http://localhost:3000/v1/platform/replicas/nodes/<node_id>/promote \
  -H "Authorization: Bearer <jwt_token>"
```

Returns a `FailoverEvent`:

```json
{
  "success": true,
  "data": {
    "id": "...",
    "old_primary_id": "...",
    "new_primary_id": "...",
    "reason": "...",
    "duration_ms": 42,
    "timestamp": "2026-08-18T12:00:00Z"
  }
}
```

### Read Routing

`ReplicaService::route_read(tenant_id)` picks a healthy node to serve a read (round-robin across registered replicas, falling back toward the primary when no healthy replica is available). This is exposed as a library method for callers that want to route reads themselves; it is not currently invoked automatically by the main query path in `handlers.rs`.

### Stats

```bash
curl http://localhost:3000/v1/platform/replicas/stats \
  -H "Authorization: Bearer <jwt_token>"
```

```json
{
  "success": true,
  "data": {
    "primary_id": "...",
    "replica_count": 2,
    "avg_replication_lag_ms": 120,
    "max_replication_lag_ms": 340,
    "total_reads_routed": 1042,
    "reads_to_primary": 210,
    "reads_to_replicas": 832
  }
}
```

## Related: Change Data Capture

Trigger + `NOTIFY`-based CDC (`stackhouse/src/platform/cdc.rs`) and PITR backup/restore (`stackhouse/src/storage/backups/pitr.rs`) are separate subsystems documented in their own sections — see [43 - Backup & Recovery](./43-Backup-Recovery.md).

---

**Next:** [Billing](./34-Billing.md)
