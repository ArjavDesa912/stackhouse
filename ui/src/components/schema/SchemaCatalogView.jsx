import React, { useMemo, useState } from 'react'
import {
  ArrowUpRight,
  Database,
  FileSearch,
  KeyRound,
  LayoutGrid,
  Network,
  RefreshCw,
  Rows3,
  Search,
  Sparkles,
  TableProperties,
} from 'lucide-react'

import { Button } from '@/components/ui/button'
import { Input } from '@/components/ui/input'
import { cn } from '@/lib/utils'

function Overline({ children, className = '' }) {
  return (
    <p className={`text-[9.5px] font-medium uppercase tracking-[0.14em] text-muted-foreground/80 ${className}`}>
      {children}
    </p>
  )
}

function Stat({ label, value, accent = false }) {
  return (
    <div
      className={cn(
        'flex-1 rounded-md border px-3 py-2',
        accent ? 'border-primary/25 bg-primary/5' : 'border-border bg-background',
      )}
    >
      <Overline>{label}</Overline>
      <p className="mt-0.5 font-serif text-[22px] font-medium leading-none tracking-tight text-foreground">
        {value}
      </p>
    </div>
  )
}

function SortPill({ active, children, onClick }) {
  return (
    <button
      type="button"
      onClick={onClick}
      className={cn(
        'rounded-full px-2 py-0.5 text-[11px] font-medium tracking-tight transition-colors',
        active
          ? 'bg-secondary text-foreground ring-1 ring-border dark:ring-1 ring-border'
          : 'text-muted-foreground hover:bg-muted/50 hover:text-foreground',
      )}
    >
      {children}
    </button>
  )
}

