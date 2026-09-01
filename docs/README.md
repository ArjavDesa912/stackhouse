<p align="center">
  <img src="../assets/logo.svg" alt="Stackhouse" width="96" />
</p>

# Stackhouse Documentation

**Welcome to Stackhouse — the schema-later Postgres backend.**

## 📚 Documentation Structure

This documentation provides comprehensive coverage of all Stackhouse features, from basic concepts to advanced production deployments.

### 🚀 Getting Started
- **[01-Introduction.md](./01-Introduction.md)** - What is Stackhouse and why should you use it?
- **[02-Quick-Start.md](./02-Quick-Start.md)** - Get up and running in 5 minutes
- **[03-Architecture.md](./03-Architecture.md)** - Understanding the system architecture

### 🎯 Core Features
- **[10-Storage-Engine.md](./10-Storage-Engine.md)** - Stackhouse-Core LSM storage engine
- **[11-Schema-Evolution.md](./11-Schema-Evolution.md)** - Automatic schema evolution, type promotion, preview, and migration audit
- **[12-Querying.md](./12-Querying.md)** - Query patterns and best practices
- **[13-Indexing.md](./13-Indexing.md)** - Secondary indexes and performance

### 🧠 Advanced Features
- **[20-Vector-Search.md](./20-Vector-Search.md)** - AI-powered vector similarity search
- **[21-WASM-Functions.md](./21-WASM-Functions.md)** - Server-side compute with JavaScript/Boa
- **[22-Realtime.md](./22-Realtime.md)** - WebSocket and SSE realtime updates

### 🔒 Security & Operations
- **[30-Authentication.md](./30-Authentication.md)** - JWT-based authentication
- **[31-Row-Level-Security.md](./31-Row-Level-Security.md)** - Fine-grained access control
- **[32-Storage.md](./32-Storage.md)** - File storage and buckets
- **[33-Replication.md](./33-Replication.md)** - Database replication

### 📊 Production
- **[40-Deployment.md](./40-Deployment.md)** - Production deployment guide
- **[41-Performance.md](./41-Performance.md)** - Performance tuning and optimization
- **[42-Monitoring.md](./42-Monitoring.md)** - Monitoring and observability
- **[43-Backup-Recovery.md](./43-Backup-Recovery.md)** - Backup and disaster recovery

### 🛠️ API Reference
- **[50-API-Reference.md](./50-API-Reference.md)** - Complete REST API documentation
- **[51-WebSocket-API.md](./51-WebSocket-API.md)** - WebSocket protocol specification

### 🔬 Developer Guide
- **[60-Contributing.md](./60-Contributing.md)** - Contributing to Stackhouse
- **[61-Testing.md](./61-Testing.md)** - Testing guide
- **[62-Benchmarks.md](./62-Benchmarks.md)** - Performance benchmarks

---

## 🌟 Key Features Overview

```
┌──────────────────────────────────────────────────────────────────┐
│                      STACKHOUSE FEATURE MATRIX                       │
├──────────────────────────────────────────────────────────────────┤
│                                                                  │
│  ┌─────────────┐  ┌─────────────┐  ┌─────────────┐             │
│  │   Schema    │  │     AI      │  │  Realtime   │             │
│  │   Later     │  │   Native    │  │  2.0        │             │
│  │             │  │             │  │             │             │
│  │ ✅ Dynamic  │  │ ✅ Vector   │  │ ✅ WebSocket │             │
│  │ ✅ JSON     │  │ ✅ HNSW     │  │ ✅ SSE       │             │
│  │ ✅ Evolution│  │ ✅ ANN      │  │ ✅ Bidirect  │             │
│  └─────────────┘  └─────────────┘  └─────────────┘             │
│                                                                  │
│  ┌─────────────┐  ┌─────────────┐  ┌─────────────┐             │
│  │   Storage   │  │  Compute    │  │  Security   │             │
│  │   Engine    │  │             │  │             │             │
│  │             │  │             │  │             │             │
│  │ ✅ LSM Tree │  │ ✅ Boa JS    │  │ ✅ JWT      │             │
│  │ ✅ WAL      │  │ ✅ Sandboxed│  │ ✅ RLS      │             │
│  │ ✅ Leveled  │  │ ✅ Fast     │  │ ✅ Policies │             │
│  └─────────────┘  └─────────────┘  └─────────────┘             │
│                                                                  │
└──────────────────────────────────────────────────────────────────┘
```

---

