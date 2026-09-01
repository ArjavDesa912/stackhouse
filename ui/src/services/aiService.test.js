import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'

import { generateInsights, generateSQL } from './aiService'

function geminiResponse(body, { ok = true } = {}) {
  return {
    ok,
    json: vi.fn().mockResolvedValue(body),
  }
}

describe('aiService', () => {
  let consoleError

  beforeEach(() => {
    vi.stubGlobal('fetch', vi.fn())
    consoleError = vi.spyOn(console, 'error').mockImplementation(() => {})
  })

  afterEach(() => {
    vi.unstubAllGlobals()
    consoleError.mockRestore()
  })

  it('requires an API key before generating SQL', async () => {
    await expect(generateSQL('show users', [], '')).rejects.toThrow(
      'API Key is required',
    )
    expect(fetch).not.toHaveBeenCalled()
  })

  it('generates SQL and strips markdown fences from model output', async () => {
    fetch.mockResolvedValueOnce(
      geminiResponse({
        candidates: [
          {
            content: {
              parts: [{ text: '```sql\nSELECT * FROM users;\n```' }],
            },
          },
        ],
      }),
    )

    await expect(
      generateSQL('show users', [{ table: 'users', columns: ['id'] }], 'test-key'),
    ).resolves.toBe('SELECT * FROM users;')

    expect(fetch).toHaveBeenCalledTimes(1)
    const [url, init] = fetch.mock.calls[0]
    expect(url).toContain('generativelanguage.googleapis.com')
    expect(url).toContain('key=test-key')
    expect(init.method).toBe('POST')
    const request = JSON.parse(init.body)
    expect(request.contents[0].parts[0].text).toContain('Schema Context')
    expect(request.contents[0].parts[0].text).toContain('show users')
  })

  it('surfaces SQL generation API errors', async () => {
    fetch.mockResolvedValueOnce(
      geminiResponse(
        {
          error: { message: 'Invalid API key' },
        },
        { ok: false },
      ),
    )

    await expect(generateSQL('show users', [], 'bad-key')).rejects.toThrow(
      'Invalid API key',
    )
  })

  it('requires an API key before generating insights', async () => {
    await expect(generateInsights({ rows: 3 }, '')).rejects.toThrow(
      'API Key is required',
    )
    expect(fetch).not.toHaveBeenCalled()
  })

  it('generates insights and parses fenced JSON output', async () => {
    fetch.mockResolvedValueOnce(
      geminiResponse({
        candidates: [
          {
            content: {
              parts: [
                {
                  text: '```json\n[{"title":"Growth","description":"Revenue rose","type":"positive"}]\n```',
                },
              ],
            },
          },
        ],
      }),
    )

    await expect(
      generateInsights({ rows: 3, revenue: 1200 }, 'test-key'),
    ).resolves.toEqual([
      {
        title: 'Growth',
        description: 'Revenue rose',
        type: 'positive',
      },
    ])
  })

  it('surfaces insight generation API errors', async () => {
    fetch.mockResolvedValueOnce(
      geminiResponse(
        {
          error: { message: 'Quota exceeded' },
        },
        { ok: false },
      ),
    )

    await expect(generateInsights({ rows: 3 }, 'test-key')).rejects.toThrow(
      'Quota exceeded',
    )
  })
})
