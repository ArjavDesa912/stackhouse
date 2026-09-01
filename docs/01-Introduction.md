# 01 - Introduction to Stackhouse

## 🎯 What is Stackhouse?

```
┌─────────────────────────────────────────────────────────────┐
│                                                             │
│     ╔══════════════════════════════════════════════════╗   │
│     ║                                                  ║   │
│     ║   Stackhouse is a next-generation database that     ║   │
│     ║   evolves with your application, not against   ║   │
│     ║   it. Schema-later, AI-native, and realtime.   ║   │
│     ║                                                  ║   │
│     ╚══════════════════════════════════════════════════╝   │
│                                                             │
└─────────────────────────────────────────────────────────────┘
```

**Stackhouse** is an open-source, AI-native database that combines the flexibility of NoSQL with the power of SQL, vector search, and serverless compute—all in one unified system.

### 📊 The Problem Stackhouse Solves

```
┌──────────────────────────────────────────────────────────────┐
│                 THE DEVELOPER PAIN PYRAMID                   │
├──────────────────────────────────────────────────────────────┤
│                                                              │
│                      ▲▲▲▲▲▲▲▲                               │
│                     ▲ MIGRATIONS ▲                          │
│                    ▲ Schema locks ▲                          │
│                   ▲ Downtime required ▲                      │
│                  ▲──────────────────▲                       │
│                 ▲  VECTOR SEARCH  ▲                         │
│                ▲  Separate service ▲                        │
│               ▲  High latency/cost ▲                        │
│              ▲────────────────────▲                         │
│             ▲   SERVERLESS COMPUTE ▲                        │
│            ▲   Cloud functions only ▲                       │
│           ▲   Vendor lock-in ▲                              │
│          ▲───────────────────▲                              │
│         ▲    REALTIME UPDATES ▲                             │
│        ▲    Multiple tools ▲                                 │
│       ▲    Complex setup ▲                                   │
│      ▲▶▶▶▶▶▶▶▶▶▶▶▶▶▶▶▶▲                                    │
│                                                              │
└──────────────────────────────────────────────────────────────┘

                    STACKHOUSE ELIMINATES ALL OF THIS
```

### ✨ The Stackhouse Solution

```
┌──────────────────────────────────────────────────────────────┐
│                   ONE DATABASE = EVERYTHING                   │
├──────────────────────────────────────────────────────────────┤
│                                                              │
│  ┌──────────────┐  ┌──────────────┐  ┌──────────────┐      │
│  │     Data     │  │   Vectors    │  │  Functions   │      │
│  │              │  │              │  │              │      │
│  │  ✅ JSON     │  │  ✅ HNSW     │  │  ✅ Boa      │      │
│  │  ✅ SQL      │  │  ✅ Semantic │  │  ✅ Custom   │      │
│  │  ✅ Auto-Rels│  │  ✅ Fast     │  │  ✅ Sandboxed│      │
│  └──────────────┘  └──────────────┘  └──────────────┘      │
│         │                  │                  │             │
│         └──────────────────┴──────────────────┘             │
│                            ↓                                 │
│              ┌─────────────────────────────┐                │
│              │     STACKHOUSE UNIFIED API      │                │
│              └─────────────────────────────┘                │
│                              ↓                               │
│              ┌─────────────────────────────┐                │
│              │   Realtime + Secure + Fast  │                │
│              └─────────────────────────────┘                │
│                                                              │
└──────────────────────────────────────────────────────────────┘
```

---

## 🌟 Core Philosophy

### 1️⃣ Schema-Later™

Don't plan your schema upfront. Let it evolve naturally.

```diff
┌─────────────────────────────────────────────────────────────┐
│                    TRADITIONAL DATABASE                     │
├─────────────────────────────────────────────────────────────┤
│                                                              │
│  Day 1:   Define Schema ──→ Wait for migration ──→ Deploy  │
│  Day 7:   Add Field ──────→ Write migration ──→ Deploy     │
│  Day 30:  Restructure ────→ Complex migration ─→ Deploy    │
│  Day 100: Performance ───→ Re-index everything ─→ Deploy   │
│                                                              │
│  Result: Development slows down as schema grows            │
└─────────────────────────────────────────────────────────────┘

┌─────────────────────────────────────────────────────────────┐
│                      STACKHOUSE                                 │
├─────────────────────────────────────────────────────────────┤
│                                                              │
│  Day 1:   Push JSON ──→ ✅ Works immediately                │
│  Day 7:   Push JSON ──→ ✅ Schema auto-evolves              │
│  Day 30:  Push JSON ──→ ✅ Handles any structure            │
│  Day 100: Query ────→ ✅ Optimized automatically            │
│                                                              │
│  Result: Development stays fast forever                     │
└─────────────────────────────────────────────────────────────┘
```

