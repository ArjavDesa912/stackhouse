import { render, screen, waitFor } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { beforeEach, describe, expect, it, vi } from 'vitest'

import BillingLayout from './BillingLayout'
import { MemoryRouter } from 'react-router'

const { listApps, listProducts, resolveInitialBillingAppId } = vi.hoisted(() => ({
  listApps: vi.fn(async () => [
    {
      id: 7,
      name: 'Alpha',
      bundle_id: 'com.example.alpha',
      platform: 'ios',
      project_id: null,
      created_at: '2026-04-23T00:00:00.000Z',
    },
  ]),
  listProducts: vi.fn(async () => []),
  resolveInitialBillingAppId: vi.fn((apps) => apps[0]?.id ?? null),
}))

vi.mock('../../../lib/billingAdmin', () => ({
  BILLING_APP_STORAGE_KEY: 'stackhouse_admin_billing_app_id',
  BillingAdminApi: {
    listApps,
    listProducts,
  },
  resolveInitialBillingAppId,
}))

describe('BillingLayout', () => {
  beforeEach(() => {
    listApps.mockClear()
    listProducts.mockClear()
    localStorage.clear()
  })

  it('renders billing subtabs and selects the first available app', async () => {
    render(
      <MemoryRouter>
        <BillingLayout />
      </MemoryRouter>
    )

    expect(await screen.findByRole('link', { name: 'Apps' })).toBeInTheDocument()
    expect(screen.getByRole('link', { name: 'Products' })).toBeInTheDocument()
    expect(screen.getByRole('link', { name: 'Entitlements' })).toBeInTheDocument()
    expect(screen.getByRole('link', { name: 'Offerings' })).toBeInTheDocument()
    expect(screen.getByRole('link', { name: 'Audiences' })).toBeInTheDocument()
    expect(screen.getByRole('link', { name: 'Experiments' })).toBeInTheDocument()
    expect(screen.getByRole('link', { name: 'Paywalls' })).toBeInTheDocument()
    expect(screen.getByRole('link', { name: 'Grant' })).toBeInTheDocument()
    expect(screen.getByRole('link', { name: 'Webhooks' })).toBeInTheDocument()

    await waitFor(() => {
      expect(screen.getByLabelText('Active app')).toHaveValue('7')
    })
  })
})
