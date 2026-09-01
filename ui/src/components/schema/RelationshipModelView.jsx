import React, { useMemo } from 'react'
import { ArrowRight, KeyRound, Link2, Network, Table2 } from 'lucide-react'

import { inferTableRelationships } from '@/lib/schemaManager'
import { cn } from '@/lib/utils'

function Overline({ children, className = '' }) {
  return (
    <p className={`text-[9.5px] font-medium uppercase tracking-[0.14em] text-muted-foreground/80 ${className}`}>
      {children}
    </p>
  )
}

function TableChip({ table, active, linked, onSelect }) {
  const pk = (table.columns || []).find((c) => c.primary_key)
  return (
    <button
      type="button"
      onClick={onSelect}
      className={cn(
        'group flex min-w-[220px] flex-col overflow-hidden rounded-md border bg-card text-left transition-all',
        active
          ? 'border-primary/40 ring-1 ring-primary'
          : linked
          ? 'border-primary/20 bg-primary/5'
          : 'border-border hover:border-primary/20',
      )}
    >
      <div className="flex items-center justify-between gap-1 border-b border-border/60 bg-muted/20 px-2 py-1">
        <div className="flex min-w-0 items-center gap-1">
          <Table2 className={cn('h-2.5 w-2.5', active ? 'text-primary' : 'text-muted-foreground')} />
          <span className="truncate font-mono text-[12px] font-medium text-foreground">{table.name}</span>
        </div>
        <span className="text-[10px] tabular-nums text-muted-foreground">
          {table.row_count?.toLocaleString?.() ?? 0}
        </span>
      </div>
      <div className="space-y-0.5 px-2 py-1">
        {(table.columns || []).slice(0, 5).map((column) => (
          <div
            key={column.name}
            className="flex items-center gap-1 font-mono text-[10.5px] text-muted-foreground"
          >
            {column.primary_key ? (
              <KeyRound className="h-1.5 w-1.5 text-primary" />
            ) : column.name.endsWith('_id') ? (
              <Link2 className="h-1.5 w-1.5 text-success dark:text-success" />
            ) : (
              <span className="inline-block h-1.5 w-1.5" />
            )}
            <span className="flex-1 truncate">{column.name}</span>
            <span className="text-[9px] uppercase text-muted-foreground/70">{column.col_type}</span>
          </div>
        ))}
        {(table.columns?.length || 0) > 5 ? (
          <p className="pt-0.5 text-[10px] text-muted-foreground/70">
            +{table.columns.length - 5} more
          </p>
        ) : null}
      </div>
      {pk ? (
        <div className="border-t border-border/60 bg-background px-2 py-0.5.5 text-[10px] text-muted-foreground">
          <span className="font-mono">PK:</span> {pk.name}
        </div>
      ) : null}
    </button>
  )
}

