import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'

import {
  apiFetch,
  apiGet,
  clearTokens,
  getRefreshToken,
  getToken,
  setTokens,
} from './apiClient'

function jsonResponse(body, { status = 200, ok = status >= 200 && status < 300 } = {}) {
  return {
    ok,
    status,
    json: vi.fn().mockResolvedValue(body),
  }
}

describe('apiClient', () => {
  beforeEach(() => {
    localStorage.clear()
    vi.stubGlobal('fetch', vi.fn())
  })

  afterEach(() => {
    vi.unstubAllGlobals()
    clearTokens()
  })

  it('stores, reads, and clears auth tokens', () => {
    setTokens('access-token', 'refresh-token')

    expect(getToken()).toBe('access-token')
    expect(getRefreshToken()).toBe('refresh-token')

    clearTokens()

    expect(getToken()).toBeNull()
    expect(getRefreshToken()).toBeNull()
  })

  it('serializes JSON bodies and attaches bearer tokens', async () => {
    setTokens('access-token', 'refresh-token')
    fetch.mockResolvedValueOnce(jsonResponse({ success: true }))

    await apiFetch('/v1/push/items', {
      method: 'POST',
      headers: { 'x-request-id': 'test-request' },
      body: { name: 'Ada' },
    })

    expect(fetch).toHaveBeenCalledTimes(1)
    const [url, init] = fetch.mock.calls[0]
    expect(url).toBe('http://localhost:3000/v1/push/items')
    expect(init.method).toBe('POST')
    expect(init.body).toBe(JSON.stringify({ name: 'Ada' }))
    expect(init.headers).toMatchObject({
      'Content-Type': 'application/json',
      Authorization: 'Bearer access-token',
      'x-request-id': 'test-request',
    })
  })

  it('uses absolute URLs without prefixing the API base', async () => {
    fetch.mockResolvedValueOnce(jsonResponse({ success: true }))

    await apiFetch('https://api.example.com/health')

    expect(fetch.mock.calls[0][0]).toBe('https://api.example.com/health')
  })

  it('refreshes access tokens once and retries unauthorized requests', async () => {
    setTokens('old-access', 'refresh-token')
    fetch
      .mockResolvedValueOnce(jsonResponse({ success: false }, { status: 401 }))
      .mockResolvedValueOnce(
        jsonResponse({
          success: true,
          data: {
            access_token: 'new-access',
            refresh_token: 'new-refresh',
          },
        }),
      )
      .mockResolvedValueOnce(jsonResponse({ success: true }))

    const response = await apiFetch('/v1/admin/logs', { method: 'GET' })

    expect(response.ok).toBe(true)
    expect(fetch).toHaveBeenCalledTimes(3)
    expect(fetch.mock.calls[1][0]).toBe('http://localhost:3000/v1/auth/refresh')
    expect(JSON.parse(fetch.mock.calls[1][1].body)).toEqual({
      refresh_token: 'refresh-token',
    })
    expect(fetch.mock.calls[2][1].headers.Authorization).toBe('Bearer new-access')
    expect(getToken()).toBe('new-access')
    expect(getRefreshToken()).toBe('new-refresh')
  })

  it('normalizes invalid JSON responses for convenience helpers', async () => {
    fetch.mockResolvedValueOnce({
      ok: false,
      status: 502,
      json: vi.fn().mockRejectedValue(new Error('not json')),
    })

    await expect(apiGet('/v1/broken')).resolves.toEqual({
      ok: false,
      status: 502,
      data: { success: false, message: 'Invalid JSON' },
    })
  })
})
