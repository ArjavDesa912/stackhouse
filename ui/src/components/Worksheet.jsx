import React, { useEffect, useMemo, useState } from 'react'
import {
  Area,
  AreaChart,
  Bar,
  BarChart,
  CartesianGrid,
  Cell,
  ComposedChart,
  Funnel,
  FunnelChart,
  LabelList,
  Legend,
  Line,
  LineChart,
  Pie,
  PieChart,
  PolarAngleAxis,
  PolarGrid,
  PolarRadiusAxis,
  Radar,
  RadarChart,
  RadialBar,
  RadialBarChart,
  ReferenceLine,
  ResponsiveContainer,
  Scatter,
  ScatterChart,
  Tooltip,
  Treemap,
  XAxis,
  YAxis,
  ZAxis,
} from 'recharts'
import { AlertCircle, BarChart3, Database, Info, LayoutPanelTop, Loader2, Table2 } from 'lucide-react'

import { normalizeField } from '@/lib/worksheet'
import { CHART_CATALOG, CHART_PALETTE, getChartSpec, pickAutoChart } from '@/lib/chartCatalog'
import { ChoroplethMap, PointOrBubbleMap } from '@/components/charts/WorldMap'
import { Badge } from '@/components/ui/badge'
import { Card, CardContent, CardHeader, CardTitle } from '@/components/ui/card'
import { Tabs, TabsContent, TabsList, TabsTrigger } from '@/components/ui/tabs'
import { apiGet } from '@/lib/apiClient'

/* ------------------------------------------------------------------ */
/* Helpers                                                             */
/* ------------------------------------------------------------------ */

const TOOLTIP_STYLE = {
  backgroundColor: 'var(--card)',
  border: '1px solid var(--border)',
  borderRadius: '0.75rem',
  color: 'var(--foreground)',
  fontSize: '12px',
}

function formatNumber(n) {
  if (n === null || n === undefined || Number.isNaN(n)) return '—'
  const abs = Math.abs(n)
  if (abs >= 1_000_000_000) return (n / 1_000_000_000).toFixed(1) + 'B'
  if (abs >= 1_000_000) return (n / 1_000_000).toFixed(1) + 'M'
  if (abs >= 1_000) return (n / 1_000).toFixed(1) + 'K'
  return Number.isInteger(n) ? n.toString() : n.toFixed(2)
}

function collectFields(shelves) {
  const all = [...(shelves.columns || []), ...(shelves.rows || [])].map(normalizeField)
  const dims = all.filter((f) => f.type === 'dimension')
  const measures = all.filter((f) => f.type === 'measure')
  return { dims, measures }
}

/** Aggregate rows into `{ label, [measureName]: sum, ... }[]`. */
function aggregateByDimension(rows, dim, measures) {
  if (!dim || rows.length === 0) return []
  const map = new Map()
  for (const r of rows) {
    const key = r[dim.name]
    const label = key === null || key === undefined ? '∅' : String(key)
    if (!map.has(label)) {
      const base = { label, key: label }
      measures.forEach((m) => (base[m.name] = 0))
      map.set(label, base)
    }
    const bucket = map.get(label)
    measures.forEach((m) => {
      const v = Number(r[m.name])
      bucket[m.name] += Number.isFinite(v) ? v : 0
    })
  }
  return Array.from(map.values())
}

/** Normalize measures per row to 100%. */
function to100Percent(aggregated, measures) {
  return aggregated.map((row) => {
    const total = measures.reduce((sum, m) => sum + (Number(row[m.name]) || 0), 0) || 1
    const out = { label: row.label, key: row.key }
    measures.forEach((m) => {
      out[m.name] = ((Number(row[m.name]) || 0) / total) * 100
    })
    return out
  })
}

/** Bucket a numeric field into `bins` equal-width intervals. */
function buildHistogram(rows, measure, bins = 12) {
  if (!measure) return []
  const values = rows
    .map((r) => Number(r[measure.name]))
    .filter((v) => Number.isFinite(v))
  if (values.length === 0) return []
  const min = Math.min(...values)
  const max = Math.max(...values)
  if (min === max) return [{ label: formatNumber(min), key: String(min), value: values.length }]
  const width = (max - min) / bins
  const buckets = Array.from({ length: bins }, (_, i) => ({
    from: min + i * width,
    to: min + (i + 1) * width,
    value: 0,
  }))
  for (const v of values) {
    const idx = Math.min(bins - 1, Math.floor((v - min) / width))
    buckets[idx].value += 1
  }
  return buckets.map((b, i) => ({
    key: `b${i}`,
    label: `${formatNumber(b.from)}–${formatNumber(b.to)}`,
    value: b.value,
  }))
}

