import React, { useCallback, useEffect, useMemo, useState } from 'react'
import {
  AlertTriangle,
  CheckCircle2,
  Download,
  FileSearch,
  KeyRound,
  Loader2,
  Lock,
  Pencil,
  RefreshCw,
  Rows3,
  Search,
} from 'lucide-react'

import { canEditTableRows } from '@/lib/schemaManager'
import { Button } from '@/components/ui/button'
import { Input } from '@/components/ui/input'
import { cn } from '@/lib/utils'
import { apiGet, apiPost } from '@/lib/apiClient'

function formatCellValue(value) {
  if (value === null || value === undefined) return ''
  if (typeof value === 'object') {
    try {
      return JSON.stringify(value)
    } catch {
      return String(value)
    }
  }
  return String(value)
}

function tryParseJson(value) {
  if (typeof value !== 'string') return value
  const trimmed = value.trim()
  if (!trimmed) return value
  const first = trimmed[0]
  if (first !== '{' && first !== '[') return value
  try {
    return JSON.parse(trimmed)
  } catch {
    return value
  }
}

function Overline({ children, className = '' }) {
  return (
    <p className={`text-[9.5px] font-medium uppercase tracking-[0.14em] text-muted-foreground/80 ${className}`}>
      {children}
    </p>
  )
}

function Banner({ kind, children }) {
  if (!children) return null
  const style =
    kind === 'error'
      ? 'border-destructive/30 bg-destructive/5 text-destructive'
      : 'border-success/30 bg-success/10 text-success dark:text-success'
  const Icon = kind === 'error' ? AlertTriangle : CheckCircle2
  return (
    <div className={cn('flex items-start gap-1 rounded-md border px-2 py-1 text-[12px]', style)}>
      <Icon className="mt-0.5 h-2.5 w-2.5 shrink-0" />
      <span className="leading-snug">{children}</span>
    </div>
  )
}

