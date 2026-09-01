<p align="center">
  <img src="assets/logo.svg" alt="Stackhouse" width="120" />
</p>

<h1 align="center">Stackhouse</h1>

<p align="center"><strong>An open-source backend platform: schema-later Postgres, auth, storage, realtime, vector search, serverless functions, and billing — in one Rust binary.</strong></p>

<p align="center">
  <a href="https://github.com/ArjavDesa912/stackhouse/actions/workflows/ci.yml"><img src="https://github.com/ArjavDesa912/stackhouse/actions/workflows/ci.yml/badge.svg" alt="CI" /></a>
  <a href="LICENSE"><img src="https://img.shields.io/badge/license-MIT-blue.svg" alt="License: MIT" /></a>
  <a href="Cargo.toml"><img src="https://img.shields.io/badge/rust-2021-orange.svg" alt="Rust" /></a>
  <a href="#project-status"><img src="https://img.shields.io/badge/status-pre--1.0-yellow.svg" alt="Status: pre-1.0" /></a>
</p>

Stackhouse pushes your JSON straight into Postgres and evolves the schema for you — no migrations, no ORM, no upfront modeling. On top of that data layer it ships the rest of what a real product needs: JWT/OAuth/SAML/WebAuthn auth, row-level security, S3-compatible object storage, WebSocket/SSE realtime, HNSW vector search (via Qdrant), JS serverless functions, and usage-based billing — all as one Rust binary backed by a real Postgres database.

> **Note:** an earlier draft of this codebase included a large library of third-party SaaS connectors (Salesforce, Slack, Stripe, etc.) and an AI-agent/RAG layer built on top of them. Both have been removed from this repository — they were unfinished and untested, and didn't meet the bar for a public release. What remains below is what's actually built and verified working.

---

## Table of Contents

