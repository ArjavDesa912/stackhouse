import React, { useMemo, useRef, useState } from 'react'
import Papa from 'papaparse'
import * as XLSX from 'xlsx'
import {
  AlertTriangle,
  CheckCircle2,
  FileSpreadsheet,
  Loader2,
  RefreshCw,
  Trash2,
  UploadCloud,
} from 'lucide-react'

import { normalizeImportRows } from '@/lib/schemaManager'
import { Button } from '@/components/ui/button'
import { Input } from '@/components/ui/input'
import { cn } from '@/lib/utils'
import { apiPost } from '@/lib/apiClient'

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

function deriveDefaultTableName(fileName) {
  return fileName.replace(/\.[^.]+$/, '').toLowerCase().replace(/[^a-z0-9]+/g, '_')
}

function formatBytes(bytes) {
  if (!bytes) return '—'
  const units = ['B', 'KB', 'MB', 'GB']
  let value = bytes
  let unitIndex = 0
  while (value >= 1024 && unitIndex < units.length - 1) {
    value /= 1024
    unitIndex += 1
  }
  return `${value.toFixed(value > 10 ? 0 : 1)} ${units[unitIndex]}`
}

export default function ImportStudioView({ onRefresh, onSelectTable }) {
  const fileInputRef = useRef(null)
  const [preview, setPreview] = useState(null)
  const [file, setFile] = useState(null)
  const [targetTableName, setTargetTableName] = useState('')
  const [error, setError] = useState('')
  const [message, setMessage] = useState('')
  const [isUploading, setIsUploading] = useState(false)
  const [progress, setProgress] = useState(0)
  const [dragOver, setDragOver] = useState(false)

  const previewRows = useMemo(() => preview?.rows.slice(0, 10) || [], [preview])

  const resetAll = () => {
    setPreview(null)
    setFile(null)
    setTargetTableName('')
    setError('')
    setMessage('')
    setProgress(0)
    if (fileInputRef.current) fileInputRef.current.value = ''
  }

  const processFile = async (nextFile) => {
    setError('')
    setMessage('')
    setPreview(null)
    setProgress(0)
    setFile(nextFile)

    if (!nextFile) return

    try {
      const extension = nextFile.name.split('.').pop().toLowerCase()
      let parsedRows = []
      if (extension === 'csv') {
        parsedRows = await new Promise((resolve, reject) => {
          Papa.parse(nextFile, {
            header: true,
            dynamicTyping: true,
            skipEmptyLines: true,
            complete: (results) => resolve(results.data),
            error: (parseError) => reject(parseError),
          })
        })
      } else if (['xlsx', 'xls'].includes(extension)) {
        const buffer = await nextFile.arrayBuffer()
        const workbook = XLSX.read(buffer)
        const firstSheet = workbook.SheetNames[0]
        parsedRows = XLSX.utils.sheet_to_json(workbook.Sheets[firstSheet], { defval: null })
      } else {
        throw new Error('Unsupported file format. Use CSV or Excel.')
      }

      const normalized = normalizeImportRows(parsedRows)
      setPreview(normalized)
      setTargetTableName(deriveDefaultTableName(nextFile.name))
    } catch (parseError) {
      setError(parseError.message)
    }
  }

  const handleFileChange = (e) => processFile(e.target.files?.[0])

  const handleDrop = (e) => {
    e.preventDefault()
    setDragOver(false)
    processFile(e.dataTransfer.files?.[0])
  }

  const handleImport = async () => {
    if (!preview || !targetTableName.trim()) {
      setError('Choose a file and target table before importing.')
      return
    }
    setIsUploading(true)
    setError('')
    setMessage('')
    setProgress(5)
    try {
      const batchSize = 500
      const totalBatches = Math.max(1, Math.ceil(preview.rows.length / batchSize))
      for (let index = 0; index < totalBatches; index += 1) {
        const batch = preview.rows.slice(index * batchSize, (index + 1) * batchSize)
        const response = await apiPost(`/v1/push/${targetTableName.trim()}/batch`, batch)
        const payload = response.data
        if (!response.ok || !payload.success) throw new Error(payload.message || 'Batch import failed.')
        setProgress(Math.round(((index + 1) / totalBatches) * 100))
      }
      await onRefresh()
      onSelectTable(targetTableName.trim())
      setMessage(`Imported ${preview.rowCount} rows into '${targetTableName.trim()}'.`)
    } catch (uploadError) {
      setError(uploadError.message)
    } finally {
      setIsUploading(false)
    }
  }

  return (
    <div className="h-full overflow-y-auto bg-background">
      <div className="mx-auto max-w-6xl space-y-3 p-3">
        <div>
          <Overline>Import Studio</Overline>
          <h2 className="mt-0.5 font-serif text-[26px] font-medium leading-tight tracking-tight text-foreground">
            Bring external data into your warehouse.
          </h2>
          <p className="mt-0.5 max-w-2xl text-[13px] leading-relaxed text-muted-foreground">
            CSV and Excel files are parsed in the browser, headers are normalized to snake_case, and rows
            are posted in batches of 500 to <span className="font-mono">/v1/push/{'{table}'}/batch</span>.
          </p>
        </div>

        {error || message ? (
          <div className="space-y-1">
            <Banner kind="error">{error}</Banner>
            <Banner kind="success">{message}</Banner>
          </div>
        ) : null}

        <div className="grid gap-3 xl:grid-cols-[1fr_1.3fr]">
          {/* Left: upload + target */}
          <div className="space-y-3">
            <div
              onDragEnter={(e) => {
                e.preventDefault()
                setDragOver(true)
              }}
              onDragOver={(e) => {
                e.preventDefault()
                setDragOver(true)
              }}
              onDragLeave={() => setDragOver(false)}
              onDrop={handleDrop}
              onClick={() => fileInputRef.current?.click()}
              className={cn(
                'relative flex cursor-pointer flex-col items-center justify-center gap-2 rounded-lg border-2 border-dashed p-5 text-center transition-colors',
                dragOver
                  ? 'border-primary bg-primary/5'
                  : 'border-border bg-muted/10 hover:border-primary/40 hover:bg-muted/20',
              )}
            >
              <div className="flex h-10 w-10 items-center justify-center rounded-full bg-primary/10 text-primary ring-1 ring-primary/20">
                <UploadCloud className="h-4 w-4" />
              </div>
              <div>
                <p className="font-serif text-[17px] font-medium tracking-tight text-foreground">
                  Drop a file, or click to browse
                </p>
                <p className="mt-0.5 text-[12px] text-muted-foreground">
                  Supports .csv, .xlsx, .xls · up to ~10MB recommended
                </p>
              </div>
              <input
                ref={fileInputRef}
                type="file"
                accept=".csv,.xlsx,.xls"
                onChange={handleFileChange}
                className="hidden"
              />
            </div>

            {file ? (
              <div className="rounded-md border border-border bg-card p-3">
                <div className="flex items-center gap-2">
                  <div className="flex h-8 w-8 shrink-0 items-center justify-center rounded-md bg-muted text-muted-foreground">
                    <FileSpreadsheet className="h-3 w-3" />
                  </div>
                  <div className="min-w-0 flex-1">
                    <p className="truncate font-medium text-[12.5px] text-foreground">{file.name}</p>
                    <p className="text-[11px] text-muted-foreground">
                      {formatBytes(file.size)} · {preview?.rowCount ?? 0} rows ·{' '}
                      {preview?.columns?.length ?? 0} columns
                    </p>
                  </div>
                  <button
                    type="button"
                    onClick={resetAll}
                    className="rounded-md p-0.5.5 text-muted-foreground transition-colors hover:bg-muted hover:text-foreground"
                    title="Clear"
                  >
                    <Trash2 className="h-2.5 w-2.5" />
                  </button>
                </div>
              </div>
            ) : null}

            <div className="space-y-1 rounded-md border border-border bg-card p-3">
              <Overline>Target table</Overline>
              <Input
                value={targetTableName}
                onChange={(e) => setTargetTableName(e.target.value.toLowerCase())}
                placeholder="import_target"
                className="h-8 rounded-md font-mono"
              />
              <p className="text-[11px] text-muted-foreground">
                If the table does not exist, it will be created automatically based on the first row.
              </p>
            </div>

            {progress > 0 ? (
              <div className="rounded-md border border-border bg-card p-3">
                <div className="mb-1 flex items-center justify-between text-[11px]">
                  <span className="font-medium text-foreground">Uploading</span>
                  <span className="tabular-nums text-muted-foreground">{progress}%</span>
                </div>
                <div className="h-0.5.5 w-full overflow-hidden rounded-full bg-muted">
                  <div
                    className="h-full rounded-full bg-primary transition-all"
                    style={{ width: `${progress}%` }}
                  />
                </div>
              </div>
            ) : null}

            <div className="flex items-center gap-1">
              <Button
                onClick={handleImport}
                disabled={!preview || isUploading}
                className="h-8 gap-0.5.5 rounded-full px-3 text-[12px]"
              >
                {isUploading ? (
                  <Loader2 className="h-2.5 w-2.5 animate-spin" />
                ) : (
                  <UploadCloud className="h-2.5 w-2.5" />
                )}
                {isUploading ? 'Importing…' : 'Run import'}
              </Button>
              <Button
                variant="ghost"
                size="sm"
                onClick={resetAll}
                disabled={!file && !preview}
                className="h-8 gap-0.5.5 text-[11.5px]"
              >
                <RefreshCw className="h-2 w-2" />
                Reset
              </Button>
            </div>
          </div>

          {/* Right: preview */}
          <div className="rounded-lg border border-border bg-card">
            <header className="flex items-center justify-between border-b border-border/60 px-3 py-2">
              <div>
                <Overline>Preview</Overline>
                <p className="mt-0.5 font-serif text-[15px] font-medium tracking-tight text-foreground">
                  First 10 rows
                </p>
              </div>
              {preview ? (
                <div className="flex items-center gap-0.5.5 text-[11px] text-muted-foreground">
                  <span className="inline-flex items-center gap-0.5 rounded-full border border-border bg-background px-1.5 py-0.5 tabular-nums">
                    {preview.rowCount} rows
                  </span>
                  <span className="inline-flex items-center gap-0.5 rounded-full border border-border bg-background px-1.5 py-0.5 tabular-nums">
                    {preview.columns.length} cols
                  </span>
                </div>
              ) : null}
            </header>

            {!preview ? (
              <div className="flex h-40 flex-col items-center justify-center gap-1 p-4 text-center">
                <FileSpreadsheet className="h-5 w-5 text-muted-foreground" />
                <p className="text-[12.5px] text-muted-foreground">
                  Choose a CSV or Excel file to generate an import preview.
                </p>
              </div>
            ) : (
              <div className="max-h-[60vh] overflow-auto">
                <table className="min-w-full border-separate border-spacing-0 text-[12px]">
                  <thead className="sticky top-0 z-10 bg-card">
                    <tr>
                      {preview.columns.map((column) => (
                        <th
                          key={column}
                          className="border-b border-border bg-card px-2 py-1 text-left font-mono text-[11px] font-medium text-foreground"
                        >
                          {column}
                        </th>
                      ))}
                    </tr>
                  </thead>
                  <tbody>
                    {previewRows.map((row, rowIndex) => (
                      <tr key={rowIndex} className="group">
                        {preview.columns.map((column) => (
                          <td
                            key={`${rowIndex}-${column}`}
                            className="border-b border-border bg-background px-2 py-0.5.5 align-top font-mono text-[11.5px] text-foreground group-hover:bg-muted/30"
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
          </div>
        </div>
      </div>
    </div>
  )
}
