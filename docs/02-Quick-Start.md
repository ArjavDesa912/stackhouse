# 02 - Quick Start Guide

## 🚀 Get Stackhouse Running in 5 Minutes

```
┌─────────────────────────────────────────────────────────────┐
│                                                             │
│     ╔══════════════════════════════════════════════════╗   │
│     ║                                                  ║   │
│     ║    From Zero to Stackhouse in Just 5 Minutes!        ║   │
│     ║                                                  ║   │
│     ╚══════════════════════════════════════════════════╝   │
│                                                             │
└─────────────────────────────────────────────────────────────┘
```

## 📋 Prerequisites

Before you begin, ensure you have:

```
┌─────────────────────────────────────────────────────────────┐
│                    REQUIREMENTS CHECKLIST                    │
├─────────────────────────────────────────────────────────────┤
│                                                              │
│  Mandatory:                                                  │
│  ☑  Rust toolchain (1.70+)                                  │
│     └─ Install: https://rustup.rs/                          │
│  ☑  Git                                                     │
│     └─ Install: https://git-scm.com/                        │
│                                                              │
│  Optional (for development):                                │
│  ☐  Docker                                                  │
│  ☐  VS Code + rust-analyzer                                │
│  ☐  curl or Postman (for API testing)                      │
│                                                              │
└─────────────────────────────────────────────────────────────┘
```

---

## 🎯 Installation Options

### Option 1: Install from Source (Recommended)

```bash
# Clone the repository
git clone https://github.com/stackhouse/stackhouse.git
cd stackhouse

# Build and install
cargo install --path .

# Verify installation
stackhouse --version
```

**Output:**
```
🛸 Stackhouse v1.0.0
Schema-Later • AI-Native • Realtime
```

### Option 2: Run with Cargo

```bash
# Clone and run directly
git clone https://github.com/stackhouse/stackhouse.git
cd stackhouse
cargo run --release
```

### Option 3: Docker (Quick Test)

```bash
# Pull and run
docker run -d \
  --name stackhouse \
  -p 3000:3000 \
  -v stackhouse_data:/app/data \
  stackhouse/server:latest

# Check logs
docker logs -f stackhouse
```

---

## 🏃 Quick Start

### Step 1: Start the Server

```bash
# Start with default settings
stackhouse

# Or customize
stackhouse \
  --db ./mydata.db \
  --port 8080 \
  --log-level debug
```

**Expected Output:**
```
┌─────────────────────────────────────────────────────────────┐
│                                                              │
│   🛸 Stackhouse Server Starting...                              │
│                                                              │
│   ✓ Database: ./data/stackhouse.db                             │
│   ✓ API: http://localhost:3000                             │
│   ✓ Explorer: http://localhost:3000/explore                │
│   ✓ WebSocket: ws://localhost:3000/v1/realtime             │
│                                                              │
│   Press Ctrl+C to stop                                       │
│                                                              │
└─────────────────────────────────────────────────────────────┘
```

### Step 2: Verify Installation

```bash
# Health check
curl http://localhost:3000/health

# Response:
{
  "status": "healthy",
  "database": "stackhouse-core"
}
```

### Step 3: Open the Dashboard

```
Open browser: http://localhost:3000/explore
```

**You'll see:**
```
┌─────────────────────────────────────────────────────────────┐
│                   STACKHOUSE EXPLORER                            │
├─────────────────────────────────────────────────────────────┤
│                                                              │
│  ┌─────────────┐  ┌─────────────┐  ┌─────────────┐        │
│  │   Tables    │  │   Query     │  │   Stats     │        │
│  │             │  │             │  │             │        │
│  │   (empty)   │  │   Builder   │  │   Overview  │        │
│  └─────────────┘  └─────────────┘  └─────────────┘        │
│                                                              │
│  💡 Tip: Use the API or dashboard to insert data           │
│                                                              │
└─────────────────────────────────────────────────────────────┘
```

---

## 🎨 Your First Queries

