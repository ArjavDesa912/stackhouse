import React, { useMemo, useState } from 'react'
import { ChevronDown, Copy, Layers, Play, Plus, Trash2 } from 'lucide-react'

import { Banner, Empty, InlineSpinner, Overline, Section, apiFetch } from './common'
import { Button } from '@/components/ui/button'
import { Input } from '@/components/ui/input'
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuTrigger,
} from '@/components/ui/dropdown-menu'

const AGG_FUNCTIONS = ['COUNT', 'SUM', 'AVG', 'MIN', 'MAX']

function buildQuery({ table, groupBy, aggregates, whereClause, orderBy, limit }) {
  if (!table) return ''
  const selectParts = []

  groupBy.forEach((column) => {
    selectParts.push(`"${column}"`)
  })

  aggregates.forEach((agg) => {
    if (!agg.func) return
    const argument = agg.func === 'COUNT' && !agg.column ? '*' : agg.column ? `"${agg.column}"` : '*'
    const alias = agg.alias || `${agg.func.toLowerCase()}_${agg.column || 'all'}`
    selectParts.push(`${agg.func}(${argument}) AS "${alias}"`)
  })

  if (selectParts.length === 0) selectParts.push('*')

  const parts = [`SELECT ${selectParts.join(', ')}`, `FROM "${table}"`]
  if (whereClause.trim()) parts.push(`WHERE ${whereClause.trim()}`)
  if (groupBy.length > 0) parts.push(`GROUP BY ${groupBy.map((c) => `"${c}"`).join(', ')}`)
  if (orderBy.trim()) parts.push(`ORDER BY ${orderBy.trim()}`)
  if (limit) parts.push(`LIMIT ${limit}`)
  return parts.join('\n')
}

function ColumnPicker({ columns, value, onChange, placeholder = 'column' }) {
  return (
    <DropdownMenu>
      <DropdownMenuTrigger asChild>
        <Button
          variant="outline"
          size="sm"
          className="h-6 min-w-[140px] max-w-[200px] justify-between gap-1 rounded-md bg-background font-mono text-[11.5px]"
        >
          <span className="truncate">{value || placeholder}</span>
          <ChevronDown className="h-2 w-2 text-muted-foreground" />
        </Button>
      </DropdownMenuTrigger>
      <DropdownMenuContent align="start" className="max-h-64 w-40 overflow-y-auto">
        {columns.map((column) => (
          <DropdownMenuItem
            key={column.name}
            onSelect={() => onChange(column.name)}
            className="justify-between font-mono text-[11.5px]"
          >
            <span>{column.name}</span>
            <span className="text-[9.5px] uppercase text-muted-foreground">{column.col_type}</span>
          </DropdownMenuItem>
        ))}
      </DropdownMenuContent>
    </DropdownMenu>
  )
}

