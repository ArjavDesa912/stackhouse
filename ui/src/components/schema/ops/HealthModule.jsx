import React, { useCallback, useEffect, useMemo, useRef, useState } from 'react'
import {
  Activity,
  ChevronDown,
  ChevronUp,
  CircleDot,
  HeartPulse,
  Pause,
  Play,
  RotateCw,
} from 'lucide-react'

import { Banner, MetricTile, Overline, Section, apiFetch } from './common'
import { Button } from '@/components/ui/button'
import { cn } from '@/lib/utils'

const POLL_INTERVAL_MS = 3000
const MAX_SAMPLES = 60

function Sparkline({ samples, width = 420, height = 80 }) {
  if (samples.length < 2) {
    return (
      <div
        style={{ height }}
        className="flex items-center justify-center rounded-md border border-dashed border-border bg-muted/10 text-[11px] text-muted-foreground"
      >
        Collecting samples…
      </div>
    )
  }
  const values = samples.map((s) => s.latency)
  const max = Math.max(...values, 1)
  const step = width / (samples.length - 1)
  const points = values.map((value, index) => {
    const x = index * step
    const y = height - (value / max) * (height - 12) - 6
    return `${x.toFixed(1)},${y.toFixed(1)}`
  })
  const areaPath = `M0,${height} L${points.join(' L')} L${width},${height} Z`
  const linePath = `M${points.join(' L')}`
  return (
    <svg viewBox={`0 0 ${width} ${height}`} width="100%" height={height} className="overflow-visible">
      <path d={areaPath} fill="var(--flux)" fillOpacity="0.12" />
      <path d={linePath} fill="none" stroke="var(--flux)" strokeWidth="1.5" strokeLinejoin="round" />
      {points.map((p, i) => {
        if (i !== points.length - 1) return null
        const [x, y] = p.split(',').map(Number)
        return <circle key={i} cx={x} cy={y} r={3} fill="var(--flux)" />
      })}
    </svg>
  )
}

