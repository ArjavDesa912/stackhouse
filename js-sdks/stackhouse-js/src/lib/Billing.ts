/**
 * BillingClient — the RevenueCat-style subscription client for Stackhouse.
 *
 * Talks to the server's `/v1/billing` routes. Customer-facing methods require
 * the user to be authenticated via `stackhouse.auth.*` (their JWT is attached via
 * shared headers). Admin methods require a service-admin JWT.
 */

export type Store = 'app_store' | 'play_store' | 'stripe' | 'promotional';

export interface EntitlementInfo {
  identifier: string;
  is_active: boolean;
  will_renew: boolean;
  period_type: 'normal' | 'trial' | 'grace' | string;
  latest_purchase_date: string | null;
  expires_date: string | null;
  grace_period_expires_date: string | null;
  store: string;
  product_identifier: string | null;
}

export interface Subscription {
  id: number;
  customer_id: number;
  product_id: number | null;
  store: string;
  original_transaction_id: string | null;
  current_period_start: string | null;
  current_period_end: string | null;
  status: string;
  auto_renew: boolean;
  is_trial: boolean;
  grace_period_expires_at: string | null;
  updated_at: string;
}

export interface Customer {
  id: number;
  app_id: number;
  app_user_id: string;
  aliases: string[];
  attributes: Record<string, unknown>;
  first_seen_at: string;
  last_seen_at: string;
}

export interface CustomerInfo {
  customer: Customer;
  subscriptions: Subscription[];
  entitlements: EntitlementInfo[];
}

export interface Package {
  id: number;
  offering_id: number;
  identifier: string;
  product_id: number;
  package_type: string | null;
}

export interface Offering {
  id: number;
  app_id: number;
  identifier: string;
  is_current: boolean;
  metadata: Record<string, unknown>;
  packages: Package[];
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

export interface App {
  id: number;
  project_id: number | null;
  name: string;
  bundle_id: string | null;
  platform: string;
  created_at: string;
}

export interface Variant {
  id: number;
  experiment_id: number;
  identifier: string;
  offering_id: number;
  is_control: boolean;
  traffic_weight: number;
}

export interface Experiment {
  id: number;
  app_id: number;
  identifier: string;
  status: 'draft' | 'running' | 'paused' | 'completed';
  metric: string | null;
  audience_id: number | null;
  started_at: string | null;
  ended_at: string | null;
  variants: Variant[];
}

export interface Audience {
  id: number;
  app_id: number;
  identifier: string;
  display_name: string | null;
  rules: Record<string, unknown>;
}

export interface Paywall {
  id: number;
  offering_id: number;
  template: string | null;
  config: Record<string, unknown>;
  draft_config: Record<string, unknown> | null;
  is_published: boolean;
}

export interface VariantResult {
  variant_id: number;
  identifier: string;
  offering_id: number;
  is_control: boolean;
  impressions: number;
  conversions: number;
  conversion_rate: number;
  z_score: number | null;
}

export interface ResolvedOfferingContext {
  country?: string;
  app_version?: string;
}

export interface ResolvedOffering {
  offering: Offering;
  paywall: Paywall | null;
  experiment: {
    id: number;
    identifier: string;
    variant_id: number;
    variant_identifier: string;
  } | null;
}

export class BillingError extends Error {
  constructor(message: string, public readonly status?: number) {
    super(message);
    this.name = 'BillingError';
  }
}

/**
 * Admin namespace exposed as `stackhouse.billing.admin.*`.
 */
export class BillingAdminClient {
  constructor(private url: string, private headers: Record<string, string>) {}

  private async req<T>(method: string, path: string, body?: unknown): Promise<T> {
    const res = await fetch(`${this.url}${path}`, {
      method,
      headers: this.headers,
      body: body === undefined ? undefined : JSON.stringify(body),
    });
    const json = await res.json().catch(() => ({}));
    if (!res.ok || (json && json.success === false)) {
      const msg = json?.error?.message || `billing admin ${method} ${path} failed`;
      throw new BillingError(msg, res.status);
    }
    return json.data as T;
  }

  // Apps
  createApp(input: {
    name: string;
    project_id?: number;
    bundle_id?: string;
    platform?: string;
  }): Promise<App> {
    return this.req('POST', '/admin/apps', input);
  }
  listApps(): Promise<App[]> {
    return this.req('GET', '/admin/apps');
  }
  updateAppSecrets(
    appId: number,
    secrets: {
      apple_shared_secret?: string;
      google_service_account?: string;
      stripe_signing_secret?: string;
      outbound_webhook_secret?: string;
    },
  ): Promise<void> {
    return this.req('POST', `/admin/apps/${appId}/secrets`, secrets);
  }