**Visual Example:**
```
┌──────────────────────────────────────────────────────────┐
│                  REQUEST TIMELINE                         │
├──────────────────────────────────────────────────────────┤
│                                                           │
│  Request 1:                                              │
│  POST /users {name: "Alice", age: 25}                    │
│  ↓                                                       │
│  ┌──────────────────────────────────────┐               │
│  │ CREATE TABLE users (                 │               │
│  │   name TEXT,                         │               │
│  │   age INTEGER                        │               │
│  │ );                                   │               │
│  └──────────────────────────────────────┘               │
│                                                           │
│  Request 2:                                              │
│  POST /users {name: "Bob", age: 30, email: "bob@..."}    │
│  ↓                                                       │
│  ┌──────────────────────────────────────┐               │
│  │ ALTER TABLE users                    │               │
│  │ ADD COLUMN email TEXT;               │               │
│  └──────────────────────────────────────┘               │
│                                                           │
│  Request 3:                                              │
│  POST /users {name: "Carol", preferences: {theme: "dark"}}│
│  ↓                                                       │
│  ┌──────────────────────────────────────┐               │
│  │ ALTER TABLE users                    │               │
│  │ ADD COLUMN IF NOT EXISTS             │               │
│  │   preferences JSONB;                 │               │
│  └──────────────────────────────────────┘               │
│                                                           │
└──────────────────────────────────────────────────────────┘
```

### 2️⃣ AI-Native

Vector search is a first-class, unified API — not a bolt-on you have to wire
up yourself — even though under the hood it proxies to a dedicated Qdrant
instance rather than an in-process index (see [Vector Search](./20-Vector-Search.md)).

```
┌─────────────────────────────────────────────────────────────┐
│                   AI WORKFLOW COMPARISON                     │
├─────────────────────────────────────────────────────────────┤
│                                                              │
│  Traditional Approach:                                       │
│  ┌──────────┐   ┌──────────┐   ┌──────────┐   ┌──────────┐│
│  │Database  │──▶│ Export   │──▶│Pinecone/ │──▶│ Query    ││
│  │          │   │ Data     │   │Weaviate  │   │          ││
│  └──────────┘   └──────────┘   └──────────┘   └──────────┘│
│                      ↑                                   │
│                  3 separate services                     │
│                  High latency, high cost                 │
│                                                              │
│  Stackhouse Approach:                                           │
│  ┌──────────┐   ┌──────────┐                               │
│  │Database  │──▶│Stackhouse    │──▶ Results!                 │
│  │+Vectors  │   │Built-in  │                               │
│  └──────────┘   └──────────┘                               │
│                     ↑                                      │
│                 One service                                │
│                 Zero latency, lower cost                   │
│                                                              │
└─────────────────────────────────────────────────────────────┘
```

**Semantic Search Example:**
```
Query: "How do I reset my password?"

         ↓
    [Embedding Model]
         ↓
[0.23, -0.45, 0.67, ...]
         ↓
    ┌─────────────────────────────────┐
    │  Stackhouse Vector API (Qdrant)     │
    ├─────────────────────────────────┤
    │ • HNSW Algorithm                │
    │ • O(log n) Search               │
    │ • Reached via Stackhouse's REST API,│
    │   backed by an external Qdrant  │
    │   instance                      │
    └─────────────────────────────────┘
         ↓
    Top Results:
    1. "Reset Password Guide" (96% match)
    2. "Account Recovery" (89% match)
    3. "Login Issues" (76% match)
```

### 3️⃣ Realtime 2.0

Bi-directional WebSocket communication, not just server-sent events.