function buildWaterfall(aggregated, measureName) {
  let running = 0
  return aggregated.map((row) => {
    const delta = Number(row[measureName]) || 0
    const start = running
    running += delta
    return {
      label: row.label,
      key: row.key,
      base: Math.min(start, running),
      delta: Math.abs(delta),
      sign: delta >= 0 ? 'up' : 'down',
      total: running,
    }
  })
}

function buildPareto(aggregated, measureName) {
  const sorted = [...aggregated].sort(
    (a, b) => (Number(b[measureName]) || 0) - (Number(a[measureName]) || 0),
  )
  const total = sorted.reduce((s, r) => s + (Number(r[measureName]) || 0), 0) || 1
  let cum = 0
  return sorted.map((r) => {
    cum += Number(r[measureName]) || 0
    return {
      label: r.label,
      key: r.key,
      value: Number(r[measureName]) || 0,
      cumulative: Math.round((cum / total) * 1000) / 10,
    }
  })
}

function buildHeatmap(rows, dimX, dimY, measure) {
  const xs = []
  const ys = []
  const cell = new Map()
  for (const r of rows) {
    const x = String(r[dimX.name] ?? '∅')
    const y = String(r[dimY.name] ?? '∅')
    if (!xs.includes(x)) xs.push(x)
    if (!ys.includes(y)) ys.push(y)
    const k = `${x}__${y}`
    const prev = cell.get(k) || 0
    cell.set(k, prev + (Number(r[measure.name]) || 0))
  }
  let min = Infinity
  let max = -Infinity
  for (const v of cell.values()) {
    if (v < min) min = v
    if (v > max) max = v
  }
  if (!Number.isFinite(min)) min = 0
  if (!Number.isFinite(max)) max = 0
  return { xs, ys, cell, min, max }
}

/* ------------------------------------------------------------------ */
/* Shared bits                                                         */
/* ------------------------------------------------------------------ */

function EmptyState({ icon: Icon, title, description }) {
  return (
    <div className="flex h-full min-h-40 flex-col items-center justify-center gap-2 rounded-md border border-dashed border-border bg-muted/20 px-3 text-center">
      <div className="flex size-12 items-center justify-center rounded-full bg-background ring-1 ring-border">
        <Icon className="size-5 text-muted-foreground" />
      </div>
      <div className="space-y-0.5">
        <p className="font-medium text-foreground">{title}</p>
        <p className="text-sm text-muted-foreground">{description}</p>
      </div>
    </div>
  )
}

function RequirementsBanner({ spec, dimsCount, measuresCount }) {
  const missing = []
  if (dimsCount < spec.dims) missing.push(`${spec.dims - dimsCount} more dimension${spec.dims - dimsCount === 1 ? '' : 's'}`)
  if (measuresCount < spec.measures) missing.push(`${spec.measures - measuresCount} more measure${spec.measures - measuresCount === 1 ? '' : 's'}`)
  if (missing.length === 0) return null
  return (
    <div className="flex items-start gap-1 rounded-md border border-primary/30 bg-primary/10 px-2 py-1 text-[12px] text-primary dark:text-primary">
      <Info className="mt-0.5 h-2.5 w-2.5 shrink-0" />
      <span>
        <span className="font-semibold">{spec.label}</span> needs {missing.join(' and ')}.
      </span>
    </div>
  )
}

function SharedAxes({ xKey = 'label', xLabel, yLabel, horizontal = false }) {
  if (horizontal) {
    return (
      <>
        <CartesianGrid strokeDasharray="3 3" horizontal={false} stroke="var(--border)" />
        <XAxis type="number" tickLine={false} axisLine={false} tick={{ fontSize: 11 }}
               label={yLabel ? { value: yLabel, position: 'insideBottom', offset: -8, fontSize: 11 } : undefined} />
        <YAxis type="category" dataKey={xKey} tickLine={false} axisLine={false} width={110} tick={{ fontSize: 11 }} />
      </>
    )
  }
  return (
    <>
      <CartesianGrid strokeDasharray="3 3" vertical={false} stroke="var(--border)" />
      <XAxis dataKey={xKey} tickLine={false} axisLine={false} tick={{ fontSize: 11 }}
             label={xLabel ? { value: xLabel, position: 'insideBottom', offset: -10, fontSize: 11 } : undefined} />
      <YAxis tickLine={false} axisLine={false} tick={{ fontSize: 11 }} tickFormatter={formatNumber}
             label={yLabel ? { value: yLabel, angle: -90, position: 'insideLeft', fontSize: 11 } : undefined} />
    </>
  )
}

/* ------------------------------------------------------------------ */
/* Individual chart renderers                                          */
/* ------------------------------------------------------------------ */