  // Products
  upsertProduct(input: {
    app_id: number;
    store: Store;
    store_product_id: string;
    product_type?: string;
    period?: string;
    price_micros?: number;
    currency?: string;
    metadata?: Record<string, unknown>;
  }): Promise<Product> {
    return this.req('POST', '/admin/products', input);
  }
  listProducts(appId: number): Promise<Product[]> {
    return this.req('GET', `/admin/products?app_id=${appId}`);
  }

  // Entitlements
  upsertEntitlement(input: {
    app_id: number;
    identifier: string;
    display_name?: string;
    product_ids?: number[];
  }): Promise<{ id: number; identifier: string; display_name: string | null }> {
    return this.req('POST', '/admin/entitlements', input);
  }
  listEntitlements(
    appId: number,
  ): Promise<Array<{ entitlement: any; product_ids: number[] }>> {
    return this.req('GET', `/admin/entitlements?app_id=${appId}`);
  }

  // Offerings
  upsertOffering(input: {
    app_id: number;
    identifier: string;
    is_current?: boolean;
    metadata?: Record<string, unknown>;
    packages?: Array<{
      identifier: string;
      product_id: number;
      package_type?: string;
    }>;
  }): Promise<Offering> {
    return this.req('POST', '/admin/offerings', input);
  }

  // Promotional grant
  grant(input: {
    app_id: number;
    app_user_id: string;
    product_id: number;
    duration_days: number;
  }): Promise<Subscription> {
    return this.req('POST', '/admin/grant', input);
  }

  // Webhook endpoints
  addWebhookEndpoint(input: {
    app_id: number;
    url: string;
    secret: string;
    events?: string[];
  }): Promise<{ id: number }> {
    return this.req('POST', '/admin/webhook-endpoints', input);
  }

  // Experiments
  upsertExperiment(input: {
    app_id: number;
    identifier: string;
    metric?: string;
    audience_id?: number;
    variants: Array<{
      identifier: string;
      offering_id: number;
      is_control?: boolean;
      traffic_weight: number;
    }>;
  }): Promise<Experiment> {
    return this.req('POST', '/admin/experiments', input);
  }
  listExperiments(appId: number): Promise<Experiment[]> {
    return this.req<Experiment[]>('GET', `/admin/experiments?app_id=${appId}`);
  }
  updateExperimentStatus(
    experimentId: number,
    status: 'draft' | 'running' | 'paused' | 'completed',
  ): Promise<Experiment> {
    return this.req('POST', `/admin/experiments/${experimentId}/status`, { status });
  }
  getExperimentResults(experimentId: number): Promise<VariantResult[]> {
    return this.req<VariantResult[]>('GET', `/admin/experiments/${experimentId}/results`);
  }

  // Audiences
  upsertAudience(input: {
    app_id: number;
    identifier: string;
    display_name?: string;
    rules?: Record<string, unknown>;
  }): Promise<Audience> {
    return this.req('POST', '/admin/audiences', input);
  }
  listAudiences(appId: number): Promise<Audience[]> {
    return this.req<Audience[]>('GET', `/admin/audiences?app_id=${appId}`);
  }
  setOfferingAudience(
    offeringId: number,
    audienceId: number | null,
  ): Promise<void> {
    return this.req('POST', `/admin/offerings/${offeringId}/audience`, { audience_id: audienceId });
  }

  // Paywalls
  upsertPaywall(input: {
    offering_id: number;
    template?: string;
    config?: Record<string, unknown>;
    draft_config?: Record<string, unknown>;
  }): Promise<Paywall> {
    return this.req('POST', '/admin/paywalls', input);
  }
  getPaywall(offeringId: number): Promise<Paywall | null> {
    return this.req<Paywall | null>('GET', `/admin/paywalls?offering_id=${offeringId}`);
  }
  publishPaywall(offeringId: number): Promise<Paywall> {
    return this.req('POST', `/admin/paywalls/${offeringId}/publish`);
  }
}

/**
 * Customer-facing billing client. Usage:
 *
 * ```ts
 * const stackhouse = createClient('http://localhost:3000');
 * await stackhouse.signIn('u@example.com', 'password');
 * const info = await stackhouse.billing.getCustomerInfo(appId, 'user-42');
 * if (info.entitlements.some(e => e.identifier === 'pro' && e.is_active)) {
 *   // unlock pro features
 * }
 * ```
 */
export class BillingClient {
  private url: string;
  private headers: Record<string, string>;
  public readonly admin: BillingAdminClient;

