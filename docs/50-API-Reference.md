# 50 - API Reference

## 📡 Complete REST API Documentation

```
┌─────────────────────────────────────────────────────────────┐
│                                                             │
│     ╔══════════════════════════════════════════════════╗   │
│     ║                                                  ║   │
│     ║         Every Endpoint, Explained               ║   │
│     ║                                                  ║   │
│     ╚══════════════════════════════════════════════════╝   │
│                                                             │
└─────────────────────────────────────────────────────────────┘
```

## 🔗 Base URL

```
Development: http://localhost:3000
Production:  https://your-domain.com
```

## 📋 Common Headers

```http
Content-Type: application/json
Authorization: Bearer <jwt_token>
```

## 📊 Standard Response Format

### Success Response
```json
{
  "success": true,
  "data": {...},
  "message": "Optional message"
}
```

### Error Response
```json
{
  "success": false,
  "error": "Error message",
  "code": "ERROR_CODE"
}
```

---

## 📝 Data Operations

### Insert Document

```http
POST /v1/push/:collection
```

**Description:** Insert a single document into a collection. Creates the collection automatically if it doesn't exist.

**Path Parameters:**
- `collection` (string) - Collection name

**Request Body:**
```json
{
  "field1": "value1",
  "field2": 123,
  "nested": {
    "key": "value"
  }
}
```

**Response:**
```json
{
  "success": true,
  "data": {
    "id": 1,
    "field1": "value1",
    "field2": 123,
    "nested": {"key": "value"},
    "created_at": "2025-01-03T12:00:00Z"
  }
}
```

**Example:**
```bash
curl -X POST http://localhost:3000/v1/push/users \
  -H "Content-Type: application/json" \
  -d '{
    "name": "Alice",
    "email": "alice@example.com",
    "age": 28
  }'
```

---

### Batch Insert

```http
POST /v1/push/:collection/batch
```

**Description:** Insert multiple documents in a single request.

**Request Body:**
```json
[
  {"name": "Alice", "age": 28},
  {"name": "Bob", "age": 35},
  {"name": "Charlie", "age": 42}
]
```

**Response:**
```json
{
  "success": true,
  "data": {
    "inserted": 3,
    "ids": [1, 2, 3]
  }
}
```

---

### Query Collection

```http
GET /v1/query/:collection
```

**Description:** Retrieve all documents from a collection.

**Query Parameters:**
- `limit` (number, optional) - Maximum number of results
- `offset` (number, optional) - Number of results to skip
- `order_by` (string, optional) - Field to order by
- `order_dir` (string, optional) - "ASC" or "DESC" (default: "ASC")

**Example:**
```bash
# Get all users
curl http://localhost:3000/v1/query/users

# Get first 10 users, ordered by age
curl "http://localhost:3000/v1/query/users?limit=10&order_by=age&order_dir=desc"
```

**Response:**
```json
{
  "success": true,
  "data": [
    {"id": 1, "name": "Alice", "age": 28},
    {"id": 2, "name": "Bob", "age": 35}
  ],
  "count": 2
}
```

---

### Get Document by ID

```http
GET /v1/query/:collection/:id
```

**Description:** Retrieve a specific document by ID.

**Example:**
```bash
curl http://localhost:3000/v1/query/users/1
```

**Response:**
```json
{
  "success": true,
  "data": {
    "id": 1,
    "name": "Alice",
    "age": 28
  }
}
```

---

### Update Document

```http
POST /v1/update/:collection/:id
```

**Description:** Update a specific document. Partial updates supported.

**Request Body:**
```json
{
  "age": 29,
  "city": "San Francisco"
}
```

**Example:**
```bash
curl -X POST http://localhost:3000/v1/update/users/1 \
  -H "Content-Type: application/json" \
  -d '{
    "age": 29,
    "city": "San Francisco"
  }'
```

**Response:**
```json
{
  "success": true,
  "data": {
    "id": 1,
    "name": "Alice",
    "age": 29,
    "city": "San Francisco"
  }
}
```

---

### Delete Document

```http
POST /v1/delete/:collection/:id
```

**Example:**
```bash
curl -X POST http://localhost:3000/v1/delete/users/1
```

**Response:**
```json
{
  "success": true,
  "message": "Document deleted successfully"
}
```

---

## 🗃️ Schema & Metadata

### List Tables

```http
GET /v1/tables
```

**Description:** List all collections/tables in the database.

**Response:**
```json
{
  "success": true,
  "data": ["users", "products", "orders"]
}
```

