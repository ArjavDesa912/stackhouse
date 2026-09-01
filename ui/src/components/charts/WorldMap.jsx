import React, { useEffect, useMemo, useRef, useState } from 'react'
import { geoNaturalEarth1, geoPath, geoGraticule10 } from 'd3-geo'
import { feature } from 'topojson-client'
import { Globe, Loader2 } from 'lucide-react'

/** CDN-hosted TopoJSON world atlas (countries-110m). Small (~100 KB). */
const WORLD_URL = 'https://cdn.jsdelivr.net/npm/world-atlas@2/countries-110m.json'

let cachedAtlas = null
let inflight = null

export function loadWorldAtlas() {
  if (cachedAtlas) return Promise.resolve(cachedAtlas)
  if (inflight) return inflight
  inflight = fetch(WORLD_URL)
    .then((r) => {
      if (!r.ok) throw new Error(`Failed to fetch world atlas (${r.status})`)
      return r.json()
    })
    .then((topo) => {
      const countries = feature(topo, topo.objects.countries)
      cachedAtlas = { countries }
      inflight = null
      return cachedAtlas
    })
    .catch((e) => {
      inflight = null
      throw e
    })
  return inflight
}

/* -------------------------------------------------------------------------- */
/* ISO-3 / name normalization                                                  */
/* -------------------------------------------------------------------------- */

function normalizeKey(v) {
  if (v === null || v === undefined) return ''
  return String(v).trim().toLowerCase()
}

/** Build a lookup keyed by both country name and numeric id. */
function buildKeyLookup(aggregated, measureName) {
  const byKey = new Map()
  for (const row of aggregated) {
    const label = row.label ?? row.key
    byKey.set(normalizeKey(label), Number(row[measureName] ?? row.value) || 0)
  }
  return byKey
}

/* -------------------------------------------------------------------------- */
/* Shared map chrome                                                           */
/* -------------------------------------------------------------------------- */

function MapFrame({ children, width = 960, height = 520 }) {
  return (
    <svg
      viewBox={`0 0 ${width} ${height}`}
      preserveAspectRatio="xMidYMid meet"
      className="h-full w-full"
      role="img"
    >
      {children}
    </svg>
  )
}

function useAtlas() {
  const [atlas, setAtlas] = useState(cachedAtlas)
  const [error, setError] = useState(null)
  useEffect(() => {
    if (cachedAtlas) {
      setAtlas(cachedAtlas)
      return
    }
    let cancelled = false
    loadWorldAtlas()
      .then((a) => !cancelled && setAtlas(a))
      .catch((e) => !cancelled && setError(e.message))
    return () => { cancelled = true }
  }, [])
  return { atlas, error }
}

function MapLoading({ message = 'Loading world atlas...' }) {
  return (
    <div className="flex h-full w-full items-center justify-center">
      <div className="flex items-center gap-1 rounded-md border border-border bg-background px-2 py-1 text-[12px] text-muted-foreground">
        <Loader2 className="h-2.5 w-2.5 animate-spin" /> {message}
      </div>
    </div>
  )
}

function MapError({ message }) {
  return (
    <div className="flex h-full w-full flex-col items-center justify-center gap-1 rounded-md border border-dashed border-border bg-muted/20 px-3 text-center">
      <Globe className="h-4 w-4 text-muted-foreground" />
      <p className="text-[12px] text-muted-foreground">Couldn't load world atlas. {message}</p>
    </div>
  )
}

function formatTipNumber(n) {
  if (!Number.isFinite(n)) return '—'
  const abs = Math.abs(n)
  if (abs >= 1e9) return (n / 1e9).toFixed(1) + 'B'
  if (abs >= 1e6) return (n / 1e6).toFixed(1) + 'M'
  if (abs >= 1e3) return (n / 1e3).toFixed(1) + 'K'
  return Number.isInteger(n) ? String(n) : n.toFixed(2)
}

/* -------------------------------------------------------------------------- */
/* Color scales                                                                */
/* -------------------------------------------------------------------------- */

