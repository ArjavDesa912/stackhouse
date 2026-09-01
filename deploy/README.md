# Deployment Assets

This folder holds deployment and infrastructure assets shared by the repository.

- `helm/` contains installable Helm charts, used for the GKE Autopilot deployment path

## Where to deploy

**GCP is the recommended and supported target.** Step-by-step instructions for both the
low-cost Cloud Run path (Free/Starter/Pro tiers) and the GKE Autopilot path (Team/Enterprise,
using the Helm chart in this folder) are in
[`stackhouse/docs/40-Deployment.md`](../stackhouse/docs/40-Deployment.md#-gcp-deployment-recommended).

The cost model and pricing tiers that deployment target is designed around are documented in
[`BUSINESS_PLAN_GCP.md`](../BUSINESS_PLAN_GCP.md) at the repo root, alongside the competitive
positioning in [`STACKHOUSE_VS_SUPABASE.md`](../STACKHOUSE_VS_SUPABASE.md).