---

### Table Stats

```http
GET /v1/tables/:collection
```

**Description:** Get statistics about a collection.

**Response:**
```json
{
  "success": true,
  "data": {
    "name": "users",
    "row_count": 1250,
    "size_bytes": 524288,
    "indexes": ["id", "email"],
    "columns": {
      "id": "INTEGER",
      "name": "TEXT",
      "email": "TEXT",
      "age": "INTEGER",
      "created_at": "TIMESTAMP"
    },
    "created_at": "2025-01-01T00:00:00Z",
    "last_updated": "2025-01-03T12:00:00Z"
  }
}
```

---

### Creating Indexes

There is currently no dedicated REST endpoint for creating indexes. A `SchemaOp::CreateIndex` variant exists in `stackhouse/src/api/dashboard.rs`, but that module is never mounted into the Axum router in `stackhouse/src/main.rs` — it's dead code today. To create an index, use the raw SQL endpoint or a versioned schema migration:

```bash
curl -X POST http://localhost:3000/v1/sql/query \
  -H "Content-Type: application/json" \
  -d '{"query": "CREATE UNIQUE INDEX IF NOT EXISTS idx_users_email ON users (email)"}'
```

---

## 🔍 Vector Search

Vector search is backed by a Qdrant instance (`storage/vectors.rs`), not an
in-process index. Collections are created in Qdrant lazily on first upsert.

### Upsert Vector

```http
POST /v1/vectors/:collection/upsert
```

**Request Body:**
```json
{
  "id": "doc1",
  "embedding": [0.1, 0.2, 0.3, 0.4, 0.5],
  "data": {
    "title": "Document Title",
    "category": "tech"
  },
  "column": "embedding"
}
```

- `id` (string, optional) — omit to auto-generate a UUID.
- `embedding` (float array, required) — the vector.
- `data` (object, optional) — payload stored alongside the vector.
- `column` (string, optional, default `"embedding"`) — name of the vector field; used to select a named vector when a collection stores more than one per point.

**Example:**
```bash
curl -X POST http://localhost:3000/v1/vectors/documents/upsert \
  -H "Content-Type: application/json" \
  -d '{
    "id": "doc1",
    "embedding": [0.1, 0.2, 0.3, 0.4, 0.5],
    "data": {"title": "Introduction"}
  }'
```

**Response:** `201 Created`
```json
{
  "success": true,
  "data": {
    "id": "doc1",
    "collection": "documents",
    "dimensions": 5
  },
  "message": "Vector upserted successfully"
}
```

---

### Batch Upsert Vectors

```http
POST /v1/vectors/:collection/batch
```

**Request Body:**
```json
{
  "records": [
    { "id": "doc1", "embedding": [0.1, 0.2, 0.3], "data": {"title": "One"} },
    { "id": "doc2", "embedding": [0.4, 0.5, 0.6], "data": {"title": "Two"} }
  ]
}
```

**Response:** `201 Created`
```json
{
  "success": true,
  "data": {
    "ids": ["doc1", "doc2"],
    "collection": "documents",
    "count": 2
  },
  "message": "Vectors batch upserted successfully"
}
```

---

### Search Vectors

```http
POST /v1/vectors/:collection/search
```

**Request Body:**
```json
{
  "vector": [0.15, 0.25, 0.35, 0.45, 0.55],
  "top_k": 10,
  "metric": "cosine",
  "filters": { "category": "tech" },
  "column": "embedding"
}
```

- `vector` (float array, required) — the query vector.
- `top_k` (number, optional, default `10`) — number of results to return.
- `metric` (string, optional, default `"cosine"`) — one of `cosine`, `l2`, `inner_product` (aliases `dot`/`inner_product` also accepted).
- `filters` (object, optional) — payload filters passed through to Qdrant.
- `column` (string, optional, default `"embedding"`) — named vector to search.

**Example:**
```bash
curl -X POST http://localhost:3000/v1/vectors/documents/search \
  -H "Content-Type: application/json" \
  -d '{
    "vector": [0.15, 0.25, 0.35, 0.45, 0.55],
    "top_k": 5
  }'
```

**Response:**
```json
{
  "success": true,
  "data": [
    {
      "id": "doc1",
      "similarity": 0.92,
      "data": {"title": "Introduction"}
    }
  ],
  "count": 1,
  "collection": "documents",
  "metric": "cosine"
}
```

