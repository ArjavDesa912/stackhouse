import React, { useState } from 'react'
import { History, Sparkles, Copy, Wand2 } from 'lucide-react'
import { Badge } from '@/components/ui/badge'
import { Button } from '@/components/ui/button'
import { formatTime, condenseQuery, Overline } from './utils'

export function InspectorRail({ runState, history, snippets, onUseSnippet, onReuse }) {
  const [tab, setTab] = useState('history')
  const metrics = [
    { label: 'Last run', value: formatTime(runState.lastRunAt) },
    { label: 'Rows', value: runState.lastResults?.rowCount ?? '—' },
    { label: 'Columns', value: runState.lastResults?.columns?.length ?? '—' },
    { label: 'Duration', value: runState.lastDurationMs ? `${runState.lastDurationMs} ms` : '—' },
  ]

  return (
    <aside className="flex w-72 shrink-0 flex-col border-l border-border bg-card/40">
      <div className="border-b border-border px-3 py-2">
        <Overline>Inspector</Overline>
        <div className="mt-2 grid grid-cols-2 gap-1">
          {metrics.map((m) => (
            <div key={m.label} className="rounded-md border border-border bg-background px-1.5 py-0.5.5">
              <p className="text-[9px] font-medium uppercase tracking-[0.12em] text-muted-foreground/70">
                {m.label}
              </p>
              <p className="mt-0.5 font-serif text-[15px] font-medium tabular-nums tracking-tight text-foreground">
                {m.value}
              </p>
            </div>
          ))}
        </div>
      </div>

      <div className="flex shrink-0 items-center gap-0.5 border-b border-border px-1 py-0.5.5">
        {[
          { id: 'history', icon: History, label: 'History' },
          { id: 'snippets', icon: Sparkles, label: 'Snippets' },
        ].map((t) => {
          const Icon = t.icon
          const active = tab === t.id
          return (
            <button
              key={t.id}
              type="button"
              onClick={() => setTab(t.id)}
              className={`flex items-center gap-0.5.5 rounded-full px-2 py-0.5 text-[11.5px] font-medium tracking-tight transition-colors ${
                active
                  ? 'bg-secondary text-foreground'
                  : 'text-muted-foreground hover:bg-muted/50 hover:text-foreground'
              }`}
            >
              <Icon className="h-2 w-2" />
              {t.label}
            </button>
          )
        })}
      </div>

      <div className="min-h-0 flex-1 overflow-y-auto p-2">
        {tab === 'history' ? (
          history.length === 0 ? (
            <div className="mt-5 rounded-md border border-dashed border-border bg-muted/10 p-3 text-center text-[12px] text-muted-foreground">
              Run a query to populate history.
            </div>
          ) : (
            <div className="space-y-1">
              {history.map((entry) => (
                <button
                  key={entry.id}
                  type="button"
                  onClick={() => onReuse(entry.query)}
                  className="group block w-full rounded-md border border-border bg-background p-2 text-left transition-colors hover:border-primary/30 hover:bg-muted/30"
                >
                  <div className="mb-0.5.5 flex items-center justify-between gap-1">
                    <span
                      className={`inline-flex items-center gap-0.5 rounded-full px-0.5.5 py-0.5 text-[9px] font-medium uppercase tracking-[0.1em] ${
                        entry.status === 'error'
                          ? 'bg-destructive/10 text-destructive'
                          : 'bg-success/10 text-success dark:text-success'
                      }`}
                    >
                      {entry.mode === 'explain' ? 'Explain' : 'Run'}
                    </span>
                    <span className="text-[10px] text-muted-foreground/70">
                      {formatTime(entry.timestamp)}
                    </span>
                  </div>
                  <p className="line-clamp-2 font-mono text-[11px] leading-relaxed text-foreground">
                    {condenseQuery(entry.query)}
                  </p>
                </button>
              ))}
            </div>
          )
        ) : (
          <div className="space-y-1">
            {snippets.map((s) => (
              <div
                key={s.id}
                className="rounded-md border border-border bg-background p-2 transition-colors hover:border-primary/30"
              >
                <div className="mb-0.5 flex items-start justify-between gap-1">
                  <div>
                    <p className="text-[12.5px] font-medium tracking-tight text-foreground">{s.label}</p>
                    <p className="mt-0.5 text-[11px] leading-snug text-muted-foreground">
                      {s.description}
                    </p>
                  </div>
                  <Badge variant="outline" className="shrink-0 text-[9px] uppercase tracking-[0.1em]">
                    {s.tag}
                  </Badge>
                </div>
                <pre className="mt-1 overflow-x-auto rounded-sm bg-muted/40 p-1 font-mono text-[10.5px] leading-relaxed text-muted-foreground">
                  {s.query}
                </pre>
                <div className="mt-1 flex items-center gap-0.5.5">
                  <Button size="sm" variant="outline" className="h-6 gap-0.5.5 text-[11px]" onClick={() => onUseSnippet(s.query)}>
                    <Wand2 className="h-2 w-2" />
                    Load
                  </Button>
                  <Button
                    size="sm"
                    variant="ghost"
                    className="h-6 gap-0.5.5 text-[11px]"
                    onClick={() => navigator.clipboard?.writeText(s.query)}
                  >
                    <Copy className="h-2 w-2" />
                    Copy
                  </Button>
                </div>
              </div>
            ))}
          </div>
        )}
      </div>
    </aside>
  )
}