- [Why Stackhouse](#why-stackhouse)
- [Features](#features)
- [Quick Start](#quick-start)
- [Architecture](#architecture)
- [Repository Structure](#repository-structure)
- [SDKs](#sdks)
- [Documentation](#documentation)
- [Project Status](#project-status)
- [Contributing](#contributing)
- [Security](#security)
- [License](#license)

---

## Why Stackhouse

Most teams building a real backend end up gluing together five or six services: Postgres for data, an ORM and migration tool on top of it, Redis for cache, a vector store for AI features, a functions platform for compute, and a separate billing system for subscriptions. Each has its own auth model, its own deploy story, its own on-call surface.

Stackhouse collapses that into one deployable:

| You'd otherwise run | Stackhouse gives you |
|---|---|
| Postgres + an ORM + a migration tool | **Schema-later ingestion** — push JSON, the schema evolves under it |
| A separate auth service | **Built-in auth** — JWT sessions, OAuth, SAML, WebAuthn, MFA, magic links |
| Pinecone / Qdrant | **Native vector search** (HNSW) in the same database |
| A functions platform (Lambda, Cloud Functions) | **Embedded JS runtime** (`boa_engine`) for serverless functions |
| Firebase / Pusher | **Built-in realtime** — WebSocket + SSE |
| Stripe integration code + a billing admin panel | **Usage-based billing** — plans, entitlements, metering, dunning, Stripe checkout |

Everything above is one Rust crate with one Postgres connection string. Fork it, self-host it, or read the source to see exactly how a production-shaped backend is put together.

## Features

### Data & Schema
- **Schema-later engine** — POST JSON to `/v1/push/:collection`; Stackhouse infers types, creates tables, and safely promotes column types as your data shape changes (`ALTER COLUMN ... USING`, coordinated with per-table locks + Postgres advisory locks).
- **Migration audit trail** — every automatic DDL change is recorded with up/down SQL and a checksum.
- **Schema preview** — see the DDL a payload *would* trigger before it's applied.
- **Vector search** — HNSW-indexed similarity search alongside your relational data.
- **Full-text search, GraphQL, and an auto-generated REST layer.**

### Auth & Security
- JWT sessions, OAuth2 social login, SAML, WebAuthn/passkeys, TOTP MFA, phone OTP, magic links, API keys, device trust, and impersonation for support tooling.
- Row-level security (policy engine) and attribute-based access control (ABAC).
- WAF, bot protection, rate limiting, encryption (AES-GCM-SIV), BYOK, GDPR tooling, and data-residency controls.
- See [`.github/SECURITY.md`](.github/SECURITY.md) for an honest breakdown of what's implemented today versus what's intentionally deferred.

### Storage, Realtime & Compute
- S3-compatible object storage with buckets, ACLs, CDN, lifecycle rules, versioning, and resumable uploads (tus).
- WebSocket + SSE realtime with presence and broadcast channels.
- Sandboxed JS functions (`boa_engine`), event bus, scheduled jobs, and webhooks.

### Billing & Platform
- Usage-based billing: subscription plans, entitlements, metering, invoices, dunning, trials, audiences/experiments, and Stripe checkout/webhooks — all with an admin UI.
- Multi-tenancy, org SSO, database branching (Neon-style branch/clone), read replicas, audit logs, quotas, and CDC.
- Prometheus metrics, OpenTelemetry, structured logging, and error tracking baked in.

### Admin UI
- A React/Vite dashboard (`ui/`) for exploring data, running SQL, and managing auth, storage, and billing — embedded directly in the server binary.

## Quick Start

**Prerequisites:** Rust (stable, 2021 edition) and PostgreSQL 15+, or just Docker.

### Option A — Docker Compose (recommended)

```bash
git clone https://github.com/ArjavDesa912/stackhouse.git
cd stackhouse
cp .env.example .env
docker compose up
```

### Option B — from source

```bash
git clone https://github.com/ArjavDesa912/stackhouse.git
cd stackhouse

# Start Postgres (or point --url at your own instance)
docker compose up -d postgres

cargo run --release -- serve --url postgres://postgres:postgres@localhost:5432/stackhouse
```

### Push and query data

```bash
# Push a document — the 'users' table and its schema are created automatically
curl -X POST http://localhost:3000/v1/push/users \
  -H "Content-Type: application/json" \
  -d '{"name": "Alice", "email": "alice@example.com", "age": 28}'

# Add a new field — the schema evolves to fit it
curl -X POST http://localhost:3000/v1/push/users \
  -H "Content-Type: application/json" \
  -d '{"name": "Bob", "email": "bob@example.com", "department": "Engineering"}'

# Query
curl "http://localhost:3000/v1/query/users?department=Engineering"
```

Then open **http://localhost:3000/explore** for the embedded admin dashboard.

Full walkthrough: [`docs/02-Quick-Start.md`](docs/02-Quick-Start.md) · Complete REST surface: [`docs/50-API-Reference.md`](docs/50-API-Reference.md)

## Architecture

```
┌──────────────────────────────────────────────────────────────────┐
│                            Stackhouse                             │
├──────────────────────────────────────────────────────────────────┤
│  Auth              Storage           Realtime          Billing    │
│  (JWT/OAuth/        (S3-compat,       (WS/SSE,          (Stripe,  │
│  SAML/MFA)          CDN, ACL)         presence)      entitlements)│
├──────────────────────────────────────────────────────────────────┤
│                    REST · GraphQL · WebSocket API (Axum)          │
├──────────────────────────────────────────────────────────────────┤
│         Schema Inference Engine  →  Migration Guard (DDL)         │
│              (JSON → Postgres types, safe promotion)              │
├──────────────────────────────────────────────────────────────────┤
│      DashMap schema cache  ·  Row-Level Security  ·  Vector idx   │
├──────────────────────────────────────────────────────────────────┤
│                       PostgreSQL (via sqlx)                       │
└──────────────────────────────────────────────────────────────────┘
```

Deep dive: [`docs/03-Architecture.md`](docs/03-Architecture.md).

## Repository Structure

```
stackhouse/
├── src/                  # Rust server (Axum) — the core crate
│   ├── api/               # HTTP handlers, REST/GraphQL/OpenAPI routing
│   ├── auth/              # JWT, OAuth, SAML, WebAuthn, MFA, magic links
│   ├── billing/           # Plans, entitlements, metering, Stripe, dunning
│   ├── compute/           # Serverless JS functions, jobs, webhooks
│   ├── platform/          # Multi-tenancy, SSO, audit log, replicas, CDC
│   ├── security/          # RLS, WAF, encryption, GDPR, BYOK
│   ├── storage/           # S3-compatible object storage, CDN, versioning
│   └── ...                # branching, realtime, teams, data_processing
├── tests/                # Integration tests
├── ui/                   # React/Vite admin dashboard (embedded in the binary)
├── sdks/                 # Python client, Terraform provider
├── js-sdks/              # JS/TS, React, and Vue client libraries
├── sdk-android/ sdk-ios/ sdk-flutter/ sdk-react-native/
├── docs/                 # Numbered guides — see docs/DOCS_INDEX.md
├── deploy/               # Helm chart for Kubernetes deployment
├── scripts/              # API test scripts, security scanner
├── docker/               # Postgres init scripts
├── Cargo.toml / Cargo.lock
└── docker-compose.yml
```

## SDKs

| Platform | Location |
|---|---|
| JavaScript / TypeScript | [`js-sdks/stackhouse-js`](js-sdks/stackhouse-js) |
| React | [`js-sdks/stackhouse-react`](js-sdks/stackhouse-react) |
| Vue | [`js-sdks/stackhouse-vue`](js-sdks/stackhouse-vue) |
| Python | [`sdks/python`](sdks/python) |
| Android (Kotlin) | [`sdk-android`](sdk-android) |
| iOS (Swift) | [`sdk-ios`](sdk-ios) |
| Flutter | [`sdk-flutter`](sdk-flutter) |
| React Native | [`sdk-react-native`](sdk-react-native) |
| Terraform | [`sdks/terraform`](sdks/terraform) |

## Documentation

The full guide set lives in [`docs/`](docs), indexed at [`docs/DOCS_INDEX.md`](docs/DOCS_INDEX.md) — from a 5-minute quick start through storage-engine internals, vector search, replication, and deployment.

## Project Status

Stackhouse is **pre-1.0 and under active development**. The core data engine, auth, storage, realtime, and billing paths are functional and tested; expect breaking changes before a 1.0 tag. See [`CHANGELOG.md`](CHANGELOG.md) for what's landed.

## Contributing

Contributions are welcome — see [`CONTRIBUTING.md`](CONTRIBUTING.md) for build/test setup, code style, and the PR process. Please also read the [`CODE_OF_CONDUCT.md`](CODE_OF_CONDUCT.md).

## Security

See [`.github/SECURITY.md`](.github/SECURITY.md) for supported versions, how to report a vulnerability, and an honest account of current security controls.

## License

MIT — see [`LICENSE`](LICENSE).