Note the result field is `similarity` (a score, not a distance), and each hit's payload is under `data`, not `metadata`.

---

### Vector Collection Info

```http
GET /v1/vectors/:collection/info
```

**Response:**
```json
{
  "success": true,
  "data": {
    "table": "documents",
    "column": "embedding",
    "dimensions": 5,
    "index_type": "hnsw",
    "row_count": 1042
  },
  "collection": "documents"
}
```

There is no endpoint to list all vector collections/indexes (no `GET /v1/vectors`); query `/v1/vectors/:collection/info` for a known collection instead.

---

## ⚡ JavaScript Functions

The `/v1/functions/*` router is mounted in `main.rs` and live.

### Deploy Function

```http
POST /v1/functions/deploy
```

**Content-Type:** `application/json`

**Description:** Deploy a function. The `runtime` field accepts `javascript` (default), `typescript`, `wasm_rust`, or `wasm_js`, and the `entrypoint` defaults to `handler` — but this is currently metadata only: every runtime value executes `source_code` the same way, as raw JavaScript through the embedded Boa engine (`compute/functions.rs::execute_function`). There is no TypeScript transpilation and no actual WASM execution yet, regardless of which `runtime` you set.

**Example:**
```bash
curl -X POST http://localhost:3000/v1/functions/deploy \
  -H "Content-Type: application/json" \
  -d '{
    "name": "double",
    "runtime": "javascript",
    "entrypoint": "handler",
    "source_code": "exports.handler = (input) => ({ result: input.value * 2 });"
  }'
```

**Response:**
```json
{
  "success": true,
  "data": {
    "id": "...",
    "name": "double",
    "runtime": "javascript",
    "created_at": "2025-01-03T12:00:00Z",
    "memory_limit_mb": 128,
    "timeout_secs": 30
  }
}
```

---

### Execute Function

```http
POST /v1/functions/invoke/:name
```

**Content-Type:** `application/json`

**Request Body:** `<any JSON>` (the raw input object) or the legacy wrapped form `{ "input": <any JSON> }`.

**Example:**
```bash
curl -X POST http://localhost:3000/v1/functions/invoke/double \
  -H "Content-Type: application/json" \
  -d '{"input": {"value": 21}}'
```

**Response:**
```json
{
  "success": true,
  "data": {
    "id": "...",
    "function_id": "...",
    "status": "success",
    "input": {"value": 21},
    "output": {"result": 42},
    "error": null,
    "duration_ms": 2
  }
}
```

---

### List Functions

```http
GET /v1/functions
```

**Response:**
```json
{
  "success": true,
  "data": [
    {
      "id": "...",
      "name": "double",
      "runtime": "javascript",
      "status": "active",
      "version": 1,
      "triggers": [],
      "created_at": "2025-01-03T12:00:00Z",
      "updated_at": "2025-01-03T12:00:00Z"
    }
  ]
}
```

---

## 🔌 Realtime

### WebSocket Connection

```http
WS /v1/realtime
```

**Description:** Table-level realtime subscriptions over a single WebSocket, backed by Postgres `LISTEN`/`NOTIFY` fanned out through in-process `tokio::broadcast` channels (`realtime/mod.rs`) — not logical replication.

On connect the server immediately sends a `connected` message with a `client_id`.

**Client → server messages:**
```json
{ "type": "subscribe", "table": "users", "event": "*", "filter": "..." }
{ "type": "unsubscribe", "table": "users" }
{ "type": "ping" }
```
`event` and `filter` are accepted but currently unused by the server — every subscription receives all INSERT/UPDATE/DELETE events for the table regardless of what you pass here.

**Server → client messages:**
```json
{ "type": "connected", "message": "Connected to Stackhouse Realtime", "client_id": 1 }
{ "type": "subscribed", "table": "users", "event": "*" }
{ "type": "unsubscribed", "table": "users" }
{ "type": "pong" }
{ "type": "error", "message": "..." }
{ "type": "INSERT", "table": "users", "record": {...}, "timestamp": "2025-01-03T12:00:00Z" }
{ "type": "UPDATE", "table": "users", "record": {...}, "old_record": {...}, "timestamp": "..." }
{ "type": "DELETE", "table": "users", "old_record": {...}, "timestamp": "..." }
```