## 🎯 Use Cases

### 1. **Modern Web Applications**
```javascript
// Auto-schema evolution means no migrations!
await db.push('users', {
  name: 'Alice',
  email: 'alice@example.com',
  preferences: { theme: 'dark' }  // Nested objects work!
});
```

### 2. **AI/ML Applications**
```python
# Semantic search out of the box
results = db.vector_search('documents',
    query=embed(text),
    k=10
)
```

### 3. **Realtime Collaboration**
```javascript
// WebSocket realtime updates
ws.subscribe('documents');
ws.on('data', (update) => {
  console.log('Live update:', update);
});
```

### 4. **Serverless Functions**
```bash
# Deploy a JavaScript function
curl -X POST http://localhost:3000/v1/functions/deploy \
  -H "Content-Type: application/json" \
  -d '{
    "name": "process",
    "runtime": "javascript",
    "entrypoint": "handler",
    "source_code": "exports.handler = (input) => input"
  }'

# Execute with custom data
curl -X POST http://localhost:3000/v1/functions/invoke/process \
  -H "Content-Type: application/json" \
  -d '{"input": {"data": "..."}}'
```

---

## 📊 Technology Stack

```
┌─────────────────────────────────────────────────────────────┐
│                    STACKHOUSE STACK                             │
├─────────────────────────────────────────────────────────────┤
│                                                              │
│  Application Layer                                          │
│  ┌──────────────────────────────────────────────────────┐  │
│  │ Axum Web Framework │ WebSocket │ SSE │ REST API     │  │
│  └──────────────────────────────────────────────────────┘  │
│                           ↓                                  │
│  Core Layer                                                 │
│  ┌──────────────────────────────────────────────────────┐  │
│  │  Stackhouse-Core LSM Engine │ WAL │ MemTable │ SSTable    │  │
│  │  Bloom Filters │ Zstd │ Leveled Compaction         │  │
│  └──────────────────────────────────────────────────────┘  │
│                           ↓                                  │
│  Storage Layer                                              │
│  ┌──────────────────────────────────────────────────────┐  │
│  │  Vector Index (HNSW) │ Boa Runtime  │ File Storage  │  │
│  └──────────────────────────────────────────────────────┘  │
│                           ↓                                  │
│  System Layer                                               │
│  ┌──────────────────────────────────────────────────────┐  │
│  │  Authentication (JWT) │ Row-Level Security │ Replication│ │
│  └──────────────────────────────────────────────────────┘  │
│                                                              │
└─────────────────────────────────────────────────────────────┘
```

---

## 🚦 Quick Navigation

### I'm new to Stackhouse
Start here: [Introduction](./01-Introduction.md) → [Quick Start](./02-Quick-Start.md)

### I want to build something
- **Web App**: [Schema Evolution](./11-Schema-Evolution.md) → [Querying](./12-Querying.md)
- **AI/ML**: [Vector Search](./20-Vector-Search.md) → [JavaScript Functions](./21-WASM-Functions.md)
- **Realtime**: [Realtime Guide](./22-Realtime.md)

### I'm deploying to production
- [Deployment Guide](./40-Deployment.md) → [Performance Tuning](./41-Performance.md) → [Monitoring](./42-Monitoring.md)

### I need API documentation
- [REST API](./50-API-Reference.md) → [WebSocket API](./51-WebSocket-API.md)

---

## 💡 Key Concepts

### Schema-Later™
```diff
- Traditional: Define schema → Migrate data → Deploy
- Stackhouse:       Push JSON → Schema evolves automatically
```

**Visual Example:**
```
┌──────────────┐          ┌──────────────┐
│   Request 1  │          │   Request 2  │
│  {name, age} │    →     │ {name,age,email}
└──────────────┘          └──────────────┘
       ↓                         ↓
┌──────────────────────────────────────┐
│         Database Schema              │
│  ┌────────────────────────────────┐ │
│  │ CREATE TABLE users (            │ │
│  │   name TEXT,                   │ │
│  │   age INTEGER                  │ │
│  │ );                             │ │
│  └────────────────────────────────┘ │
│           ↓ AUTO-MIGRATE            │
│  ┌────────────────────────────────┐ │
│  │ ALTER TABLE users              │ │
│  │ ADD COLUMN email TEXT;         │ │
│  └────────────────────────────────┘ │
└──────────────────────────────────────┘
```

