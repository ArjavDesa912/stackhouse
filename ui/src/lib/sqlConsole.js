/**
 * SQL Console utilities — Postgres-compatible query builder & response normalizer.
 */

const READ_PREFIXES = ['select', 'with', 'explain', 'pragma', 'show']

/**
 * Strip leading SQL comments (both -- line and /* block * / styles) and whitespace
 * so we can reliably detect the first real keyword.
 */
function stripLeadingComments(sql) {
  let s = sql
  // eslint-disable-next-line no-constant-condition
  while (true) {
    s = s.trimStart()
    if (s.startsWith('--')) {
      const nl = s.indexOf('\n')
      s = nl === -1 ? '' : s.slice(nl + 1)
    } else if (s.startsWith('/*')) {
      const end = s.indexOf('*/')
      s = end === -1 ? '' : s.slice(end + 2)
    } else {
      break
    }
  }
  return s.trimStart()
}

function normalizeQuery(query) {
  return (query ?? '').trim().replace(/;+\s*$/, '')
}

export function isReadQuery(query) {
  const stripped = stripLeadingComments(query).toLowerCase()
  return READ_PREFIXES.some((prefix) => stripped.startsWith(prefix))
}

/**
 * Build the API request for a SQL statement.
 * Uses EXPLAIN (Postgres syntax, NOT SQLite's EXPLAIN QUERY PLAN).
 * Routes SELECT/WITH/EXPLAIN to /v1/sql/query, everything else to /v1/sql/execute.
 */
export function buildSqlRequest(query, options = {}) {
  const normalized = normalizeQuery(query)
  const explain = options.explain === true
  const requestQuery = explain ? `EXPLAIN ${normalized}` : normalized
  // For explain mode we always use the query endpoint (returns rows).
  // For normal mode we inspect the first keyword after stripping comments.
  const endpoint = explain || isReadQuery(normalized) ? '/v1/sql/query' : '/v1/sql/execute'

  return {
    endpoint,
    payload: { query: requestQuery },
    mode: explain ? 'explain' : 'results',
  }
}

/**
 * Normalize the backend JSON response into one of:
 *   { kind: 'error',   message }
 *   { kind: 'explain', plan }
 *   { kind: 'results', columns, rows, rowCount, empty }
 *   { kind: 'message', affected, message }
 */
export function normalizeSqlResponse(response, options = {}) {
  if (!response?.success) {
    return {
      kind: 'error',
      message: response?.error?.message || response?.message || 'Query failed',
    }
  }

  // --- Explain plan ---
  if (options.explain) {
    const raw = Array.isArray(response.data) ? response.data : []
    // Postgres returns rows like { "QUERY PLAN": "..." }
    const plan = raw.map((row, idx) => ({
      id: idx,
      detail: row['QUERY PLAN'] || row.detail || JSON.stringify(row),
      parent: row.parent ?? null,
    }))
    return { kind: 'explain', plan }
  }

  // --- Tabular results ---
  if (Array.isArray(response.data)) {
    const columns = response.data.length > 0 ? Object.keys(response.data[0]) : []

    return {
      kind: 'results',
      columns,
      rows: response.data.map((row) => columns.map((column) => row[column])),
      rowCount: response.data.length,
      empty: response.data.length === 0,
    }
  }

  // --- DML affected-rows ---
  if (typeof response.affected === 'number') {
    const rowLabel = response.affected === 1 ? 'row' : 'rows'

    return {
      kind: 'message',
      affected: response.affected,
      message: `${response.affected} ${rowLabel} affected`,
    }
  }

  return {
    kind: 'message',
    affected: 0,
    message: response.message || 'Statement completed',
  }
}