function BarLikeChart({ aggregated, measures, variant = 'simple', horizontal = false, xLabel, yLabel }) {
  const stackId = variant.startsWith('stacked') || variant === '100' ? 'stack' : undefined
  const data = variant === '100' ? to100Percent(aggregated, measures) : aggregated
  return (
    <ResponsiveContainer width="100%" height="100%">
      <BarChart layout={horizontal ? 'vertical' : 'horizontal'} data={data} margin={{ top: 12, right: 24, bottom: 40, left: horizontal ? 16 : 16 }}>
        <SharedAxes xLabel={xLabel} yLabel={yLabel} horizontal={horizontal} />
        <Tooltip contentStyle={TOOLTIP_STYLE} formatter={(v) => (variant === '100' ? `${Number(v).toFixed(1)}%` : formatNumber(Number(v)))} />
        {measures.length > 1 && <Legend wrapperStyle={{ fontSize: 11 }} />}
        {measures.map((m, i) => (
          <Bar key={m.name} dataKey={m.name} stackId={stackId} fill={CHART_PALETTE[i % CHART_PALETTE.length]} radius={horizontal ? [0, 4, 4, 0] : [4, 4, 0, 0]}>
            {measures.length === 1 && variant === 'simple' && data.map((entry, idx) => (
              <Cell key={`c-${idx}`} fill={CHART_PALETTE[idx % CHART_PALETTE.length]} />
            ))}
          </Bar>
        ))}
      </BarChart>
    </ResponsiveContainer>
  )
}

function LineLikeChart({ aggregated, measures, area = false, stacked = false, stepped = false, percent = false, xLabel, yLabel }) {
  const data = percent ? to100Percent(aggregated, measures) : aggregated
  const Comp = area ? AreaChart : LineChart
  const SeriesComp = area ? Area : Line
  const stackId = stacked || percent ? 'stack' : undefined
  return (
    <ResponsiveContainer width="100%" height="100%">
      <Comp data={data} margin={{ top: 12, right: 24, bottom: 40, left: 16 }}>
        <SharedAxes xLabel={xLabel} yLabel={yLabel} />
        <Tooltip contentStyle={TOOLTIP_STYLE} formatter={(v) => (percent ? `${Number(v).toFixed(1)}%` : formatNumber(Number(v)))} />
        {measures.length > 1 && <Legend wrapperStyle={{ fontSize: 11 }} />}
        {measures.map((m, i) => (
          <SeriesComp
            key={m.name}
            type={stepped ? 'stepAfter' : 'monotone'}
            dataKey={m.name}
            stroke={CHART_PALETTE[i % CHART_PALETTE.length]}
            fill={area ? `${CHART_PALETTE[i % CHART_PALETTE.length]}33` : undefined}
            strokeWidth={2.5}
            stackId={stackId}
            dot={{ r: 3 }}
            activeDot={{ r: 5 }}
          />
        ))}
      </Comp>
    </ResponsiveContainer>
  )
}

function PieLikeChart({ aggregated, measure, donut = false }) {
  const data = aggregated.map((r) => ({ label: r.label, value: Number(r[measure.name]) || 0, key: r.key }))
  return (
    <ResponsiveContainer width="100%" height="100%">
      <PieChart>
        <Pie data={data} dataKey="value" nameKey="label" outerRadius="75%" innerRadius={donut ? '50%' : 0} paddingAngle={donut ? 2 : 0}>
          {data.map((_, i) => <Cell key={i} fill={CHART_PALETTE[i % CHART_PALETTE.length]} />)}
        </Pie>
        <Tooltip contentStyle={TOOLTIP_STYLE} formatter={(v) => formatNumber(Number(v))} />
        <Legend wrapperStyle={{ fontSize: 11 }} />
      </PieChart>
    </ResponsiveContainer>
  )
}

function RadialBarSimple({ aggregated, measure }) {
  const data = aggregated.map((r, i) => ({
    label: r.label,
    value: Number(r[measure.name]) || 0,
    fill: CHART_PALETTE[i % CHART_PALETTE.length],
  }))
  return (
    <ResponsiveContainer width="100%" height="100%">
      <RadialBarChart cx="50%" cy="50%" innerRadius="20%" outerRadius="90%" data={data} startAngle={90} endAngle={-270}>
        <RadialBar dataKey="value" background cornerRadius={8} />
        <Legend iconSize={8} wrapperStyle={{ fontSize: 11 }} />
        <Tooltip contentStyle={TOOLTIP_STYLE} formatter={(v) => formatNumber(Number(v))} />
      </RadialBarChart>
    </ResponsiveContainer>
  )
}

