import React from 'react'
import { Database } from 'lucide-react'
import { EmptyBlock } from './utils'

export function ResultsGrid({ model, stale }) {
  if (!model || model.kind !== 'results') {
    return (
      <EmptyBlock
        icon={Database}
        title="No results yet"
        description="Run a SELECT query to populate the result grid."
      />
    )
  }
  return (
    <div className="flex h-full min-h-0 flex-col">
      <div className="flex shrink-0 items-center justify-between border-b border-border bg-muted/20 px-2 py-0.5.5 text-[11px]">
        <div className="flex items-center gap-1 text-muted-foreground">
          <Database className="h-2 w-2" />
          <span className="tabular-nums">
            {model.rowCount} rows · {model.columns.length} columns
          </span>
          {stale ? (
            <span className="rounded-full border border-border bg-background px-1 py-0.5 text-[9.5px] uppercase tracking-[0.1em] text-muted-foreground">
              stale
            </span>
          ) : null}
        </div>
      </div>
      <div className="min-h-0 flex-1 overflow-auto">
        <table className="min-w-full border-separate border-spacing-0 text-[12px]">
          <thead className="sticky top-0 z-10 bg-card">
            <tr>
              <th className="sticky left-0 z-20 w-10 border-b border-r border-border bg-card px-2 py-1 text-left text-[9.5px] font-medium uppercase tracking-[0.12em] text-muted-foreground">
                #
              </th>
              {model.columns.map((column) => (
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
            {model.rows.map((row, rowIndex) => (
              <tr key={rowIndex} className="group">
                <td className="sticky left-0 z-10 border-b border-r border-border bg-background px-2 py-0.5.5 text-[10.5px] tabular-nums text-muted-foreground group-hover:bg-muted/30">
                  {rowIndex + 1}
                </td>
                {row.map((value, columnIndex) => (
                  <td
                    key={columnIndex}
                    className="border-b border-border bg-background px-2 py-0.5.5 align-top font-mono text-[11.5px] text-foreground group-hover:bg-muted/30"
                  >
                    {value === null ? (
                      <span className="italic text-muted-foreground/60">NULL</span>
                    ) : (
                      String(value)
                    )}
                  </td>
                ))}
              </tr>
            ))}
          </tbody>
        </table>
      </div>
    </div>
  )
}
