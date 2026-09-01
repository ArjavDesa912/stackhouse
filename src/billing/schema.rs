//! Idempotent Postgres DDL for the billing module.

pub const MIGRATIONS: &str = r#"
CREATE TABLE IF NOT EXISTS billing_apps (
    id            BIGSERIAL PRIMARY KEY,
    project_id    BIGINT,
    name          TEXT NOT NULL,
    bundle_id     TEXT,
    platform      TEXT NOT NULL DEFAULT 'multi',
    apple_shared_secret       TEXT,
    google_service_account    TEXT,
    stripe_signing_secret     TEXT,
    outbound_webhook_secret   TEXT,
    created_at    TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE TABLE IF NOT EXISTS billing_products (
    id                BIGSERIAL PRIMARY KEY,
    app_id            BIGINT NOT NULL REFERENCES billing_apps(id) ON DELETE CASCADE,
    store             TEXT NOT NULL,
    store_product_id  TEXT NOT NULL,
    product_type      TEXT NOT NULL DEFAULT 'auto_renewable',
    period            TEXT,
    price_micros      BIGINT,
    currency          TEXT,
    metadata          JSONB NOT NULL DEFAULT '{}'::jsonb,
    created_at        TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    UNIQUE (app_id, store, store_product_id)
);

CREATE TABLE IF NOT EXISTS billing_entitlements (
    id            BIGSERIAL PRIMARY KEY,
    app_id        BIGINT NOT NULL REFERENCES billing_apps(id) ON DELETE CASCADE,
    identifier    TEXT NOT NULL,
    display_name  TEXT,
    created_at    TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    UNIQUE (app_id, identifier)
);

CREATE TABLE IF NOT EXISTS billing_entitlement_products (
    entitlement_id  BIGINT NOT NULL REFERENCES billing_entitlements(id) ON DELETE CASCADE,
    product_id      BIGINT NOT NULL REFERENCES billing_products(id) ON DELETE CASCADE,
    PRIMARY KEY (entitlement_id, product_id)
);

CREATE TABLE IF NOT EXISTS billing_offerings (
    id          BIGSERIAL PRIMARY KEY,
    app_id      BIGINT NOT NULL REFERENCES billing_apps(id) ON DELETE CASCADE,
    identifier  TEXT NOT NULL,
    is_current  BOOLEAN NOT NULL DEFAULT FALSE,
    metadata    JSONB NOT NULL DEFAULT '{}'::jsonb,
    created_at  TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    UNIQUE (app_id, identifier)
);

CREATE TABLE IF NOT EXISTS billing_packages (
    id            BIGSERIAL PRIMARY KEY,
    offering_id   BIGINT NOT NULL REFERENCES billing_offerings(id) ON DELETE CASCADE,
    identifier    TEXT NOT NULL,
    product_id    BIGINT NOT NULL REFERENCES billing_products(id) ON DELETE CASCADE,
    package_type  TEXT,
    UNIQUE (offering_id, identifier)
);

CREATE TABLE IF NOT EXISTS billing_customers (
    id            BIGSERIAL PRIMARY KEY,
    app_id        BIGINT NOT NULL REFERENCES billing_apps(id) ON DELETE CASCADE,
    app_user_id   TEXT NOT NULL,
    aliases       JSONB NOT NULL DEFAULT '[]'::jsonb,
    attributes    JSONB NOT NULL DEFAULT '{}'::jsonb,
    first_seen_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    last_seen_at  TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    UNIQUE (app_id, app_user_id)
);
CREATE INDEX IF NOT EXISTS billing_customers_app_user_idx
    ON billing_customers (app_id, app_user_id);

CREATE TABLE IF NOT EXISTS billing_subscriptions (
    id                          BIGSERIAL PRIMARY KEY,
    customer_id                 BIGINT NOT NULL REFERENCES billing_customers(id) ON DELETE CASCADE,
    product_id                  BIGINT REFERENCES billing_products(id) ON DELETE SET NULL,
    store                       TEXT NOT NULL,
    original_transaction_id     TEXT,
    current_period_start        TIMESTAMPTZ,
    current_period_end          TIMESTAMPTZ,
    status                      TEXT NOT NULL DEFAULT 'active',
    auto_renew                  BOOLEAN NOT NULL DEFAULT TRUE,
    unsubscribe_detected_at     TIMESTAMPTZ,
    billing_issues_detected_at  TIMESTAMPTZ,
    grace_period_expires_at     TIMESTAMPTZ,
    is_trial                    BOOLEAN NOT NULL DEFAULT FALSE,
    updated_at                  TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    UNIQUE (customer_id, store, original_transaction_id)
);
CREATE INDEX IF NOT EXISTS billing_subs_customer_expires_idx
    ON billing_subscriptions (customer_id, current_period_end);

CREATE TABLE IF NOT EXISTS billing_transactions (
    id                     BIGSERIAL PRIMARY KEY,
    subscription_id        BIGINT REFERENCES billing_subscriptions(id) ON DELETE SET NULL,
    customer_id            BIGINT NOT NULL REFERENCES billing_customers(id) ON DELETE CASCADE,
    product_id             BIGINT REFERENCES billing_products(id) ON DELETE SET NULL,
    store                  TEXT NOT NULL,
    store_transaction_id   TEXT NOT NULL,
    purchased_at           TIMESTAMPTZ,
    expires_at             TIMESTAMPTZ,
    price_micros           BIGINT,
    currency               TEXT,
    is_trial               BOOLEAN NOT NULL DEFAULT FALSE,
    is_renewal             BOOLEAN NOT NULL DEFAULT FALSE,
    raw                    JSONB NOT NULL DEFAULT '{}'::jsonb,
    created_at             TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    UNIQUE (store, store_transaction_id)
);

CREATE TABLE IF NOT EXISTS billing_receipts (
    id                 BIGSERIAL PRIMARY KEY,
    customer_id        BIGINT NOT NULL REFERENCES billing_customers(id) ON DELETE CASCADE,
    store              TEXT NOT NULL,
    raw                TEXT NOT NULL,
    validated_at       TIMESTAMPTZ,
    validation_result  JSONB,
    created_at         TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE TABLE IF NOT EXISTS billing_webhook_endpoints (
    id         BIGSERIAL PRIMARY KEY,
    app_id     BIGINT NOT NULL REFERENCES billing_apps(id) ON DELETE CASCADE,
    url        TEXT NOT NULL,
    secret     TEXT NOT NULL,
    events     JSONB NOT NULL DEFAULT '[]'::jsonb,
    active     BOOLEAN NOT NULL DEFAULT TRUE,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE TABLE IF NOT EXISTS billing_webhook_deliveries (
    id           BIGSERIAL PRIMARY KEY,
    endpoint_id  BIGINT NOT NULL REFERENCES billing_webhook_endpoints(id) ON DELETE CASCADE,
    event_type   TEXT NOT NULL,
    payload      JSONB NOT NULL,
    status       TEXT NOT NULL DEFAULT 'pending',
    attempts     INTEGER NOT NULL DEFAULT 0,
    next_attempt_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    last_error   TEXT,
    created_at   TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at   TIMESTAMPTZ NOT NULL DEFAULT NOW()
);
CREATE INDEX IF NOT EXISTS billing_webhook_deliveries_due_idx
    ON billing_webhook_deliveries (status, next_attempt_at);

CREATE TABLE IF NOT EXISTS billing_audiences (
    id            BIGSERIAL PRIMARY KEY,
    app_id        BIGINT NOT NULL REFERENCES billing_apps(id) ON DELETE CASCADE,
    identifier    TEXT NOT NULL,
    display_name  TEXT,
    rules         JSONB NOT NULL DEFAULT '[]'::jsonb,
    created_at    TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    UNIQUE (app_id, identifier)
);

ALTER TABLE billing_offerings
    ADD COLUMN IF NOT EXISTS audience_id BIGINT REFERENCES billing_audiences(id) ON DELETE SET NULL;

CREATE TABLE IF NOT EXISTS billing_experiments (
    id            BIGSERIAL PRIMARY KEY,
    app_id        BIGINT NOT NULL REFERENCES billing_apps(id) ON DELETE CASCADE,
    identifier    TEXT NOT NULL,
    status        TEXT NOT NULL DEFAULT 'draft',
    metric        TEXT,
    audience_id   BIGINT REFERENCES billing_audiences(id) ON DELETE SET NULL,
    started_at    TIMESTAMPTZ,
    ended_at      TIMESTAMPTZ,
    created_at    TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    UNIQUE (app_id, identifier)
);

CREATE TABLE IF NOT EXISTS billing_experiment_variants (
    id             BIGSERIAL PRIMARY KEY,
    experiment_id  BIGINT NOT NULL REFERENCES billing_experiments(id) ON DELETE CASCADE,
    identifier     TEXT NOT NULL,
    offering_id    BIGINT NOT NULL REFERENCES billing_offerings(id) ON DELETE CASCADE,
    is_control     BOOLEAN NOT NULL DEFAULT FALSE,
    traffic_weight INTEGER NOT NULL DEFAULT 0,
    UNIQUE (experiment_id, identifier)
);

CREATE TABLE IF NOT EXISTS billing_experiment_assignments (
    id            BIGSERIAL PRIMARY KEY,
    experiment_id BIGINT NOT NULL REFERENCES billing_experiments(id) ON DELETE CASCADE,
    customer_id   BIGINT NOT NULL REFERENCES billing_customers(id) ON DELETE CASCADE,
    variant_id    BIGINT NOT NULL REFERENCES billing_experiment_variants(id) ON DELETE CASCADE,
    created_at    TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    UNIQUE (experiment_id, customer_id)
);

CREATE TABLE IF NOT EXISTS billing_experiment_events (
    id           BIGSERIAL PRIMARY KEY,
    experiment_id BIGINT NOT NULL REFERENCES billing_experiments(id) ON DELETE CASCADE,
    variant_id   BIGINT NOT NULL REFERENCES billing_experiment_variants(id) ON DELETE CASCADE,
    customer_id  BIGINT NOT NULL REFERENCES billing_customers(id) ON DELETE CASCADE,
    event_type   TEXT NOT NULL,
    metadata     JSONB NOT NULL DEFAULT '{}'::jsonb,
    created_at   TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE TABLE IF NOT EXISTS billing_paywalls (
    id            BIGSERIAL PRIMARY KEY,
    offering_id   BIGINT NOT NULL UNIQUE REFERENCES billing_offerings(id) ON DELETE CASCADE,
    template      TEXT,
    config        JSONB NOT NULL DEFAULT '{}'::jsonb,
    draft_config  JSONB,
    is_published  BOOLEAN NOT NULL DEFAULT FALSE,
    created_at    TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at    TIMESTAMPTZ NOT NULL DEFAULT NOW()
);
"#;