function TableCard({ table, active, onSelect, onOpenWorkspace, onSelectTable }) {
  const primaryKey = (table.columns || []).find((c) => c.primary_key)
  const sampleColumns = (table.columns || []).slice(0, 4)

  return (
    <button
      type="button"
      onClick={onSelect}
      className={cn(
        'group relative flex flex-col overflow-hidden rounded-lg border bg-card text-left transition-all',
        active
          ? 'border-primary/30 ring-1 ring-primary'
          : 'border-border hover:-translate-y-0.5 hover:shadow-none',
      )}
    >
      {/* Decorative header strip */}
      <div
        className={cn(
          'flex items-center justify-between gap-2 px-3 pt-3 pb-2',
          active ? 'bg-primary/5' : 'bg-muted/20',
        )}
      >
        <div className="flex min-w-0 items-center gap-1.5">
          <div
            className={cn(
              'flex h-8 w-8 shrink-0 items-center justify-center rounded-md',
              active ? 'bg-primary text-primary-foreground' : 'bg-background text-primary ring-1 ring-border',
            )}
          >
            <Database className="h-3 w-3" />
          </div>
          <div className="min-w-0">
            <p className="truncate font-serif text-[17px] font-medium leading-tight tracking-tight text-foreground">
              {table.name}
            </p>
            {primaryKey ? (
              <p className="mt-0.5 flex items-center gap-0.5 text-[10.5px] text-muted-foreground">
                <KeyRound className="h-2 w-2" />
                {primaryKey.name}
              </p>
            ) : (
              <p className="mt-0.5 text-[10.5px] text-muted-foreground">No primary key</p>
            )}
          </div>
        </div>
        <ArrowUpRight
          className={cn(
            'h-3 w-3 shrink-0 transition-transform',
            active ? 'text-primary' : 'text-muted-foreground group-hover:-translate-y-0.5 group-hover:translate-x-0.5 group-hover:text-foreground',
          )}
        />
      </div>

      <div className="flex items-center gap-2 border-t border-border/60 px-3 py-1.5 text-[11px] text-muted-foreground">
        <span className="inline-flex items-center gap-0.5 tabular-nums">
          <Rows3 className="h-2 w-2" />
          {table.row_count?.toLocaleString?.() ?? 0} rows
        </span>
        <span className="h-2 w-px bg-border" />
        <span className="inline-flex items-center gap-0.5 tabular-nums">
          <TableProperties className="h-2 w-2" />
          {table.column_count ?? table.columns?.length ?? 0} cols
        </span>
      </div>

      <div className="px-3 py-2">
        <Overline>Columns</Overline>
        <div className="mt-1 flex flex-wrap gap-0.5.5">
          {sampleColumns.map((c) => (
            <span
              key={c.name}
              className="rounded-md border border-border bg-background px-1 py-0.5 font-mono text-[10.5px] text-muted-foreground"
            >
              {c.name}
            </span>
          ))}
          {(table.columns?.length || 0) > 4 ? (
            <span className="rounded-md px-1 py-0.5 text-[10.5px] text-muted-foreground/70">
              +{table.columns.length - 4} more
            </span>
          ) : null}
        </div>
      </div>

      <div className="mt-auto flex items-center gap-0.5 border-t border-border/60 bg-muted/10 px-2 py-1">
        <button
          type="button"
          onClick={(e) => {
            e.stopPropagation()
            onSelectTable(table.name)
            onOpenWorkspace('designer')
          }}
          className="flex items-center gap-0.5.5 rounded-md px-1.5 py-0.5 text-[11px] font-medium text-muted-foreground transition-colors hover:bg-background hover:text-foreground"
        >
          <TableProperties className="h-2 w-2" />
          Design
        </button>
        <button
          type="button"
          onClick={(e) => {
            e.stopPropagation()
            onSelectTable(table.name)
            onOpenWorkspace('explorer')
          }}
          className="flex items-center gap-0.5.5 rounded-md px-1.5 py-0.5 text-[11px] font-medium text-muted-foreground transition-colors hover:bg-background hover:text-foreground"
        >
          <FileSearch className="h-2 w-2" />
          Explore
        </button>
        <button
          type="button"
          onClick={(e) => {
            e.stopPropagation()
            onSelectTable(table.name)
            onOpenWorkspace('model')
          }}
          className="flex items-center gap-0.5.5 rounded-md px-1.5 py-0.5 text-[11px] font-medium text-muted-foreground transition-colors hover:bg-background hover:text-foreground"
        >
          <Network className="h-2 w-2" />
          Model
        </button>
      </div>
    </button>
  )
}