**JavaScript Example:**
```javascript
const ws = new WebSocket('ws://localhost:3000/v1/realtime');

ws.onopen = () => {
  ws.send(JSON.stringify({ type: 'subscribe', table: 'users', event: '*' }));
};

ws.onmessage = (event) => {
  const msg = JSON.parse(event.data);
  if (msg.type === 'INSERT' || msg.type === 'UPDATE' || msg.type === 'DELETE') {
    console.log('Change:', msg.table, msg.record ?? msg.old_record);
  }
};
```

---

### Presence & Broadcast (REST, mounted under `/v1/realtime`)

In addition to the WebSocket above, the realtime router also mounts plain REST endpoints for presence tracking and channel broadcast (`realtime/presence.rs`, `realtime/broadcast.rs`):

| Method | Path | Description |
| --- | --- | --- |
| POST | `/v1/realtime/presence/track` | Mark a user as present on a channel |
| POST | `/v1/realtime/presence/untrack` | Remove a user's presence from a channel |
| GET | `/v1/realtime/presence/:channel` | List users currently present on a channel |
| GET | `/v1/realtime/presence` | List all active presence channels |
| POST | `/v1/realtime/broadcast/send` | Publish a message to a broadcast channel |
| GET | `/v1/realtime/broadcast/channels` | List active broadcast channels |
| GET | `/v1/realtime/broadcast/:channel/history` | Last 50 messages sent to a channel |

All responses follow `{"success": true, "data": ...}`.

---

### SSE Stream (Legacy)

```http
GET /v1/stream/:collection
```

**Description:** Server-Sent Events stream for collection updates. This is a separate, older push mechanism from the `/v1/realtime` WebSocket above — it broadcasts an event for every push/update/delete against a collection made through the `/v1/push`, `/v1/update`, `/v1/delete` handlers.

**Example:**
```bash
curl http://localhost:3000/v1/stream/users
```

**Response Stream:**
```
data: {"event":"connected","collection":"users"}

data: {"event":"insert","id":1,"data":{"name":"Alice"}}

data: {"event":"batch_insert","count":25}
```

The `event` field varies by operation (`insert`, `batch_insert`, `update`, `delete`, etc.) and the payload shape varies accordingly — it is not a fixed `key`/`value`/`seq` envelope.

---

## 🔐 Authentication

### Sign Up

```http
POST /v1/auth/signup
```

**Request Body:**
```json
{
  "email": "user@example.com",
  "password": "secure_password"
}
```

**Response:** `201 Created`
```json
{
  "success": true,
  "data": {
    "access_token": "jwt_token_here",
    "refresh_token": "refresh_token_here",
    "expires_in": 900,
    "token_type": "Bearer",
    "user": {
      "id": 1,
      "email": "user@example.com",
      "created_at": "2025-01-03T12:00:00Z",
      "updated_at": "2025-01-03T12:00:00Z",
      "metadata": {}
    }
  }
}
```

Note the token field is `access_token`, not `token`.

---

### Login

```http
POST /v1/auth/login
```

**Request Body:**
```json
{
  "email": "user@example.com",
  "password": "secure_password"
}
```

**Response:** same shape as Sign Up above (`access_token`, `refresh_token`, `expires_in`, `token_type`, `user`).

---

### Refresh Token

```http
POST /v1/auth/refresh
```

**Request Body:**
```json
{
  "refresh_token": "your_refresh_token"
}
```

**Response:** same shape as Sign Up above — a full new token pair, not just a bare `token` field.

---

### Other auth endpoints

`signup`/`login`/`refresh`/`logout` are rate-limited. The auth router (`src/auth/mod.rs`) also exposes, none of which are detailed above:

| Method | Path | Description |
| --- | --- | --- |
| POST | `/v1/auth/logout` | Revoke a refresh token and blacklist the current access token's `jti` |
| GET | `/v1/auth/me` | Get the current authenticated user |
| PUT | `/v1/auth/user` | Update the current user |
| POST | `/v1/auth/change-password` | Change the current user's password |
| GET | `/v1/auth/sessions` | List active sessions |
| DELETE | `/v1/auth/sessions/:id` | Revoke a specific session |

Separate routers are also nested under `/v1/auth` for OAuth (`create_oauth_router`), magic links (`create_magic_link_router`), MFA (`create_mfa_router`), phone OTP (`create_phone_otp_router`), and CAPTCHA (`create_captcha_router`) — see `30-Authentication.md` for those in depth.

---

## 📊 System

### Health Check

```http
GET /health
```

**Response:**
```json
{
  "status": "healthy",
  "database": "connected"
}
```
On failure: `{"status": "unhealthy", "database": "disconnected", "error": "..."}`. There is no `version` field.

