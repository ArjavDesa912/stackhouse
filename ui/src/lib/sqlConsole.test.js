import { describe, expect, it } from 'vitest'

import {
  buildSqlRequest,
  isReadQuery,
  normalizeSqlResponse,
} from './sqlConsole'

describe('isReadQuery', () => {
  it('recognizes supported read-only query prefixes after trimming', () => {
    expect(isReadQuery('  SELECT * FROM users;  ')).toBe(true)
    expect(isReadQuery('WITH recent AS (SELECT 1) SELECT * FROM recent')).toBe(true)
    expect(isReadQuery('EXPLAIN SELECT * FROM users')).toBe(true)
    expect(isReadQuery('PRAGMA table_info(users)')).toBe(true)
  })

  it('rejects mutating statements', () => {
    expect(isReadQuery('INSERT INTO users (name) VALUES ("Ada")')).toBe(false)
    expect(isReadQuery('DELETE FROM users')).toBe(false)
  })
})

describe('buildSqlRequest', () => {
  it('routes read queries through the query endpoint', () => {
    expect(buildSqlRequest('SELECT * FROM users LIMIT 10')).toEqual({
      endpoint: '/v1/sql/query',
      payload: { query: 'SELECT * FROM users LIMIT 10' },
      mode: 'results',
    })
  })

  it('normalizes trailing semicolons before routing', () => {
    expect(buildSqlRequest('SELECT * FROM users;;;')).toEqual({
      endpoint: '/v1/sql/query',
      payload: { query: 'SELECT * FROM users' },
      mode: 'results',
    })
  })

  it('routes explain requests through the query endpoint with explain query plan', () => {
    expect(buildSqlRequest('SELECT * FROM users', { explain: true })).toEqual({
      endpoint: '/v1/sql/query',
      payload: { query: 'EXPLAIN SELECT * FROM users' },
      mode: 'explain',
    })
  })

  it('routes write statements through the execute endpoint', () => {
    expect(buildSqlRequest("UPDATE users SET active = 0 WHERE id = 1")).toEqual({
      endpoint: '/v1/sql/execute',
      payload: { query: "UPDATE users SET active = 0 WHERE id = 1" },
      mode: 'results',
    })
  })
})

describe('normalizeSqlResponse', () => {
  it('normalizes row results into columns and rows', () => {
    expect(
      normalizeSqlResponse({
        success: true,
        data: [
          { id: 1, name: 'Ada', active: true },
          { id: 2, name: 'Lin', active: false },
        ],
      }),
    ).toEqual({
      kind: 'results',
      columns: ['id', 'name', 'active'],
      rows: [
        [1, 'Ada', true],
        [2, 'Lin', false],
      ],
      rowCount: 2,
      empty: false,
    })
  })

  it('normalizes explain rows into a plan model', () => {
    expect(
      normalizeSqlResponse(
        {
          success: true,
          data: [{ id: 3, parent: 1, detail: 'SCAN users' }],
        },
        { explain: true },
      ),
    ).toEqual({
      kind: 'explain',
      plan: [{ id: 0, parent: 1, detail: 'SCAN users' }],
    })
  })

  it('normalizes execute responses into a message model', () => {
    expect(
      normalizeSqlResponse({
        success: true,
        affected: 4,
      }),
    ).toEqual({
      kind: 'message',
      affected: 4,
      message: '4 rows affected',
    })
  })

  it('uses a singular row label for single-row execute responses', () => {
    expect(
      normalizeSqlResponse({
        success: true,
        affected: 1,
      }),
    ).toEqual({
      kind: 'message',
      affected: 1,
      message: '1 row affected',
    })
  })

  it('normalizes empty result sets with no columns', () => {
    expect(
      normalizeSqlResponse({
        success: true,
        data: [],
      }),
    ).toEqual({
      kind: 'results',
      columns: [],
      rows: [],
      rowCount: 0,
      empty: true,
    })
  })

  it('normalizes failed responses into a structured error', () => {
    expect(
      normalizeSqlResponse({
        success: false,
        message: 'syntax error near FROM',
      }),
    ).toEqual({
      kind: 'error',
      message: 'syntax error near FROM',
    })
  })

  it('falls back to a generic error message', () => {
    expect(normalizeSqlResponse({ success: false })).toEqual({
      kind: 'error',
      message: 'Query failed',
    })
  })
})
