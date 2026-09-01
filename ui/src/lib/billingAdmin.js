import { apiFetch } from './apiClient'

export const BILLING_APP_STORAGE_KEY = 'stackhouse_admin_billing_app_id'

export function resolveInitialBillingAppId(apps, savedValue) {
  const savedId = Number(savedValue)
  if (savedId && apps.some((app) => app.id === savedId)) {
    return savedId
  }

  return apps[0]?.id ?? null
}

async function billingRequest(path, options = {}) {
  const res = await apiFetch(path, {
    method: options.method || 'GET',
    body: options.body,
  })
  const json = await res.json().catch(() => ({}))

  if (!res.ok || json?.success === false) {
    const message =
      json?.error?.message ||
      json?.message ||
      json?.error ||
      `${options.method || 'GET'} ${path} failed (${res.status})`
    throw new Error(
      typeof message === 'string' ? message : JSON.stringify(message),
    )
  }

  return json.data ?? json
}

export const BillingAdminApi = {
  addWebhookEndpoint: (input) =>
    billingRequest('/v1/billing/admin/webhook-endpoints', {
      method: 'POST',
      body: input,
    }),
  createApp: (input) =>
    billingRequest('/v1/billing/admin/apps', { method: 'POST', body: input }),
  grant: (input) =>
    billingRequest('/v1/billing/admin/grant', {
      method: 'POST',
      body: input,
    }),
  listApps: () => billingRequest('/v1/billing/admin/apps'),
  listEntitlements: (appId) =>
    billingRequest(`/v1/billing/admin/entitlements?app_id=${appId}`),
  listOfferings: (appId) =>
    billingRequest(`/v1/billing/offerings?app_id=${appId}`),
  listProducts: (appId) =>
    billingRequest(`/v1/billing/admin/products?app_id=${appId}`),
  updateSecrets: (appId, secrets) =>
    billingRequest(`/v1/billing/admin/apps/${appId}/secrets`, {
      method: 'POST',
      body: secrets,
    }),
  upsertEntitlement: (input) =>
    billingRequest('/v1/billing/admin/entitlements', {
      method: 'POST',
      body: input,
    }),
  upsertOffering: (input) =>
    billingRequest('/v1/billing/admin/offerings', {
      method: 'POST',
      body: input,
    }),
  upsertProduct: (input) =>
    billingRequest('/v1/billing/admin/products', {
      method: 'POST',
      body: input,
    }),

  // Audiences
  listAudiences: (appId) =>
    billingRequest(`/v1/billing/admin/audiences?app_id=${appId}`),
  upsertAudience: (input) =>
    billingRequest('/v1/billing/admin/audiences', {
      method: 'POST',
      body: input,
    }),
  setOfferingAudience: (offeringId, audienceId) =>
    billingRequest(`/v1/billing/admin/offerings/${offeringId}/audience`, {
      method: 'POST',
      body: { audience_id: audienceId },
    }),

  // Experiments
  listExperiments: (appId) =>
    billingRequest(`/v1/billing/admin/experiments?app_id=${appId}`),
  upsertExperiment: (input) =>
    billingRequest('/v1/billing/admin/experiments', {
      method: 'POST',
      body: input,
    }),
  updateExperimentStatus: (experimentId, status) =>
    billingRequest(`/v1/billing/admin/experiments/${experimentId}/status`, {
      method: 'POST',
      body: { status },
    }),
  getExperimentResults: (experimentId) =>
    billingRequest(`/v1/billing/admin/experiments/${experimentId}/results`),

  // Paywalls
  getPaywall: (offeringId) =>
    billingRequest(`/v1/billing/admin/paywalls?offering_id=${offeringId}`),
  upsertPaywall: (input) =>
    billingRequest('/v1/billing/admin/paywalls', {
      method: 'POST',
      body: input,
    }),
  publishPaywall: (offeringId) =>
    billingRequest(`/v1/billing/admin/paywalls/${offeringId}/publish`, {
      method: 'POST',
    }),
}