export default function QueryBuilderModule({ tables = [] }) {
  const [tableName, setTableName] = useState(tables[0]?.name || '')
  const [groupBy, setGroupBy] = useState([])
  const [aggregates, setAggregates] = useState([{ func: 'COUNT', column: '', alias: 'total' }])
  const [whereClause, setWhereClause] = useState('')
  const [orderBy, setOrderBy] = useState('')
  const [limit, setLimit] = useState(100)
  const [result, setResult] = useState(null)
  const [error, setError] = useState('')
  const [running, setRunning] = useState(false)

  const table = useMemo(() => tables.find((t) => t.name === tableName), [tables, tableName])
  const columns = table?.columns ?? []

  const sql = useMemo(
    () => buildQuery({ table: tableName, groupBy, aggregates, whereClause, orderBy, limit }),
    [tableName, groupBy, aggregates, whereClause, orderBy, limit],
  )

  const addAggregate = () =>
    setAggregates((current) => [...current, { func: 'COUNT', column: '', alias: '' }])
  const removeAggregate = (index) =>
    setAggregates((current) => current.filter((_, i) => i !== index))
  const updateAggregate = (index, patch) =>
    setAggregates((current) => current.map((a, i) => (i === index ? { ...a, ...patch } : a)))

  const addGroupBy = (columnName) => {
    if (!columnName) return
    setGroupBy((current) => (current.includes(columnName) ? current : [...current, columnName]))
  }
  const removeGroupBy = (columnName) =>
    setGroupBy((current) => current.filter((c) => c !== columnName))

  const handleRun = async () => {
    if (!sql) return
    setRunning(true)
    setError('')
    setResult(null)
    try {
      const payload = await apiFetch('/v1/sql/query', {
        method: 'POST',
        body: JSON.stringify({ query: sql }),
      })
      const rows = Array.isArray(payload?.data) ? payload.data : []
      const cols = rows.length > 0 ? Object.keys(rows[0]) : []
      setResult({ columns: cols, rows })
    } catch (e) {
      setError(e.message)
    } finally {
      setRunning(false)
    }
  }

  if (tables.length === 0) {
    return (
      <div className="p-3">
        <Empty icon={Layers} title="No tables available" description="Create a table before building queries." />
      </div>
    )
  }

  return (
    <div className="h-full overflow-y-auto">
      <div className="space-y-3 p-3">
        <div className="flex items-center gap-2">
          <div className="flex h-8 w-8 items-center justify-center rounded-md bg-primary/10 text-primary ring-1 ring-primary/20">
            <Layers className="h-3 w-3" />
          </div>
          <div>
            <Overline>Query builder</Overline>
            <p className="font-serif text-[20px] font-medium leading-tight tracking-tight text-foreground">
              Compose aggregations without writing SQL
            </p>
          </div>
        </div>

        <div className="grid gap-3 lg:grid-cols-[1fr_1fr]">
          <Section title="Query shape" description="Pick a table and define how rows should be grouped and aggregated.">
            <div className="space-y-3">
              <div className="space-y-0.5.5">
                <Overline>Source table</Overline>
                <DropdownMenu>
                  <DropdownMenuTrigger asChild>
                    <Button
                      variant="outline"
                      size="sm"
                      className="h-8 w-full justify-between gap-1 rounded-md font-mono text-[12.5px]"
                    >
                      <span>{tableName || 'Select table'}</span>
                      <ChevronDown className="h-2 w-2 text-muted-foreground" />
                    </Button>
                  </DropdownMenuTrigger>
                  <DropdownMenuContent className="max-h-72 w-40 overflow-y-auto">
                    {tables.map((t) => (
                      <DropdownMenuItem
                        key={t.name}
                        onSelect={() => {
                          setTableName(t.name)
                          setGroupBy([])
                          setAggregates([{ func: 'COUNT', column: '', alias: 'total' }])
                        }}
                        className="justify-between font-mono text-[12px]"
                      >
                        <span>{t.name}</span>
                        <span className="text-[10px] text-muted-foreground">
                          {t.columns?.length ?? 0} cols
                        </span>
                      </DropdownMenuItem>
                    ))}
                  </DropdownMenuContent>
                </DropdownMenu>
              </div>

              <div className="space-y-0.5.5">
                <div className="flex items-center justify-between">
                  <Overline>Group by</Overline>
                  <DropdownMenu>
                    <DropdownMenuTrigger asChild>
                      <Button size="sm" variant="ghost" className="h-6 gap-0.5.5 text-[11px]">
                        <Plus className="h-2 w-2" />
                        Add
                      </Button>
                    </DropdownMenuTrigger>
                    <DropdownMenuContent align="end" className="max-h-64 w-40 overflow-y-auto">
                      {columns.map((column) => (
                        <DropdownMenuItem
                          key={column.name}
                          onSelect={() => addGroupBy(column.name)}
                          disabled={groupBy.includes(column.name)}
                          className="font-mono text-[11.5px]"
                        >
                          {column.name}
                        </DropdownMenuItem>
                      ))}
                    </DropdownMenuContent>
                  </DropdownMenu>
                </div>
                {groupBy.length === 0 ? (
                  <p className="text-[11.5px] text-muted-foreground">
                    No grouping — results will return a single aggregate row.
                  </p>
                ) : (
                  <div className="flex flex-wrap gap-0.5.5">
                    {groupBy.map((column) => (
                      <span
                        key={column}
                        className="inline-flex items-center gap-0.5.5 rounded-md border border-primary/30 bg-primary/5 px-1 py-0.5 font-mono text-[11px] text-primary"
                      >
                        {column}
                        <button
                          type="button"
                          onClick={() => removeGroupBy(column)}
                          className="rounded-sm p-0.5 hover:bg-primary/10"
                          title="Remove"
                        >
                          <Trash2 className="h-1.5 w-1.5" />
                        </button>
                      </span>
                    ))}
                  </div>
                )}
              </div>

              <div className="space-y-0.5.5">
                <div className="flex items-center justify-between">
                  <Overline>Aggregates</Overline>
                  <Button size="sm" variant="ghost" onClick={addAggregate} className="h-6 gap-0.5.5 text-[11px]">
                    <Plus className="h-2 w-2" /> Add
                  </Button>
                </div>
                <div className="space-y-1">
                  {aggregates.map((aggregate, index) => (
                    <div
                      key={index}
                      className="grid gap-1 rounded-md border border-border bg-background p-1 md:grid-cols-[90px_1fr_1fr_auto]"
                    >
                      <select
                        value={aggregate.func}
                        onChange={(e) => updateAggregate(index, { func: e.target.value })}
                        className="h-6 rounded-md border border-border bg-background px-1 font-mono text-[11.5px] text-foreground outline-none focus-visible:ring-2 focus-visible:ring-primary/30"
                      >
                        {AGG_FUNCTIONS.map((fn) => (
                          <option key={fn} value={fn}>
                            {fn}
                          </option>
                        ))}
                      </select>
                      <ColumnPicker
                        columns={columns}
                        value={aggregate.column}
                        onChange={(column) => updateAggregate(index, { column })}
                        placeholder={aggregate.func === 'COUNT' ? '* (all rows)' : 'column'}
                      />
                      <Input
                        value={aggregate.alias}
                        onChange={(e) => updateAggregate(index, { alias: e.target.value })}
                        placeholder="alias"
                        className="h-6 font-mono"
                      />
                      <button
                        type="button"
                        onClick={() => removeAggregate(index)}
                        className="flex h-6 w-6 items-center justify-center rounded-md text-muted-foreground transition-colors hover:bg-destructive/10 hover:text-destructive"
                        title="Remove"
                      >
                        <Trash2 className="h-2.5 w-2.5" />
                      </button>
                    </div>
                  ))}
                </div>
              </div>

              <div className="grid gap-2 md:grid-cols-2">
                <div className="space-y-0.5.5">
                  <Overline>WHERE</Overline>
                  <Input
                    value={whereClause}
                    onChange={(e) => setWhereClause(e.target.value)}
                    placeholder="e.g. status = 'active'"
                    className="h-6 font-mono text-[12px]"
                  />
                </div>
                <div className="space-y-0.5.5">
                  <Overline>ORDER BY</Overline>
                  <Input
                    value={orderBy}
                    onChange={(e) => setOrderBy(e.target.value)}
                    placeholder="e.g. total DESC"
                    className="h-6 font-mono text-[12px]"
                  />
                </div>
              </div>

              <div className="space-y-0.5.5">
                <Overline>Limit</Overline>
                <Input
                  type="number"
                  value={limit}
                  onChange={(e) => setLimit(Math.max(1, Math.min(10000, Number(e.target.value) || 0)))}
                  className="h-6 w-24 font-mono text-[12px]"
                />
              </div>

              <div className="flex items-center gap-1 pt-1">
                <Button
                  onClick={handleRun}
                  disabled={running || !tableName}
                  size="sm"
                  className="h-8 gap-0.5.5 rounded-full px-3 text-[12px]"
                >
                  {running ? <InlineSpinner /> : <Play className="h-2 w-2 fill-current" />}
                  {running ? 'Running…' : 'Run query'}
                </Button>
                <Button
                  size="sm"
                  variant="outline"
                  onClick={() => navigator.clipboard?.writeText(sql)}
                  disabled={!sql}
                  className="h-8 gap-0.5.5 rounded-full text-[11.5px]"
                >
                  <Copy className="h-2 w-2" />
                  Copy SQL
                </Button>
              </div>
            </div>
          </Section>

          <Section title="Generated SQL" description="Live preview — exactly the statement that will run.">
            <pre className="max-h-72 min-h-[180px] overflow-auto rounded-md border border-border bg-foreground p-3 font-mono text-[12px] leading-relaxed text-foreground">
              {sql || <span className="text-muted-foreground">-- Select a table to generate SQL</span>}
            </pre>
          </Section>
        </div>

        {error ? <Banner kind="error">{error}</Banner> : null}

        {result ? (
          <Section
            title="Results"
            description={`${result.rows.length} rows · ${result.columns.length} columns`}
          >
            {result.rows.length === 0 ? (
              <p className="text-[12px] text-muted-foreground">Query returned no rows.</p>
            ) : (
              <div className="overflow-auto rounded-md border border-border">
                <table className="min-w-full text-left text-[12px]">
                  <thead className="bg-muted/30">
                    <tr className="border-b border-border">
                      {result.columns.map((column) => (
                        <th
                          key={column}
                          className="px-2 py-1 font-mono text-[11px] font-medium text-foreground"
                        >
                          {column}
                        </th>
                      ))}
                    </tr>
                  </thead>
                  <tbody>
                    {result.rows.map((row, rowIndex) => (
                      <tr key={rowIndex} className="border-b border-border/60 last:border-b-0">
                        {result.columns.map((column) => (
                          <td
                            key={column}
                            className="px-2 py-0.5.5 font-mono text-[11.5px] text-muted-foreground"
                          >
                            {row[column] === null || row[column] === undefined ? (
                              <span className="italic text-muted-foreground/60">NULL</span>
                            ) : (
                              String(row[column])
                            )}
                          </td>
                        ))}
                      </tr>
                    ))}
                  </tbody>
                </table>
              </div>
            )}
          </Section>
        ) : null}
      </div>
    </div>
  )
}
