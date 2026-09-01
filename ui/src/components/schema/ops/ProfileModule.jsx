import React, { useCallback, useEffect, useMemo, useState } from 'react'
import { ChevronDown, KeyRound, Play, RotateCw, Scan } from 'lucide-react'

import { Banner, Empty, InlineSpinner, MetricTile, Overline, Section, apiFetch } from './common'
import { Button } from '@/components/ui/button'
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuTrigger,
} from '@/components/ui/dropdown-menu'
import { cn } from '@/lib/utils'

const NUMERIC_TYPES = new Set(['INTEGER', 'REAL', 'NUMERIC'])

function isNumericType(type) {
  if (!type) return false
  return NUMERIC_TYPES.has(String(type).toUpperCase())
}

function Bar({ value, tone = 'primary' }) {
  const pct = Math.max(0, Math.min(100, value))
  const color =
    tone === 'primary'
      ? 'bg-primary'
      : tone === 'warn'
      ? 'bg-primary/70'
      : tone === 'muted'
      ? 'bg-muted-foreground'
      : 'bg-success'
  return (
    <div className="h-0.5.5 w-full overflow-hidden rounded-full bg-muted">
      <div className={cn('h-full rounded-full transition-all', color)} style={{ width: `${pct}%` }} />
    </div>
  )
}

async function profileColumn(tableName, column) {
  const isNumeric = isNumericType(column.col_type)
  const parts = [
    'COUNT(*) AS total',
    `COUNT(DISTINCT "${column.name}") AS distinct_count`,
    `SUM(CASE WHEN "${column.name}" IS NULL THEN 1 ELSE 0 END) AS null_count`,
  ]
  if (isNumeric) {
    parts.push(`MIN("${column.name}") AS min_value`, `MAX("${column.name}") AS max_value`)
  } else {
    parts.push(`MIN(LENGTH("${column.name}")) AS min_len`, `MAX(LENGTH("${column.name}")) AS max_len`)
  }
  const query = `SELECT ${parts.join(', ')} FROM "${tableName}"`
  const payload = await apiFetch('/v1/sql/query', {
    method: 'POST',
    body: JSON.stringify({ query }),
  })
  const row = Array.isArray(payload?.data) && payload.data[0] ? payload.data[0] : {}
  return {
    name: column.name,
    col_type: column.col_type,
    primary_key: column.primary_key,
    nullable: column.nullable,
    total: Number(row.total ?? 0),
    distinct: Number(row.distinct_count ?? 0),
    nulls: Number(row.null_count ?? 0),
    minValue: isNumeric ? row.min_value : undefined,
    maxValue: isNumeric ? row.max_value : undefined,
    minLen: !isNumeric ? row.min_len : undefined,
    maxLen: !isNumeric ? row.max_len : undefined,
    isNumeric,
  }
}

