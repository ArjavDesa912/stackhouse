import React, { useMemo, useState } from 'react'
import {
  AlertTriangle,
  CheckCircle2,
  Copy,
  KeyRound,
  Plus,
  ShieldAlert,
  Sparkles,
  Table2,
  Trash2,
  X,
} from 'lucide-react'

import { buildAddColumnSql, buildCreateTableSql, confirmTableDeletion } from '@/lib/schemaManager'
import { Button } from '@/components/ui/button'
import { Input } from '@/components/ui/input'
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from '@/components/ui/dialog'
import { cn } from '@/lib/utils'
import { apiPost, apiDelete } from '@/lib/apiClient'

const COLUMN_TYPES = ['TEXT', 'INTEGER', 'REAL', 'BLOB', 'NUMERIC']
const EMPTY_COLUMN = { name: '', type: 'TEXT', pk: false, notNull: false }

function Overline({ children, className = '' }) {
  return (
    <p className={`text-[9.5px] font-medium uppercase tracking-[0.14em] text-muted-foreground/80 ${className}`}>
      {children}
    </p>
  )
}

function SectionCard({ title, description, action, children, tone = 'default' }) {
  return (
    <section
      className={cn(
        'rounded-lg border bg-card ring-1 ring-border',
        tone === 'danger' ? 'border-destructive/30 bg-destructive/5' : 'border-border',
      )}
    >
      <header className="flex items-start justify-between gap-2 border-b border-border/60 px-3 py-3">
        <div>
          <h3 className="font-serif text-[19px] font-medium leading-tight tracking-tight text-foreground">
            {title}
          </h3>
          {description ? (
            <p className="mt-0.5 text-[12.5px] leading-relaxed text-muted-foreground">{description}</p>
          ) : null}
        </div>
        {action}
      </header>
      <div className="p-3">{children}</div>
    </section>
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

export default function TableDesignerView({ selectedTable, onRefresh, onSelectTable }) {
  const [newTableName, setNewTableName] = useState('')
  const [newColumns, setNewColumns] = useState([
    { name: 'id', type: 'INTEGER', pk: true, notNull: true },
  ])
  const [addColumnName, setAddColumnName] = useState('')
  const [addColumnType, setAddColumnType] = useState('TEXT')
  const [addColumnNotNull, setAddColumnNotNull] = useState(false)
  const [error, setError] = useState('')
  const [message, setMessage] = useState('')
  const [isCreating, setIsCreating] = useState(false)
  const [isAddingColumn, setIsAddingColumn] = useState(false)
  const [isDropDialogOpen, setIsDropDialogOpen] = useState(false)
  const [typedConfirmation, setTypedConfirmation] = useState('')
  const [isDropping, setIsDropping] = useState(false)

  const canDropTable = useMemo(
    () => confirmTableDeletion(selectedTable?.name || '', typedConfirmation),
    [selectedTable?.name, typedConfirmation],
  )

  const previewSql = useMemo(() => {
    if (!newTableName.trim() || newColumns.length === 0) return ''
    try {
      return buildCreateTableSql(newTableName.trim(), newColumns)
    } catch {
      return ''
    }
  }, [newTableName, newColumns])

  const updateColumn = (index, key, value) => {
    setNewColumns((current) =>
      current.map((column, currentIndex) =>
        currentIndex === index ? { ...column, [key]: value } : column,
      ),
    )
  }

  const addColumnRow = () => setNewColumns((current) => [...current, { ...EMPTY_COLUMN }])
  const removeColumnRow = (index) =>
    setNewColumns((current) => current.filter((_, currentIndex) => currentIndex !== index))

  const handleCreateTable = async () => {
    setError('')
    setMessage('')
    if (!newTableName.trim()) return setError('Table name is required.')
    if (newColumns.some((column) => !column.name.trim())) return setError('Every column requires a name.')

    setIsCreating(true)
    try {
      const response = await apiPost('/v1/sql/execute', { query: buildCreateTableSql(newTableName.trim(), newColumns) })
      const payload = response.data
      if (!response.ok || !payload.success) throw new Error(payload.message || 'Failed to create table.')
      await onRefresh()
      onSelectTable(newTableName.trim())
      setMessage(`Created table '${newTableName.trim()}'.`)
      setNewTableName('')
      setNewColumns([{ name: 'id', type: 'INTEGER', pk: true, notNull: true }])
    } catch (e) {
      setError(e.message)
    } finally {
      setIsCreating(false)
    }
  }

  const handleAddColumn = async () => {
    if (!selectedTable) return
    setError('')
    setMessage('')
    if (!addColumnName.trim()) return setError('Column name is required.')
    setIsAddingColumn(true)
    try {
      const response = await apiPost('/v1/sql/execute', {
        query: buildAddColumnSql(selectedTable.name, {
          name: addColumnName.trim().toLowerCase(),
          type: addColumnType,
          notNull: addColumnNotNull,
        }),
      })
      const payload = response.data
      if (!response.ok || !payload.success) throw new Error(payload.message || 'Failed to add column.')
      await onRefresh()
      setMessage(`Added column '${addColumnName.trim().toLowerCase()}' to '${selectedTable.name}'.`)
      setAddColumnName('')
      setAddColumnNotNull(false)
    } catch (e) {
      setError(e.message)
    } finally {
      setIsAddingColumn(false)
    }
  }

  const handleDropTable = async () => {
    if (!selectedTable) return
    setIsDropping(true)
    setError('')
    setMessage('')
    try {
      const response = await apiDelete(`/v1/tables/${selectedTable.name}`)
      const payload = response.data
      if (!response.ok || !payload.success) throw new Error(payload.message || 'Failed to drop table.')
      await onRefresh()
      onSelectTable('')
      setTypedConfirmation('')
      setIsDropDialogOpen(false)
      setMessage(`Dropped table '${selectedTable.name}'.`)
    } catch (e) {
      setError(e.message)
    } finally {
      setIsDropping(false)
    }
  }

  return (
    <div className="h-full overflow-y-auto bg-background">
      <div className="mx-auto max-w-6xl space-y-3 p-3">
        {error || message ? (
          <div className="space-y-1">
            <Banner kind="error">{error}</Banner>
            <Banner kind="success">{message}</Banner>
          </div>
        ) : null}

        {/* Existing table structure */}
        <SectionCard
          title={selectedTable ? `Structure · ${selectedTable.name}` : 'Structure'}
          description="Inspect columns, constraints, and issue safe mutations."
          action={
            selectedTable ? (
              <Button
                size="sm"
                variant="ghost"
                onClick={() => setIsDropDialogOpen(true)}
                className="h-6 gap-0.5.5 rounded-full text-[11.5px] text-destructive hover:bg-destructive/10"
              >
                <Trash2 className="h-2 w-2" />
                Drop table
              </Button>
            ) : null
          }
        >
          {!selectedTable ? (
            <div className="flex flex-col items-center justify-center gap-2 rounded-md border border-dashed border-border bg-muted/10 p-5 text-center">
              <Table2 className="h-5 w-5 text-muted-foreground" />
              <div>
                <p className="font-serif text-[17px] font-medium tracking-tight text-foreground">
                  Pick a table from the header
                </p>
                <p className="mt-0.5 text-[12.5px] text-muted-foreground">
                  Use the table picker above, or create a new one on the right.
                </p>
              </div>
            </div>
          ) : (
            <div className="space-y-3">
              <div className="flex flex-wrap items-center gap-1 text-[11.5px]">
                <span className="inline-flex items-center gap-0.5.5 rounded-full border border-border bg-background px-1.5 py-0.5 tabular-nums text-muted-foreground">
                  {selectedTable.row_count?.toLocaleString?.() ?? 0} rows
                </span>
                <span className="inline-flex items-center gap-0.5.5 rounded-full border border-border bg-background px-1.5 py-0.5 tabular-nums text-muted-foreground">
                  {selectedTable.column_count ?? selectedTable.columns?.length ?? 0} columns
                </span>
              </div>

              <div className="overflow-hidden rounded-md border border-border">
                <table className="min-w-full text-left text-[12.5px]">
                  <thead className="bg-muted/30">
                    <tr className="border-b border-border">
                      <th className="px-3 py-1 text-[10px] font-medium uppercase tracking-[0.12em] text-muted-foreground">
                        Column
                      </th>
                      <th className="px-3 py-1 text-[10px] font-medium uppercase tracking-[0.12em] text-muted-foreground">
                        Type
                      </th>
                      <th className="px-3 py-1 text-[10px] font-medium uppercase tracking-[0.12em] text-muted-foreground">
                        Constraints
                      </th>
                    </tr>
                  </thead>
                  <tbody>
                    {selectedTable.columns.map((column, idx) => (
                      <tr
                        key={column.name}
                        className={cn(
                          idx !== selectedTable.columns.length - 1 && 'border-b border-border/60',
                        )}
                      >
                        <td className="px-3 py-1.5 font-mono text-foreground">
                          <div className="flex items-center gap-1">
                            {column.primary_key ? (
                              <KeyRound className="h-2 w-2 text-primary" />
                            ) : (
                              <span className="inline-block h-2 w-2" />
                            )}
                            {column.name}
                          </div>
                        </td>
                        <td className="px-3 py-1.5 font-mono text-[11px] uppercase text-muted-foreground">
                          {column.col_type}
                        </td>
                        <td className="px-3 py-1.5">
                          <div className="flex flex-wrap gap-0.5.5">
                            {column.primary_key ? (
                              <span className="rounded-full bg-primary/10 px-1 py-0.5 text-[10px] font-medium text-primary">
                                Primary key
                              </span>
                            ) : null}
                            {!column.nullable ? (
                              <span className="rounded-full bg-muted px-1 py-0.5 text-[10px] text-muted-foreground">
                                NOT NULL
                              </span>
                            ) : null}
                            {column.nullable && !column.primary_key ? (
                              <span className="text-[10.5px] text-muted-foreground/70">—</span>
                            ) : null}
                          </div>
                        </td>
                      </tr>
                    ))}
                  </tbody>
                </table>
              </div>

              {/* Inline add-column */}
              <div className="rounded-md border border-border bg-muted/10 p-3">
                <div className="mb-2 flex items-center justify-between gap-2">
                  <div>
                    <Overline>Add column</Overline>
                    <p className="mt-0.5 font-serif text-[15px] font-medium tracking-tight text-foreground">
                      Extend the schema safely
                    </p>
                  </div>
                </div>
                <div className="grid gap-1 md:grid-cols-[1.4fr_1fr_auto_auto]">
                  <Input
                    value={addColumnName}
                    onChange={(e) => setAddColumnName(e.target.value.toLowerCase())}
                    placeholder="column_name"
                    className="h-8 rounded-md font-mono"
                  />
                  <select
                    value={addColumnType}
                    onChange={(e) => setAddColumnType(e.target.value)}
                    className="h-8 rounded-md border border-border bg-background px-2 font-mono text-[12px] text-foreground outline-none focus-visible:ring-2 focus-visible:ring-primary/30"
                  >
                    {COLUMN_TYPES.map((t) => (
                      <option key={t} value={t}>
                        {t}
                      </option>
                    ))}
                  </select>
                  <label className="flex items-center gap-0.5.5 px-1 text-[11.5px] text-muted-foreground">
                    <input
                      type="checkbox"
                      checked={addColumnNotNull}
                      onChange={(e) => setAddColumnNotNull(e.target.checked)}
                      className="accent-primary"
                    />
                    NOT NULL
                  </label>
                  <Button
                    onClick={handleAddColumn}
                    disabled={isAddingColumn}
                    size="sm"
                    className="h-8 gap-0.5.5 rounded-full px-3 text-[12px]"
                  >
                    <Plus className="h-2 w-2" />
                    {isAddingColumn ? 'Adding…' : 'Add'}
                  </Button>
                </div>
              </div>
            </div>
          )}
        </SectionCard>

        {/* Create table */}
        <SectionCard
          title="Create a new table"
          description="Define columns and constraints explicitly. A live DDL preview shows exactly what will run."
        >
          <div className="grid gap-3 lg:grid-cols-[1.2fr_1fr]">
            <div className="space-y-3">
              <div className="space-y-0.5.5">
                <Overline>Table name</Overline>
                <Input
                  value={newTableName}
                  onChange={(e) => setNewTableName(e.target.value.toLowerCase())}
                  placeholder="orders_archive"
                  className="h-8 rounded-md font-mono text-[13px]"
                />
              </div>

              <div className="space-y-1">
                <div className="flex items-center justify-between">
                  <Overline>Columns</Overline>
                  <Button
                    size="sm"
                    variant="ghost"
                    onClick={addColumnRow}
                    className="h-6 gap-0.5.5 text-[11px]"
                  >
                    <Plus className="h-2 w-2" />
                    Add column
                  </Button>
                </div>
                <div className="space-y-1">
                  {newColumns.map((column, index) => (
                    <div
                      key={index}
                      className="rounded-md border border-border bg-background p-2"
                    >
                      <div className="grid gap-1 md:grid-cols-[minmax(0,1.3fr)_minmax(0,1fr)_auto]">
                        <Input
                          value={column.name}
                          onChange={(e) => updateColumn(index, 'name', e.target.value.toLowerCase())}
                          placeholder="column_name"
                          className="h-8 font-mono"
                        />
                        <select
                          value={column.type}
                          onChange={(e) => updateColumn(index, 'type', e.target.value)}
                          className="h-8 rounded-md border border-border bg-background px-2 font-mono text-[12px] text-foreground outline-none focus-visible:ring-2 focus-visible:ring-primary/30"
                        >
                          {COLUMN_TYPES.map((t) => (
                            <option key={t} value={t}>
                              {t}
                            </option>
                          ))}
                        </select>
                        <div className="flex items-center gap-0.5">
                          {newColumns.length > 1 ? (
                            <button
                              type="button"
                              onClick={() => removeColumnRow(index)}
                              className="flex h-8 w-8 items-center justify-center rounded-md text-muted-foreground transition-colors hover:bg-destructive/10 hover:text-destructive"
                              title="Remove"
                            >
                              <X className="h-2.5 w-2.5" />
                            </button>
                          ) : null}
                        </div>
                      </div>
                      <div className="mt-1 flex items-center gap-3 text-[11.5px] text-muted-foreground">
                        <label className="flex items-center gap-0.5.5">
                          <input
                            type="checkbox"
                            checked={column.pk}
                            onChange={(e) => updateColumn(index, 'pk', e.target.checked)}
                            className="accent-primary"
                          />
                          Primary key
                        </label>
                        <label className="flex items-center gap-0.5.5">
                          <input
                            type="checkbox"
                            checked={column.notNull}
                            onChange={(e) => updateColumn(index, 'notNull', e.target.checked)}
                            className="accent-primary"
                          />
                          NOT NULL
                        </label>
                      </div>
                    </div>
                  ))}
                </div>
              </div>

              <Button
                onClick={handleCreateTable}
                disabled={isCreating}
                className="h-8 gap-0.5.5 rounded-full px-3 text-[12px]"
              >
                <Sparkles className="h-2.5 w-2.5" />
                {isCreating ? 'Creating…' : 'Create table'}
              </Button>
            </div>

            <div className="flex min-h-[220px] flex-col rounded-md border border-border bg-foreground p-0 text-foreground ring-1 ring-border">
              <div className="flex items-center justify-between border-b border-border px-3 py-1">
                <span className="text-[9.5px] font-medium uppercase tracking-[0.14em] text-muted-foreground">
                  DDL preview
                </span>
                <button
                  type="button"
                  onClick={() => previewSql && navigator.clipboard?.writeText(previewSql)}
                  disabled={!previewSql}
                  className="flex items-center gap-0.5 rounded-md px-1 py-0.5 text-[10.5px] text-muted-foreground transition-colors hover:bg-foreground hover:text-foreground disabled:opacity-40"
                >
                  <Copy className="h-2 w-2" />
                  Copy
                </button>
              </div>
              <pre className="flex-1 overflow-auto whitespace-pre-wrap p-3 font-mono text-[12px] leading-relaxed text-foreground">
                {previewSql || (
                  <span className="text-muted-foreground">
                    -- Enter a table name and at least one column{'\n'}-- to see the CREATE TABLE statement.
                  </span>
                )}
              </pre>
            </div>
          </div>
        </SectionCard>
      </div>

      <Dialog open={isDropDialogOpen} onOpenChange={setIsDropDialogOpen}>
        <DialogContent>
          <DialogHeader>
            <DialogTitle className="flex items-center gap-1 font-serif">
              <ShieldAlert className="h-4 w-4 text-destructive" />
              Drop table
            </DialogTitle>
            <DialogDescription>
              This permanently removes the table <strong>{selectedTable?.name}</strong> and all its
              rows. Type the table name to confirm.
            </DialogDescription>
          </DialogHeader>
          <div className="space-y-1">
            <Overline>Confirmation</Overline>
            <Input
              value={typedConfirmation}
              onChange={(e) => setTypedConfirmation(e.target.value)}
              placeholder={selectedTable?.name || ''}
              className="font-mono"
            />
          </div>
          <DialogFooter>
            <Button variant="outline" onClick={() => setIsDropDialogOpen(false)} disabled={isDropping}>
              Cancel
            </Button>
            <Button variant="destructive" onClick={handleDropTable} disabled={!canDropTable || isDropping}>
              {isDropping ? 'Dropping…' : 'Drop table'}
            </Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>
    </div>
  )
}