```
┌─────────────────────────────────────────────────────────────┐
│                REALTIME ARCHITECTURE                         │
├─────────────────────────────────────────────────────────────┤
│                                                              │
│   Client                                      Stackhouse        │
│     │                                           │           │
│     │ ──── WebSocket Connection ─────────────▶  │           │
│     │      GET /v1/realtime                    │           │
│     │                                           │           │
│     │ ── {"type":"subscribe","table":"users",  │           │
│     │     "event":"*"} ──────────────────────▶  │           │
│     │                                           │           │
│     │ ── {"type":"subscribe","table":"docs",   │           │
│     │     "event":"INSERT"} ─────────────────▶  │           │
│     │                                           │           │
│     │ ←─ {"type":"INSERT","table":"users",     │           │
│     │     "record":{...}} ──────────────────────┤           │
│     │                                           │           │
│     │ ←─ {"type":"INSERT","table":"docs",      │           │
│     │     "record":{...}} ──────────────────────┤           │
│     │                                           │           │
└─────────────────────────────────────────────────────────────┘

Benefits:
✅ Bidirectional subscribe/unsubscribe control messages
✅ Multiple table subscriptions per connection
✅ Server-initiated push on INSERT/UPDATE/DELETE

Note: this channel is for table-change subscriptions only — there is no
arbitrary SQL query-over-WebSocket capability; use the REST query/SQL
endpoints for that.
```

---

## 🏗️ Architecture Overview

```
┌──────────────────────────────────────────────────────────────┐
│                    STACKHOUSE ARCHITECTURE                       │
├──────────────────────────────────────────────────────────────┤
│                                                              │
│  ┌─────────────────────────────────────────────────────┐   │
│  │              APPLICATION LAYER                       │   │
│  │  REST API │ WebSocket API │ SSE │ Web Dashboard     │   │
│  └─────────────────────────────────────────────────────┘   │
│                            ↓                                 │
│  ┌─────────────────────────────────────────────────────┐   │
│  │              SECURITY LAYER                          │   │
│  │  JWT Auth │ Row-Level Security │ API Keys           │   │
│  └─────────────────────────────────────────────────────┘   │
│                            ↓                                 │
│  ┌─────────────────────────────────────────────────────┐   │
│  │              CORE LAYER                              │   │
│  │  ┌──────────────┐  ┌──────────────┐  ┌───────────┐ │   │
│  │  │ Schema-Later │  │  Vector      │  │   Boa      │ │   │
│  │  │   Guard      │  │   Search     │  │  Engine   │ │   │
│  │  │  • Auto      │  │  • Qdrant-   │  │  • JS/TS  │ │   │
│  │  │    ALTER     │  │    backed    │  │    exec   │ │   │
│  │  │    TABLE     │  │    HNSW/ANN  │  │  • Not yet│ │   │
│  │  │  • Type      │  │    (external │  │    wired  │ │   │
│  │  │    inference │  │    service)  │  │    to a   │ │   │
│  │  │              │  │              │  │    route  │ │   │
│  │  └──────────────┘  └──────────────┘  └───────────┘ │   │
│  └─────────────────────────────────────────────────────┘   │
│                            ↓                                 │
│  ┌─────────────────────────────────────────────────────┐   │
│  │              STORAGE LAYER                            │   │
│  │  PostgreSQL (sqlx) │ Object Storage │ Replication    │   │
│  └─────────────────────────────────────────────────────┘   │
│                                                              │
└──────────────────────────────────────────────────────────────┘
```

> All data lives in **PostgreSQL** — Stackhouse does not implement its own
> storage engine. See [Architecture](./03-Architecture.md) and
> [Storage Engine](./10-Storage-Engine.md) for the verified breakdown. The
> JavaScript function runtime (`compute/functions.rs`, powered by
> `boa_engine`) is implemented and its HTTP router is mounted under
> `/v1/functions` in `main.rs`.

---

## 🎯 Key Features Deep Dive

### 1. Automatic Schema Evolution

