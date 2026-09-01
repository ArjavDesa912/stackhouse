import React from 'react'
import { AlertTriangle, CheckCircle2, Loader2, Lock } from 'lucide-react'

import { cn } from '@/lib/utils'

import { apiFetch as authenticatedApiFetch, API_Base as API_BASE } from '@/lib/apiClient'

export { API_BASE }

export async function apiFetch(path, init = {}) {
  const response = await authenticatedApiFetch(path, init)
  let payload = null
  try {
    payload = await response.json()
  } catch {
    payload = null
  }
  if (response.status === 401 || response.status === 403) {
    const err = new Error(payload?.message || 'Admin authentication required')
    err.kind = 'auth'
    err.status = response.status
    throw err
  }
  if (!response.ok) {
    const err = new Error(payload?.message || `Request failed (${response.status})`)
    err.status = response.status
    throw err
  }
  return payload
}

export function Overline({ children, className = '' }) {
  return (
    <p className={cn('text-[9.5px] font-medium uppercase tracking-[0.14em] text-muted-foreground/80', className)}>
      {children}
    </p>
  )
}

export function Banner({ kind = 'success', children }) {
  if (!children) return null
  const Icon = kind === 'error' ? AlertTriangle : kind === 'auth' ? Lock : CheckCircle2
  const style =
    kind === 'error'
      ? 'border-destructive/30 bg-destructive/5 text-destructive'
      : kind === 'auth'
      ? 'border-border bg-muted/30 text-muted-foreground'
      : 'border-success/30 bg-success/10 text-success dark:text-success'
  return (
    <div className={cn('flex items-start gap-1 rounded-md border px-2 py-1 text-[12px]', style)}>
      <Icon className="mt-0.5 h-2.5 w-2.5 shrink-0" />
      <span className="leading-snug">{children}</span>
    </div>
  )
}

export function Section({ title, description, action, children, className = '' }) {
  return (
    <section className={cn('rounded-md border border-border bg-card', className)}>
      <header className="flex items-start justify-between gap-2 border-b border-border/60 px-3 py-2">
        <div>
          <h3 className="font-serif text-[17px] font-medium leading-tight tracking-tight text-foreground">
            {title}
          </h3>
          {description ? (
            <p className="mt-0.5 text-[12px] leading-relaxed text-muted-foreground">{description}</p>
          ) : null}
        </div>
        {action}
      </header>
      <div className="p-3">{children}</div>
    </section>
  )
}

export function MetricTile({ label, value, suffix, tone = 'default' }) {
  return (
    <div
      className={cn(
        'rounded-md border px-3 py-2',
        tone === 'accent' ? 'border-primary/25 bg-primary/5' : 'border-border bg-background',
      )}
    >
      <Overline>{label}</Overline>
      <p className="mt-0.5 flex items-baseline gap-0.5">
        <span className="font-serif text-[22px] font-medium leading-none tracking-tight text-foreground tabular-nums">
          {value}
        </span>
        {suffix ? <span className="text-[11px] text-muted-foreground">{suffix}</span> : null}
      </p>
    </div>
  )
}

export function Empty({ icon: Icon, title, description, action }) {
  return (
    <div className="flex flex-col items-center justify-center gap-2 rounded-md border border-dashed border-border bg-muted/10 p-5 text-center">
      {Icon ? (
        <div className="flex h-10 w-10 items-center justify-center rounded-full bg-background ring-1 ring-border">
          <Icon className="h-4 w-4 text-muted-foreground" />
        </div>
      ) : null}
      <div>
        <p className="font-serif text-[17px] font-medium tracking-tight text-foreground">{title}</p>
        {description ? (
          <p className="mt-0.5 max-w-md text-[12.5px] text-muted-foreground">{description}</p>
        ) : null}
      </div>
      {action}
    </div>
  )
}

export function InlineSpinner({ className = '' }) {
  return <Loader2 className={cn('h-2.5 w-2.5 animate-spin', className)} />
}

export function formatBytes(bytes) {
  if (bytes === null || bytes === undefined || Number.isNaN(bytes)) return '—'
  const units = ['B', 'KB', 'MB', 'GB', 'TB']
  let value = Number(bytes)
  let unitIndex = 0
  while (value >= 1024 && unitIndex < units.length - 1) {
    value /= 1024
    unitIndex += 1
  }
  return `${value.toFixed(value >= 10 ? 0 : 1)} ${units[unitIndex]}`
}

export function formatRelative(timestamp) {
  if (!timestamp) return '—'
  const date = typeof timestamp === 'number' ? new Date(timestamp * (timestamp < 1e12 ? 1000 : 1)) : new Date(timestamp)
  const seconds = Math.round((Date.now() - date.getTime()) / 1000)
  if (seconds < 60) return `${seconds}s ago`
  if (seconds < 3600) return `${Math.round(seconds / 60)}m ago`
  if (seconds < 86400) return `${Math.round(seconds / 3600)}h ago`
  return `${Math.round(seconds / 86400)}d ago`
}
