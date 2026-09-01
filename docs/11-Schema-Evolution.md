# 11 - Schema Evolution

## Overview

Stackhouse is a **Schema-Later** database: you push arbitrary JSON and it
automatically creates tables, adds columns, and widens existing columns to fit
new data. All of this happens safely, concurrently, and is recorded in an
auditable migration history.

## How It Works

```
JSON payload
    ↓
Infer PostgreSQL type for each field
    ↓
Unify types across a whole batch
    ↓
Compute diff against live schema
    ↓
Acquire per-table advisory lock
    ↓
Generate and run CREATE / ALTER TABLE ... ADD / ALTER COLUMN ... TYPE
    ↓
Record migration in stackhouse_schema_migrations
    ↓
Notify schema_changed for cache eviction
    ↓
Insert data
```

## Type Inference

| JSON value | PostgreSQL type |
|---|---|
| `null` | skipped for column creation; `NULL` for common-type math |
| boolean | `BOOLEAN` |
| integer | `BIGINT` |
| float | `DOUBLE PRECISION` |
| ISO 8601 date (`YYYY-MM-DD`) | `DATE` |
| ISO 8601 timestamp | `TIMESTAMPTZ` |
| UUID (`8-4-4-4-12`) | `UUID` |
| other string | `TEXT` |
| object | `JSONB` |
| array | `JSONB` |

## Type Promotion

When a later value does not fit the existing column type, Stackhouse computes the
**common supertype** and runs a safe `ALTER COLUMN ... TYPE ... USING` cast.

Examples:

| Existing | Incoming | Common |
|---|---|---|
| `BIGINT` | `DOUBLE PRECISION` | `DOUBLE PRECISION` |
| `DATE` | `TIMESTAMPTZ` | `TIMESTAMPTZ` |
| `TEXT` and `JSONB` (either direction) | `JSONB` |
| any other conflict | `JSONB` |

All `USING` casts are data-loss-safe for promotion paths. Notable special
handling:

- `JSONB → TEXT` uses `col #>> ARRAY[]::text[]` to avoid quoted JSON strings.
- `TEXT → JSONB` uses `to_jsonb(col)` so any string (including non-JSON text)
  can be stored.
- `BIGINT → DOUBLE PRECISION` uses `col::double precision`.
- Temporal / UUID casts from `JSONB` use
  `(col #>> ARRAY[]::text[])::type` to extract scalar JSON text first.

## Concurrency Safety

Schema evolution is protected on multiple levels:

1. **Per-process per-table Tokio mutex** in `SchemaGuard`.
2. **PostgreSQL advisory transaction lock** keyed on `hashtext('stackhouse:schema:' || table)`.
3. **Live schema re-check inside the transaction** to avoid redundant DDL.
4. **`LISTEN schema_changed / NOTIFY`** so horizontally-scaled instances evict
their local DashMap cache.

## Schema Preview

You can preview the DDL a payload would trigger without mutating the schema:

```text
POST /v1/preview/:collection
```

Request body: the same JSON object (or array for batch preview) you would push.

Response includes:

- `table_exists`
- `create_table_sql`
- `additions` (column name → target type)
- `widenings` (column name, from type, to type)
- `add_sql` and `widen_sql`
- `insert_columns`
- `would_exceed_limit`

## Safeguards

- **Hard column cap**: a table may not exceed 1000 columns.
- **Rolling churn rate limit**: configurable via
  `STACKHOUSE_SCHEMA_CHURN_MAX` (default `20`) and
  `STACKHOUSE_SCHEMA_CHURN_WINDOW_SECS` (default `60`).

These are distinct. A `StackhouseError::RateLimited` is raised for churn, while
`StackhouseError::ColumnLimitExceeded` is raised for the hard cap.

## Migration Audit Trail

Every automatic schema change is recorded in `stackhouse_schema_migrations` with:

- a unique, monotonically increasing version
- a descriptive name
- `up_sql` (the actual DDL that ran)
- `down_sql` (best-effort reverse DDL)
- a SHA-256 truncated checksum
- `applied` status

This integrates with `SchemaMigrationService`, so automatic and manual
migrations share one audit and rollback surface.

## Batch Writes

Batch writes call `ensure_batch_columns` once for the whole set. This unifies
conflicting types across every document before any `INSERT` happens, preventing
intra-batch type drift.

## Example Evolution

```javascript
// Request 1
{ "name": "Alice", "age": 25 }
→ CREATE TABLE users (...)
  ADD name TEXT, age BIGINT

// Request 2
{ "name": "Bob", "email": "bob@example.com" }
→ ALTER TABLE users ADD COLUMN email TEXT

// Request 3
{ "name": "Carol", "age": 25.5 }
→ ALTER TABLE users
  ALTER COLUMN age TYPE DOUBLE PRECISION USING age::double precision

// Request 4
{ "dob": "1990-05-21", "registered": "2024-01-01T12:00:00Z" }
→ ADD dob DATE
→ ALTER registered would conflict with dob? no; both are temporal/UUID, stored independently.
```

## GraphQL

The GraphQL mutation resolvers use the same typed schema preparation and
parameter binding as the REST `push`/`batch_push` handlers.

---

**Next:** [Querying](./12-Querying.md)
