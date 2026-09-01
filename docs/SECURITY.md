# Stackhouse Security Documentation

## Overview

This document describes the current technical security controls on this branch and the deployment assumptions they rely on. It is not a full security program and it is not a compliance statement.

## Implemented Controls

### Privileged access control

The current branch requires `service_admin` access on the covered admin surfaces, including branching, extensions, network, backup, log-drain, raw-SQL, and destructive admin routes. Raw SQL and destructive admin capabilities remain disabled by default unless an operator intentionally enables them.

### Audit evidence on covered admin surfaces

The readiness pass adds route-level admin audit entries for the privileged surfaces covered on this branch. These entries record actor, action, target or target type, outcome, and route metadata for the touched handlers.

### Team-scoped authorization

Membership and invitation routes now enforce team-scoped authorization so those actions stay within the owning team context.

### Protected authentication secrets

MFA TOTP secrets are encrypted before persistence. This protects the stored TOTP secret material only; it does not mean all product data is encrypted at rest.

### Protected backup artifacts

Backup artifacts are encrypted at rest before they are written to disk or storage.

### Safer runtime defaults

Cycle 1 also tightens default deployment posture:

- CORS uses an explicit allowlist.
- Helm/runtime defaults keep risky capabilities disabled unless they are deliberately enabled.
- The repo no longer treats raw SQL or destructive admin operations as safe-by-default capabilities.

## Safe Deployment Notes

- Configure an explicit CORS allowlist before exposing the service.
- Provide the encryption key material required by the MFA TOTP and backup workflows.
- Keep raw SQL and destructive admin capabilities disabled unless a trusted operator explicitly needs them.
- Review service-admin assignment carefully because it gates privileged operations.
- Treat the repo as a technical control baseline, not as evidence of SOC 2, ISO 27001, or FedRAMP readiness on its own.
- Use `docs/audit-readiness/` for the branch-specific evidence map, gaps, and readiness checklist.

## Not Implemented In Cycle 1

These items are intentionally deferred and should not be inferred from the current codebase or this document:

- Enforced MFA for every account.
- HTTP hardening headers.
- Payload size caps.
- Outbound request protections.
- Blanket OWASP Top 10 coverage.
- FIPS validation.
- Blanket encryption-at-rest claims for all product data.
- Organizational evidence and operating-effectiveness claims.

## Review Checklist

When auditing the Cycle 1 hardening work, verify:

- Service-admin authorization is present on the currently covered admin surfaces.
- Route-level admin audit entries are present on the covered privileged handlers.
- Raw SQL and destructive admin capabilities stay disabled by default.
- TOTP secrets are encrypted before storage.
- Backup artifacts are encrypted before storage.
- Membership and invitation routes are team-scoped.
- CORS is configured with an explicit allowlist.
- Remaining readiness gaps are tracked in `docs/audit-readiness/gap-register.md`.
