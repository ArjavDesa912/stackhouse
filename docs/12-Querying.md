# 12 - Querying

## 🔍 Query Patterns & Best Practices

### REST API Queries

```bash
# Get all documents
GET /v1/query/:collection

# Get specific document
GET /v1/query/:collection/:id

# With pagination
GET /v1/query/:collection?limit=100&offset=0

# With ordering
GET /v1/query/:collection?order_by=created_at&order=desc
```

### Filtering

```javascript
// Client-side filtering from full result set
const users = await fetch('/v1/query/users')
  .then(r => r.json())
  .then(data => data.data);

const active = users.filter(u => u.active === true);
```

### Advanced Patterns

#### 1. Prefix Scans

```rust
// Stackhouse internal API
db.scan(b"users:123")  // Scan keys with prefix
```

#### 2. Range Queries

```bash
# Using SQL direct access
POST /v1/sql/query
{"query": "SELECT * FROM users WHERE age > 25"}
```

### Performance Tips

```
✅ Use specific ID lookups
✅ Limit result sets
✅ Create indexes on frequently queried fields
✅ Use batch operations
❌ Avoid large result sets (>10K rows)
```

---

**Next:** [Indexing](./13-Indexing.md)