function TreemapChart({ aggregated, measure }) {
  const data = aggregated.map((r, i) => ({
    name: r.label,
    size: Math.max(0, Number(r[measure.name]) || 0),
    fill: CHART_PALETTE[i % CHART_PALETTE.length],
  }))
  return (
    <ResponsiveContainer width="100%" height="100%">
      <Treemap data={data} dataKey="size" nameKey="name" stroke="var(--background)" />
    </ResponsiveContainer>
  )
}

function FunnelShape({ aggregated, measure }) {
  const data = [...aggregated]
    .map((r, i) => ({ name: r.label, value: Number(r[measure.name]) || 0, fill: CHART_PALETTE[i % CHART_PALETTE.length] }))
    .sort((a, b) => b.value - a.value)
  return (
    <ResponsiveContainer width="100%" height="100%">
      <FunnelChart>
        <Tooltip contentStyle={TOOLTIP_STYLE} formatter={(v) => formatNumber(Number(v))} />
        <Funnel dataKey="value" data={data} isAnimationActive>
          <LabelList position="right" dataKey="name" style={{ fontSize: 11, fill: 'var(--foreground)' }} />
        </Funnel>
      </FunnelChart>
    </ResponsiveContainer>
  )
}

function ScatterChartPlot({ rows, x, y, z }) {
  const data = rows
    .map((r) => {
      const xv = Number(r[x.name])
      const yv = Number(r[y.name])
      const zv = z ? Number(r[z.name]) : undefined
      if (!Number.isFinite(xv) || !Number.isFinite(yv)) return null
      return { x: xv, y: yv, z: Number.isFinite(zv) ? zv : 60 }
    })
    .filter(Boolean)
  return (
    <ResponsiveContainer width="100%" height="100%">
      <ScatterChart margin={{ top: 12, right: 24, bottom: 40, left: 16 }}>
        <CartesianGrid strokeDasharray="3 3" stroke="var(--border)" />
        <XAxis type="number" dataKey="x" name={x.name} tickLine={false} axisLine={false} tick={{ fontSize: 11 }} tickFormatter={formatNumber}
               label={{ value: x.name, position: 'insideBottom', offset: -10, fontSize: 11 }} />
        <YAxis type="number" dataKey="y" name={y.name} tickLine={false} axisLine={false} tick={{ fontSize: 11 }} tickFormatter={formatNumber}
               label={{ value: y.name, angle: -90, position: 'insideLeft', fontSize: 11 }} />
        {z && <ZAxis type="number" dataKey="z" range={[40, 600]} name={z.name} />}
        <Tooltip contentStyle={TOOLTIP_STYLE} cursor={{ strokeDasharray: '3 3' }} />
        <Scatter data={data} fill={CHART_PALETTE[0]} fillOpacity={0.7} />
      </ScatterChart>
    </ResponsiveContainer>
  )
}

function RadarPlot({ aggregated, measure }) {
  const data = aggregated.map((r) => ({ label: r.label, value: Number(r[measure.name]) || 0 }))
  return (
    <ResponsiveContainer width="100%" height="100%">
      <RadarChart data={data} outerRadius="75%">
        <PolarGrid stroke="var(--border)" />
        <PolarAngleAxis dataKey="label" tick={{ fontSize: 11 }} />
        <PolarRadiusAxis tick={{ fontSize: 10 }} tickFormatter={formatNumber} />
        <Radar dataKey="value" stroke={CHART_PALETTE[0]} fill={CHART_PALETTE[0]} fillOpacity={0.35} strokeWidth={2} />
        <Tooltip contentStyle={TOOLTIP_STYLE} formatter={(v) => formatNumber(Number(v))} />
      </RadarChart>
    </ResponsiveContainer>
  )
}

function ComposedDualChart({ aggregated, measures }) {
  const [m1, m2] = measures
  return (
    <ResponsiveContainer width="100%" height="100%">
      <ComposedChart data={aggregated} margin={{ top: 12, right: 24, bottom: 40, left: 16 }}>
        <SharedAxes />
        <Tooltip contentStyle={TOOLTIP_STYLE} formatter={(v) => formatNumber(Number(v))} />
        <Legend wrapperStyle={{ fontSize: 11 }} />
        <Bar dataKey={m1.name} fill={CHART_PALETTE[0]} radius={[4, 4, 0, 0]} />
        <Line type="monotone" dataKey={m2.name} stroke={CHART_PALETTE[3]} strokeWidth={2.5} dot={{ r: 3 }} />
      </ComposedChart>
    </ResponsiveContainer>
  )
}