export default function HealthModule({ tables = [] }) {
  const [samples, setSamples] = useState([])
  const [status, setStatus] = useState('idle') // idle | ok | down
  const [paused, setPaused] = useState(false)
  const [error, setError] = useState('')
  const [rawOpen, setRawOpen] = useState(false)
  const [rawHealth, setRawHealth] = useState(null)
  const timerRef = useRef(null)

  const totalRows = useMemo(() => tables.reduce((sum, t) => sum + (t.row_count || 0), 0), [tables])
  const totalColumns = useMemo(
    () => tables.reduce((sum, t) => sum + (t.columns?.length || 0), 0),
    [tables],
  )

  const probe = useCallback(async () => {
    const startedAt = performance.now()
    try {
      const payload = await apiFetch('/health')
      const elapsed = performance.now() - startedAt
      setRawHealth(payload)
      setStatus('ok')
      setError('')
      setSamples((current) =>
        [...current, { t: Date.now(), latency: Math.max(1, Math.round(elapsed)) }].slice(-MAX_SAMPLES),
      )
    } catch (e) {
      setStatus('down')
      setError(e.message)
    }
  }, [])

  useEffect(() => {
    probe()
  }, [probe])

  useEffect(() => {
    if (paused) return undefined
    timerRef.current = setInterval(probe, POLL_INTERVAL_MS)
    return () => clearInterval(timerRef.current)
  }, [paused, probe])

  const latest = samples[samples.length - 1]?.latency
  const average = samples.length
    ? Math.round(samples.reduce((sum, s) => sum + s.latency, 0) / samples.length)
    : null
  const peak = samples.length ? Math.max(...samples.map((s) => s.latency)) : null

  return (
    <div className="h-full overflow-y-auto">
      <div className="space-y-3 p-3">
        {error && status === 'down' ? (
          <Banner kind="error">Health probe failed: {error}</Banner>
        ) : null}

        {/* Top bar: live status + controls */}
        <div className="flex flex-wrap items-center justify-between gap-2">
          <div className="flex items-center gap-2">
            <div className="flex h-8 w-8 items-center justify-center rounded-md bg-primary/10 text-primary ring-1 ring-primary/20">
              <HeartPulse className="h-3 w-3" />
            </div>
            <div>
              <Overline>Service health</Overline>
              <p className="flex items-center gap-1 font-serif text-[20px] font-medium leading-tight tracking-tight text-foreground">
                {status === 'ok' ? (
                  <>
                    <span className="inline-flex items-center gap-0.5.5 text-success dark:text-success">
                      <CircleDot className="h-2 w-2" />
                      Online
                    </span>
                  </>
                ) : status === 'down' ? (
                  <span className="inline-flex items-center gap-0.5.5 text-destructive">
                    <CircleDot className="h-2 w-2" />
                    Unreachable
                  </span>
                ) : (
                  <span className="text-muted-foreground">Probing…</span>
                )}
              </p>
            </div>
          </div>
          <div className="flex items-center gap-1">
            <Button
              variant="outline"
              size="sm"
              onClick={() => setPaused((v) => !v)}
              className="h-6 gap-0.5.5 rounded-full text-[11.5px]"
            >
              {paused ? (
                <>
                  <Play className="h-2 w-2" /> Resume
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
              onClick={probe}
              className="h-6 gap-0.5.5 rounded-full text-[11.5px]"
            >
              <RotateCw className="h-2 w-2" />
              Probe now
            </Button>
          </div>
        </div>

        {/* Metric tiles */}
        <div className="grid gap-2 sm:grid-cols-2 lg:grid-cols-4">
          <MetricTile
            label="Latest RTT"
            value={latest ?? '—'}
            suffix={latest ? 'ms' : undefined}
            tone="accent"
          />
          <MetricTile label="Avg RTT" value={average ?? '—'} suffix={average ? 'ms' : undefined} />
          <MetricTile label="Peak RTT" value={peak ?? '—'} suffix={peak ? 'ms' : undefined} />
          <MetricTile label="Samples" value={samples.length} suffix={`/ ${MAX_SAMPLES}`} />
        </div>

        {/* Chart */}
        <Section
          title="Round-trip latency"
          description={`Polling /health every ${POLL_INTERVAL_MS / 1000}s. Measured client-side.`}
          action={
            <span className="inline-flex items-center gap-0.5.5 rounded-full border border-border bg-background px-1.5 py-0.5 text-[10.5px] text-muted-foreground">
              <Activity className="h-2 w-2" />
              Client-side probe
            </span>
          }
        >
          <Sparkline samples={samples} />
          <div className="mt-1 flex items-center justify-between text-[10.5px] text-muted-foreground">
            <span>
              {samples[0]
                ? `Since ${new Date(samples[0].t).toLocaleTimeString()}`
                : 'Waiting for samples…'}
            </span>
            <span className="tabular-nums">
              {samples.length > 0
                ? `${samples.length} pts · ${paused ? 'paused' : 'live'}`
                : 'idle'}
            </span>
          </div>
        </Section>

        {/* Warehouse summary */}
        <div className="grid gap-2 md:grid-cols-3">
          <MetricTile label="Tables" value={tables.length} />
          <MetricTile label="Columns" value={totalColumns} />
          <MetricTile label="Total rows" value={totalRows.toLocaleString()} />
        </div>

        {/* Raw /health payload */}
        <Section
          title="Raw /health payload"
          description="Verbatim JSON returned by the server."
          action={
            <Button
              size="sm"
              variant="ghost"
              onClick={() => setRawOpen((v) => !v)}
              className="h-6 gap-0.5.5 text-[11px]"
            >
              {rawOpen ? <ChevronUp className="h-2 w-2" /> : <ChevronDown className="h-2 w-2" />}
              {rawOpen ? 'Hide' : 'Show'}
            </Button>
          }
        >
          {rawOpen ? (
            <pre
              className={cn(
                'max-h-40 overflow-auto rounded-md border border-border bg-foreground p-3 font-mono text-[11px] leading-relaxed text-foreground',
              )}
            >
              {rawHealth ? JSON.stringify(rawHealth, null, 2) : '// no payload yet'}
            </pre>
          ) : (
            <p className="text-[11.5px] text-muted-foreground">
              Payload hidden — expand to inspect.
            </p>
          )}
        </Section>
      </div>
    </div>
  )
}