### Example 1: Auto-Schema Creation

```bash
# Insert your first document
curl -X POST http://localhost:3000/v1/push/users \
  -H "Content-Type: application/json" \
  -d '{
    "name": "Alice Johnson",
    "email": "alice@example.com",
    "age": 28
  }'
```

**What happens behind the scenes:**
```
┌─────────────────────────────────────────────────────────────┐
│           AUTOMATIC SCHEMA CREATION                          │
├─────────────────────────────────────────────────────────────┤
│                                                              │
│  1. Stackhouse receives JSON                                    │
│  2. Analyzes structure:                                     │
│     • name: String → TEXT                                   │
│     • email: String → TEXT                                  │
│     • age: Number → INTEGER                                 │
│  3. Creates table:                                          │
│     ┌──────────────────────────────────────┐               │
│     │ CREATE TABLE users (                 │               │
│     │   id INTEGER PRIMARY KEY,            │               │
│     │   name TEXT,                         │               │
│     │   email TEXT,                        │               │
│     │   age INTEGER,                       │               │
│     │   created_at TIMESTAMP DEFAULT NOW()  │               │
│     │ );                                  │               │
│     └──────────────────────────────────────┘               │
│  4. Inserts data                                             │
│  5. Returns success                                         │
│                                                              │
└─────────────────────────────────────────────────────────────┘
```

**Response:**
```json
{
  "success": true,
  "data": {
    "id": 1,
    "name": "Alice Johnson",
    "email": "alice@example.com",
    "age": 28,
    "created_at": "2025-01-03T12:00:00Z"
  }
}
```

### Example 2: Schema Evolution

```bash
# Insert data with new fields
curl -X POST http://localhost:3000/v1/push/users \
  -H "Content-Type: application/json" \
  -d '{
    "name": "Bob Smith",
    "age": 35,
    "city": "San Francisco",
    "skills": ["Rust", "Python", "JavaScript"]
  }'
```

**Schema automatically evolves:**
```
┌─────────────────────────────────────────────────────────────┐
│           SCHEMA EVOLUTION                                   │
├─────────────────────────────────────────────────────────────┤
│                                                              │
│  Detected new fields:                                       │
│  • city: String → TEXT (added)                              │
│  • skills: Array → JSON (added)                             │
│                                                              │
│  Automatic migration:                                        │
│  ┌──────────────────────────────────────┐                   │
│  │ ALTER TABLE users                    │                   │
│  │ ADD COLUMN city TEXT;                │                   │
│  │ ADD COLUMN skills JSON;              │                   │
│  └──────────────────────────────────────┘                   │
│                                                              │
│  ⚡ Zero downtime required                                  │
│                                                              │
└─────────────────────────────────────────────────────────────┘
```

### Example 3: Query Data

```bash
# Get all users
curl http://localhost:3000/v1/query/users

# Get specific user
curl http://localhost:3000/v1/query/users/1
```

**Response:**
```json
{
  "success": true,
  "data": [
    {
      "id": 1,
      "name": "Alice Johnson",
      "email": "alice@example.com",
      "age": 28,
      "created_at": "2025-01-03T12:00:00Z"
    },
    {
      "id": 2,
      "name": "Bob Smith",
      "age": 35,
      "city": "San Francisco",
      "skills": ["Rust", "Python", "JavaScript"],
      "created_at": "2025-01-03T12:01:00Z"
    }
  ]
}
```

---

## 🧪 Advanced Quick Start

### Vector Search Setup

