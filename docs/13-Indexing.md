# 13 - Indexing

## 📇 Secondary Indexes & Performance

### Creating Indexes

```bash
curl -X POST http://localhost:3000/v1/index \
  -H "Content-Type: application/json" \
  -d '{
    "collection": "users",
    "column": "email",
    "unique": true
  }'
```

### Index Types

```
┌─────────────────────────────────────────────────────────────┐
│                   INDEX TYPES                                 │
├─────────────────────────────────────────────────────────────┤
│                                                              │
│  Primary Index:                                               │
│  • Auto-created on 'id' column                              │
│  • Unique, auto-incrementing                                 │
│  • Fast lookups                                              │
│                                                              │
│  Secondary Index:                                            │
│  • User-created                                              │
│  • Can be unique or non-unique                               │
│  • Speeds up queries on indexed column                      │
│                                                              │
│  Vector Index:                                               │
│  • For similarity search                                     │
│  • HNSW algorithm                                            │
│  • See [Vector Search](./20-Vector-Search.md)                │
│                                                              │
└─────────────────────────────────────────────────────────────┘
```

### When to Create Indexes

```
✅ Frequently queried columns
✅ Join columns
✅ Filter/order by columns
❌ Low-cardinality columns (e.g., boolean)
❌ Columns updated frequently
```

### Performance Impact

```
Without Index:  O(n) full scan
With Index:     O(log n) index lookup + O(1) row fetch

Query Speedup: 10-100x for large datasets
```

---

**Next:** [Vector Search](./20-Vector-Search.md)
