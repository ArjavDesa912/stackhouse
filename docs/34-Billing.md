# Stackhouse-Billing (RevenueCat-style Subscriptions)

Stackhouse-Billing is a native, opt-in subscription backend for Stackhouse that mirrors
RevenueCat's core server surface: apps, products, entitlements, offerings,
customers, subscriptions, and lifecycle webhooks. It integrates Apple App
Store, Google Play, and Stripe receipts/events out of the box.

## Enabling

Set the environment variable `STACKHOUSE_ENABLE_BILLING=1` before starting
`stackhouse`. When enabled, the module:

- runs idempotent migrations on boot (tables prefixed `billing_`)
- mounts its router at **`/v1/billing`**
- spawns a background outbound-webhook dispatcher

Optional environment fallbacks (overridable per-app via the admin API):

| Variable                               | Purpose                                   |
|----------------------------------------|-------------------------------------------|
| `STACKHOUSE_BILLING_APPLE_SHARED_SECRET`   | Apple `verifyReceipt` shared secret       |
| `STACKHOUSE_BILLING_STRIPE_SIGNING_SECRET` | Stripe webhook signing secret             |
| `STACKHOUSE_BILLING_GOOGLE_ACCESS_TOKEN`   | Service-account OAuth token (androidpublisher scope) |

## Data model

Tables (all `billing_*`):

- `apps`, `products`, `entitlements`, `entitlement_products`
- `offerings`, `packages`
- `customers`, `subscriptions`, `transactions`, `receipts`
- `webhook_endpoints`, `webhook_deliveries`

See `src/billing/schema.rs` for the full DDL.

## REST API

All JSON. Admin routes require a service-admin JWT; customer routes require
any authenticated user; inbound store-webhook routes are unauthenticated but
signature-verified.

### Admin

- `POST /v1/billing/admin/apps` — create an app
- `GET  /v1/billing/admin/apps`
- `POST /v1/billing/admin/apps/:app_id/secrets` — per-app Apple/Google/Stripe/webhook secrets
- `POST /v1/billing/admin/products` — upsert `{app_id, store, store_product_id, product_type, …}`
- `GET  /v1/billing/admin/products?app_id=…`
- `POST /v1/billing/admin/entitlements` — upsert; accepts `product_ids: []`
- `GET  /v1/billing/admin/entitlements?app_id=…`
- `POST /v1/billing/admin/offerings` — upsert with nested packages; setting `is_current=true` clears the flag on the app's other offerings
- `POST /v1/billing/admin/offerings/:offering_id/audience` — target an offering to an audience: `{audience_id?}` (omit/null to clear targeting)
- `POST /v1/billing/admin/experiments` / `GET /v1/billing/admin/experiments?app_id=…` — upsert or list A/B tests: `{app_id, identifier, metric?, audience_id?, variants: [{identifier, offering_id, is_control, traffic_weight}]}`
- `POST /v1/billing/admin/experiments/:id/status` — `{status}` (e.g. `draft`/`running`/`completed`)
- `GET  /v1/billing/admin/experiments/:id/results` — per-variant impression/conversion results
- `POST /v1/billing/admin/audiences` / `GET /v1/billing/admin/audiences?app_id=…` — upsert or list targeting audiences: `{app_id, identifier, display_name?, rules?}`
- `POST /v1/billing/admin/paywalls` / `GET /v1/billing/admin/paywalls?offering_id=…` — upsert or fetch a paywall config: `{offering_id, template?, config?, draft_config?}`
- `POST /v1/billing/admin/paywalls/:offering_id/publish` — promote `draft_config` to the live `config`
- `POST /v1/billing/admin/grant` — manually grant an entitlement (promo / refund recovery): `{app_id, app_user_id, product_id, duration_days}`
- `POST /v1/billing/admin/webhook-endpoints` — register an outbound listener

### Customer

- `GET  /v1/billing/customers/:app_user_id?app_id=…`
- `GET  /v1/billing/customers/:app_user_id/entitlements?app_id=…`
- `POST /v1/billing/customers/:app_user_id/attributes` — merge JSONB attributes
- `POST /v1/billing/customers/:app_user_id/alias`
- `POST /v1/billing/customers/:app_user_id/receipts/apple` — `{app_id, receipt_data}`
- `POST /v1/billing/customers/:app_user_id/receipts/google` — `{app_id, package_name, subscription_id, purchase_token, access_token?}`
- `POST /v1/billing/customers/:app_user_id/receipts/stripe` — `{app_id, event}`
- `GET  /v1/billing/customers/:app_user_id/offerings/resolve?app_id=…` — resolve the offering this specific user should see (audience targeting + running experiments)
- `POST /v1/billing/customers/:app_user_id/experiments/impression` — record a paywall impression: `{app_id, metadata?}`
- `POST /v1/billing/customers/:app_user_id/experiments/conversion` — record a conversion: `{app_id, metadata?}`