```
┌─────────────────────────────────────────────────────────────┐
│          SCHEMA EVOLUTION IN ACTION                          │
├─────────────────────────────────────────────────────────────┤
│                                                              │
│  Initial State: Empty Database                              │
│  ┌──────────────────────────────────────────────┐          │
│  │  No tables, no schema                        │          │
│  └──────────────────────────────────────────────┘          │
│                                                              │
│  Step 1: Insert User Document                                │
│  POST /users {                                               │
│    "name": "Alice",                                          │
│    "age": 25                                                 │
│  }                                                           │
│  ↓                                                           │
│  Stackhouse infers:                                              │
│  ┌──────────────────────────────────────────────┐          │
│  │  CREATE TABLE IF NOT EXISTS users (          │          │
│  │    id BIGSERIAL PRIMARY KEY,                 │          │
│  │    created_at TIMESTAMPTZ DEFAULT NOW(),     │          │
│  │    updated_at TIMESTAMPTZ DEFAULT NOW()      │          │
│  │  );                                          │          │
│  │  ALTER TABLE users ADD COLUMN name TEXT;     │          │
│  │  ALTER TABLE users ADD COLUMN age BIGINT;    │          │
│  └──────────────────────────────────────────────┘          │
│                                                              │
│  Step 2: Insert Document with New Field                     │
│  POST /users {                                               │
│    "name": "Bob",                                            │
│    "email": "bob@example.com"  ← NEW FIELD                  │
│  }                                                           │
│  ↓                                                           │
│  Stackhouse evolves:                                             │
│  ┌──────────────────────────────────────────────┐          │
│  │  ALTER TABLE users                           │          │
│  │  ADD COLUMN email TEXT;                     │          │
│  └──────────────────────────────────────────────┘          │
│                                                              │
│  Step 3: Insert Complex Nested Object                       │
│  POST /users {                                               │
│    "name": "Carol",                                          │
│    "settings": {                                             │
│      "theme": "dark",                                        │
│      "notifications": true                                   │
│    }                                                         │
│  }                                                           │
│  ↓                                                           │
│  Stackhouse adapts:                                              │
│  ┌──────────────────────────────────────────────┐          │
│  │  ALTER TABLE users                           │          │
│  │  ADD COLUMN IF NOT EXISTS settings JSONB;   │          │
│  └──────────────────────────────────────────────┘          │
│                                                              │
│  Result: Zero downtime, zero manual migrations!            │
└─────────────────────────────────────────────────────────────┘
```

### 2. PostgreSQL-Backed Storage

```
┌─────────────────────────────────────────────────────────────┐
│                  STACKHOUSE DATA PATH                             │
├─────────────────────────────────────────────────────────────┤
│                                                              │
│  Write:                                                      │
│  ┌────────┐   ┌──────────────┐   ┌──────────┐   ┌────────┐ │
│  │ Client │──▶│ Schema-Later │──▶│ StackhouseStore│──▶│Postgres│ │
│  │        │   │ Guard (auto  │   │ (sqlx)   │   │        │ │
│  │        │   │ ALTER TABLE) │   │          │   │        │ │
│  └────────┘   └──────────────┘   └──────────┘   └────────┘ │
│                                                              │
│  Read:                                                       │
│  ┌────────┐   ┌──────────────┐   ┌──────────┐   ┌────────┐ │
│  │ Client │──▶│ Filter/order │──▶│ StackhouseStore│──▶│Postgres│ │
│  │        │   │ from query   │──▶│ (sqlx)   │   │        │ │
│  │        │   │ params       │   │          │   │        │ │
│  └────────┘   └──────────────┘   └──────────┘   └────────┘ │
│                                                              │
│  Durability, MVCC, indexing, and caching are all handled by │
│  PostgreSQL itself — Stackhouse does not implement a custom      │
│  storage engine, WAL, or compaction of its own.               │
│                                                              │
└─────────────────────────────────────────────────────────────┘
```

### 3. Vector Similarity Search

