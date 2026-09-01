import React, { useCallback, useEffect, useRef, useState } from 'react'
import { Download, Filter, Pause, Play, RotateCw, ScrollText, Terminal } from 'lucide-react'

import { Banner, Empty, InlineSpinner, Overline, Section, apiFetch } from './common'
import { Button } from '@/components/ui/button'
import { Input } from '@/components/ui/input'
import { cn } from '@/lib/utils'

const LEVELS = ['all', 'error', 'warn', 'info', 'debug', 'trace']
const POLL_MS = 5000

const LEVEL_STYLES = {
  error: 'text-destructive',
  warn: 'text-primary',
  info: 'text-foreground',
  debug: 'text-muted-foreground',
  trace: 'text-muted-foreground/70',
}

function levelOf(entry) {
  return String(entry.level ?? entry.LEVEL ?? 'info').toLowerCase()
}

function fmtTime(ts) {
  if (!ts) return '—'
  const millis = ts < 1e12 ? ts * 1000 : ts
  const date = new Date(millis)
  if (Number.isNaN(date.getTime())) return '—'
  return date.toLocaleTimeString(undefined, {
    hour: '2-digit',
    minute: '2-digit',
    second: '2-digit',
  })
}

export default function LogsModule() {
  const [level, setLevel] = useState('all')
  const [service, setService] = useState('')
  const [limit, setLimit] = useState(100)
  const [entries, setEntries] = useState([])
  const [loading, setLoading] = useState(false)
  const [error, setError] = useState('')
  const [authError, setAuthError] = useState(false)
  const [paused, setPaused] = useState(true)
  const [lastRefresh, setLastRefresh] = useState(null)
  const timerRef = useRef(null)
  const scrollRef = useRef(null)

  const load = useCallback(async () => {
    setLoading(true)
    setError('')
    try {
      const params = new URLSearchParams()
      if (level !== 'all') params.set('level', level)
      if (service.trim()) params.set('service', service.trim())
      params.set('limit', String(limit))
      const payload = await apiFetch(`/v1/admin/logs?${params.toString()}`)
      setEntries(Array.isArray(payload?.data) ? payload.data : [])
      setAuthError(false)
      setLastRefresh(new Date())
    } catch (e) {
      if (e.kind === 'auth') setAuthError(true)
      else setError(e.message)
    } finally {
      setLoading(false)
    }
  }, [level, service, limit])

  useEffect(() => {
    load()
  }, [load])

  useEffect(() => {
    if (paused || authError) return undefined
    timerRef.current = setInterval(load, POLL_MS)
    return () => clearInterval(timerRef.current)
  }, [paused, authError, load])

  useEffect(() => {
    if (scrollRef.current) {
      scrollRef.current.scrollTop = scrollRef.current.scrollHeight
    }
  }, [entries])

  const handleExport = () => {
    const blob = new Blob([JSON.stringify(entries, null, 2)], { type: 'application/json' })
    const url = URL.createObjectURL(blob)
    const a = document.createElement('a')
    a.href = url
    a.download = `stackhouse-logs-${Date.now()}.json`
    a.click()
    URL.revokeObjectURL(url)
  }

  return (
    <div className="flex h-full flex-col">
      <div className="shrink-0 space-y-2 border-b border-border bg-card/40 px-3 py-3">
        <div className="flex flex-wrap items-center justify-between gap-2">
          <div className="flex items-center gap-2">
            <div className="flex h-8 w-8 items-center justify-center rounded-md bg-primary/10 text-primary ring-1 ring-primary/20">
              <ScrollText className="h-3 w-3" />
            </div>
            <div>
              <Overline>Log drain</Overline>
              <p className="font-serif text-[20px] font-medium leading-tight tracking-tight text-foreground">
                Platform logs
              </p>
            </div>
          </div>
          <div className="flex items-center gap-1">
            <Button
              size="sm"
              variant="outline"
              onClick={() => setPaused((v) => !v)}
              disabled={authError}
              className="h-6 gap-0.5.5 rounded-full text-[11.5px]"
            >
              {paused ? (
                <>
                  <Play className="h-2 w-2" /> Live tail
                </>
              ) : (
                <>
                  <Pause className="h-2 w-2" /> Pause
                </>
              )}
            </Button>
            <Button
              size="sm"
              variant="outline"
              onClick={load}
              disabled={loading}
              className="h-6 gap-0.5.5 rounded-full text-[11.5px]"
            >
              {loading ? <InlineSpinner /> : <RotateCw className="h-2 w-2" />}
              Refresh
            </Button>
            <Button
              size="sm"
              variant="ghost"
              onClick={handleExport}
              disabled={entries.length === 0}
              className="h-6 gap-0.5.5 text-[11.5px]"
            >
              <Download className="h-2 w-2" />
              Export
            </Button>
          </div>
        </div>

        <div className="flex flex-wrap items-center gap-1">
          <Filter className="h-2.5 w-2.5 text-muted-foreground" />
          <div className="flex items-center gap-0.5 rounded-full border border-border bg-background p-0.5">
            {LEVELS.map((l) => (
              <button
                key={l}
                type="button"
                onClick={() => setLevel(l)}
                className={cn(
                  'rounded-full px-1.5 py-0.5 text-[10.5px] font-medium uppercase tracking-[0.12em] transition-colors',
                  level === l
                    ? 'bg-secondary text-foreground ring-1 ring-border dark:ring-1 ring-border'
                    : 'text-muted-foreground hover:bg-muted/50 hover:text-foreground',
                )}
              >
                {l}
              </button>
            ))}
          </div>
          <Input
            value={service}
            onChange={(e) => setService(e.target.value)}
            placeholder="Service filter…"
            className="h-6 w-32 rounded-full border-border bg-muted/20 text-[12px]"
          />
          <select
            value={limit}
            onChange={(e) => setLimit(Number(e.target.value))}
            className="h-6 rounded-full border border-border bg-background px-2 text-[11.5px] text-foreground outline-none focus-visible:ring-2 focus-visible:ring-primary/30"
          >
            {[50, 100, 250, 500].map((n) => (
              <option key={n} value={n}>
                {n} lines
              </option>
            ))}
          </select>
          <span className="ml-auto text-[10.5px] text-muted-foreground">
            {lastRefresh ? `Refreshed ${lastRefresh.toLocaleTimeString()}` : 'Not yet loaded'}
          </span>
        </div>
      </div>

      <div className="min-h-0 flex-1 overflow-hidden p-3">
        {authError ? (
          <Section title="Admin authentication required" description="/v1/admin/logs is gated by service-admin.">
            <Banner kind="auth">
              Sign in as a service admin and include your bearer token in the request headers to view logs.
            </Banner>
          </Section>
        ) : error ? (
          <Banner kind="error">{error}</Banner>
        ) : entries.length === 0 ? (
          <Empty
            icon={Terminal}
            title="No log entries"
            description="Either no logs match these filters, or the drain store is empty."
          />
        ) : (
          <div className="flex h-full flex-col overflow-hidden rounded-md border border-border bg-foreground text-foreground ring-1 ring-border">
            <div className="flex items-center justify-between border-b border-border px-3 py-1 text-[10.5px] text-muted-foreground">
              <span className="font-mono uppercase tracking-[0.14em]">
                /v1/admin/logs · {entries.length} entries
              </span>
              <span>{paused ? 'paused' : `tail every ${POLL_MS / 1000}s`}</span>
            </div>
            <div ref={scrollRef} className="min-h-0 flex-1 overflow-auto font-mono text-[11.5px] leading-relaxed">
              {entries.map((entry, index) => {
                const lvl = levelOf(entry)
                return (
                  <div
                    key={index}
                    className={cn(
                      'flex items-start gap-2 border-b border-border/50 px-3 py-0.5.5 hover:bg-foreground',
                    )}
                  >
                    <span className="shrink-0 text-[10.5px] text-muted-foreground tabular-nums">
                      {fmtTime(entry.timestamp)}
                    </span>
                    <span
                      className={cn(
                        'w-12 shrink-0 text-[10px] font-semibold uppercase tracking-[0.12em]',
                        LEVEL_STYLES[lvl] || 'text-muted-foreground',
                      )}
                    >
                      {lvl}
                    </span>
                    {entry.service ? (
                      <span className="shrink-0 text-[10.5px] text-success">{entry.service}</span>
                    ) : null}
                    <span className="min-w-0 flex-1 break-words text-foreground">
                      {entry.message}
                    </span>
                  </div>
                )
              })}
            </div>
          </div>
        )}
      </div>
    </div>
  )
}