### Checkout & subscription management (JWT required)

A simpler, non-RevenueCat-style Stripe Checkout flow that coexists with the receipt-based customer API above:

- `POST /v1/billing/checkout` — create a Stripe Checkout session: `{app_id, price_id, app_user_id, customer_email?, success_url, cancel_url}`
- `POST /v1/billing/cancel` — cancel the caller's subscription: `{app_id, app_user_id}`
- `GET  /v1/billing/me` — the authenticated user's current subscription

### Public

- `GET  /v1/billing/health` — module health check
- `GET  /v1/billing/offerings?app_id=…`
- `GET  /v1/billing/plans?app_id=…` — list subscription plans

### Inbound store webhooks

- `POST /v1/billing/webhooks/apple` — App Store Server Notifications V2 `signedPayload`
- `POST /v1/billing/webhooks/google` — Google Cloud Pub/Sub push envelope
- `POST /v1/billing/webhooks/stripe?app_id=…` — Stripe signed webhook (`Stripe-Signature` header)

## Outbound webhook signature

Each delivery carries:

- `X-StackhouseBilling-Signature: t=<unix>,v1=<hex hmac-sha256>`
- `X-StackhouseBilling-Event: <EVENT_TYPE>`

Signature is computed over `<unix>.<raw-body>` using the endpoint secret.
Deliveries are retried up to 5 times with exponential backoff (60s → 5m → 30m → 2h → 12h).

## Entitlement resolver

`resolve_entitlements(app_id, customer_id, now)` returns an array of
`EntitlementInfo { identifier, is_active, will_renew, period_type,
latest_purchase_date, expires_date, grace_period_expires_date, store,
product_identifier }`. An entitlement is active when any linked subscription's
`current_period_end > now` **or** its `grace_period_expires_at > now`.

## Security caveats

- Apple App Store Server Notification V2 JWS payloads get real cryptographic
  signature verification (ES256/RS256 against the `x5c` leaf cert), `x5c`
  chain consistency (leaf → intermediate → root), and root-CA pinning to
  Apple's published Root CA - G3. A forged, internally-consistent chain
  whose root is not Apple Root CA - G3 is rejected. The pinned PEM is
  embedded in `stackhouse/src/billing/validators.rs`.
- Stripe signature verification uses the documented `t=…,v1=…` scheme with
  constant-time comparison and a 5-minute clock-skew tolerance.
- Google validation requires you to mint the OAuth2 service-account token
  yourself; pass it per-request or via `STACKHOUSE_BILLING_GOOGLE_ACCESS_TOKEN`.

## Frontend

Three companion frontend pieces ship alongside the server module:

### `@stackhouse/js` (JS/TS SDK) — `stackhouse/js-sdks/stackhouse-js`

```ts
import { createClient } from '@stackhouse/js';
const stackhouse = createClient('http://localhost:3000');
await stackhouse.signIn(email, password);

// Customer
const info = await stackhouse.billing.getCustomerInfo(appId, 'user-42');
if (await stackhouse.billing.hasEntitlement(appId, 'user-42', 'pro')) { /* unlock */ }
await stackhouse.billing.submitAppleReceipt(appId, 'user-42', receiptDataB64);

// Admin (requires service-admin JWT)
await stackhouse.billing.admin.upsertProduct({
  app_id: appId, store: 'app_store', store_product_id: 'pro.monthly',
});
```

### `@stackhouse/react` (React bindings) — `stackhouse/js-sdks/stackhouse-react`

```tsx
import { BillingProvider, EntitlementGate, Paywall } from '@stackhouse/react';

<BillingProvider appId={42} appUserId="user-42">
  <EntitlementGate identifier="pro" fallback={<Paywall onSelectPackage={buy} />}>
    <ProOnlyFeature />
  </EntitlementGate>
</BillingProvider>
```

Exports: `BillingProvider`, `useOfferings`, `useEntitlements`,
`useCustomerInfo`, `useHasEntitlement`, `<EntitlementGate>`, `<Paywall>`.

### Admin dashboard — `stackhouse/web/billing-admin`

Standalone React + Tailwind Vite app that talks to `/v1/billing/admin/*`:

```bash
cd stackhouse/web/billing-admin
npm install
STACKHOUSE_URL=http://localhost:8080 npm run dev   # serves on :5174
```

Manages apps, secrets, products, entitlements, offerings, promo grants, and
outbound webhook endpoints.

## Tests

Unit tests (no DB required):

```bash
cargo test --lib billing
```

Covers the entitlement resolver, Stripe signature round-trip, Apple JWS
decoding, and webhook payload signing/filtering.
