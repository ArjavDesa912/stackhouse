/**
 * Thin wrapper over the Stackhouse billing REST API.
 *
 * In dev, Vite proxies `/v1/**` to the Stackhouse server (see `vite.config.ts`),
 * so relative URLs just work. In production, set `VITE_API_BASE` at build time.
 */

const BASE = (import.meta as any).env?.VITE_API_BASE ?? '';

export interface ApiOptions {
    method?: 'GET' | 'POST' | 'PUT' | 'DELETE';
    body?: unknown;
    token?: string;
}

export async function api<T = any>(path: string, opts: ApiOptions = {}): Promise<T> {
    const headers: Record<string, string> = { 'Content-Type': 'application/json' };
    if (opts.token) headers['Authorization'] = `Bearer ${opts.token}`;

    const res = await fetch(`${BASE}${path}`, {
        method: opts.method ?? 'GET',
        headers,
        body: opts.body === undefined ? undefined : JSON.stringify(opts.body),
    });
    const json = await res.json().catch(() => ({}));
    if (!res.ok || (json && json.success === false)) {
        const msg =
            json?.error?.message ||
            json?.error ||
            `${opts.method ?? 'GET'} ${path} failed (${res.status})`;
        throw new Error(typeof msg === 'string' ? msg : JSON.stringify(msg));
    }
    // Admin endpoints return `{ success, data }`; a few (secrets update) just `{ success }`.
    return (json.data ?? json) as T;
}

// ---------- Typed helpers for each admin endpoint ----------

export interface App {
    id: number;
    name: string;
    bundle_id: string | null;
    platform: string;
    project_id: number | null;
    created_at: string;
}
export interface Product {
    id: number;
    app_id: number;
    store: string;
    store_product_id: string;
    product_type: string;
    period: string | null;
    price_micros: number | null;
    currency: string | null;
    metadata: Record<string, unknown>;
}
export interface Entitlement {
    id: number;
    identifier: string;
    display_name: string | null;
}
export interface EntitlementWithProducts {
    entitlement: Entitlement;
    product_ids: number[];
}
export interface PackageRow {
    id: number;
    identifier: string;
    product_id: number;
    package_type: string | null;
}
export interface Offering {
    id: number;
    identifier: string;
    is_current: boolean;
    metadata: Record<string, unknown>;
    packages: PackageRow[];
}

const auth = (token: string) => ({ token });

export const BillingApi = {
    // Apps
    listApps: (token: string) => api<App[]>('/v1/billing/admin/apps', auth(token)),
    createApp: (
        token: string,
        input: { name: string; bundle_id?: string; platform?: string; project_id?: number },
    ) => api<App>('/v1/billing/admin/apps', { method: 'POST', body: input, ...auth(token) }),
    updateSecrets: (
        token: string,
        appId: number,
        secrets: {
            apple_shared_secret?: string;
            google_service_account?: string;
            stripe_signing_secret?: string;
            outbound_webhook_secret?: string;
        },
    ) =>
        api(`/v1/billing/admin/apps/${appId}/secrets`, {
            method: 'POST',
            body: secrets,
            ...auth(token),
        }),

    // Products
    listProducts: (token: string, appId: number) =>
        api<Product[]>(`/v1/billing/admin/products?app_id=${appId}`, auth(token)),
    upsertProduct: (
        token: string,
        input: {
            app_id: number;
            store: string;
            store_product_id: string;
            product_type?: string;
            period?: string;
            price_micros?: number;
            currency?: string;
        },
    ) =>
        api<Product>('/v1/billing/admin/products', {
            method: 'POST',
            body: input,
            ...auth(token),
        }),

    // Entitlements
    listEntitlements: (token: string, appId: number) =>
        api<EntitlementWithProducts[]>(
            `/v1/billing/admin/entitlements?app_id=${appId}`,
            auth(token),
        ),
    upsertEntitlement: (
        token: string,
        input: {
            app_id: number;
            identifier: string;
            display_name?: string;
            product_ids?: number[];
        },
    ) =>
        api<Entitlement>('/v1/billing/admin/entitlements', {
            method: 'POST',
            body: input,
            ...auth(token),
        }),

    // Offerings
    upsertOffering: (
        token: string,
        input: {
            app_id: number;
            identifier: string;
            is_current?: boolean;
            packages?: Array<{ identifier: string; product_id: number; package_type?: string }>;
        },
    ) =>
        api<Offering>('/v1/billing/admin/offerings', {
            method: 'POST',
            body: input,
            ...auth(token),
        }),
    listOfferings: (appId: number) =>
        api<Offering[]>(`/v1/billing/offerings?app_id=${appId}`),

    // Grant promo
    grant: (
        token: string,
        input: {
            app_id: number;
            app_user_id: string;
            product_id: number;
            duration_days: number;
        },
    ) =>
        api('/v1/billing/admin/grant', {
            method: 'POST',
            body: input,
            ...auth(token),
        }),

    // Webhook endpoints
    addWebhookEndpoint: (
        token: string,
        input: { app_id: number; url: string; secret: string; events?: string[] },
    ) =>
        api<{ id: number }>('/v1/billing/admin/webhook-endpoints', {
            method: 'POST',
            body: input,
            ...auth(token),
        }),
};

// Login endpoint of the core auth module. Returns `{ access_token, user, ... }`.
export async function login(email: string, password: string): Promise<string> {
    const res = await fetch(`${BASE}/v1/auth/login`, {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ email, password }),
    });
    const json = await res.json().catch(() => ({}));
    if (!res.ok) throw new Error(json?.error?.message || 'Login failed');
    const token = json?.data?.access_token ?? json?.access_token;
    if (!token) throw new Error('Login response missing access_token');
    return token;
}