function downloadCsv(filename, columns, rows) {
  const esc = (v) => {
    if (v === null || v === undefined) return ''
    const s = formatCellValue(v)
    return /[",\n]/.test(s) ? `"${s.replace(/"/g, '""')}"` : s
  }
  const lines = [columns.join(','), ...rows.map((r) => columns.map((c) => esc(r[c])).join(','))]
  const blob = new Blob([lines.join('\n')], { type: 'text/csv;charset=utf-8;' })
  const url = URL.createObjectURL(blob)
  const a = document.createElement('a')
  a.href = url
  a.download = filename
  a.click()
  URL.revokeObjectURL(url)
}

export default function DataExplorerView({ selectedTable }) {
  const [rows, setRows] = useState([])
  const [loading, setLoading] = useState(false)
  const [error, setError] = useState('')
  const [message, setMessage] = useState('')
  const [editMode, setEditMode] = useState(false)
  const [editingCell, setEditingCell] = useState(null)
  const [draftValue, setDraftValue] = useState('')
  const [filter, setFilter] = useState('')

  const editability = useMemo(() => canEditTableRows(selectedTable, rows), [rows, selectedTable])
  const columnNames = useMemo(() => (rows[0] ? Object.keys(rows[0]) : []), [rows])

  const filteredRows = useMemo(() => {
    const q = filter.trim().toLowerCase()
    if (!q) return rows
    return rows.filter((row) =>
      columnNames.some((c) => {
        const value = row[c]
        return value !== null && value !== undefined && formatCellValue(value).toLowerCase().includes(q)
      }),
    )
  }, [rows, filter, columnNames])

  const loadRows = useCallback(async () => {
    if (!selectedTable?.name) {
      setRows([])
      setError('')
      return
    }
    setLoading(true)
    setError('')
    try {
      const response = await apiGet(`/v1/query/${selectedTable.name}?limit=100`)
      const payload = response.data
      if (!response.ok || !payload.success) throw new Error(payload.message || `Failed to load ${selectedTable.name}`)
      setRows(payload.data || [])
    } catch (e) {
      setError(e.message)
      setRows([])
    } finally {
      setLoading(false)
    }
  }, [selectedTable?.name])

  useEffect(() => {
    loadRows()
  }, [loadRows])

  const beginEdit = (row, columnName) => {
    if (!editMode || !editability.editable || columnName === 'id') return
    setEditingCell({ rowId: row.id, columnName })
    setDraftValue(formatCellValue(row[columnName]))
  }

  const commitEdit = async () => {
    if (!editingCell || !selectedTable?.name) return
    const { rowId, columnName } = editingCell
    const nextValue = tryParseJson(draftValue)
    setError('')
    setMessage('')
    try {
      const response = await apiPost(`/v1/update/${selectedTable.name}/${rowId}`, { [columnName]: nextValue })
      const payload = response.data
      if (!response.ok || !payload.success) throw new Error(payload.message || 'Failed to update row.')
      setRows((current) => current.map((r) => (r.id === rowId ? { ...r, [columnName]: nextValue } : r)))
      setMessage(`Updated row ${rowId} · ${columnName}.`)
    } catch (e) {
      setError(e.message)
    } finally {
      setEditingCell(null)
      setDraftValue('')
    }
  }

  const primaryKeyColumn = (selectedTable?.columns || []).find((c) => c.primary_key)?.name

  if (!selectedTable) {
    return (
      <div className="flex h-full items-center justify-center p-4">
        <div className="flex flex-col items-center gap-2 rounded-lg border border-dashed border-border bg-muted/10 px-5 py-5 text-center">
          <FileSearch className="h-5 w-5 text-muted-foreground" />
          <p className="font-serif text-[19px] font-medium tracking-tight text-foreground">
            Pick a table to explore
          </p>
          <p className="max-w-sm text-[12.5px] text-muted-foreground">
            Use the table picker in the header. Explorer reads the first 100 rows and lets you edit cells
            inline when the table has an <span className="font-mono">id</span> column.
          </p>
        </div>
      </div>
    )
  }

  return (
    <div className="flex h-full min-h-0 flex-col bg-background">
      {/* Header */}
      <div className="shrink-0 border-b border-border bg-card/40 px-3 py-3">
        <div className="flex flex-wrap items-start justify-between gap-3">
          <div>
            <Overline>Data Explorer</Overline>
            <h2 className="mt-0.5 flex items-center gap-1 font-serif text-[22px] font-medium leading-tight tracking-tight text-foreground">
              {selectedTable.name}
              {editMode ? (
                <span className="inline-flex items-center gap-0.5 rounded-full bg-primary/10 px-1 py-0.5 text-[10px] font-medium text-primary">
                  <Pencil className="h-2 w-2" />
                  Edit mode
                </span>
              ) : null}
            </h2>
            <p className="mt-0.5 text-[12.5px] text-muted-foreground">
              Read-first preview with scoped inline edits. Double-click a cell to edit.
            </p>
          </div>
          <div className="flex items-center gap-1">
            <Button
              variant="outline"
              size="sm"
              onClick={loadRows}
              className="h-6 gap-0.5.5 rounded-full text-[11.5px]"
            >
              {loading ? <Loader2 className="h-2 w-2 animate-spin" /> : <RefreshCw className="h-2 w-2" />}
              Reload
            </Button>
            <Button
              variant="outline"
              size="sm"
              onClick={() => downloadCsv(`${selectedTable.name}.csv`, columnNames, rows)}
              disabled={rows.length === 0}
              className="h-6 gap-0.5.5 rounded-full text-[11.5px]"
            >
              <Download className="h-2 w-2" />
              Export CSV
            </Button>
            <Button
              size="sm"
              variant={editMode ? 'default' : 'outline'}
              onClick={() => setEditMode((v) => !v)}
              disabled={!editability.editable}
              className="h-6 gap-0.5.5 rounded-full px-3 text-[11.5px]"
            >
              {editMode ? (
                <>
                  <Lock className="h-2 w-2" /> Exit edit
                </>
              ) : (
                <>
                  <Pencil className="h-2 w-2" /> Enter edit
                </>
              )}
            </Button>
          </div>
        </div>

        <div className="mt-3 flex flex-wrap items-center gap-2">
          <div className="relative w-full max-w-sm">
            <Search className="pointer-events-none absolute left-3 top-1/2 h-2.5 w-2.5 -translate-y-1/2 text-muted-foreground" />
            <Input
              value={filter}
              onChange={(e) => setFilter(e.target.value)}
              placeholder="Filter rows…"
              className="h-6 rounded-full border-border bg-muted/30 pl-9 text-[12px]"
            />
          </div>
          <div className="flex items-center gap-1 text-[11px] text-muted-foreground">
            <span className="inline-flex items-center gap-0.5 rounded-full border border-border bg-background px-1.5 py-0.5 tabular-nums">
              <Rows3 className="h-2 w-2" />
              {filteredRows.length} / {rows.length} rows
            </span>
            {primaryKeyColumn ? (
              <span className="inline-flex items-center gap-0.5 rounded-full border border-border bg-background px-1.5 py-0.5">
                <KeyRound className="h-2 w-2 text-primary" />
                <span className="font-mono">{primaryKeyColumn}</span>
              </span>
            ) : null}
            {!editability.editable ? (
              <span className="inline-flex items-center gap-0.5 rounded-full border border-border bg-background px-1.5 py-0.5 text-muted-foreground">
                <Lock className="h-2 w-2" />
                Read-only
              </span>
            ) : null}
          </div>
        </div>

        {error || message ? (
          <div className="mt-2 space-y-1">
            <Banner kind="error">{error}</Banner>
            <Banner kind="success">{message}</Banner>
          </div>
        ) : null}
      </div>

      {/* Grid */}
      <div className="min-h-0 flex-1 overflow-auto">
        {loading ? (
          <div className="flex h-full items-center justify-center text-[12.5px] text-muted-foreground">
            <Loader2 className="mr-1 h-3 w-3 animate-spin" /> Loading rows…
          </div>
        ) : rows.length === 0 ? (
          <div className="flex h-full items-center justify-center p-4">
            <div className="rounded-lg border border-dashed border-border bg-muted/10 px-5 py-5 text-center text-[12.5px] text-muted-foreground">
              No rows returned for this table.
            </div>
          </div>
        ) : (
          <table className="min-w-full border-separate border-spacing-0 text-[12px]">
            <thead className="sticky top-0 z-10 bg-card">
              <tr>
                <th className="sticky left-0 z-20 w-10 border-b border-r border-border bg-card px-2 py-1 text-left text-[9.5px] font-medium uppercase tracking-[0.12em] text-muted-foreground">
                  #
                </th>
                {columnNames.map((name) => (
                  <th
                    key={name}
                    className="border-b border-border bg-card px-2 py-1 text-left font-mono text-[11px] font-medium text-foreground"
                  >
                    <div className="flex items-center gap-0.5.5">
                      {name === primaryKeyColumn ? (
                        <KeyRound className="h-2 w-2 text-primary" />
                      ) : null}
                      {name}
                    </div>
                  </th>
                ))}
              </tr>
            </thead>
            <tbody>
              {filteredRows.map((row, rowIndex) => (
                <tr key={row.id ?? rowIndex} className="group">
                  <td className="sticky left-0 z-10 border-b border-r border-border bg-background px-2 py-0.5.5 text-[10.5px] tabular-nums text-muted-foreground group-hover:bg-muted/30">
                    {rowIndex + 1}
                  </td>
                  {columnNames.map((columnName) => {
                    const isEditing =
                      editingCell?.rowId === row.id && editingCell?.columnName === columnName
                    const editable = editMode && editability.editable && columnName !== 'id'
                    return (
                      <td
                        key={columnName}
                        className={cn(
                          'border-b border-border bg-background px-2 py-0.5.5 align-top font-mono text-[11.5px] text-foreground group-hover:bg-muted/30',
                          editable && 'cursor-text',
                          isEditing && 'ring-2 ring-primary/40',
                        )}
                        onDoubleClick={() => beginEdit(row, columnName)}
                      >
                        {isEditing ? (
                          <input
                            autoFocus
                            value={draftValue}
                            onChange={(e) => setDraftValue(e.target.value)}
                            onBlur={commitEdit}
                            onKeyDown={(e) => {
                              if (e.key === 'Enter') commitEdit()
                              if (e.key === 'Escape') {
                                setEditingCell(null)
                                setDraftValue('')
                              }
                            }}
                            className="w-full rounded-sm border-0 bg-transparent p-0 font-mono text-[11.5px] text-foreground outline-none"
                          />
                        ) : row[columnName] === null ? (
                          <span className="italic text-muted-foreground/60">NULL</span>
                        ) : (
                          formatCellValue(row[columnName])
                        )}
                      </td>
                    )
                  })}
                </tr>
              ))}
            </tbody>
          </table>
        )}
      </div>

      {!editability.editable && rows.length > 0 ? (
        <div className="shrink-0 border-t border-border bg-muted/20 px-3 py-1 text-[11px] text-muted-foreground">
          <Lock className="mr-0.5.5 inline h-2 w-2" />
          {editability.reason}
        </div>
      ) : null}
    </div>
  )
}
