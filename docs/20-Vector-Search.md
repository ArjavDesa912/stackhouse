# 20 - Vector Search

## 🔍 AI-Native Similarity Search

```
┌─────────────────────────────────────────────────────────────┐
│                                                             │
│     ╔══════════════════════════════════════════════════╗   │
│     ║                                                  ║   │
│     ║      Find Similar Data in Milliseconds           ║   │
│     ║                                                  ║   │
│     ║        [0.1, 0.2, 0.3]  ──→  Top K Matches      ║   │
│     ║                                                  ║   │
│     ╚══════════════════════════════════════════════════╝   │
│                                                             │
└─────────────────────────────────────────────────────────────┘
```

## 📚 Table of Contents
- [Concepts](#concepts)
- [Getting Started](#getting-started)
- [API Reference](#api-reference)
- [Distance Metrics](#distance-metrics)
- [Performance](#performance)
- [Use Cases](#use-cases)

---

> **Implementation note:** Stackhouse's vector search (`stackhouse/src/storage/vectors.rs`, mounted at
> `/v1/vectors`) is a REST proxy in front of an external **Qdrant** instance — Stackhouse does not
> implement its own HNSW index in-process. Distance computation, indexing, and ANN search all
> happen inside Qdrant; Stackhouse stores the vector column config and forwards requests.

## 🎯 Concepts

### What is Vector Search?

```
┌─────────────────────────────────────────────────────────────┐
│           FROM KEYWORD SEARCH TO SEMANTIC SEARCH             │
├─────────────────────────────────────────────────────────────┤
│                                                              │
│  Traditional Keyword Search:                                │
│  ┌──────────────────────────────────────────────┐          │
│  │  Query: "apple"                             │          │
│  │  Results:                                   │          │
│  │  • Apple Inc. (company)                     │          │
│  │  • apple (fruit)                            │          │
│  │  • Apple Records (music)                    │          │
│  └──────────────────────────────────────────────┘          │
│  ❌ Doesn't understand meaning                          │
│                                                              │
│  Vector Semantic Search:                                    │
│  ┌──────────────────────────────────────────────┐          │
│  │  Query: "tech company founded by jobs"       │          │
│  │  → [0.23, -0.45, 0.67, ...]                 │          │
│  │  Results:                                   │          │
│  │  • Apple Inc. (96% similarity) ✓            │          │
│  │  • Microsoft (89% similarity)               │          │
│  │  • Google (85% similarity)                  │          │
│  └──────────────────────────────────────────────┘          │
│  ✅ Understands semantic meaning                          │
│                                                              │
└─────────────────────────────────────────────────────────────┘
```

### How It Works

```
┌─────────────────────────────────────────────────────────────┐
│              VECTOR SEARCH PIPELINE                          │
├─────────────────────────────────────────────────────────────┤
│                                                              │
│  1. Text Input                                              │
│     "The quick brown fox jumps"                             │
│              ↓                                               │
│  2. Embedding Model                                         │
│     ┌─────────────────────────────────┐                    │
│     │  sentence-transformers,        │                    │
│     │  OpenAI embeddings,            │                    │
│     │  Cohere embeddings, etc.       │                    │
│     └─────────────────────────────────┘                    │
│              ↓                                               │
│  3. Vector Representation                                   │
│     [0.23, -0.45, 0.67, 0.12, ..., 0.89]                    │
│              ↑                                               │
│              384 dimensions (example)                        │
│              ↓                                               │
│  4. Qdrant HNSW Index (external service)                    │
│     ┌─────────────────────────────────┐                    │
│     │  Hierarchical Navigable        │                    │
│     │  Small World Graph             │                    │
│     │  • Approximate Nearest Neighbor│                    │
│     │  • O(log n) search             │                    │
│     │  • Built and served by Qdrant, │                    │
│     │    not Stackhouse itself           │                    │
│     └─────────────────────────────────┘                    │
│              ↓                                               │
│  5. Similarity Search                                       │
│     Query: [0.25, -0.43, 0.65, ...]                          │
│              ↓                                               │
│     Compare with all vectors                                 │
│              ↓                                               │
│     Sort by similarity                                       │
│              ↓                                               │
│     Return Top K results                                     │
│                                                              │
└─────────────────────────────────────────────────────────────┘
```

### HNSW Algorithm Visualized (as implemented by Qdrant)

```
┌─────────────────────────────────────────────────────────────┐
│            HNSW (Hierarchical Navigable Small World)         │
├─────────────────────────────────────────────────────────────┤
│                                                              │
│  Layer 2 (Sparse - Long connections)                        │
│     ┌─────────────────────────────────────────┐            │
│     │                                          │            │
│     │     ●───────●                           │            │
│     │              \                          │            │
│     │               ●                         │            │
│     │                                          │            │
│     └─────────────────────────────────────────┘            │
│                    ↓                                        │
│  Layer 1 (Medium density)                                   │
│     ┌─────────────────────────────────────────┐            │
│     │                                          │            │
│     │  ●───●───●           ●───●              │            │
│     │    \     \            \                  │            │
│     │     ●───●───●───●───●───●                │            │
│     │            \        /                    │            │
│     │              ●───●                       │            │
│     │                                          │            │
│     └─────────────────────────────────────────┘            │
│                    ↓                                        │
│  Layer 0 (Dense - All points)                               │
│     ┌─────────────────────────────────────────┐            │
│     │                                          │            │
│     │  ●─●─●─●─●─●─●─●─●─●─●─●─●─●─●─●─●        │            │
│     │   │ \ │ / │ \ │ / │ \ │ / │ \ │          │            │
│     │  ●─●─●─●─●─●─●─●─●─●─●─●─●─●─●─●─●─●       │            │
│     │   │ / │ \ │ / │ \ │ / │ \ │ \ │          │            │
│     │  ●─●─●─●─●─●─●─●─●─●─●─●─●─●─●─●─●─●       │            │
│     │                                          │            │
│     └─────────────────────────────────────────┘            │
│                                                              │
│  Search Process:                                            │
│  1. Start at Layer 2 (entry point)                          │
│  2. Greedy search to find closest point                     │
│  3. Move to Layer 1, repeat                                 │
│  4. Move to Layer 0, refine search                          │
│  5. Return nearest neighbors                                │
│                                                              │
│  Complexity: O(log n) vs O(n) for brute force              │
│                                                              │
└─────────────────────────────────────────────────────────────┘
```

---

## 🚀 Getting Started

### Step 1: Generate Embeddings

First, you need an embedding model. Here are popular options:

```python
# Option 1: sentence-transformers (Python)
from sentence_transformers import SentenceTransformer

model = SentenceTransformer('all-MiniLM-L6-v2')
text = "The quick brown fox jumps over the lazy dog"
embedding = model.encode(text)

print(embedding.shape)  # (384,)
print(embedding[:5])    # [0.23, -0.45, 0.67, 0.12, -0.34]
```

```javascript
// Option 2: OpenAI API (Node.js)
const openai = require('openai');

async function getEmbedding(text) {
  const response = await openai.embeddings.create({
    model: "text-embedding-3-small",
    input: text
  });
  return response.data[0].embedding;
}
```

```bash
# Option 3: Use a pre-computed embedding service
curl https://api.embeddings.com/v1/embed \
  -H "Content-Type: application/json" \
  -d '{"text": "Your text here"}'
```

### Step 2: Insert Vectors

```bash
# Upsert a vector into the "documents" collection
curl -X POST http://localhost:3000/v1/vectors/documents/upsert \
  -H "Content-Type: application/json" \
  -d '{
    "id": "doc1",
    "embedding": [0.23, -0.45, 0.67, 0.12, -0.34, ...],
    "data": {
      "title": "Introduction to Stackhouse",
      "content": "Stackhouse is a schema-later database...",
      "category": "database",
      "url": "https://stackhouse.dev/intro"
    }
  }'
```

**Response (201 Created):**
```json
{
  "success": true,
  "data": { "id": "doc1", "collection": "documents", "dimensions": 5 },
  "message": "Vector upserted successfully"
}
```

`id` is optional — omit it to get an auto-generated UUID. `column` defaults to
`"embedding"` and only needs to be set if a collection stores more than one
named vector per point.

### Step 3: Search for Similar Vectors

```bash
curl -X POST http://localhost:3000/v1/vectors/documents/search \
  -H "Content-Type: application/json" \
  -d '{
    "vector": [0.25, -0.43, 0.65, 0.10, -0.30, ...],
    "top_k": 10,
    "metric": "cosine"
  }'
```

**Response:**
```json
{
  "success": true,
  "count": 3,
  "collection": "documents",
  "metric": "cosine",
  "data": [
    { "id": "doc1", "similarity": 0.88, "data": { "title": "Introduction to Stackhouse", "category": "database" } },
    { "id": "doc5", "similarity": 0.77, "data": { "title": "Getting Started with Databases", "category": "database" } },
    { "id": "doc12", "similarity": 0.66, "data": { "title": "Python Programming Guide", "category": "programming" } }
  ]
}
```

---

## 📖 API Reference

All routes are mounted under `/v1/vectors` (`stackhouse/src/storage/vectors.rs`,
`create_vector_router`). There is no list-all-collections or delete-by-id
endpoint — only the four routes below exist.

### Upsert Vector

```bash
POST /v1/vectors/:collection/upsert
```

**Request Body:**
```json
{
  "id": "string",             // Optional: omit for an auto-generated UUID
  "embedding": [float, ...],  // Required: the vector
  "data": {...},              // Optional: payload stored alongside the vector
  "column": "embedding"       // Optional: named vector column (default: "embedding")
}
```

**Example:**
```bash
curl -X POST http://localhost:3000/v1/vectors/products/upsert \
  -H "Content-Type: application/json" \
  -d '{
    "id": "prod_12345",
    "embedding": [0.12, 0.34, -0.56, ...],
    "data": {
      "name": "Wireless Headphones",
      "price": 99.99,
      "category": "Electronics"
    }
  }'
```

### Batch Upsert

```bash
POST /v1/vectors/:collection/batch
```

**Request Body:**
```json
{ "records": [ { "id": "...", "embedding": [...], "data": {...} }, ... ] }
```

Returns `{"success": true, "data": {"ids": [...], "collection": "...", "count": N}}`.
Errors with `400` if `records` is empty.

### Search Vectors

```bash
POST /v1/vectors/:collection/search
```

**Request Body:**
```json
{
  "vector": [float, ...],   // Required: query vector
  "top_k": 10,               // Optional (default: 10)
  "metric": "cosine",        // Optional: "cosine" | "l2" | "inner_product" (default: "cosine")
  "filters": {...},          // Optional: metadata filter, forwarded to Qdrant
  "column": "embedding"      // Optional: named vector column (default: "embedding")
}
```

**Response:**
```json
{
  "success": true,
  "count": 1,
  "collection": "products",
  "metric": "cosine",
  "data": [
    { "id": "prod_12345", "similarity": 0.95, "data": { "name": "Wireless Headphones", "price": 99.99 } }
  ]
}
```

### Collection Info

```bash
GET /v1/vectors/:collection/info
```

Returns `{"success": true, "data": [...], "collection": "..."}` with per-vector-column
metadata (`table`, `column`, `dimensions`, `index_type`, `row_count`).

---

## 📏 Distance Metrics

Set via the `metric` field on a search request (`DistanceMetric` in
`stackhouse/src/storage/vectors.rs`); Qdrant performs the actual computation.
Three metrics are supported: `cosine` (default), `l2` (Euclidean), and
`inner_product` (alias `dot`).

### Cosine Similarity (Default)

```
┌─────────────────────────────────────────────────────────────┐
│              COSINE SIMILARITY                               │
├─────────────────────────────────────────────────────────────┤
│                                                              │
│  Measures the angle between two vectors                     │
│  Range: [-1, 1]                                             │
│  • 1 = Identical direction                                  │
│  • 0 = Orthogonal (uncorrelated)                            │
│  • -1 = Opposite direction                                  │
│                                                              │
│  Formula:                                                   │
│  ┌──────────────────────────────────────┐                  │
│  │  similarity = A · B                 │                  │
│  │              ─────                  │                  │
│  │              │A│ × │B│               │                  │
│  │                                      │                  │
│  │  distance = 1 - similarity          │                  │
│  └──────────────────────────────────────┘                  │
│                                                              │
│  Best for:                                                  │
│  ✅ Semantic similarity                                     │
│  ✅ Text embeddings                                         │
│  ✅ Recommendation systems                                  │
│                                                              │
│  Example:                                                   │
│  A = [1, 0, 0]                                            │
│  B = [1, 0, 0]                                            │
│  Similarity = 1.0 (same direction)                         │
│  Distance = 0.0                                            │
│                                                              │
└─────────────────────────────────────────────────────────────┘
```

### Euclidean Distance

```
┌─────────────────────────────────────────────────────────────┐
│              EUCLIDEAN DISTANCE                              │
├─────────────────────────────────────────────────────────────┤
│                                                              │
│  Measures straight-line distance                            │
│  Range: [0, ∞)                                              │
│  • 0 = Identical                                            │
│  • Larger = More different                                 │
│                                                              │
│  Formula:                                                   │
│  ┌──────────────────────────────────────┐                  │
│  │  distance = √Σ(Aᵢ - Bᵢ)²            │                  │
│  └──────────────────────────────────────┘                  │
│                                                              │
│  Best for:                                                  │
│  ✅ Geometric data                                          │
│  ✅ Image embeddings                                        │
│  ✅ Physical coordinates                                    │
│                                                              │
│  Example:                                                   │
│  A = [0, 0]                                               │
│  B = [3, 4]                                               │
│  Distance = 5.0 (Pythagorean theorem)                      │
│                                                              │
└─────────────────────────────────────────────────────────────┘
```

### Inner Product (Dot Product)

```
┌─────────────────────────────────────────────────────────────┐
│              INNER PRODUCT / DOT PRODUCT                     │
├─────────────────────────────────────────────────────────────┤
│                                                              │
│  Measures the raw dot product of two vectors                │
│  Sensitive to magnitude as well as direction                │
│                                                              │
│  Formula:                                                   │
│  ┌──────────────────────────────────────┐                  │
│  │  score = A · B = Σ(Aᵢ × Bᵢ)         │                  │
│  └──────────────────────────────────────┘                  │
│                                                              │
│  Best for:                                                  │
│  ✅ Models trained with dot-product similarity (e.g. some    │
│     recommender embeddings)                                 │
│  ✅ When embedding magnitude is itself meaningful            │
│                                                              │
│  Set via `"metric": "inner_product"` (or `"dot"`) in a       │
│  search request.                                             │
│                                                              │
└─────────────────────────────────────────────────────────────┘
```

### Choosing the Right Metric

```
┌─────────────────────────────────────────────────────────────┐
│           METRIC SELECTION GUIDE                             │
├─────────────────────────────────────────────────────────────┤
│                                                              │
│  Use Cosine Similarity when:                                │
│  ✅ Comparing text/documents                                 │
│  ✅ Magnitude doesn't matter                                 │
│  ✅ Using semantic embeddings                                │
│                                                              │
│  Use Euclidean Distance when:                               │
│  ✅ Physical distance matters                                │
│  ✅ Working with coordinates                                 │
│  ✅ Image feature vectors                                    │
│                                                              │
│  Comparison:                                                │
│  ┌────────────────────────────────────┐                    │
│  │  A = [1, 2, 3]                    │                    │
│  │  B = [2, 4, 6]  (A × 2)           │                    │
│  │                                   │                    │
│  │  Cosine: 0 (same direction)       │                    │
│  │  Euclidean: 3.74 (different)      │                    │
│  └────────────────────────────────────┘                    │
│                                                              │
└─────────────────────────────────────────────────────────────┘
```

---

## ⚡ Performance

### Benchmarks

There is no bundled benchmark suite for this path (see
[Benchmarks](./62-Benchmarks.md) for what Stackhouse does measure — it does not
currently include vector search). Search performance and index-build time are
governed entirely by the external Qdrant deployment's own HNSW implementation,
its configured `ef_construct`/`m` parameters, and hardware — not by anything in
Stackhouse's code — so no specific latency/recall numbers are quoted here. Consult
Qdrant's own published benchmarks for representative figures, and measure
against your own Qdrant deployment before relying on any number for capacity
planning.

### Optimization Tips

```
┌─────────────────────────────────────────────────────────────┐
│            PERFORMANCE OPTIMIZATION                          │
├─────────────────────────────────────────────────────────────┤
│                                                              │
│  1. Vector Dimensionality                                   │
│     ┌────────────────────────────────────┐                 │
│     │  128-384: Fast, good for text     │                 │
│     │  768-1024: Better accuracy        │                 │
│     │  1536+: Best quality, slower      │                 │
│     └────────────────────────────────────┘                 │
│                                                              │
│  2. Index Size                                               │
│     • More vectors = Better accuracy, slower search         │
│     • Consider sharding for >10M vectors                    │
│                                                              │
│  3. K Value                                                  │
│     • Small K (5-10): Fast                                  │
│     • Large K (50-100): More comprehensive                  │
│                                                              │
│  4. Batch Insertions                                         │
│     ┌────────────────────────────────────┐                 │
│     │  for (const doc of documents) {    │                 │
│     │    await insertVector(doc);        │                 │
│     │  }                                │                 │
│     │                                   │                 │
│     │  // BETTER:                       │                 │
│     │  await insertVectorBatch(docs);   │                 │
│     └────────────────────────────────────┘                 │
│                                                              │
└─────────────────────────────────────────────────────────────┘
```

---

## 💡 Use Cases

### 1. Semantic Document Search

```python
import requests

STACKHOUSE_URL = "http://localhost:8080"

# Index documents
documents = [
    {
        "id": "doc1",
        "text": "Stackhouse is a schema-later database",
        "vector": encode("Stackhouse is a schema-later database")
    },
    {
        "id": "doc2",
        "text": "Python is a programming language",
        "vector": encode("Python is a programming language")
    }
]

# Insert vectors
for doc in documents:
    requests.post(
        f"{STACKHOUSE_URL}/v1/vectors/docs/upsert",
        json={
            "id": doc["id"],
            "embedding": doc["vector"],
            "data": {"text": doc["text"]}
        }
    )

# Semantic search
query = "database that adapts to my data"
query_vector = encode(query)

response = requests.post(
    f"{STACKHOUSE_URL}/v1/vectors/docs/search",
    json={"vector": query_vector, "top_k": 5}
)

print(response.json())
# Returns: {"success": true, "data": [{"id": "doc1", "similarity": 0.85, ...}], ...}
```

### 2. Product Recommendations

```javascript
// Find similar products
async function recommendProducts(productId) {
  // Get product vector
  const product = await getVector('products', productId);

  // Search for similar products
  const response = await fetch(
    'http://localhost:3000/v1/vectors/products/search',
    {
      method: 'POST',
      headers: {'Content-Type': 'application/json'},
      body: JSON.stringify({
        vector: product.vector,
        top_k: 10
      })
    }
  );

  const results = await response.json();

  // Filter out the same product
  return results.data.filter(r => r.id !== productId);
}
```

### 3. Image Similarity Search

```python
from PIL import Image
import torchvision.models as models
import torchvision.transforms as transforms

# Load pre-trained ResNet
resnet = models.resnet50(pretrained=True)
resnet.eval()

# Transform and extract features
transform = transforms.Compose([
    transforms.Resize(256),
    transforms.CenterCrop(224),
    transforms.ToTensor(),
    transforms.Normalize(mean=[0.485, 0.456, 0.406],
                       std=[0.229, 0.224, 0.225])
])

def extract_features(image_path):
    image = Image.open(image_path)
    image = transform(image).unsqueeze(0)
    with torch.no_grad():
        features = resnet(image)
    return features.flatten().tolist()

# Index images
for img_path in glob("images/*.jpg"):
    features = extract_features(img_path)
    requests.post(
        f"{STACKHOUSE_URL}/v1/vectors/images/upsert",
        json={
            "id": img_path,
            "embedding": features,
            "data": {"path": img_path}
        }
    )

# Search similar images
query_features = extract_features("query.jpg")
response = requests.post(
    f"{STACKHOUSE_URL}/v1/vectors/images/search",
    json={"vector": query_features, "top_k": 10}
)
```

### 4. RAG (Retrieval Augmented Generation)

```python
import openai

def rag_query(question):
    # 1. Encode question
    question_vector = encode(question)

    # 2. Retrieve relevant documents
    response = requests.post(
        f"{STACKHOUSE_URL}/v1/vectors/knowledge_base/search",
        json={"vector": question_vector, "top_k": 5}
    )

    context = "\n".join([
        r["data"]["text"]
        for r in response.json()["data"]
    ])

    # 3. Generate answer with context
    completion = openai.ChatCompletion.create(
        model="gpt-4",
        messages=[
            {"role": "system", "content": "Answer using this context:\n" + context},
            {"role": "user", "content": question}
        ]
    )

    return completion.choices[0].message.content
```

---

## 🎓 Best Practices

### 1. Embedding Model Selection

```
┌─────────────────────────────────────────────────────────────┐
│          EMBEDDING MODEL COMPARISON                          │
├─────────────────────────────────────────────────────────────┤
│                                                              │
│  Model                    │ Dim  │ Speed  │ Quality         │
│  ─────────────────────────────────────────────────────────  │
│  all-MiniLM-L6-v2        │ 384  │ ⚡⚡⚡ │ ⭐⭐⭐          │
│  all-mpnet-base-v2       │ 768  │ ⚡⚡  │ ⭐⭐⭐⭐         │
│  text-embedding-3-small  │ 1536 │ ⚡   │ ⭐⭐⭐⭐⭐        │
│  text-embedding-3-large  │ 3072 │ ⚡   │ ⭐⭐⭐⭐⭐        │
│                                                              │
│  Recommendations:                                            │
│  • Start with all-MiniLM-L6-v2 (fast, good enough)          │
│  • Upgrade to OpenAI for production                         │
│  • Use consistent model across all data                     │
│                                                              │
└─────────────────────────────────────────────────────────────┘
```

### 2. Index Organization

```python
# ✅ GOOD: Separate indexes by use case
/v1/vectors/documents     # Text search
/v1/vectors/products      # Product recommendations
/v1/vectors/users         # User similarity

# ❌ BAD: Everything in one index
/v1/vectors/everything    # Harder to manage
```

### 3. Metadata Design

```python
# ✅ GOOD: Rich payload data for filtering
{
  "id": "doc123",
  "embedding": [...],
  "data": {
    "title": "...",
    "category": "tech",
    "created_at": "2025-01-03",
    "author": "alice",
    "tags": ["database", "rust", "performance"]
  }
}

# ❌ BAD: Minimal payload data
{
  "id": "doc123",
  "embedding": [...],
  "data": {"title": "..."}
}
```

---

## 📚 Further Reading

- [JavaScript Functions](./21-WASM-Functions.md) - Process vectors with custom logic
- [Realtime](./22-Realtime.md) - Live vector updates
- [Performance Guide](./41-Performance.md) - Optimize vector operations

---

**Ready to add AI to your app?** Continue to [JavaScript Functions](./21-WASM-Functions.md) 🚀
