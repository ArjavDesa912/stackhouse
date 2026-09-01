import { apiGet, apiPost, API_Base } from './apiClient';

const BILLING_APP_ID = Number(import.meta.env.VITE_BILLING_APP_ID || 1);

export async function listPlans() {
  const res = await apiGet('/v1/billing/plans');
  if (!res.ok || !res.data.success) throw new Error(res.data?.message || 'Failed to load plans');
  return res.data.data;
}

export async function getMySubscription() {
  const res = await apiGet(`/v1/billing/me?app_id=${BILLING_APP_ID}`);
  if (!res.ok || !res.data.success) throw new Error(res.data?.message || 'Failed to load subscription');
  return res.data.data;
}

export async function createCheckout({ priceId, appUserId, customerEmail, successUrl, cancelUrl }) {
  const res = await apiPost('/v1/billing/checkout', {
    app_id: BILLING_APP_ID,
    price_id: priceId,
    app_user_id: String(appUserId || 'unknown'),
    customer_email: customerEmail || undefined,
    success_url: successUrl,
    cancel_url: cancelUrl,
  });
  if (!res.ok || !res.data.success) throw new Error(res.data?.message || 'Failed to create checkout session');
  return res.data.data;
}

export async function cancelSubscription(appUserId) {
  const res = await apiPost('/v1/billing/cancel', {
    app_id: BILLING_APP_ID,
    app_user_id: String(appUserId || 'unknown'),
  });
  if (!res.ok || !res.data.success) throw new Error(res.data?.message || 'Failed to cancel subscription');
  return res.data.data;
}

export { BILLING_APP_ID };
