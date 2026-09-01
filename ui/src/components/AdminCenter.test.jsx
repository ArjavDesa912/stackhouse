import { render, screen } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { beforeEach, describe, expect, it, vi } from 'vitest'

import AdminCenter from './AdminCenter'
import { MemoryRouter } from 'react-router'

const { apiDelete, apiGet, apiPost } = vi.hoisted(() => ({
  apiDelete: vi.fn(),
  apiGet: vi.fn(async (path) => {
    if (path === '/v1/metrics/summary') {
      return { ok: true, data: { uptime_seconds: 60 } }
    }

    return { ok: true, data: { success: true, data: [] } }
  }),
  apiPost: vi.fn(),
}))

vi.mock('../context/AuthContext', () => ({
  useAuth: () => ({
    isAdmin: () => true,
  }),
}))

vi.mock('../lib/apiClient', () => ({
  apiGet,
  apiPost,
  apiDelete,
}))

vi.mock('./admin/billing/BillingSection', () => ({
  default: () => <div>Billing section mock</div>,
}))

describe('AdminCenter', () => {
  beforeEach(() => {
    apiGet.mockClear()
  })

  it('renders admin center layout with navigation links', async () => {
    render(
      <MemoryRouter initialEntries={['/admin/overview']}>
        <AdminCenter />
      </MemoryRouter>
    )

    expect(screen.getByRole('link', { name: 'Billing' })).toBeInTheDocument()
    expect(screen.getByRole('link', { name: 'Overview' })).toBeInTheDocument()
  })
})
