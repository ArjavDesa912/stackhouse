# 40 - Deployment

## 🚀 Production Deployment Guide

> **Recommended path:** deploy on **Google Cloud Platform** — see [GCP Deployment](#-gcp-deployment-recommended) below. The cost model and pricing tiers this deployment target is designed around are documented in [`BUSINESS_PLAN_GCP.md`](../../BUSINESS_PLAN_GCP.md). The generic Docker/systemd instructions further down remain valid for self-hosting on any VM or bare metal.

---

## ☁️ GCP Deployment (Recommended)

Two paths, matched to the tiers in `BUSINESS_PLAN_GCP.md`:

- **Cloud Run** — for Free/Starter/Pro tiers and most self-hosters. True scale-to-zero, no cluster to manage, cheapest option by a wide margin below sustained-load traffic.
- **GKE Autopilot** — for Team/Enterprise tiers, or once you need Qdrant/Redis/Postgres running as first-class stateful workloads alongside Stackhouse. Uses the existing Helm chart at `deploy/helm/stackhouse/`.

Both assume the `stackhouse/Dockerfile` in this repo, which already builds the UI and a slim Debian runtime image.

### Option A: Cloud Run (launch / low-cost path)

**1. One-time project setup**

```bash
export PROJECT_ID=your-gcp-project
export REGION=us-central1
gcloud config set project $PROJECT_ID

gcloud services enable \
  run.googleapis.com \
  sqladmin.googleapis.com \
  redis.googleapis.com \
  artifactregistry.googleapis.com \
  secretmanager.googleapis.com \
  vpcaccess.googleapis.com

gcloud artifacts repositories create stackhouse \
  --repository-format=docker --location=$REGION
```

**2. Provision managed dependencies**

```bash
# Cloud SQL for PostgreSQL — start with the smallest tier, resize later
gcloud sql instances create stackhouse-pg \
  --database-version=POSTGRES_16 \
  --tier=db-f1-micro \
  --region=$REGION \
  --storage-auto-increase

gcloud sql databases create stackhouse --instance=stackhouse-pg

# Memorystore (Redis) — Basic tier is enough below the Team plan
gcloud redis instances create stackhouse-cache \
  --size=1 --region=$REGION --tier=basic

# Serverless VPC connector so Cloud Run can reach Memorystore's private IP
gcloud compute networks vpc-access connectors create stackhouse-connector \
  --region=$REGION --range=10.8.0.0/28
```

**3. Store secrets**

```bash
echo -n "$(openssl rand -hex 32)" | gcloud secrets create stackhouse-jwt-secret --data-file=-
echo -n "sk_live_..." | gcloud secrets create stackhouse-stripe-secret --data-file=-
echo -n "whsec_..." | gcloud secrets create stackhouse-stripe-webhook-secret --data-file=-
```

**4. Build and push**

```bash
cd stackhouse
gcloud builds submit --tag $REGION-docker.pkg.dev/$PROJECT_ID/stackhouse/stackhouse:latest
```

**5. Deploy**

```bash
gcloud run deploy stackhouse \
  --image=$REGION-docker.pkg.dev/$PROJECT_ID/stackhouse/stackhouse:latest \
  --region=$REGION \
  --platform=managed \
  --allow-unauthenticated \
  --port=8080 \
  --min-instances=0 \
  --max-instances=10 \
  --cpu=1 --memory=512Mi \
  --vpc-connector=stackhouse-connector \
  --add-cloudsql-instances=$PROJECT_ID:$REGION:stackhouse-pg \
  --set-env-vars="STACKHOUSE_HOST=0.0.0.0,STACKHOUSE_PORT=8080,STACKHOUSE_ENABLE_BILLING=true" \
  --set-env-vars="DATABASE_URL=postgres://postgres:PASSWORD@/stackhouse?host=/cloudsql/$PROJECT_ID:$REGION:stackhouse-pg" \
  --set-secrets="STACKHOUSE_JWT_SECRET=stackhouse-jwt-secret:latest,STRIPE_SECRET_KEY=stackhouse-stripe-secret:latest,STACKHOUSE_BILLING_STRIPE_SIGNING_SECRET=stackhouse-stripe-webhook-secret:latest"
```

`--min-instances=0` is what makes Free/Starter tenants near-free to host when idle — see `BUSINESS_PLAN_GCP.md` §5–6 for the cost model this relies on. Raise it to `1` for Pro-tier tenants that need to avoid cold starts.

**Cost note:** at list price this is $0.000024/vCPU-sec + $0.0000025/GiB-sec, with the first 180K vCPU-sec and 360K GiB-sec/month free — a low-traffic tenant costs cents per month. Re-check current rates at [cloud.google.com/run/pricing](https://cloud.google.com/run/pricing).

### Option B: GKE Autopilot (Team / Enterprise / high-sustained-load)

The Helm chart in `deploy/helm/stackhouse/` already ships Qdrant, PostgreSQL (Bitnami sub-chart), and Redis as dependencies — Autopilot just removes node management.

```bash
gcloud container clusters create-auto stackhouse-cluster --region=$REGION

gcloud container clusters get-credentials stackhouse-cluster --region=$REGION

# Create the secrets the chart expects (see deploy/helm/stackhouse/values.yaml)
kubectl create secret generic stackhouse-secrets \
  --from-literal=database-url="postgres://stackhouse:PASSWORD@stackhouse-postgresql:5432/stackhouse" \
  --from-literal=jwt-secret="$(openssl rand -hex 32)" \
  --from-literal=redis-url="redis://:PASSWORD@stackhouse-redis-master:6379" \
  --from-literal=qdrant-url="http://stackhouse-qdrant:6333" \
  --from-literal=data-encryption-key="$(openssl rand -hex 32)"

helm dependency update deploy/helm/stackhouse
helm install stackhouse deploy/helm/stackhouse \
  --set stackhouse.image.repository=$REGION-docker.pkg.dev/$PROJECT_ID/stackhouse/stackhouse \
  --set stackhouse.image.tag=latest
```

Autopilot bills per-Pod CPU/memory request rather than per-node, so the chart's existing `resources.requests` in `values.yaml` directly determine cost — tune them down for smaller Team-tier tenants rather than over-provisioning by default.

### Choosing between them

| | Cloud Run | GKE Autopilot |
|---|---|---|
| Best for | Free, Starter, Pro, most self-hosters | Team, Enterprise, sustained high load |
| Scale-to-zero | Yes | No (Autopilot still bills scheduled Pod requests) |
| Stateful sidecars (Qdrant/Redis/Postgres in-cluster) | No — use managed Cloud SQL/Memorystore instead | Yes, via the Helm chart |
| Ops overhead | Near zero | Low (no node management, but still a cluster) |

---

### Docker Deployment (generic / any host)

**docker-compose.yml:**
```yaml
version: '3.8'
services:
  stackhouse:
    image: stackhouse:latest
    ports:
      - "3000:3000"
    volumes:
      - stackhouse_data:/app/data
    environment:
      - RUST_LOG=info
      - STACKHOUSE_DB_PATH=/app/data

volumes:
  stackhouse_data:
```

```bash
docker-compose up -d
```

### Systemd Service

```ini
[Unit]
Description=Stackhouse Server
After=network.target

[Service]
Type=simple
User=stackhouse
WorkingDirectory=/opt/stackhouse
ExecStart=/usr/local/bin/stackhouse \
  --db /var/lib/stackhouse/data \
  --port 3000
Restart=always
RestartSec=10

[Install]
WantedBy=multi-user.target
```

### Configuration

```toml
# /etc/stackhouse/config.toml
[server]
port = 3000
host = "0.0.0.0"
workers = 4

[database]
path = "/var/lib/stackhouse/data"
wal_size = "1GB"
memtable_size = "128MB"

[compaction]
trigger_l0 = 4
trigger_levels = "size_based"

[security]
jwt_secret = "your-secret-key"
token_expiry = "3600"
```

### Reverse Proxy (Nginx)

```nginx
location / {
    proxy_pass http://localhost:3000;
    proxy_http_version 1.1;
    proxy_set_header Upgrade $http_upgrade;
    proxy_set_header Connection "upgrade";
    proxy_set_header Host $host;
}
```

### Scaling

```
┌─────────────────────────────────────────────────────────────┐
│              SCALING STRATEGIES                               │
├─────────────────────────────────────────────────────────────┤
│                                                              │
│  Vertical Scaling:                                           │
│  • More CPU cores                                            │
│  • More memory                                               │
│  • Faster SSD                                                │
│                                                              │
│  Horizontal Scaling:                                         │
│  • Read replicas (followers)                                 │
│  • Connection pooling                                       │
│  • Load balancing                                           │
│                                                              │
└─────────────────────────────────────────────────────────────┘
```

---

**Next:** [Performance](./41-Performance.md)