export default function ProfileModule({ tables = [] }) {
  const [selectedTable, setSelectedTable] = useState(tables[0]?.name || '')
  const [rows, setRows] = useState([])
  const [loading, setLoading] = useState(false)
  const [error, setError] = useState('')

  useEffect(() => {
    if (!selectedTable && tables.length > 0) setSelectedTable(tables[0].name)
  }, [tables, selectedTable])

  const table = useMemo(() => tables.find((t) => t.name === selectedTable), [tables, selectedTable])

  const run = useCallback(async () => {
    if (!table) return
    setLoading(true)
    setError('')
    setRows([])
    try {
      const results = []
      for (const column of table.columns || []) {
        // sequential to keep SQL load low
        // eslint-disable-next-line no-await-in-loop
        const result = await profileColumn(table.name, column)
        results.push(result)
        setRows([...results])
      }
    } catch (e) {
      setError(e.message)
    } finally {
      setLoading(false)
    }
  }, [table])

  const totalRows = rows[0]?.total ?? table?.row_count ?? 0
  const avgNullPct =
    rows.length && totalRows
      ? (rows.reduce((sum, r) => sum + r.nulls / (r.total || 1), 0) / rows.length) * 100
      : 0
  const highCardinality = rows.filter((r) => r.total > 0 && r.distinct / r.total > 0.9).length

  if (tables.length === 0) {
    return (
      <div className="p-3">
        <Empty icon={Scan} title="No tables available" description="Create a table first to profile its columns." />
      </div>
    )
  }

  return (
    <div className="h-full overflow-y-auto">
      <div className="space-y-3 p-3">
        {/* Controls */}
        <div className="flex flex-wrap items-center justify-between gap-2">
          <div className="flex items-center gap-2">
            <div className="flex h-8 w-8 items-center justify-center rounded-md bg-primary/10 text-primary ring-1 ring-primary/20">
              <Scan className="h-3 w-3" />
            </div>
            <div>
              <Overline>Column profile</Overline>
              <p className="font-serif text-[20px] font-medium leading-tight tracking-tight text-foreground">
                Data shape & cardinality
              </p>
            </div>
          </div>
          <div className="flex items-center gap-1">
            <DropdownMenu>
              <DropdownMenuTrigger asChild>
                <Button
                  variant="outline"
                  size="sm"
                  className="h-6 max-w-[220px] gap-1 rounded-full text-[12px]"
                >
                  <span className="truncate font-mono">{selectedTable || 'Select table'}</span>
                  <ChevronDown className="h-2 w-2 text-muted-foreground" />
                </Button>
              </DropdownMenuTrigger>
              <DropdownMenuContent align="end" className="max-h-72 w-40 overflow-y-auto">
                {tables.map((t) => (
                  <DropdownMenuItem
                    key={t.name}
                    onSelect={() => {
                      setSelectedTable(t.name)
                      setRows([])
                    }}
                    className="font-mono text-[12px]"
                  >
                    {t.name}
                    <span className="ml-auto text-[10px] tabular-nums text-muted-foreground">
                      {t.row_count?.toLocaleString?.() ?? 0}
                    </span>
                  </DropdownMenuItem>
                ))}
              </DropdownMenuContent>
            </DropdownMenu>
            <Button
              size="sm"
              onClick={run}
              disabled={!table || loading}
              className="h-6 gap-0.5.5 rounded-full px-3 text-[11.5px]"
            >
              {loading ? <InlineSpinner /> : <Play className="h-2 w-2 fill-current" />}
              {loading ? 'Profiling…' : rows.length > 0 ? 'Re-run' : 'Run profile'}
            </Button>
            {rows.length > 0 ? (
              <Button
                size="sm"
                variant="ghost"
                onClick={() => setRows([])}
                className="h-6 gap-0.5.5 text-[11.5px]"
              >
                <RotateCw className="h-2 w-2" />
                Reset
              </Button>
            ) : null}
          </div>
        </div>

        {error ? <Banner kind="error">{error}</Banner> : null}

        {/* Summary */}
        {rows.length > 0 ? (
          <div className="grid gap-2 md:grid-cols-4">
            <MetricTile label="Columns profiled" value={rows.length} tone="accent" />
            <MetricTile label="Row count" value={Number(totalRows).toLocaleString()} />
            <MetricTile label="Avg null rate" value={avgNullPct.toFixed(1)} suffix="%" />
            <MetricTile label="High-cardinality" value={highCardinality} suffix=">90% distinct" />
          </div>
        ) : null}

        {/* Per-column table */}
        <Section
          title={table ? `Columns of ${table.name}` : 'Columns'}
          description="Each row runs a COUNT/DISTINCT/NULL aggregation via /v1/sql/query."
        >
          {rows.length === 0 && !loading ? (
            <Empty
              icon={Scan}
              title="No profile yet"
              description="Pick a table and run the profiler to compute per-column statistics."
            />
          ) : (
            <div className="overflow-hidden rounded-md border border-border">
              <table className="min-w-full text-left text-[12px]">
                <thead className="bg-muted/30">
                  <tr className="border-b border-border">
                    {[
                      'Column',
                      'Type',
                      'Distinct',
                      'Null %',
                      'Min',
                      'Max',
                    ].map((h) => (
                      <th
                        key={h}
                        className="px-2 py-1 text-[10px] font-medium uppercase tracking-[0.12em] text-muted-foreground"
                      >
                        {h}
                      </th>
                    ))}
                  </tr>
                </thead>
                <tbody>
                  {rows.map((row, index) => {
                    const total = row.total || 1
                    const distinctPct = (row.distinct / total) * 100
                    const nullPct = (row.nulls / total) * 100
                    return (
                      <tr
                        key={row.name}
                        className={cn(
                          index !== rows.length - 1 && 'border-b border-border/60',
                        )}
                      >
                        <td className="px-2 py-1.5 font-mono text-foreground">
                          <div className="flex items-center gap-1">
                            {row.primary_key ? (
                              <KeyRound className="h-2 w-2 text-primary" />
                            ) : (
                              <span className="inline-block h-2 w-2" />
                            )}
                            {row.name}
                          </div>
                        </td>
                        <td className="px-2 py-1.5 font-mono text-[11px] uppercase text-muted-foreground">
                          {row.col_type}
                        </td>
                        <td className="px-2 py-1.5 align-middle">
                          <div className="flex items-center gap-1">
                            <span className="w-10 shrink-0 text-[11px] tabular-nums text-foreground">
                              {row.distinct.toLocaleString()}
                            </span>
                            <div className="flex-1">
                              <Bar value={distinctPct} tone="primary" />
                            </div>
                            <span className="w-8 shrink-0 text-right text-[10px] tabular-nums text-muted-foreground">
                              {distinctPct.toFixed(0)}%
                            </span>
                          </div>
                        </td>
                        <td className="px-2 py-1.5 align-middle">
                          <div className="flex items-center gap-1">
                            <span className="w-10 shrink-0 text-[11px] tabular-nums text-foreground">
                              {row.nulls.toLocaleString()}
                            </span>
                            <div className="flex-1">
                              <Bar
                                value={nullPct}
                                tone={nullPct > 50 ? 'warn' : nullPct > 10 ? 'muted' : 'success'}
                              />
                            </div>
                            <span className="w-8 shrink-0 text-right text-[10px] tabular-nums text-muted-foreground">
                              {nullPct.toFixed(0)}%
                            </span>
                          </div>
                        </td>
                        <td className="px-2 py-1.5 font-mono text-[11px] text-muted-foreground">
                          {row.isNumeric
                            ? row.minValue ?? '—'
                            : row.minLen !== null && row.minLen !== undefined
                            ? `len ${row.minLen}`
                            : '—'}
                        </td>
                        <td className="px-2 py-1.5 font-mono text-[11px] text-muted-foreground">
                          {row.isNumeric
                            ? row.maxValue ?? '—'
                            : row.maxLen !== null && row.maxLen !== undefined
                            ? `len ${row.maxLen}`
                            : '—'}
                        </td>
                      </tr>
                    )
                  })}
                </tbody>
              </table>
            </div>
          )}
          {loading ? (
            <p className="mt-2 flex items-center gap-1 text-[11.5px] text-muted-foreground">
              <InlineSpinner /> Profiled {rows.length} of {table?.columns?.length || 0} columns…
            </p>
          ) : null}
        </Section>
      </div>
    </div>
  )
}