function WaterfallChart({ aggregated, measure }) {
  const data = buildWaterfall(aggregated, measure.name)
  return (
    <ResponsiveContainer width="100%" height="100%">
      <BarChart data={data} margin={{ top: 12, right: 24, bottom: 40, left: 16 }}>
        <SharedAxes />
        <Tooltip contentStyle={TOOLTIP_STYLE} formatter={(v, n) => (n === 'base' ? null : formatNumber(Number(v)))} />
        <Bar dataKey="base" stackId="w" fill="transparent" />
        <Bar dataKey="delta" stackId="w" radius={[4, 4, 0, 0]}>
          {data.map((entry, i) => (
            <Cell key={i} fill={entry.sign === 'up' ? 'var(--success)' : 'var(--destructive)'} />
          ))}
        </Bar>
      </BarChart>
    </ResponsiveContainer>
  )
}

function ParetoChart({ aggregated, measure }) {
  const data = buildPareto(aggregated, measure.name)
  return (
    <ResponsiveContainer width="100%" height="100%">
      <ComposedChart data={data} margin={{ top: 12, right: 32, bottom: 40, left: 16 }}>
        <CartesianGrid strokeDasharray="3 3" vertical={false} stroke="var(--border)" />
        <XAxis dataKey="label" tickLine={false} axisLine={false} tick={{ fontSize: 11 }} />
        <YAxis yAxisId="left" tickLine={false} axisLine={false} tick={{ fontSize: 11 }} tickFormatter={formatNumber} />
        <YAxis yAxisId="right" orientation="right" tickLine={false} axisLine={false} domain={[0, 100]} tick={{ fontSize: 11 }} tickFormatter={(v) => `${v}%`} />
        <Tooltip contentStyle={TOOLTIP_STYLE} formatter={(v, n) => (n === 'cumulative' ? `${v}%` : formatNumber(Number(v)))} />
        <Legend wrapperStyle={{ fontSize: 11 }} />
        <Bar yAxisId="left" dataKey="value" fill={CHART_PALETTE[0]} radius={[4, 4, 0, 0]} />
        <Line yAxisId="right" type="monotone" dataKey="cumulative" stroke={CHART_PALETTE[4]} strokeWidth={2.5} dot={{ r: 3 }} />
        <ReferenceLine yAxisId="right" y={80} stroke="var(--muted-foreground)" strokeDasharray="4 4" />
      </ComposedChart>
    </ResponsiveContainer>
  )
}

function Heatmap({ rows, dimX, dimY, measure }) {
  const { xs, ys, cell, min, max } = buildHeatmap(rows, dimX, dimY, measure)
  const range = max - min || 1
  const color = (v) => {
    const t = (v - min) / range
    // interpolate from warm sand → terracotta
    const hex = (a, b) => Math.round(a + (b - a) * t).toString(16).padStart(2, '0')
    return `#${hex(0xf5, 0xc9)}${hex(0xe9, 0x64)}${hex(0xd9, 0x42)}`
  }
  return (
    <div className="flex h-full w-full flex-col overflow-auto">
      <div className="min-w-max">
        <div className="flex gap-px pl-28">
          {xs.map((x) => (
            <div key={x} className="w-12 truncate text-center text-[10px] font-medium uppercase tracking-wide text-muted-foreground">{x}</div>
          ))}
        </div>
        {ys.map((y) => (
          <div key={y} className="mt-px flex items-stretch gap-px">
            <div className="flex w-20 items-center truncate pr-1 text-right text-[10.5px] font-medium text-foreground/80">{y}</div>
            {xs.map((x) => {
              const v = cell.get(`${x}__${y}`) || 0
              return (
                <div
                  key={`${x}-${y}`}
                  className="flex h-8 w-12 items-center justify-center rounded-[3px] text-[10px] font-semibold text-foreground ring-1 ring-inset ring-black/5 transition-transform hover:scale-[1.05]"
                  style={{ backgroundColor: color(v) }}
                  title={`${y} · ${x}: ${formatNumber(v)}`}
                >
                  {formatNumber(v)}
                </div>
              )
            })}
          </div>
        ))}
      </div>
      <div className="mt-3 flex items-center gap-1 text-[10.5px] text-muted-foreground">
        <span>{formatNumber(min)}</span>
        <div className="h-1 flex-1 rounded-full bg-primary/10" />
        <span>{formatNumber(max)}</span>
      </div>
    </div>
  )
}

function KPICard({ rows, measure }) {
  const total = rows.reduce((s, r) => s + (Number(r[measure.name]) || 0), 0)
  const count = rows.length
  const avg = count ? total / count : 0
  return (
    <div className="flex h-full w-full flex-col justify-center gap-1 rounded-md border border-border bg-card p-5">
      <p className="text-[10px] font-medium uppercase tracking-[0.18em] text-muted-foreground">Total · {measure.name}</p>
      <p className="font-serif text-[72px] font-medium leading-none tracking-tight text-foreground tabular-nums">{formatNumber(total)}</p>
      <div className="mt-2 flex flex-wrap gap-x-3 gap-y-0.5 text-[12px] text-muted-foreground">
        <span><span className="font-medium text-foreground">{formatNumber(count)}</span> rows</span>
        <span>avg <span className="font-medium text-foreground">{formatNumber(avg)}</span></span>
      </div>
    </div>
  )
}