---

### Root Endpoint

```http
GET /
```

**Response:** (this lists only the core CRUD/table/stream/health/explorer routes registered directly in `api/routes.rs` — it does not include vectors, functions, realtime, or auth, which are mounted separately)
```json
{
  "name": "Stackhouse",
  "version": "1.0.0",
  "description": "🛸 Schema-Later Database with Automatic Evolution",
  "endpoints": {
    "push": "POST /v1/push/:collection",
    "batch_push": "POST /v1/push/:collection/batch",
    "query": "GET /v1/query/:collection",
    "get_by_id": "GET /v1/query/:collection/:id",
    "update": "POST /v1/update/:collection/:id",
    "bulk_update": "POST /v1/update/:collection",
    "delete": "POST /v1/delete/:collection/:id",
    "bulk_delete": "POST /v1/delete/:collection",
    "tables": "GET /v1/tables",
    "table_stats": "GET /v1/tables/:collection",
    "drop_table": "DELETE /v1/tables/:collection",
    "stream": "GET /v1/stream/:collection",
    "health": "GET /health",
    "explorer": "GET /explore"
  }
}
```

---

## 🚨 Error Codes

| Code | Description |
|------|-------------|
| `INVALID_JSON` | Malformed JSON in request body |
| `COLLECTION_NOT_FOUND` | Collection doesn't exist |
| `DOCUMENT_NOT_FOUND` | Document ID doesn't exist |
| `VALIDATION_ERROR` | Input validation failed |
| `AUTHENTICATION_FAILED` | Invalid credentials |
| `AUTHORIZATION_FAILED` | Insufficient permissions |
| `VECTOR_INDEX_ERROR` | Vector operation failed |
| `FUNCTION_EXECUTION_ERROR` | Function execution failed |
| `RATE_LIMIT_EXCEEDED` | Too many requests |

---

## 📝 Status Codes

| Code | Meaning |
|------|---------|
| 200 | Success |
| 201 | Created |
| 400 | Bad Request |
| 401 | Unauthorized |
| 403 | Forbidden |
| 404 | Not Found |
| 429 | Rate Limit Exceeded |
| 500 | Internal Server Error |

---

## 🔄 Pagination

For large result sets, use pagination:

```bash
# First page
curl "http://localhost:3000/v1/query/users?limit=100&offset=0"

# Second page
curl "http://localhost:3000/v1/query/users?limit=100&offset=100"
```

**Response includes pagination info:**
```json
{
  "success": true,
  "data": [...],
  "pagination": {
    "total": 1250,
    "limit": 100,
    "offset": 0,
    "has_more": true
  }
}
```

---

## 🧪 Testing the API

### Using curl

```bash
# Health check
curl http://localhost:3000/health

# Insert data
curl -X POST http://localhost:3000/v1/push/test \
  -H "Content-Type: application/json" \
  -d '{"message": "Hello, Stackhouse!"}'

# Query data
curl http://localhost:3000/v1/query/test
```

### Using Postman

1. Import API endpoints
2. Set base URL to `http://localhost:3000`
3. Add `Content-Type: application/json` header
4. Send requests!

### Using JavaScript

```javascript
const BASE_URL = 'http://localhost:3000';

async function insert(collection, data) {
  const response = await fetch(`${BASE_URL}/v1/push/${collection}`, {
    method: 'POST',
    headers: {'Content-Type': 'application/json'},
    body: JSON.stringify(data)
  });
  return response.json();
}

async function query(collection) {
  const response = await fetch(`${BASE_URL}/v1/query/${collection}`);
  return response.json();
}

// Usage
await insert('users', {name: 'Alice', age: 28});
const users = await query('users');
console.log(users);
```

### Using Python

```python
import requests

BASE_URL = 'http://localhost:3000'

def insert(collection, data):
    response = requests.post(
        f'{BASE_URL}/v1/push/{collection}',
        json=data
    )
    return response.json()

def query(collection):
    response = requests.get(f'{BASE_URL}/v1/query/{collection}')
    return response.json()

# Usage
insert('users', {'name': 'Alice', 'age': 28})
users = query('users')
print(users)
```

---

## 📚 Related Documentation

- [WebSocket API](./51-WebSocket-API.md) - Realtime protocol details
- [Quick Start](./02-Quick-Start.md) - Get started quickly
- [Examples](../examples/) - Code examples

---

**Need help?** Check the [examples](../examples/) or open an issue on GitHub! 🚀