### AI-Native
Stackhouse has vector search built-in:
```
Query Vector ──→ [HNSW Index] ──→ Top K Similar Vectors
      ↓                                   ↓
  [0.1, 0.2, ...]              →  [doc1, doc2, doc3]
```

### Realtime 2.0
Bidirectional WebSocket communication:
```
┌──────────┐                    ┌──────────┐
│  Client  │◄────WebSocket─────▶│  Stackhouse  │
└──────────┘    Subscribe/Data  └──────────┘
     │                                │
     ├─ Subscribe("users") ──────────→│
     ├─ Query ───────────────────────→│
     │←───── Realtime Update ─────────┤
     │←───── Realtime Update ─────────┤
```

---

## 🎓 Learning Path

### Beginner (1-2 hours)
1. ✅ Read [Introduction](./01-Introduction.md)
2. ✅ Complete [Quick Start](./02-Quick-Start.md)
3. ✅ Learn [Schema Evolution](./11-Schema-Evolution.md)

### Intermediate (1 day)
4. ✅ Understand [Architecture](./03-Architecture.md)
5. ✅ Master [Querying](./12-Querying.md)
6. ✅ Explore [Indexing](./13-Indexing.md)
7. ✅ Implement [Authentication](./30-Authentication.md)

### Advanced (1 week)
8. ✅ [Vector Search](./20-Vector-Search.md)
9. ✅ [JavaScript Functions](./21-WASM-Functions.md)
10. ✅ [Realtime](./22-Realtime.md)
11. ✅ [Row-Level Security](./31-Row-Level-Security.md)
12. ✅ [Performance Tuning](./41-Performance.md)

### Expert (ongoing)
13. ✅ [Deployment](./40-Deployment.md)
14. ✅ [Monitoring](./42-Monitoring.md)
15. ✅ [Backup & Recovery](./43-Backup-Recovery.md)
16. ✅ [Contributing](./60-Contributing.md)

---

## 📈 Feature Comparison

| Feature | Stackhouse | Supabase | Firebase | MongoDB |
|---------|--------|----------|----------|---------|
| **Schema-Later** | ✅ Native | ❌ SQL required | ❌ Fixed | ⚠️ Flexible |
| **Vector Search** | ✅ Built-in | ⚠️ Extension | ❌ Separate | ⚠️ Atlas only |
| **Boa JS Runtime** | ✅ Built-in | ❌ No | ❌ No | ❌ No |
| **Realtime** | ✅ WS + SSE | ✅ WS | ✅ WS | ✅ WS |
| **RLS** | ✅ Native | ✅ Yes | ⚠️ Rules | ❌ No |
| **Storage** | ✅ Built-in | ✅ Yes | ✅ Yes | ⚠️ GridFS |
| **Self-Host** | ✅ Yes | ✅ Yes | ❌ No | ✅ Yes |
| **Open Source** | ✅ MIT | ✅ OSS | ❌ No | ✅ SSPL |

---

## 🔗 Resources

### Official Resources
- **GitHub**: [github.com/stackhouse/stackhouse](https://github.com/stackhouse/stackhouse)
- **Discord**: [discord.gg/stackhouse](https://discord.gg/stackhouse)
- **Twitter**: [@stackhouse](https://twitter.com/stackhouse)

### Community
- **Blog**: [blog.stackhouse.dev](https://blog.stackhouse.dev)
- **Examples**: [github.com/stackhouse/examples](https://github.com/stackhouse/examples)
- **Showcase**: [showcase.stackhouse.dev](https://showcase.stackhouse.dev)

---

## 🎉 Getting Started

```bash
# Install Stackhouse
cargo install stackhouse

# Start the server
stackhouse

# Or run with Docker
docker run -p 3000:3000 stackhouse/server

# Open your browser
open http://localhost:3000/explore
```

**Next Steps**: Continue to [Introduction](./01-Introduction.md) to learn more about Stackhouse's philosophy and features.

---

## 📝 License

MIT License - See [LICENSE](../LICENSE) for details.

## 🙏 Acknowledgments

Built with love using:
- **Rust** 🦀 - Systems programming
- **Axum** 📡 - Web framework
- **StackhouseCore/LSM** 💾 - Storage engines
- **Boa** ⚡ - JavaScript engine
- And many other amazing open-source projects!

---

**Version**: 1.0.0
**Last Updated**: 2025-01-03
**Maintainers**: Stackhouse Team

---

*Ready to explore? Start with [Introduction](./01-Introduction.md)* 🚀