```bash
# 1. Create a vector index (by inserting)
curl -X POST http://localhost:3000/v1/vectors/documents \
  -H "Content-Type: application/json" \
  -d '{
    "id": "doc1",
    "vector": [0.1, 0.2, 0.3, 0.4, 0.5],
    "metadata": {
      "title": "Introduction to Stackhouse",
      "category": "database"
    }
  }'

# 2. Insert more vectors
curl -X POST http://localhost:3000/v1/vectors/documents \
  -H "Content-Type: application/json" \
  -d '{
    "id": "doc2",
    "vector": [0.15, 0.25, 0.35, 0.45, 0.55],
    "metadata": {
      "title": "Advanced Stackhouse Features",
      "category": "database"
    }
  }'

# 3. Search for similar vectors
curl -X POST http://localhost:3000/v1/vectors/documents/search \
  -H "Content-Type: application/json" \
  -d '{
    "query": [0.12, 0.22, 0.32, 0.42, 0.52],
    "k": 5
  }'
```

**Response:**
```json
{
  "success": true,
  "results": [
    {
      "id": "doc1",
      "distance": 0.015,
      "metadata": {
        "title": "Introduction to Stackhouse",
        "category": "database"
      }
    },
    {
      "id": "doc2",
      "distance": 0.045,
      "metadata": {
        "title": "Advanced Stackhouse Features",
        "category": "database"
      }
    }
  ]
}
```

### WebSocket Realtime Connection

```javascript
// Connect to Stackhouse WebSocket
const ws = new WebSocket('ws://localhost:3000/v1/realtime');

// Connection opened
ws.addEventListener('open', () => {
  console.log('✅ Connected to Stackhouse');

  // Subscribe to a collection
  ws.send(JSON.stringify({
    type: 'Subscribe',
    key: 'users'
  }));
});

// Listen for messages
ws.addEventListener('message', (event) => {
  const msg = JSON.parse(event.data);

  if (msg.type === 'Data') {
    console.log('📨 New data:', msg.key, msg.value);

    // Example output:
    // 📨 New data: users {
    //   id: 3,
    //   name: "Charlie",
    //   ...
    // }
  }
});

// Now try inserting data from another terminal:
// curl -X POST http://localhost:3000/v1/push/users \
//   -H "Content-Type: application/json" \
//   -d '{"name": "Charlie", "age": 30}'
//
// You'll see it appear instantly in the WebSocket!
```

---

## 📊 Quick Reference Commands

### Data Operations

```bash
# Insert data
POST   /v1/push/:collection
POST   /v1/push/:collection/batch

# Query data
GET    /v1/query/:collection
GET    /v1/query/:collection/:id

# Update data
POST   /v1/update/:collection/:id

# Delete data
POST   /v1/delete/:collection/:id
```

### Schema & Metadata

```bash
# List all tables
GET    /v1/tables

# Get table stats
GET    /v1/tables/:collection

# Create index
POST   /v1/index
```

### Vector Search

```bash
# Insert vector
POST   /v1/vectors/:index

# Search vectors
POST   /v1/vectors/:index/search

# List indexes
GET    /v1/vectors
```

### JavaScript Functions

```bash
# Deploy a JavaScript/TypeScript function
POST   /v1/functions/deploy

# Execute a function
POST   /v1/functions/invoke/:name

# List functions
GET    /v1/functions
```

### Realtime

```bash
# WebSocket connection
WS     /v1/realtime

# SSE stream (legacy)
GET    /v1/stream/:collection
```

---

## 🎯 Common Use Cases

### Use Case 1: REST API Backend

```javascript
// server.js
const express = require('express');
const axios = require('axios');

const app = express();
app.use(express.json());

// Create user
app.post('/users', async (req, res) => {
  const response = await axios.post(
    'http://localhost:3000/v1/push/users',
    req.body
  );
  res.json(response.data);
});

// Get user
app.get('/users/:id', async (req, res) => {
  const response = await axios.get(
    `http://localhost:3000/v1/query/users/${req.params.id}`
  );
  res.json(response.data);
});

app.listen(4000);
```

### Use Case 2: Python Client

```python
# client.py
import requests

STACKHOUSE_URL = "http://localhost:8080"

# Insert data
response = requests.post(
    f"{STACKHOUSE_URL}/v1/push/products",
    json={
        "name": "Laptop",
        "price": 999.99,
        "in_stock": True
    }
)
print(response.json())