function GaugeChart({ rows, measure }) {
  const total = rows.reduce((s, r) => s + (Number(r[measure.name]) || 0), 0)
  const max = Math.max(total, 1) * 1.25
  const pct = Math.min(100, Math.round((total / max) * 100))
  const data = [{ name: measure.name, value: total, fill: CHART_PALETTE[0] }]
  return (
    <div className="relative h-full w-full">
      <ResponsiveContainer width="100%" height="100%">
        <RadialBarChart cx="50%" cy="65%" innerRadius="60%" outerRadius="95%" barSize={22}
                        data={data} startAngle={180} endAngle={0}>
          <PolarAngleAxis type="number" domain={[0, max]} tick={false} />
          <RadialBar dataKey="value" cornerRadius={12} background={{ fill: 'var(--muted)' }} />
        </RadialBarChart>
      </ResponsiveContainer>
      <div className="pointer-events-none absolute inset-x-0 bottom-[22%] flex flex-col items-center">
        <p className="font-serif text-[42px] font-medium leading-none tracking-tight">{pct}%</p>
        <p className="mt-0.5 text-[10px] font-medium uppercase tracking-[0.16em] text-muted-foreground">{formatNumber(total)} / {formatNumber(max)}</p>
      </div>
    </div>
  )
}

function SparklineGrid({ rows, dim, measure }) {
  // One card per distinct category on dim, with a tiny line of measure values in file order.
  const groups = new Map()
  rows.forEach((r, i) => {
    const k = String(r[dim.name] ?? '∅')
    if (!groups.has(k)) groups.set(k, [])
    groups.get(k).push({ i, v: Number(r[measure.name]) || 0 })
  })
  const entries = Array.from(groups.entries())
  return (
    <div className="grid h-full w-full auto-rows-fr grid-cols-2 gap-2 overflow-auto md:grid-cols-3 lg:grid-cols-4">
      {entries.map(([name, series], idx) => {
        const total = series.reduce((s, p) => s + p.v, 0)
        return (
          <div key={name} className="flex flex-col justify-between rounded-md border border-border bg-card p-2">
            <div>
              <p className="truncate text-[11px] font-medium text-foreground">{name}</p>
              <p className="mt-0.5 font-serif text-[22px] font-medium leading-none tabular-nums">{formatNumber(total)}</p>
            </div>
            <div className="mt-1 h-8">
              <ResponsiveContainer width="100%" height="100%">
                <LineChart data={series}>
                  <Line type="monotone" dataKey="v" stroke={CHART_PALETTE[idx % CHART_PALETTE.length]} strokeWidth={1.8} dot={false} isAnimationActive={false} />
                </LineChart>
              </ResponsiveContainer>
            </div>
          </div>
        )
      })}
    </div>
  )
}

function DataTableView({ columns, rows }) {
  if (columns.length === 0) {
    return <EmptyState icon={Table2} title="No rows" description="Load a table to see the underlying data." />
  }
  return (
    <div className="h-full overflow-auto rounded-md border border-border">
      <table className="min-w-full divide-y divide-border text-left text-[12px]">
        <thead className="sticky top-0 bg-muted/40 backdrop-blur">
          <tr>
            {columns.map((c) => (
              <th key={c} className="px-2 py-1 font-medium text-foreground">{c}</th>
            ))}
          </tr>
        </thead>
        <tbody className="divide-y divide-border bg-background">
          {rows.map((row, ri) => (
            <tr key={ri} className="hover:bg-muted/30">
              {columns.map((c) => (
                <td key={`${ri}-${c}`} className="px-2 py-1 text-muted-foreground">
                  {row[c] === null ? <span className="italic text-muted-foreground/70">NULL</span> : String(row[c])}
                </td>
              ))}
            </tr>
          ))}
        </tbody>
      </table>
    </div>
  )
}

/* ------------------------------------------------------------------ */
/* Chart dispatch                                                      */
/* ------------------------------------------------------------------ */

