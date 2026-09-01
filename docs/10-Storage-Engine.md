# 10 - Storage Engine

## 💾 Stackhouse-Core LSM Storage Engine

### Architecture Overview

```
┌─────────────────────────────────────────────────────────────┐
│              STACKHOUSE-CORE STORAGE ENGINE                         │
├─────────────────────────────────────────────────────────────┤
│                                                              │
│  Write Path:                                                │
│  Client → WAL → MemTable → SSTable → Compaction             │
│                                                              │
│  Read Path:                                                 │
│  Client → Bloom? → MemTable → SSTables → Result            │
│                                                              │
└─────────────────────────────────────────────────────────────┘
```

### Components

#### 1. Write-Ahead Log (WAL)

**Location:** `src/stackhouse_core/wal.rs`

```
Purpose: Crash recovery
Format:  [CRC32: 4b][Length: 8b][Payload]
```

Every write goes to WAL first for durability.

#### 2. MemTable

**Location:** `src/stackhouse_core/memtable.rs`

```
Type: Lock-free SkipList (crossbeam)
Operations: O(log n)
```

In-memory write buffer.

#### 3. SSTable

**Location:** `src/stackhouse_core/sstable.rs`

```
Format:
- Data Blocks (4KB, Zstd compressed)
- Index Block (fast lookup)
- Bloom Filter (negative lookup)
- Footer (metadata)
```

#### 4. Compaction

**Location:** `src/stackhouse_core/compaction.rs`

```
Levels: L0 → L1 → L2 → L3 → L4 → L5 → L6
Strategy: Leveled compaction
Trigger: Size threshold
```

### Performance

```
Sequential Write:  100 MB/s
Random Read:        50 MB/s
Compression Ratio:  3-10x
Cache Hit Rate:     95%+
```

---

**Next:** [Schema Evolution](./11-Schema-Evolution.md)
