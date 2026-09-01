import { beforeEach, describe, expect, it, vi } from 'vitest'

const { apiFetch } = vi.hoisted(() => ({
  apiFetch: vi.fn(),
}))

vi.mock('./apiClient', () => ({
  apiFetch,
}))

import {
  BILLING_APP_STORAGE_KEY,
  BillingAdminApi,
  resolveInitialBillingAppId,
} from './billingAdmin'

function response(body, { ok = true, status = 200 } = {}) {
  return {
    ok,
    status,
    json: vi.fn().mockResolvedValue(body),
  }
}

describe('resolveInitialBillingAppId', () => {
  const apps = [
    { id: 7, name: 'Alpha' },
    { id: 11, name: 'Beta' },
  ]

  it('uses a saved app id when it still exists', () => {
    expect(resolveInitialBillingAppId(apps, '11')).toBe(11)
  })

  it('falls back to the first app when saved state is missing or stale', () => {
    expect(resolveInitialBillingAppId(apps, null)).toBe(7)
    expect(resolveInitialBillingAppId(apps, '999')).toBe(7)
  })

  it('returns null when no apps are available', () => {
    expect(resolveInitialBillingAppId([], '7')).toBeNull()
  })
})

describe('BillingAdminApi', () => {
  beforeEach(() => {
    apiFetch.mockReset()
  })

  it('exports the storage key used by the admin UI', () => {
    expect(BILLING_APP_STORAGE_KEY).toBe('stackhouse_admin_billing_app_id')
  })

  it('lists billing resources and unwraps success envelopes', async () => {
    apiFetch.mockResolvedValueOnce(
      response({
        success: true,
        data: [{ id: 1, identifier: 'pro.monthly' }],
      }),
    )

    await expect(BillingAdminApi.listProducts(7)).resolves.toEqual([
      { id: 1, identifier: 'pro.monthly' },
    ])
    expect(apiFetch).toHaveBeenCalledWith('/v1/billing/admin/products?app_id=7', {
      method: 'GET',
      body: undefined,
    })
  })

  it('sends POST bodies for mutating billing admin calls', async () => {
    apiFetch.mockResolvedValueOnce(
      response({
        success: true,
        data: { id: 3, identifier: 'premium' },
      }),
    )

    const input = {
      app_id: 7,
      identifier: 'premium',
      display_name: 'Premium',
    }

    await expect(BillingAdminApi.upsertEntitlement(input)).resolves.toEqual({
      id: 3,
      identifier: 'premium',
    })
    expect(apiFetch).toHaveBeenCalledWith('/v1/billing/admin/entitlements', {
      method: 'POST',
      body: input,
    })
  })

  it('throws the most useful message from failed billing responses', async () => {
    apiFetch.mockResolvedValueOnce(
      response(
        {
          success: false,
          error: { message: 'Service admin access required' },
        },
        { ok: false, status: 403 },
      ),
    )

    await expect(BillingAdminApi.listApps()).rejects.toThrow(
      'Service admin access required',
    )
  })

  it('falls back to status-aware errors when the server body is empty', async () => {
    apiFetch.mockResolvedValueOnce(response({}, { ok: false, status: 500 }))

    await expect(BillingAdminApi.listApps()).rejects.toThrow(
      'GET /v1/billing/admin/apps failed (500)',
    )
  })
})