function ChartCanvas({ chartType, rows, dims, measures }) {
  const spec = getChartSpec(chartType)
  const effectiveType = chartType === 'auto'
    ? pickAutoChart({ dims: dims.length, measures: measures.length })
    : chartType

  // Requirements gating
  if (dims.length < spec.dims || measures.length < spec.measures) {
    return (
      <EmptyState
        icon={BarChart3}
        title={`${spec.label} needs more fields`}
        description={`Assign at least ${spec.dims} dimension${spec.dims === 1 ? '' : 's'} and ${spec.measures} measure${spec.measures === 1 ? '' : 's'} to render this chart.`}
      />
    )
  }

  const primaryDim = dims[0]
  const aggregated = primaryDim ? aggregateByDimension(rows, primaryDim, measures) : []

  switch (effectiveType) {
    case 'bar':           return <BarLikeChart aggregated={aggregated} measures={measures.slice(0, 1)} variant="simple" xLabel={primaryDim?.name} yLabel={measures[0]?.name} />
    case 'barHorizontal': return <BarLikeChart aggregated={aggregated} measures={measures.slice(0, 1)} variant="simple" horizontal xLabel={primaryDim?.name} yLabel={measures[0]?.name} />
    case 'barGrouped':    return <BarLikeChart aggregated={aggregated} measures={measures} variant="simple" xLabel={primaryDim?.name} />
    case 'barStacked':    return <BarLikeChart aggregated={aggregated} measures={measures} variant="stacked" xLabel={primaryDim?.name} />
    case 'bar100':        return <BarLikeChart aggregated={aggregated} measures={measures} variant="100" xLabel={primaryDim?.name} yLabel="%" />

    case 'line':      return <LineLikeChart aggregated={aggregated} measures={measures.slice(0, 1)} xLabel={primaryDim?.name} yLabel={measures[0]?.name} />
    case 'lineMulti': return <LineLikeChart aggregated={aggregated} measures={measures} xLabel={primaryDim?.name} />
    case 'step':      return <LineLikeChart aggregated={aggregated} measures={measures.slice(0, 1)} stepped xLabel={primaryDim?.name} yLabel={measures[0]?.name} />
    case 'area':      return <LineLikeChart aggregated={aggregated} measures={measures.slice(0, 1)} area xLabel={primaryDim?.name} yLabel={measures[0]?.name} />
    case 'areaStacked': return <LineLikeChart aggregated={aggregated} measures={measures} area stacked xLabel={primaryDim?.name} />
    case 'area100':   return <LineLikeChart aggregated={aggregated} measures={measures} area percent xLabel={primaryDim?.name} yLabel="%" />

    case 'pie':       return <PieLikeChart aggregated={aggregated} measure={measures[0]} />
    case 'donut':     return <PieLikeChart aggregated={aggregated} measure={measures[0]} donut />
    case 'radialBar': return <RadialBarSimple aggregated={aggregated} measure={measures[0]} />
    case 'treemap':   return <TreemapChart aggregated={aggregated} measure={measures[0]} />
    case 'funnel':    return <FunnelShape aggregated={aggregated} measure={measures[0]} />

    case 'histogram': return <BarLikeChart aggregated={buildHistogram(rows, measures[0])} measures={[{ name: 'value' }]} variant="simple" xLabel={measures[0]?.name} yLabel="Frequency" />
    case 'scatter':   return <ScatterChartPlot rows={rows} x={measures[0]} y={measures[1]} />
    case 'bubble':    return <ScatterChartPlot rows={rows} x={measures[0]} y={measures[1]} z={measures[2]} />
    case 'radar':     return <RadarPlot aggregated={aggregated} measure={measures[0]} />

    case 'composed':  return <ComposedDualChart aggregated={aggregated} measures={measures.slice(0, 2)} />
    case 'waterfall': return <WaterfallChart aggregated={aggregated} measure={measures[0]} />
    case 'pareto':    return <ParetoChart aggregated={aggregated} measure={measures[0]} />

    case 'heatmap':   return <Heatmap rows={rows} dimX={dims[0]} dimY={dims[1]} measure={measures[0]} />

    case 'mapChoropleth': return <ChoroplethMap aggregated={aggregated} measure={measures[0]} />
    case 'mapBubble':     return <PointOrBubbleMap rows={rows} lat={measures[0]} lng={measures[1]} size={measures[2]} />
    case 'mapPoints':     return <PointOrBubbleMap rows={rows} lat={measures[0]} lng={measures[1]} />

    case 'kpi':       return <KPICard rows={rows} measure={measures[0]} />
    case 'gauge':     return <GaugeChart rows={rows} measure={measures[0]} />
    case 'sparkGrid': return <SparklineGrid rows={rows} dim={dims[0]} measure={measures[0]} />
    case 'table':     return <DataTableView columns={rows[0] ? Object.keys(rows[0]) : []} rows={rows} />

    default:
      return <BarLikeChart aggregated={aggregated} measures={measures.slice(0, 1) || []} variant="simple" xLabel={primaryDim?.name} yLabel={measures[0]?.name} />
  }
}

