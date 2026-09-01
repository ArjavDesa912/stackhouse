# Security Policy

## Reporting a Vulnerability

If you find a security vulnerability in Stackhouse, please **do not open a public issue**. Instead, report it privately via [GitHub Security Advisories](https://github.com/ArjavDesa912/stackhouse/security/advisories/new) for this repository.

Please include:
- A description of the vulnerability and its impact
- Steps to reproduce (a minimal repro is ideal)
- Any suggested fix, if you have one

You should receive an acknowledgment within a few days. This is a single-maintainer project — please be patient, but reports won't be ignored.

## Supported Versions

Stackhouse is pre-1.0. Security fixes are made against the `main` branch; there is no long-term-support branch yet.

| Version | Supported |
|---|---|
| `main` | ✅ |
| Tagged pre-1.0 releases | Best effort |

## Current Security Posture

This describes the technical security controls currently implemented and the deployment assumptions they rely on. **It is not a compliance statement** — Stackhouse does not claim SOC 2, ISO 27001, or FedRAMP readiness on code alone.

### Implemented controls

- **Privileged access control** — admin surfaces (branching, extensions, network, backup, log-drain, raw SQL, destructive admin routes) require `service_admin` access. Raw SQL and destructive admin capabilities are **disabled by default**.
- **Audit logging** — route-level admin audit entries record actor, action, target, outcome, and route metadata for privileged handlers.
- **Team-scoped authorization** — membership and invitation routes are scoped to their owning team.
- **Encrypted secrets at rest** — MFA TOTP secrets and backup artifacts are encrypted before persistence.
- **Safer runtime defaults** — CORS uses an explicit allowlist; risky capabilities default to disabled rather than enabled.

### Deployment checklist

- [ ] Configure an explicit CORS allowlist before exposing the service publicly.
- [ ] Provide real encryption key material for MFA TOTP and backup workflows (don't ship with defaults).
- [ ] Keep raw SQL and destructive admin capabilities disabled unless a trusted operator explicitly needs them.
- [ ] Review `service_admin` assignment carefully — it gates every privileged operation.
- [ ] Treat this document as a technical control baseline, not proof of an audited security program.

### Explicitly not implemented yet

These are intentionally deferred and should not be inferred from the codebase:

- Enforced MFA for every account
- HTTP hardening headers by default
- Payload size caps
- Outbound request (SSRF) protections
- Blanket OWASP Top 10 coverage
- FIPS validation
- Encryption-at-rest for all product data (only the items listed above are encrypted today)

If you're evaluating Stackhouse for a regulated environment, read this list carefully and file an issue if you need one of these gaps closed.