  constructor(baseUrl: string, headers: Record<string, string>) {
    this.url = `${baseUrl}/v1/billing`;
    this.headers = headers;
    this.admin = new BillingAdminClient(this.url, this.headers);
  }

  private async req<T>(method: string, path: string, body?: unknown): Promise<T> {
    const res = await fetch(`${this.url}${path}`, {
      method,
      headers: this.headers,
      body: body === undefined ? undefined : JSON.stringify(body),
    });
    const json = await res.json().catch(() => ({}));
    if (!res.ok || (json && json.success === false)) {
      const msg = json?.error?.message || `billing ${method} ${path} failed`;
      throw new BillingError(msg, res.status);
    }
    return json.data as T;
  }

  // ------------------------ Customer ------------------------

  getCustomerInfo(appId: number, appUserId: string): Promise<CustomerInfo> {
    return this.req<CustomerInfo>(
      'GET',
      `/customers/${encodeURIComponent(appUserId)}?app_id=${appId}`,
    );
  }

  getEntitlements(appId: number, appUserId: string): Promise<EntitlementInfo[]> {
    return this.req<EntitlementInfo[]>(
      'GET',
      `/customers/${encodeURIComponent(appUserId)}/entitlements?app_id=${appId}`,
    );
  }

  setAttributes(
    appId: number,
    appUserId: string,
    attributes: Record<string, unknown>,
  ): Promise<void> {
    return this.req('POST', `/customers/${encodeURIComponent(appUserId)}/attributes`, {
      app_id: appId,
      attributes,
    });
  }

  addAlias(appId: number, appUserId: string, alias: string): Promise<void> {
    return this.req('POST', `/customers/${encodeURIComponent(appUserId)}/alias`, {
      app_id: appId,
      alias,
    });
  }

  submitAppleReceipt(
    appId: number,
    appUserId: string,
    receiptData: string,
  ): Promise<{ entitlements: EntitlementInfo[] }> {
    return this.req(
      'POST',
      `/customers/${encodeURIComponent(appUserId)}/receipts/apple`,
      { app_id: appId, receipt_data: receiptData },
    );
  }

  submitGoogleReceipt(
    appId: number,
    appUserId: string,
    input: {
      package_name: string;
      subscription_id: string;
      purchase_token: string;
      access_token?: string;
    },
  ): Promise<{ subscription: Subscription; entitlements: EntitlementInfo[] }> {
    return this.req(
      'POST',
      `/customers/${encodeURIComponent(appUserId)}/receipts/google`,
      { app_id: appId, ...input },
    );
  }

  submitStripeEvent(
    appId: number,
    appUserId: string,
    event: unknown,
  ): Promise<Subscription> {
    return this.req(
      'POST',
      `/customers/${encodeURIComponent(appUserId)}/receipts/stripe`,
      { app_id: appId, event },
    );
  }

  // ------------------------ Public ------------------------

  listOfferings(appId: number): Promise<Offering[]> {
    return this.req<Offering[]>('GET', `/offerings?app_id=${appId}`);
  }

  /** Returns the current (default) offering for the app, if one is marked. */
  async getCurrentOffering(appId: number): Promise<Offering | null> {
    const offerings = await this.listOfferings(appId);
    return offerings.find((o) => o.is_current) ?? null;
  }

  /** Convenience: does the user currently have an active entitlement with `identifier`? */
  async hasEntitlement(
    appId: number,
    appUserId: string,
    identifier: string,
  ): Promise<boolean> {
    const entitlements = await this.getEntitlements(appId, appUserId);
    return entitlements.some((e) => e.identifier === identifier && e.is_active);
  }

  // ------------------------ Experiments / Paywalls ------------------------

  getResolvedOffering(
    appId: number,
    appUserId: string,
    context?: ResolvedOfferingContext,
  ): Promise<ResolvedOffering> {
    const params = new URLSearchParams({ app_id: String(appId) });
    if (context?.country) params.set('country', context.country);
    if (context?.app_version) params.set('app_version', context.app_version);
    return this.req<ResolvedOffering>(
      'GET',
      `/customers/${encodeURIComponent(appUserId)}/offerings/resolve?${params.toString()}`,
    );
  }

  trackImpression(appId: number, appUserId: string, metadata?: Record<string, unknown>): Promise<void> {
    return this.req('POST', `/customers/${encodeURIComponent(appUserId)}/experiments/impression`, {
      app_id: appId,
      metadata,
    });
  }

  trackConversion(appId: number, appUserId: string, metadata?: Record<string, unknown>): Promise<void> {
    return this.req('POST', `/customers/${encodeURIComponent(appUserId)}/experiments/conversion`, {
      app_id: appId,
      metadata,
    });
  }
}
