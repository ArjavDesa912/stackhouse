import React, { useState, useMemo } from 'react'
import {
  Activity,
  ChevronRight,
  Columns3,
  Copy,
  Database,
  Eraser,
  FileCode2,
  KeyRound,
  Play,
  Search,
  Table as TableIcon,
  Wand2,
} from 'lucide-react'
import { Input } from '@/components/ui/input'
import { ContextMenu } from './ContextMenu'
import { Overline } from './utils'

export function TableRail({ tables, filter, onFilter, onInsert, activeTableName, onSelect, onSetQuery }) {
  const [expanded, setExpanded] = useState(() => new Set())
  const [ctxMenu, setCtxMenu] = useState(null)

  const filtered = useMemo(() => {
    const q = filter.trim().toLowerCase()
    if (!q) return tables
    return tables.filter(
      (t) =>
        t.name.toLowerCase().includes(q) ||
        (t.columns || []).some((c) => c.name.toLowerCase().includes(q)),
    )
  }, [tables, filter])

  const toggle = (name) => {
    setExpanded((current) => {
      const next = new Set(current)
      if (next.has(name)) next.delete(name)
      else next.add(name)
      return next
    })
  }

  const handleTableContext = (e, table) => {
    e.preventDefault()
    const cols = (table.columns || []).filter((c) => !c.primary_key).map((c) => c.name)
    const allCols = (table.columns || []).map((c) => c.name)
    setCtxMenu({
      x: e.clientX,
      y: e.clientY,
      items: [
        {
          icon: Database,
          label: `SELECT * FROM ${table.name}`,
          hint: 'Query all',
          action: () => onSetQuery(`SELECT *\nFROM ${table.name}\nLIMIT 100;`),
        },
        {
          icon: Columns3,
          label: 'SELECT specific columns',
          hint: allCols.length + ' cols',
          action: () => onSetQuery(`SELECT ${allCols.join(', ')}\nFROM ${table.name}\nLIMIT 100;`),
        },
        { separator: true },
        {
          icon: Play,
          label: 'COUNT rows',
          action: () => onSetQuery(`SELECT COUNT(*) AS total\nFROM ${table.name};`),
        },
        {
          icon: Activity,
          label: 'DESCRIBE table',
          action: () =>
            onSetQuery(
              `SELECT column_name, data_type, is_nullable, column_default\nFROM information_schema.columns\nWHERE table_name = '${table.name}'\nORDER BY ordinal_position;`,
            ),
        },
        { separator: true },
        {
          icon: Wand2,
          label: 'INSERT template',
          action: () => {
            const placeholders = cols.map(() => "''").join(', ')
            onSetQuery(`INSERT INTO ${table.name} (${cols.join(', ')})\nVALUES (${placeholders});`)
          },
        },
        {
          icon: FileCode2,
          label: 'UPDATE template',
          action: () => {
            const sets = cols.slice(0, 2).map((c) => `${c} = ''`).join(', ')
            onSetQuery(`UPDATE ${table.name}\nSET ${sets}\nWHERE id = 1;`)
          },
        },
        {
          icon: Eraser,
          label: 'DELETE template',
          action: () => onSetQuery(`DELETE FROM ${table.name}\nWHERE id = 1;`),
        },
      ],
    })
  }

  const handleColumnContext = (e, table, column) => {
    e.preventDefault()
    setCtxMenu({
      x: e.clientX,
      y: e.clientY,
      items: [
        {
          icon: Search,
          label: `SELECT ${column.name}`,
          action: () => onSetQuery(`SELECT ${column.name}\nFROM ${table.name}\nLIMIT 100;`),
        },
        {
          icon: Database,
          label: `DISTINCT values`,
          action: () => onSetQuery(`SELECT DISTINCT ${column.name}, COUNT(*) AS cnt\nFROM ${table.name}\nGROUP BY ${column.name}\nORDER BY cnt DESC\nLIMIT 50;`),
        },
        {
          icon: Activity,
          label: `WHERE ${column.name} = ...`,
          action: () => onSetQuery(`SELECT *\nFROM ${table.name}\nWHERE ${column.name} = ''\nLIMIT 100;`),
        },
        { separator: true },
        {
          icon: Columns3,
          label: 'Insert column name',
          action: () => onInsert(column.name),
        },
        {
          icon: Copy,
          label: 'Copy column name',
          action: () => navigator.clipboard?.writeText(column.name),
        },
      ],
    })
  }

  return (
    <aside className="flex w-64 shrink-0 flex-col border-r border-border bg-card/40">
      <div className="border-b border-border px-3 py-2">
        <Overline>Schema Browser</Overline>
        <p className="mt-0.5 font-serif text-[17px] font-medium leading-tight tracking-tight text-foreground">
          {tables.length} tables
        </p>
        <div className="relative mt-2">
          <Search className="pointer-events-none absolute left-2.5 top-1/2 h-2.5 w-2.5 -translate-y-1/2 text-muted-foreground/70" />
          <Input
            value={filter}
            onChange={(e) => onFilter(e.target.value)}
            placeholder="Filter tables, columns…"
            className="h-6 rounded-md border-border bg-background pl-4 text-[12px]"
          />
        </div>
      </div>

      <div className="min-h-0 flex-1 overflow-y-auto px-1 py-1">
        {filtered.length === 0 ? (
          <div className="mt-4 px-3 text-center text-[12px] text-muted-foreground">No matches</div>
        ) : (
          filtered.map((table) => {
            const open = expanded.has(table.name)
            const isActive = table.name === activeTableName
            return (
              <div key={table.name} className="mb-0.5">
                <button
                  type="button"
                  onClick={() => {
                    toggle(table.name)
                    onSelect(table.name)
                  }}
                  onContextMenu={(e) => handleTableContext(e, table)}
                  onDoubleClick={() => onSetQuery(`SELECT *\nFROM ${table.name}\nLIMIT 100;`)}
                  className={`group flex w-full items-center gap-1 rounded-md px-1 py-0.5.5 text-left transition-colors ${
                    isActive
                      ? 'bg-secondary text-foreground'
                      : 'text-muted-foreground hover:bg-muted/50 hover:text-foreground'
                  }`}
                >
                  <ChevronRight
                    className={`h-2 w-2 shrink-0 transition-transform ${open ? 'rotate-90' : ''}`}
                  />
                  <TableIcon className="h-2.5 w-2.5 shrink-0 text-primary/80" />
                  <span className="flex-1 truncate text-[12.5px] font-medium tracking-tight">
                    {table.name}
                  </span>
                  <span className="text-[10px] tabular-nums text-muted-foreground/70">
                    {table.row_count?.toLocaleString?.() ?? table.row_count ?? 0}
                  </span>
                </button>
                {open ? (
                  <div className="ml-3 mt-0.5 border-l border-border pl-1">
                    {(table.columns || []).map((column) => (
                      <button
                        key={column.name}
                        type="button"
                        onClick={() => onInsert(column.name)}
                        onContextMenu={(e) => handleColumnContext(e, table, column)}
                        className="flex w-full items-center gap-1 rounded-md px-1 py-0.5 text-left text-[11.5px] text-muted-foreground transition-colors hover:bg-muted/40 hover:text-foreground"
                        title={`Click to insert · Right-click for options`}
                      >
                        {column.primary_key ? (
                          <KeyRound className="h-2 w-2 shrink-0 text-primary" />
                        ) : (
                          <Columns3 className="h-2 w-2 shrink-0 text-muted-foreground/60" />
                        )}
                        <span className="flex-1 truncate font-mono">{column.name}</span>
                        <span className="text-[9.5px] uppercase tracking-wide text-muted-foreground/60">
                          {column.col_type}
                        </span>
                      </button>
                    ))}
                  </div>
                ) : null}
              </div>
            )
          })
        )}
      </div>

      {ctxMenu && (
        <ContextMenu
          x={ctxMenu.x}
          y={ctxMenu.y}
          items={ctxMenu.items}
          onClose={() => setCtxMenu(null)}
        />
      )}
    </aside>
  )
}
