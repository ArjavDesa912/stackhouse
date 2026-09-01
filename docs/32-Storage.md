# 32 - Storage

## 📦 File Storage System

### Creating Buckets

```bash
# Public bucket (files accessible via URL)
curl -X POST http://localhost:3000/v1/storage/buckets/public \
  -H "Content-Type: application/json" \
  -d '{"public": true}'

# Private bucket (requires auth)
curl -X POST http://localhost:3000/v1/storage/buckets/private \
  -H "Content-Type: application/json" \
  -d '{"public": false}'
```

### Uploading Files

```bash
curl -X POST http://localhost:3000/v1/storage/upload/public \
  -F "file=@document.pdf" \
  -F "metadata={\"title\": \"My Document\"}"
```

### Downloading Files

```bash
# Public file (no auth required)
curl http://localhost:3000/v1/storage/file/public/document.pdf -O

# Private file (requires auth)
curl http://localhost:3000/v1/storage/file/private/secret.pdf \
  -H "Authorization: Bearer <token>" -O
```

### Listing Files

```bash
curl http://localhost:3000/v1/storage/files/public
```

**Response:**
```json
{
  "files": [
    {
      "id": "uuid",
      "name": "document.pdf",
      "size": 1024000,
      "content_type": "application/pdf",
      "created_at": "2025-01-03T12:00:00Z"
    }
  ]
}
```

### Deleting Files

```bash
curl -X DELETE http://localhost:3000/v1/storage/file/public/document.pdf
```

---

**Next:** [Replication](./33-Replication.md)