function interpolateWarm(t) {
  // Ramps toward the app's single primary accent (--flux / --primary, #0FB5A6).
  // Endpoints are numeric mixes of the real token values — CSS var() can't be
  // interpolated numerically in JS, so this mirrors the token instead of the
  // token itself. Keep in sync with --canvas-2 / --flux in styles/tokens.css.
  const clamp = Math.max(0, Math.min(1, t))
  const mix = (a, b) => Math.round(a + (b - a) * clamp)
  const r = mix(0xf4, 0x0f)
  const g = mix(0xf4, 0xb5)
  const b = mix(0xf2, 0xa6)
  return `rgb(${r}, ${g}, ${b})`
}

/* -------------------------------------------------------------------------- */
/* Projection helpers                                                          */
/* -------------------------------------------------------------------------- */

const DEFAULT_W = 960
const DEFAULT_H = 520

function buildProjection(width = DEFAULT_W, height = DEFAULT_H) {
  const projection = geoNaturalEarth1()
  projection.fitExtent(
    [[10, 10], [width - 10, height - 10]],
    { type: 'Sphere' },
  )
  return projection
}

/* -------------------------------------------------------------------------- */
/* Choropleth                                                                  */
/* -------------------------------------------------------------------------- */

export function ChoroplethMap({ aggregated, measure }) {
  const { atlas, error } = useAtlas()
  const [hover, setHover] = useState(null)

  const lookup = useMemo(() => buildKeyLookup(aggregated, measure?.name), [aggregated, measure])

  const { geoPathStr, features, graticule, scale } = useMemo(() => {
    if (!atlas) return { features: [], geoPathStr: null, graticule: null, scale: { min: 0, max: 0 } }
    const projection = buildProjection()
    const path = geoPath(projection)
    const feats = atlas.countries.features.map((f) => {
      const nameKey = normalizeKey(f.properties?.name)
      const idKey = normalizeKey(f.id)
      const v = lookup.get(nameKey) ?? lookup.get(idKey)
      return { ...f, __value: v ?? null, __d: path(f) }
    })
    let min = Infinity, max = -Infinity
    for (const f of feats) if (Number.isFinite(f.__value)) {
      if (f.__value < min) min = f.__value
      if (f.__value > max) max = f.__value
    }
    if (!Number.isFinite(min)) { min = 0; max = 0 }
    return {
      features: feats,
      geoPathStr: path,
      graticule: path(geoGraticule10()),
      scale: { min, max },
    }
  }, [atlas, lookup])

  if (error) return <MapError message={error} />
  if (!atlas) return <MapLoading />

  const range = scale.max - scale.min || 1
  return (
    <div className="relative h-full w-full">
      <MapFrame>
        <rect x={0} y={0} width={DEFAULT_W} height={DEFAULT_H} fill="var(--muted)" opacity="0.25" />
        {graticule && <path d={graticule} fill="none" stroke="var(--border)" strokeWidth={0.4} opacity={0.6} />}
        {features.map((f, i) => {
          const hasValue = Number.isFinite(f.__value)
          const t = hasValue ? (f.__value - scale.min) / range : 0
          const fill = hasValue ? interpolateWarm(t) : 'rgba(135, 134, 127, 0.15)'
          return (
            <path
              key={i}
              d={f.__d}
              fill={fill}
              stroke="var(--background)"
              strokeWidth={0.5}
              onMouseEnter={() => setHover({
                name: f.properties?.name,
                value: hasValue ? f.__value : null,
              })}
              onMouseLeave={() => setHover(null)}
              style={{ cursor: 'pointer', transition: 'fill 150ms' }}
            />
          )
        })}
      </MapFrame>

      {/* Legend */}
      <div className="absolute bottom-3 left-3 flex items-center gap-1 rounded-md border border-border bg-background/90 px-1 py-0.5.5 text-[10.5px] text-muted-foreground shadow-none backdrop-blur">
        <span>{formatTipNumber(scale.min)}</span>
        <div className="h-1 w-16 rounded-full bg-primary/10" />
        <span>{formatTipNumber(scale.max)}</span>
      </div>

      {hover && (
        <div className="pointer-events-none absolute right-3 top-3 rounded-md border border-border bg-card/95 px-2 py-1 text-[11.5px] shadow-none backdrop-blur">
          <p className="font-medium text-foreground">{hover.name || 'Unknown'}</p>
          <p className="text-muted-foreground">
            {measure?.name || 'value'}: <span className="font-medium text-foreground">{hover.value === null ? '—' : formatTipNumber(hover.value)}</span>
          </p>
        </div>
      )}
    </div>
  )
}

