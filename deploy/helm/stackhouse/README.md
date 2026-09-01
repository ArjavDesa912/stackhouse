# Stackhouse Helm Chart

## Overview

Production-grade Helm chart for deploying Stackhouse with all its dependencies:

- **Stackhouse API** — Rust/Axum application pods
- **PostgreSQL** — Primary relational database (Bitnami sub-chart)
- **Qdrant** — Dedicated vector database for similarity search
- **Redis** — Cache, rate limiting, and pub/sub (Bitnami sub-chart)

## Prerequisites

- Kubernetes 1.25+
- Helm 3.10+
- PV provisioner (for persistent storage)

## Quick Start

```bash
# Add dependency repositories
helm repo add bitnami https://charts.bitnami.com/bitnami
helm repo add qdrant https://qdrant.github.io/qdrant-helm
helm repo update

# Install dependencies
cd deploy/helm/stackhouse
helm dependency build

# Create application and dependency secrets
kubectl create namespace stackhouse
POSTGRES_PASSWORD="$(openssl rand -hex 32)"
REDIS_PASSWORD="$(openssl rand -hex 32)"
JWT_SECRET="$(openssl rand -hex 32)"
ENCRYPTION_KEY="$(openssl rand -hex 32)"

kubectl create secret generic stackhouse-postgresql-auth \
  --namespace stackhouse \
  --from-literal=postgres-password="$POSTGRES_PASSWORD" \
  --from-literal=password="$POSTGRES_PASSWORD" \
  --from-literal=replication-password="$POSTGRES_PASSWORD" \
  --from-literal=metrics-password="$POSTGRES_PASSWORD"

kubectl create secret generic stackhouse-redis-auth \
  --namespace stackhouse \
  --from-literal=redis-password="$REDIS_PASSWORD"

kubectl create secret generic stackhouse-secrets \
  --namespace stackhouse \
  --from-literal=database-url="postgres://stackhouse:${POSTGRES_PASSWORD}@stackhouse-postgresql:5432/stackhouse" \
  --from-literal=jwt-secret="$JWT_SECRET" \
  --from-literal=redis-url="redis://:${REDIS_PASSWORD}@stackhouse-redis-master:6379/0" \
  --from-literal=qdrant-url="http://stackhouse-qdrant:6333" \
  --from-literal=data-encryption-key="$ENCRYPTION_KEY"

# Install the chart
helm install stackhouse ./deploy/helm/stackhouse \
  --namespace stackhouse \
  --values deploy/helm/stackhouse/values.yaml
```

## Configuration

See `values.yaml` for all configurable parameters.

### Key Parameters

| Parameter | Description | Default |
|-----------|-------------|---------|
| `stackhouse.replicaCount` | Number of API pods | `3` |
| `stackhouse.image.repository` | Docker image | `ghcr.io/arjavdesa912/stackhouse` |
| `stackhouse.autoscaling.enabled` | Enable HPA | `true` |
| `qdrant.enabled` | Deploy Qdrant | `true` |
| `postgresql.enabled` | Deploy PostgreSQL | `true` |
| `redis.enabled` | Deploy Redis | `true` |

### Secure Deployment Notes

- Keep `stackhouse.secrets.secretName` pointing at a Kubernetes Secret and populate `database-url`, `jwt-secret`, `redis-url`, `qdrant-url`, and `data-encryption-key`.
- Set `stackhouse.env.STACKHOUSE_CORS_ALLOWED_ORIGINS` to a comma-separated allowlist such as `https://app.example.com,https://admin.example.com`.
- Do not leave the dependency credentials in `values.yaml`; create the `stackhouse-postgresql-auth` and `stackhouse-redis-auth` secrets before installing.
- Leave `STACKHOUSE_ENABLE_RAW_SQL` and `STACKHOUSE_ENABLE_DESTRUCTIVE_ADMIN` disabled unless you are intentionally granting service-admin access.

### Production Overrides

```yaml
# production-values.yaml
stackhouse:
  replicaCount: 10
  autoscaling:
    minReplicas: 10
    maxReplicas: 100
  resources:
    requests:
      cpu: "1000m"
      memory: "512Mi"
    limits:
      cpu: "4000m"
      memory: "2Gi"
  ingress:
    enabled: true
    className: nginx
    annotations:
      cert-manager.io/cluster-issuer: letsencrypt-prod
    hosts:
      - host: api.stackhouse.io
        paths:
          - path: /
            pathType: Prefix
    tls:
      - secretName: stackhouse-tls
        hosts:
          - api.stackhouse.io

qdrant:
  replicaCount: 3
  resources:
    requests:
      cpu: "2000m"
      memory: "4Gi"
    limits:
      cpu: "4000m"
      memory: "8Gi"
  persistence:
    size: 100Gi
```

## Architecture

```
┌─────────────────────────────────────────────┐
│              Kubernetes Cluster              │
│                                             │
│  ┌─────────────────────────────────────┐   │
│  │         Ingress Controller           │   │
│  └──────────────┬──────────────────────┘   │
│                 │                           │
│  ┌──────────────▼──────────────────────┐   │
│  │       Stackhouse API (3–100 pods)        │   │
│  │       Port 3000 · Rust/Axum          │   │
│  └──┬──────────┬────────────┬──────────┘   │
│     │          │            │              │
│  ┌──▼────┐ ┌──▼─────┐ ┌───▼──────────┐   │
│  │  PG   │ │ Redis  │ │   Qdrant     │   │
│  │ :5432 │ │ :6379  │ │ :6333/:6334  │   │
│  └───────┘ └────────┘ └──────────────┘   │
└─────────────────────────────────────────────┘
```

## Uninstall

```bash
helm uninstall stackhouse --namespace stackhouse
kubectl delete namespace stackhouse
```