```
┌─────────────────────────────────────────────────────────────┐
│           VECTOR SEARCH PIPELINE                             │
├─────────────────────────────────────────────────────────────┤
│                                                              │
│  Input: Text document                                        │
│  "The quick brown fox jumps over the lazy dog"              │
│                         ↓                                    │
│  [Embedding Model - e.g., sentence-transformers]            │
│                         ↓                                    │
│  Vector: [0.23, -0.45, 0.67, 0.12, ..., 0.89]              │
│              (384 dimensions for example)                   │
│                         ↓                                    │
│  ┌────────────────────────────────────────────┐            │
│  │         INSERT INTO Stackhouse                 │            │
│  │  /v1/vectors/documents/upsert               │            │
│  │  {                                        │            │
│  │    "id": "doc1",                          │            │
│  │    "embedding": [0.23, -0.45, ...],       │            │
│  │    "data": {"title": "..."}               │            │
│  │  }                                        │            │
│  └────────────────────────────────────────────┘            │
│                         ↓                                    │
│  ┌────────────────────────────────────────────┐            │
│  │      HNSW INDEX BUILD (in Qdrant)          │            │
│  │  • Approximate Nearest Neighbor            │            │
│  │  • Hierarchical graph structure            │            │
│  │  • O(log n) search complexity              │            │
│  │  • Built/served by an external Qdrant      │            │
│  │    instance, not Stackhouse's own code         │            │
│  └────────────────────────────────────────────┘            │
│                                                              │
│  Query Time:                                                │
│  Input: "What did the fox do?"                              │
│         ↓                                                    │
│  [Same Embedding Model]                                      │
│         ↓                                                    │
│  Query Vector: [0.25, -0.43, 0.65, ...]                      │
│         ↓                                                    │
│  ┌────────────────────────────────────────────┐            │
│  │         VECTOR SEARCH                      │            │
│  │  /v1/vectors/documents/search              │            │
│  │  {                                        │            │
│  │    "vector": [0.25, -0.43, ...],           │            │
│  │    "top_k": 10  ← Top 10 results          │            │
│  │  }                                        │            │
│  └────────────────────────────────────────────┘            │
│                         ↓                                    │
│  Results (sorted by similarity):                            │
│  ┌────────────────────────────────────────────┐            │
│  │  1. doc1 (similarity: 0.95) ← Most similar│            │
│  │  2. doc15 (similarity: 0.87)               │            │
│  │  3. doc7 (similarity: 0.81)                │            │
│  │  ...                                      │            │
│  │  10. doc42 (similarity: 0.52)              │            │
│  └────────────────────────────────────────────┘            │
│                                                              │
│  Distance Metrics:                                          │
│  • Cosine Similarity (default) - Semantic similarity        │
│  • Euclidean (L2) Distance - Geometric distance              │
│  • Inner Product (Dot) - Magnitude-sensitive similarity      │
│                                                              │
│  Use Cases:                                                 │
│  ✅ Semantic search                                          │
│  ✅ Recommendation systems                                   │
│  ✅ Document similarity                                      │
│  ✅ Image search (via vision embeddings)                     │
│  ✅ Duplicate detection                                      │
│                                                              │
└─────────────────────────────────────────────────────────────┘
```

---

## 💪 Performance Characteristics

The specific comparison numbers previously shown here were not measured and
have been removed — they're not defensible. A few things worth knowing
instead, verified against the current code:

- **Stackhouse stores all relational data in PostgreSQL** (`stackhouse/src/platform/db.rs`,
  via `sqlx`). Its write/read throughput and latency are bounded by — not
  faster than — the underlying Postgres deployment, since every request goes
  through Postgres. There is no independent Stackhouse storage engine to benchmark
  separately from Postgres itself.
- **Schema changes**: new JSON fields on existing tables are handled without a
  manual migration step (`ALTER TABLE ... ADD COLUMN IF NOT EXISTS`, see
  [Schema Evolution](./11-Schema-Evolution.md)) — this part of the "instant
  schema changes" claim is real and code-verified.
- **Vector search** is proxied to an external **Qdrant** instance
  (`stackhouse/src/storage/vectors.rs`) — it is a real, working integration, but
  it is not "built into" Stackhouse's own storage layer any more than adding the
  `pgvector` extension is "built into" Postgres; it's an external ANN service
  reached over HTTP.

If you need real numbers, benchmark your own deployment — see
[Benchmarks](./62-Benchmarks.md) for what Stackhouse does measure and publish.

---

## 🎓 When to Use Stackhouse?

### ✅ Perfect For:

1. **Rapid Prototyping**
   - Changing requirements? No problem.
   - Unknown data model? Start anyway.
   - Quick iterations? Native workflow.

2. **AI/ML Applications**
   - Semantic search
   - RAG (Retrieval Augmented Generation)
   - Recommendation engines
   - Similarity matching

3. **Realtime Features**
   - Live dashboards
   - Collaborative apps
   - Notifications
   - Gaming leaderboards

4. **Serverless Compute** (implemented, not yet exposed via HTTP)
   - JS/TS execution via `boa_engine` exists in `compute/functions.rs`, but
     its router isn't mounted in `main.rs` yet — see
     [JavaScript Functions](./21-WASM-Functions.md) for current status
     before relying on this for a project today

### ⚠️ Consider Alternatives For:

1. **Legacy SQL Migrations**
   - If you have strict migration requirements
   - Consider: Postgres, MySQL

2. **Massive Analytics**
   - Petabyte-scale data warehousing
   - Consider: Snowflake, BigQuery

3. **Distributed Transactions**
   - Multi-region ACID transactions
   - Consider: CockroachDB, Spanner

---

## 🚀 What's Next?

Continue your journey:

- **[Quick Start Guide](./02-Quick-Start.md)** - Get Stackhouse running in 5 minutes
- **[Architecture Deep Dive](./03-Architecture.md)** - Understand how it works
- **[API Reference](./50-API-Reference.md)** - Explore the API

---

**Ready to start?** Continue to [Quick Start](./02-Quick-Start.md) 🚀
