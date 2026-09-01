import React from 'react'
import { TerminalSquare, XCircle, CheckCircle2 } from 'lucide-react'
import { EmptyBlock, formatTime } from './utils'

export function MessagesView({ entries }) {
  if (entries.length === 0) {
    return (
      <EmptyBlock
        icon={TerminalSquare}
        title="No messages"
        description="Execution messages and errors will appear here."
      />
    )
  }
  return (
    <div className="space-y-1 p-3">
      {entries.map((entry) => (
        <div
          key={entry.id}
          className={`rounded-md border p-2 ${
            entry.status === 'error'
              ? 'border-destructive/30 bg-destructive/5'
              : 'border-border bg-background'
          }`}
        >
          <div className="mb-0.5 flex items-center justify-between">
            <div className="flex items-center gap-1">
              {entry.status === 'error' ? (
                <XCircle className="h-2.5 w-2.5 text-destructive" />
              ) : (
                <CheckCircle2 className="h-2.5 w-2.5 text-success dark:text-success" />
              )}
              <span className="text-[12px] font-medium text-foreground">{entry.title}</span>
            </div>
            <span className="text-[10px] text-muted-foreground/70">{formatTime(entry.timestamp)}</span>
          </div>
          <p className="font-mono text-[11.5px] leading-relaxed text-muted-foreground">
            {entry.description}
          </p>
        </div>
      ))}
    </div>
  )
}