# Query data
response = requests.get(f"{STACKHOUSE_URL}/v1/query/products")
print(response.json())
```

### Use Case 3: Realtime Dashboard

```html
<!-- dashboard.html -->
<!DOCTYPE html>
<html>
<head>
  <title>Stackhouse Dashboard</title>
</head>
<body>
  <h1>Live Users</h1>
  <ul id="users"></ul>

  <script>
    const ws = new WebSocket('ws://localhost:3000/v1/realtime');
    const userList = document.getElementById('users');

    ws.onopen = () => {
      // Subscribe to users
      ws.send(JSON.stringify({
        type: 'Subscribe',
        key: 'users'
      }));

      // Load existing users
      fetch('http://localhost:3000/v1/query/users')
        .then(r => r.json())
        .then(data => {
          data.data.forEach(user => addUser(user));
        });
    };

    ws.onmessage = (event) => {
      const msg = JSON.parse(event.data);
      if (msg.type === 'Data') {
        addUser(msg.value);
      }
    };

    function addUser(user) {
      const li = document.createElement('li');
      li.textContent = `${user.name} (age: ${user.age})`;
      userList.appendChild(li);
    }
  </script>
</body>
</html>
```

---

## 🔍 Troubleshooting

### Problem: Port already in use

```bash
# Use a different port
stackhouse --port 8080

# Or find and kill the process
# On Linux/Mac:
lsof -ti:3000 | xargs kill -9

# On Windows:
netstat -ano | findstr :3000
taskkill /PID <PID> /F
```

### Problem: Permission denied

```bash
# Ensure directory is writable
mkdir -p ./data
chmod 755 ./data

# Or specify a different data directory
stackhouse --db /tmp/stackhouse.db
```

### Problem: Can't connect

```bash
# Verify server is running
curl http://localhost:3000/health

# Check firewall settings
# Try connecting with telnet
telnet localhost 3000

# Check server logs for errors
stackhouse --log-level debug
```

---

## 📚 Next Steps

Now that you have Stackhouse running:

1. **Explore the Dashboard**
   - Visit http://localhost:3000/explore
   - Try the visual query builder
   - Monitor real-time stats

2. **Build Something**
   - [Architecture Guide](./03-Architecture.md) - Understand the system
   - [Schema Evolution](./11-Schema-Evolution.md) - Master auto-schema
   - [Vector Search](./20-Vector-Search.md) - Add AI capabilities

3. **Go Production**
   - [Deployment Guide](./40-Deployment.md) - Deploy to production
   - [Performance Tuning](./41-Performance.md) - Optimize your setup
   - [Monitoring](./42-Monitoring.md) - Set up observability

---

## 💡 Pro Tips

### Tip 1: Use Environment Variables

```bash
# .env file
STACKHOUSE_PORT=3000
STACKHOUSE_DB_PATH=./data/stackhouse.db
STACKHOUSE_LOG_LEVEL=info

# Load and run
export $(cat .env | xargs)
stackhouse
```

### Tip 2: Enable CORS for Development

```bash
# Stackhouse has CORS enabled by default
# But you can configure it in production
stackhouse --cors-origins "http://localhost:8080,https://example.com"
```

### Tip 3: Use Batch Inserts

```bash
# Faster than individual inserts
curl -X POST http://localhost:3000/v1/push/users/batch \
  -H "Content-Type: application/json" \
  -d '[
    {"name": "User 1", "age": 25},
    {"name": "User 2", "age": 30},
    {"name": "User 3", "age": 35}
  ]'
```

### Tip 4: Monitor Performance

```bash
# Check table stats
curl http://localhost:3000/v1/tables/users

# Response includes:
# - Row count
# - Size on disk
# - Indexes
# - Last update time
```

---

**Congratulations!** 🎉 You now have Stackhouse running and ready to use.

**Next:** Learn about the [Architecture](./03-Architecture.md) to understand how everything works under the hood.
