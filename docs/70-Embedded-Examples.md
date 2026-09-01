# Stackhouse Read Replicas — Usage Examples

This document provides practical examples for using Stackhouse's read-replica routing
and failover system (`src/platform/replicas.rs`).

> **This page previously described a bespoke WAL-based leader/follower embedding
> API (`stackhouse::replica::{Follower, FollowerConfig}`, an embedded file-backed
> `StackhouseStore::new(data_path)`, a `/v1/wal/stream` SSE endpoint). None of that exists
> in the current codebase — Stackhouse is Postgres-backed, and the module that actually
> exists today is a **read-replica registry and read router**, not a data-replication
> engine. This page has been rewritten to match `platform/replicas.rs`.

## What this module actually does

`ReplicaService` does **not** replicate data itself. It assumes you already have a
Postgres primary and one or more Postgres read replicas set up via your own
infrastructure (e.g. cloud-managed streaming replication). Stackhouse's job is to:

1. Keep a registry of known nodes (primary/replica/standby) per tenant, persisted in
   a `stackhouse_replica_nodes` table.
2. Health-check each node every 30 seconds — **this is a plain TCP connect to
   `host:port`**, not a Postgres protocol check or a replication-lag query.
3. Round-robin read traffic across healthy replicas via `route_read()`, falling back
   to the primary if no replica is healthy.
4. Support manual failover (`promote_to_primary`) that flips roles in the registry
   and records a `FailoverEvent` — it does not reconfigure the underlying Postgres
   servers for you.

`replication_lag_ms` on a `ReplicaNode` is set once at registration and is **not**
automatically updated by the health checker — don't rely on it for real lag
monitoring today.

> **Important:** `ReplicaService`/`create_replicas_router` are public library APIs
> (`stackhouse::platform::replicas::*`) but are **not instantiated or mounted** by the
> default `stackhouse` server binary (`src/main.rs`) — there is currently no live
> `/v1/replicas/*` HTTP endpoint when you run the server as-is. To use this
> subsystem today you either embed it yourself (example below) or wire it into
> `main.rs`.

## Library Usage

```rust
use std::sync::Arc;
use stackhouse::db::StackhouseStore;
use stackhouse::platform::replicas::{ReplicaService, NodeRole};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let store = Arc::new(StackhouseStore::in_memory().await?); // or your real StackhouseStore
    let replicas = ReplicaService::new(store).await?;

    let tenant_id = 1_i64;

    // Register the primary
    replicas.register_node(
        tenant_id, "primary", "primary.db.internal", 5432, "postgres", "us-east-1",
        NodeRole::Primary,
    ).await?;

    // Register a read replica
    replicas.register_node(
        tenant_id, "replica-1", "replica1.db.internal", 5432, "postgres", "us-east-1",
        NodeRole::Replica,
    ).await?;

    // Route a read — returns a healthy replica, or the primary if none are healthy
    let node = replicas.route_read(tenant_id).await?;
    println!("Routing read to {}:{}", node.host, node.port);

    // Inspect aggregate stats
    let stats = replicas.get_stats(tenant_id).await;
    println!("{} replicas, avg lag {}ms", stats.replica_count, stats.avg_replication_lag_ms);

    Ok(())
}
```

## Mounting the REST API

If you want the HTTP surface, wire it up yourself (this is not done in
`src/main.rs` today):

```rust
use stackhouse::platform::replicas::{ReplicaService, ReplicaState, create_replicas_router};

let replica_state = ReplicaState { replicas: Arc::new(replicas), auth: auth_state };
let replicas_router = create_replicas_router(replica_state);
let app = app.nest("/v1/replicas", replicas_router);
```

### Endpoints (once mounted)

| Method | Path | Description |
| --- | --- | --- |
| POST | `/v1/replicas/nodes` | Register a node (`name`, `host`, `port`, `database`, `region`, `role`) |
| GET | `/v1/replicas/nodes` | List nodes for the authenticated tenant |
| POST | `/v1/replicas/nodes/:id/promote` | Promote a node to primary (manual failover) |
| DELETE | `/v1/replicas/nodes/:id` | Remove a node from the registry |
| GET | `/v1/replicas/stats` | Aggregate replication stats for the tenant |

All routes require authentication (`extract_auth_user`); the tenant is taken from
the authenticated user's ID.

**Register a node:**
```bash
curl -X POST http://localhost:8080/v1/replicas/nodes \
  -H "Authorization: Bearer $TOKEN" \
  -H "Content-Type: application/json" \
  -d '{
    "name": "replica-1",
    "host": "replica1.db.internal",
    "port": 5432,
    "database": "postgres",
    "region": "us-east-1",
    "role": "replica"
  }'
```

**Promote a replica to primary:**
```bash
curl -X POST http://localhost:8080/v1/replicas/nodes/<node_id>/promote \
  -H "Authorization: Bearer $TOKEN"
```

**Get stats:**
```bash
curl http://localhost:8080/v1/replicas/stats -H "Authorization: Bearer $TOKEN"
```

Response:
```json
{
  "success": true,
  "data": {
    "primary_id": "...",
    "replica_count": 1,
    "avg_replication_lag_ms": 0,
    "max_replication_lag_ms": 0,
    "total_reads_routed": 42,
    "reads_to_primary": 5,
    "reads_to_replicas": 37
  }
}
```

## Summary

This page covers:
- ✅ What `ReplicaService` actually is: a read-replica registry + round-robin read
  router over externally-provisioned Postgres nodes
- ✅ Its real limitations: TCP-only health checks, no automatic lag measurement,
  manual (not automatic) failover
- ✅ That it is mounted by the default server binary under `/v1/platform/replicas`
- ✅ REST endpoint examples for the live router

For deeper background on Stackhouse's replication story, see `33-Replication.md`.