/* -------------------------------------------------------------------------- */
/* Bubble / Point Map                                                          */
/* -------------------------------------------------------------------------- */

export function PointOrBubbleMap({ rows, lat, lng, size }) {
  const { atlas, error } = useAtlas()
  const [hover, setHover] = useState(null)

  const { landPath, graticule, points } = useMemo(() => {
    if (!atlas) return { landPath: null, graticule: null, points: [] }
    const projection = buildProjection()
    const path = geoPath(projection)
    const land = atlas.countries.features.map((f) => path(f)).filter(Boolean).join(' ')
    const grat = path(geoGraticule10())

    const raw = rows
      .map((r) => {
        const latVal = Number(r[lat?.name])
        const lngVal = Number(r[lng?.name])
        const s = size ? Number(r[size.name]) : null
        if (!Number.isFinite(latVal) || !Number.isFinite(lngVal)) return null
        const projected = projection([lngVal, latVal])
        if (!projected) return null
        return { x: projected[0], y: projected[1], s: Number.isFinite(s) ? s : null, raw: r }
      })
      .filter(Boolean)

    let minSize = Infinity, maxSize = -Infinity
    for (const p of raw) if (p.s !== null) {
      if (p.s < minSize) minSize = p.s
      if (p.s > maxSize) maxSize = p.s
    }
    const rangeSize = Number.isFinite(maxSize) ? (maxSize - minSize) || 1 : 1

    const pts = raw.map((p) => {
      const radius = size && p.s !== null
        ? 3 + ((p.s - minSize) / rangeSize) * 22
        : 3.5
      return { ...p, r: radius }
    })
    return { landPath: land, graticule: grat, points: pts }
  }, [atlas, rows, lat, lng, size])

  if (error) return <MapError message={error} />
  if (!atlas) return <MapLoading />

  return (
    <div className="relative h-full w-full">
      <MapFrame>
        <rect x={0} y={0} width={DEFAULT_W} height={DEFAULT_H} fill="var(--muted)" opacity="0.25" />
        {graticule && <path d={graticule} fill="none" stroke="var(--border)" strokeWidth={0.4} opacity={0.6} />}
        <path d={landPath} fill="rgba(135, 134, 127, 0.22)" stroke="var(--background)" strokeWidth={0.5} />
        {points.map((p, i) => (
          <circle
            key={i}
            cx={p.x}
            cy={p.y}
            r={p.r}
            fill="var(--primary)"
            fillOpacity={0.55}
            stroke="var(--primary)"
            strokeWidth={0.8}
            onMouseEnter={() => setHover(p)}
            onMouseLeave={() => setHover(null)}
            style={{ cursor: 'pointer', transition: 'r 120ms' }}
          />
        ))}
      </MapFrame>

      <div className="absolute bottom-3 left-3 rounded-md border border-border bg-background/90 px-1 py-0.5.5 text-[10.5px] text-muted-foreground shadow-none backdrop-blur">
        {points.length.toLocaleString()} points · {lat?.name} × {lng?.name}{size ? ` · sized by ${size.name}` : ''}
      </div>

      {hover && (
        <div className="pointer-events-none absolute right-3 top-3 rounded-md border border-border bg-card/95 px-2 py-1 text-[11.5px] shadow-none backdrop-blur">
          <p className="font-medium text-foreground">{lat?.name}: {hover.raw[lat.name]}, {lng?.name}: {hover.raw[lng.name]}</p>
          {size && hover.s !== null && (
            <p className="text-muted-foreground">{size.name}: <span className="font-medium text-foreground">{formatTipNumber(hover.s)}</span></p>
          )}
        </div>
      )}
    </div>
  )
}
