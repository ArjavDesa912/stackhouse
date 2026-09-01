# 03 - Architecture

## 🏗️ Stackhouse System Architecture

```
┌─────────────────────────────────────────────────────────────┐
│                                                             │
│     ╔══════════════════════════════════════════════════╗   │
│     ║                                                  ║   │
│     ║     Understanding How Stackhouse Works Under the    ║   │
│     ║                   Hood                             ║   │
│     ║                                                  ║   │
│     ╚══════════════════════════════════════════════════╝   │
│                                                             │
└─────────────────────────────────────────────────────────────┘
```

## Table of Contents
- [High-Level Architecture](#high-level-architecture)
- [Component Overview](#component-overview)
- [Data Flow](#data-flow)
- [Request Processing Flow](#request-processing-flow)

---

> **Correction:** earlier versions of this page described a custom in-process
> LSM-tree storage engine ("Stackhouse-Core": WAL/MemTable/SSTable/compaction files
> under `src/stackhouse_core/`). No such module exists in this codebase — it was
> aspirational/fictional documentation. Stackhouse stores all relational data in
> **PostgreSQL** via `sqlx` (`stackhouse/src/platform/db.rs`, `StackhouseStore`). The
> diagrams below have been corrected to reflect the actual implementation.

## High-Level Architecture

```
┌─────────────────────────────────────────────────────────────┐
│                    STACKHOUSE ARCHITECTURE                       │
├─────────────────────────────────────────────────────────────┤
│                                                              │
│  ┌─────────────────────────────────────────────────────┐   │
│  │              CLIENT LAYER                             │   │
│  │  HTTP Clients │ WebSocket │ Web Dashboard           │   │
│  └─────────────────────────────────────────────────────┘   │
│                         ↓                                   │
│  ┌─────────────────────────────────────────────────────┐   │
│  │              API LAYER (Axum)                       │   │
│  │  REST API │ WebSocket │ SSE │ Middleware            │   │
│  └─────────────────────────────────────────────────────┘   │
│                         ↓                                   │
│  ┌─────────────────────────────────────────────────────┐   │
│  │              BUSINESS LOGIC LAYER                   │   │
│  │  Auth │ Security (RLS) │ Schema-Later Guard         │   │
│  └─────────────────────────────────────────────────────┘   │
│                         ↓                                   │
│  ┌─────────────────────────────────────────────────────┐   │
│  │              StackhouseStore (sqlx PgPool)                │   │
│  └─────────────────────────────────────────────────────┘   │
│                         ↓                                   │
│  ┌─────────────────────────────────────────────────────┐   │
│  │              PostgreSQL                              │   │
│  └─────────────────────────────────────────────────────┘   │
│                                                              │
│  Adjacent services reached over their own APIs, not part    │
│  of the write/read path above:                              │
│  • Qdrant (vector search, HTTP)                              │
│  • boa_engine JS runtime (REST router mounted under          │
│    `/v1/functions` in `main.rs`)                             │
│  • Object storage subsystem (buckets/objects in Postgres,    │
│    S3-compatible API, CDN, tus resumable uploads)            │
│                                                              │
└─────────────────────────────────────────────────────────────┘
```

---

## Component Overview

### 1. API Layer

**Location:** `stackhouse/src/api/` (`handlers.rs`, `routes.rs`, `admin.rs`,
`dashboard.rs`, `graphql.rs`, `openapi.rs`, `mcp_server.rs`,
`auto_rest.rs`, `versioned_api.rs`, `platform.rs`) — not a single `api.rs` file.

Handles all incoming HTTP/WebSocket requests.

```
Request → Router → Middleware → Handlers → Response
   ↓           ↓         ↓          ↓         ↓
 Client    Axum    Auth     Logic     JSON
```

### 2. Data Storage

**Location:** `stackhouse/src/platform/db.rs` (`StackhouseStore`)

A `sqlx::PgPool`-backed wrapper providing `execute`, `query`, `query_simple`,
`insert_returning_id`, and simple `insert`/`scan`/`delete` helpers. See
[Storage Engine](./10-Storage-Engine.md) for the full breakdown, including the
separate Schema-Later Guard (auto schema evolution) and versioned migration
service.

### 3. Data Flow

#### Write Path
```
┌─────────────────────────────────────────────────────────────┐
│                    WRITE PATH                                │
├─────────────────────────────────────────────────────────────┤
│                                                              │
│  1. Client Request (e.g. POST /v1/push/users)               │
│                  ↓                                           │
│  2. API validation / auth / RLS context injection            │
│                  ↓                                           │
│  3. Schema-Later Guard: diff payload keys vs. cached schema, │
│     ALTER TABLE ADD COLUMN for any new fields                │
│                  ↓                                           │
│  4. INSERT via StackhouseStore (sqlx) → PostgreSQL                 │
│                  ↓                                           │
│  5. Response to client                                       │
│                                                              │
└─────────────────────────────────────────────────────────────┘
```

#### Read Path
```
┌─────────────────────────────────────────────────────────────┐
│                    READ PATH                                 │
├─────────────────────────────────────────────────────────────┤
│                                                              │
│  1. Client Request (GET /v1/query/users?...)                │
│                  ↓                                           │
│  2. API validation / auth / RLS context injection            │
│                  ↓                                           │
│  3. Build SELECT with WHERE (equality filters from query     │
│     params), ORDER BY, LIMIT/OFFSET — see Querying           │
│                  ↓                                           │
│  4. Execute via StackhouseStore (sqlx) → PostgreSQL                │
│                  ↓                                           │
│  5. Return Result                                            │
│                                                              │
└─────────────────────────────────────────────────────────────┘
```

---

## Request Processing Flow

### HTTP Request Flow

```
Client Request
     ↓
Axum Router
     ↓
┌─────────────────────────────────────┐
│ Middleware Stack                   │
├─────────────────────────────────────┤
│ 1. CORS                            │
│ 2. Logging                         │
│ 3. Authentication (JWT validation)  │
│ 4. Row-Level Security (policy check)│
└─────────────────────────────────────┘
     ↓
Handler (api/handlers.rs)
     ↓
┌─────────────────────────────────────┐
│ Processing                         │
├─────────────────────────────────────┤
│ • Schema-Later Guard (auto-evolve) │
│ • Type inference                   │
│ • Business logic                   │
└─────────────────────────────────────┘
     ↓
StackhouseStore (platform/db.rs)
     ↓
PostgreSQL
     ↓
Response
```

---

## Component Communication

```
┌─────────────────────────────────────────────────────────────┐
│           COMPONENT INTERACTIONS                             │
├─────────────────────────────────────────────────────────────┤
│                                                              │
│  api/handlers.rs  ←→  auth/*             (JWT validation)   │
│  api/handlers.rs  ←→  security/guard.rs  (RLS + auto-evolve)│
│  api/handlers.rs  ←→  platform/db.rs     (database ops)     │
│  api/handlers.rs  ←→  realtime/mod.rs    (WebSocket, via    │
│                                            LISTEN/NOTIFY)    │
│                                                              │
│  platform/db.rs   ←→  db/schema_migrations.rs (versioned    │
│                                                  migrations) │
│  security/guard.rs ←→ inference.rs       (type inference)   │
│                                                              │
│  storage/vectors.rs ←→ Qdrant (HTTP)     (vector search)    │
│  compute/functions.rs ←→ boa_engine      (JS execution —    │
│    REST router mounted under `/v1/functions`)               │
│    (see JavaScript Functions docs)                          │
│                                                              │
└─────────────────────────────────────────────────────────────┘
```

---

## Concurrency Model

```
┌─────────────────────────────────────────────────────────────┐
│              CONCURRENCY & THREADING                          │
├─────────────────────────────────────────────────────────────┤
│                                                              │
│  Tokio Runtime (Async, multi-threaded)                       │
│  ┌──────────────────────────────────────┐                  │
│  │  Worker Threads                      │                  │
│  │  ┌────┐ ┌────┐ ┌────┐ ┌────┐       │                  │
│  │  │ T1 │ │ T2 │ │ T3 │ │ T4 │       │                  │
│  │  └────┘ └────┘ └────┘ └────┘       │                  │
│  │       ↓                              │                  │
│  │  Task Scheduling                    │                  │
│  └──────────────────────────────────────┘                  │
│                                                              │
│  Concurrency-relevant components:                             │
│  • Postgres connection pool: sqlx PgPool (default 20 max     │
│    connections, 3s acquire timeout)                          │
│  • Schema cache: DashMap (security/guard.rs)                 │
│  • Realtime fan-out: DashMap of tokio::sync::broadcast        │
│    channels, one per subscribed table                        │
│                                                              │
└─────────────────────────────────────────────────────────────┘
```

---

## Key Design Decisions

### 1. Why PostgreSQL, Not a Custom Engine

Stackhouse deliberately does not implement its own storage engine. Data
durability, MVCC, indexing, and query execution are delegated entirely to
PostgreSQL; Stackhouse's own code is the "schema-later" layer on top —
automatic `ALTER TABLE` on new JSON fields, RLS policy management, and the
REST/GraphQL/WebSocket surface — rather than a database kernel.

### 2. Async Architecture

```
Why Tokio Async?
• High concurrency without threads
• Efficient I/O operations
• Better resource utilization
• Scalable to thousands of connections
```

---

## Extension Points

Real, verified extension points in the current codebase:

```
┌─────────────────────────────────────────────────────────────┐
│                  EXTENSION POINTS                            │
├─────────────────────────────────────────────────────────────┤
│                                                              │
│  Row-Level Security Policies:                                │
│  Defined per-table via the RLS API (/v1/rls), enforced by    │
│  security/guard.rs on each request                            │
│                                                              │
│  JavaScript Functions (implemented but NOT currently          │
│  reachable):                                                  │
│  compute/functions.rs implements deploy/invoke via            │
│  boa_engine and defines create_functions_router(), but that  │
│  router is never nested into the app in main.rs, and the     │
│  FunctionsService instance built there is never otherwise     │
│  used — there is no live HTTP path to this feature today.    │
│  Wiring it up (a one-line `.nest(...)` in main.rs) is a      │
│  prerequisite for this to work end-to-end.                    │
│                                                              │
└─────────────────────────────────────────────────────────────┘
```

There is no `StorageEngine`/`AuthService` trait or `PolicyEngine` type in
this codebase — those were aspirational claims in an earlier version of this
page, not real extension mechanisms.

---

## Performance Characteristics

No per-layer latency numbers are published here — actual performance is
governed by the underlying PostgreSQL deployment and, for vector search, by
the external Qdrant deployment, not by fixed constants in Stackhouse's own code.
Measure against your own deployment rather than relying on any previously
quoted figures on this page.

---

## Next Steps

To dive deeper into specific components:

- [Storage Engine](./10-Storage-Engine.md) - PostgreSQL-backed storage, schema evolution, object storage
- [Schema Evolution](./11-Schema-Evolution.md) - Auto-schema magic
- [Vector Search](./20-Vector-Search.md) - AI features (Qdrant-backed)
- [JavaScript Functions](./21-WASM-Functions.md) - Serverless compute

---

**Continue to [Quick Start](../02-Quick-Start.md) or back to [Index](./DOCS_INDEX.md)** 🚀