export default function RelationshipModelView({ tables = [], selectedTableName, onSelectTable }) {
  const relationships = useMemo(() => inferTableRelationships(tables), [tables])

  const linkedSet = useMemo(() => {
    const set = new Set()
    if (!selectedTableName) return set
    relationships.forEach((r) => {
      if (r.sourceTable === selectedTableName) set.add(r.targetTable)
      if (r.targetTable === selectedTableName) set.add(r.sourceTable)
    })
    return set
  }, [relationships, selectedTableName])

  const incoming = relationships.filter((r) => r.targetTable === selectedTableName)
  const outgoing = relationships.filter((r) => r.sourceTable === selectedTableName)

  return (
    <div className="flex h-full min-h-0 flex-col bg-background">
      <div className="shrink-0 border-b border-border bg-card/40 px-3 py-3">
        <Overline>Relationship Model</Overline>
        <h2 className="mt-0.5 font-serif text-[22px] font-medium leading-tight tracking-tight text-foreground">
          Inferred foreign-key map
        </h2>
        <p className="mt-0.5 max-w-2xl text-[12.5px] text-muted-foreground">
          Links are inferred from <span className="font-mono">_id</span> suffix naming. Read-only — actual
          foreign keys are not enforced here.
        </p>
        <div className="mt-3 flex gap-1 text-[11px]">
          <span className="inline-flex items-center gap-0.5.5 rounded-full border border-border bg-background px-1.5 py-0.5 tabular-nums text-muted-foreground">
            <Table2 className="h-2 w-2" />
            {tables.length} tables
          </span>
          <span className="inline-flex items-center gap-0.5.5 rounded-full border border-border bg-background px-1.5 py-0.5 tabular-nums text-muted-foreground">
            <Link2 className="h-2 w-2 text-primary" />
            {relationships.length} links
          </span>
        </div>
      </div>

      <div className="grid min-h-0 flex-1 gap-0 overflow-hidden lg:grid-cols-[1.5fr_1fr]">
        <div className="min-h-0 overflow-y-auto border-r border-border p-3">
          <Overline>All tables</Overline>
          <div className="mt-2 flex flex-wrap gap-2">
            {tables.length === 0 ? (
              <div className="w-full rounded-md border border-dashed border-border bg-muted/10 p-4 text-center text-[12.5px] text-muted-foreground">
                No tables yet.
              </div>
            ) : (
              tables.map((table) => (
                <TableChip
                  key={table.name}
                  table={table}
                  active={table.name === selectedTableName}
                  linked={linkedSet.has(table.name)}
                  onSelect={() => onSelectTable(table.name)}
                />
              ))
            )}
          </div>
        </div>

        <div className="min-h-0 overflow-y-auto bg-muted/10 p-3">
          <Overline>
            {selectedTableName ? `Connections · ${selectedTableName}` : 'Inferred links'}
          </Overline>

          {!selectedTableName ? (
            <div className="mt-2 space-y-1">
              {relationships.length === 0 ? (
                <div className="rounded-md border border-dashed border-border bg-background p-4 text-center">
                  <Network className="mx-auto mb-1 h-5 w-5 text-muted-foreground" />
                  <p className="font-serif text-[15px] font-medium text-foreground">
                    No relationships inferred
                  </p>
                  <p className="mt-0.5 text-[11.5px] text-muted-foreground">
                    Add columns named <span className="font-mono">entity_id</span> pointing to an
                    existing table to auto-link them.
                  </p>
                </div>
              ) : (
                relationships.map((r) => (
                  <div
                    key={`${r.sourceTable}-${r.sourceColumn}-${r.targetTable}`}
                    className="flex items-center gap-1 rounded-md border border-border bg-background p-2"
                  >
                    <button
                      type="button"
                      onClick={() => onSelectTable(r.sourceTable)}
                      className="font-mono text-[12px] font-medium text-foreground hover:text-primary"
                    >
                      {r.sourceTable}
                    </button>
                    <span className="font-mono text-[10px] text-muted-foreground">
                      .{r.sourceColumn}
                    </span>
                    <ArrowRight className="h-2.5 w-2.5 text-primary" />
                    <button
                      type="button"
                      onClick={() => onSelectTable(r.targetTable)}
                      className="font-mono text-[12px] font-medium text-foreground hover:text-primary"
                    >
                      {r.targetTable}
                    </button>
                  </div>
                ))
              )}
            </div>
          ) : (
            <div className="mt-2 space-y-3">
              <div>
                <p className="mb-1 text-[10.5px] font-medium uppercase tracking-[0.12em] text-muted-foreground">
                  Outgoing ({outgoing.length})
                </p>
                {outgoing.length === 0 ? (
                  <p className="rounded-md border border-dashed border-border bg-background p-2 text-[11.5px] text-muted-foreground">
                    No outgoing references.
                  </p>
                ) : (
                  <div className="space-y-0.5.5">
                    {outgoing.map((r) => (
                      <div
                        key={`${r.sourceColumn}-${r.targetTable}`}
                        className="flex items-center gap-1 rounded-md border border-border bg-background p-1.5 text-[12px]"
                      >
                        <span className="font-mono text-muted-foreground">{r.sourceColumn}</span>
                        <ArrowRight className="h-2 w-2 text-primary" />
                        <button
                          type="button"
                          onClick={() => onSelectTable(r.targetTable)}
                          className="font-mono font-medium text-foreground hover:text-primary"
                        >
                          {r.targetTable}
                        </button>
                      </div>
                    ))}
                  </div>
                )}
              </div>
              <div>
                <p className="mb-1 text-[10.5px] font-medium uppercase tracking-[0.12em] text-muted-foreground">
                  Incoming ({incoming.length})
                </p>
                {incoming.length === 0 ? (
                  <p className="rounded-md border border-dashed border-border bg-background p-2 text-[11.5px] text-muted-foreground">
                    Nothing references this table.
                  </p>
                ) : (
                  <div className="space-y-0.5.5">
                    {incoming.map((r) => (
                      <div
                        key={`${r.sourceTable}-${r.sourceColumn}`}
                        className="flex items-center gap-1 rounded-md border border-border bg-background p-1.5 text-[12px]"
                      >
                        <button
                          type="button"
                          onClick={() => onSelectTable(r.sourceTable)}
                          className="font-mono font-medium text-foreground hover:text-primary"
                        >
                          {r.sourceTable}
                        </button>
                        <span className="font-mono text-muted-foreground">.{r.sourceColumn}</span>
                        <ArrowRight className="h-2 w-2 text-primary" />
                        <span className="font-mono text-muted-foreground">{selectedTableName}</span>
                      </div>
                    ))}
                  </div>
                )}
              </div>
            </div>
          )}
        </div>
      </div>
    </div>
  )
}