export default function Worksheet({ datasetId, shelves }) {
  const [data, setData] = useState([])
  const [loading, setLoading] = useState(false)
  const [error, setError] = useState(null)
  const [activePreviewTab, setActivePreviewTab] = useState('chart')

  useEffect(() => {
    if (!datasetId) {
      setData([])
      setError(null)
      return
    }
    let cancelled = false
    const fetchData = async () => {
      setLoading(true)
      setError(null)
      try {
        const response = await apiGet(`/v1/datasets/${datasetId}/preview?limit=500`)
        const payload = response.data
        if (!response.ok || !payload.success) {
          throw new Error(payload.message || `Failed to load dataset`)
        }
        if (!cancelled) setData(payload.data || [])
      } catch (e) {
        if (!cancelled) {
          setError(e.message)
          setData([])
        }
      } finally {
        if (!cancelled) setLoading(false)
      }
    }
    fetchData()
    return () => { cancelled = true }
  }, [datasetId])

  const chartType = shelves.chartType || 'auto'
  const spec = useMemo(() => getChartSpec(chartType), [chartType])
  const SpecIcon = spec.icon
  const { dims, measures } = useMemo(() => collectFields(shelves), [shelves])

  const satisfied = dims.length >= spec.dims && measures.length >= spec.measures
  const isEmptyShelves = dims.length === 0 && measures.length === 0 && chartType === 'auto'

  if (!datasetId) {
    return (
      <div className="flex h-full flex-col p-3">
        <EmptyState
          icon={LayoutPanelTop}
          title="Select a data source"
          description="Choose a table from the field browser to start building a worksheet."
        />
      </div>
    )
  }

  if (loading) {
    return (
      <div className="flex h-full items-center justify-center">
        <div className="flex items-center gap-2 rounded-md border border-border bg-background px-3 py-2 text-sm text-muted-foreground">
          <Loader2 className="size-4 animate-spin" />
          Loading worksheet data...
        </div>
      </div>
    )
  }

  if (error) {
    return (
      <div className="flex h-full flex-col p-3">
        <EmptyState icon={AlertCircle} title="Worksheet data could not be loaded" description={error} />
      </div>
    )
  }

  return (
    <div className="flex h-full flex-col gap-3 p-3">
      {isEmptyShelves ? (
        <div className="flex flex-1 items-center justify-center">
          <div className="max-w-md rounded-md border border-dashed border-border bg-card p-4 text-center shadow-none-whisper">
            <div className="mx-auto mb-3 flex h-10 w-10 items-center justify-center rounded-md bg-primary/10">
              <Database className="h-4 w-4 text-primary" />
            </div>
            <p className="font-serif text-[20px] font-medium leading-tight tracking-tight text-foreground">
              Let's build a chart
            </p>
            <p className="mt-1 text-[12.5px] leading-relaxed text-muted-foreground">
              Drag fields from the browser onto the shelves above, then pick one of {CHART_CATALOG.length - 1} visualizations from the toolbar.
            </p>
          </div>
        </div>
      ) : (
        <Card className="flex min-h-0 flex-1 flex-col py-0">
          <CardHeader className="shrink-0 border-b border-border py-2 px-3">
            <div className="flex items-center justify-between gap-2">
              <div className="min-w-0">
                <p className="text-[9.5px] font-medium uppercase tracking-[0.14em] text-muted-foreground">Preview</p>
                <CardTitle className="mt-0.5 flex items-center gap-1 font-serif text-[16px] font-medium tracking-tight">
                  <SpecIcon className="h-3 w-3 text-primary" />
                  {spec.label}
                </CardTitle>
                <p className="mt-0.5 truncate text-[11.5px] text-muted-foreground">{spec.description}</p>
              </div>
              <Badge variant={satisfied ? 'brand' : 'outline'}>
                {satisfied ? 'Ready' : 'Configure'}
              </Badge>
            </div>
          </CardHeader>

          <CardContent className="flex min-h-0 flex-1 flex-col gap-2 py-3 px-3">
            <RequirementsBanner spec={spec} dimsCount={dims.length} measuresCount={measures.length} />
            <Tabs value={activePreviewTab} onValueChange={setActivePreviewTab} className="flex min-h-0 flex-1 flex-col">
              <TabsList>
                <TabsTrigger value="chart">Chart</TabsTrigger>
                <TabsTrigger value="data">Data</TabsTrigger>
              </TabsList>

              <TabsContent value="chart" className="min-h-0 flex-1">
                <div className="h-full min-h-[360px] rounded-md border border-border bg-background p-3">
                  <ChartCanvas chartType={chartType} rows={data} dims={dims} measures={measures} />
                </div>
              </TabsContent>

              <TabsContent value="data" className="min-h-0 flex-1 overflow-auto">
                <DataTableView columns={data[0] ? Object.keys(data[0]) : []} rows={data} />
              </TabsContent>
            </Tabs>
          </CardContent>
        </Card>
      )}
    </div>
  )
}