export default function SchemaCatalogView({
  tables = [],
  selectedTableName,
  onSelectTable,
  onOpenWorkspace,
  onRefresh,
}) {
  const [search, setSearch] = useState('')
  const [sort, setSort] = useState('name') // 'name' | 'rows' | 'columns'

  const filtered = useMemo(() => {
    const q = search.trim().toLowerCase()
    const list = q
      ? tables.filter(
          (t) =>
            t.name.toLowerCase().includes(q) ||
            (t.columns || []).some((c) => c.name.toLowerCase().includes(q)),
        )
      : tables
    const sorted = [...list]
    if (sort === 'name') sorted.sort((a, b) => a.name.localeCompare(b.name))
    if (sort === 'rows') sorted.sort((a, b) => (b.row_count || 0) - (a.row_count || 0))
    if (sort === 'columns')
      sorted.sort((a, b) => (b.columns?.length || 0) - (a.columns?.length || 0))
    return sorted
  }, [tables, search, sort])

  const totalRows = tables.reduce((sum, t) => sum + (t.row_count || 0), 0)
  const totalCols = tables.reduce((sum, t) => sum + (t.columns?.length || 0), 0)
  const withPk = tables.filter((t) => (t.columns || []).some((c) => c.primary_key)).length

  return (
    <div className="flex h-full flex-col overflow-y-auto bg-background">
      {/* Hero strip */}
      <div className="border-b border-border bg-background px-3 pb-3 pt-3">
        <div className="mb-3 flex items-start justify-between gap-3">
          <div className="max-w-xl">
            <Overline>Schema Catalog</Overline>
            <h2 className="mt-0.5 font-serif text-[28px] font-medium leading-[1.12] tracking-tight text-foreground">
              Every table in your warehouse.
            </h2>
            <p className="mt-0.5.5 text-[13px] leading-relaxed text-muted-foreground">
              A curated index — search, sort, and jump into the right workspace. Selecting a table sets it
              globally for every other tool.
            </p>
          </div>
          <div className="hidden items-center gap-1 md:flex">
            <Button
              variant="outline"
              size="sm"
              onClick={onRefresh}
              className="h-6 gap-0.5.5 rounded-full text-[11.5px]"
            >
              <RefreshCw className="h-2 w-2" />
              Refresh
            </Button>
          </div>
        </div>

        <div className="flex flex-wrap gap-2">
          <Stat label="Tables" value={tables.length} accent />
          <Stat label="Total rows" value={totalRows.toLocaleString()} />
          <Stat label="Columns" value={totalCols} />
          <Stat label="With PK" value={`${withPk}/${tables.length}`} />
        </div>
      </div>

      <div className="sticky top-0 z-10 flex flex-wrap items-center justify-between gap-2 border-b border-border bg-background/95 px-3 py-2 backdrop-blur">
        <div className="relative w-full max-w-sm">
          <Search className="pointer-events-none absolute left-3 top-1/2 h-2.5 w-2.5 -translate-y-1/2 text-muted-foreground" />
          <Input
            value={search}
            onChange={(e) => setSearch(e.target.value)}
            placeholder="Search tables or columns…"
            className="h-8 rounded-full border-border bg-muted/30 pl-9 text-[12.5px]"
          />
        </div>
        <div className="flex items-center gap-0.5 rounded-full border border-border bg-card px-0.5 py-0.5">
          <LayoutGrid className="ml-1 h-2.5 w-2.5 text-muted-foreground" />
          <SortPill active={sort === 'name'} onClick={() => setSort('name')}>
            Name
          </SortPill>
          <SortPill active={sort === 'rows'} onClick={() => setSort('rows')}>
            Rows
          </SortPill>
          <SortPill active={sort === 'columns'} onClick={() => setSort('columns')}>
            Columns
          </SortPill>
        </div>
      </div>

      <div className="px-3 py-3">
        {filtered.length === 0 ? (
          <div className="flex h-40 flex-col items-center justify-center rounded-lg border border-dashed border-border bg-muted/10 text-center">
            <Sparkles className="mb-2 h-5 w-5 text-muted-foreground" />
            <p className="font-serif text-[17px] font-medium tracking-tight text-foreground">
              {tables.length === 0 ? 'No tables yet' : 'No matches'}
            </p>
            <p className="mt-0.5 max-w-sm text-[12.5px] text-muted-foreground">
              {tables.length === 0
                ? 'Create one in the Designer or import a file through the Import Studio.'
                : 'Try a different search term or clear the filter to see the full catalog.'}
            </p>
            {tables.length === 0 ? (
              <div className="mt-3 flex gap-1">
                <Button size="sm" onClick={() => onOpenWorkspace('designer')} className="rounded-full">
                  Open Designer
                </Button>
                <Button
                  size="sm"
                  variant="outline"
                  onClick={() => onOpenWorkspace('import')}
                  className="rounded-full"
                >
                  Import data
                </Button>
              </div>
            ) : null}
          </div>
        ) : (
          <div className="grid gap-3 md:grid-cols-2 xl:grid-cols-3 2xl:grid-cols-4">
            {filtered.map((table) => (
              <TableCard
                key={table.name}
                table={table}
                active={table.name === selectedTableName}
                onSelect={() => onSelectTable(table.name)}
                onOpenWorkspace={onOpenWorkspace}
                onSelectTable={onSelectTable}
              />
            ))}
          </div>
        )}
      </div>
    </div>
  )
}
