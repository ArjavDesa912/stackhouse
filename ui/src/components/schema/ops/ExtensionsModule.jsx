import React, { useCallback, useEffect, useMemo, useState } from 'react'
import { CheckCircle2, Puzzle, RotateCw, Search, Trash2 } from 'lucide-react'

import {
  Banner,
  Empty,
  InlineSpinner,
  Overline,
  Section,
  apiFetch,
} from './common'
import { Button } from '@/components/ui/button'
import { Input } from '@/components/ui/input'
import { cn } from '@/lib/utils'

function extName(ext) {
  return ext?.extname || ext?.name || ''
}
function extVersion(ext) {
  return ext?.extversion || ext?.version || ''
}

export default function ExtensionsModule() {
  const [installed, setInstalled] = useState([])
  const [available, setAvailable] = useState([])
  const [loading, setLoading] = useState(false)
  const [error, setError] = useState('')
  const [message, setMessage] = useState('')
  const [authError, setAuthError] = useState(false)
  const [search, setSearch] = useState('')
  const [busyName, setBusyName] = useState(null)

  const load = useCallback(async () => {
    setLoading(true)
    setError('')
    try {
      const [installedPayload, availablePayload] = await Promise.all([
        apiFetch('/v1/admin/extensions'),
        apiFetch('/v1/admin/extensions/available'),
      ])
      setInstalled(Array.isArray(installedPayload?.data) ? installedPayload.data : [])
      setAvailable(Array.isArray(availablePayload?.data) ? availablePayload.data : [])
      setAuthError(false)
    } catch (e) {
      if (e.kind === 'auth') setAuthError(true)
      else setError(e.message)
    } finally {
      setLoading(false)
    }
  }, [])

  useEffect(() => {
    load()
  }, [load])

  const installedSet = useMemo(
    () => new Set(installed.map((e) => extName(e).toLowerCase())),
    [installed],
  )

  const filteredAvailable = useMemo(() => {
    const q = search.trim().toLowerCase()
    if (!q) return available
    return available.filter((e) => extName(e).toLowerCase().includes(q))
  }, [available, search])

  const action = async (name, kind) => {
    setBusyName(name)
    setError('')
    setMessage('')
    try {
      await apiFetch(`/v1/admin/extensions/${kind === 'install' ? 'install' : 'uninstall'}`, {
        method: 'POST',
        body: JSON.stringify({ name }),
      })
      setMessage(`Extension '${name}' ${kind === 'install' ? 'installed' : 'uninstalled'}.`)
      await load()
    } catch (e) {
      setError(e.message)
    } finally {
      setBusyName(null)
    }
  }

  return (
    <div className="h-full overflow-y-auto">
      <div className="space-y-3 p-3">
        <div className="flex flex-wrap items-center justify-between gap-2">
          <div className="flex items-center gap-2">
            <div className="flex h-8 w-8 items-center justify-center rounded-md bg-primary/10 text-primary ring-1 ring-primary/20">
              <Puzzle className="h-3 w-3" />
            </div>
            <div>
              <Overline>Extensions</Overline>
              <p className="font-serif text-[20px] font-medium leading-tight tracking-tight text-foreground">
                PostgreSQL extensions
              </p>
            </div>
          </div>
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
        </div>

        {authError ? (
          <Banner kind="auth">/v1/admin/extensions requires service-admin authentication.</Banner>
        ) : null}
        {error ? <Banner kind="error">{error}</Banner> : null}
        {message ? <Banner kind="success">{message}</Banner> : null}

        <Section
          title={`Installed · ${installed.length}`}
          description="Extensions currently available on the running PostgreSQL instance."
        >
          {installed.length === 0 && !authError ? (
            <Empty icon={Puzzle} title="No extensions installed" description="Pick one from the catalog below to install." />
          ) : (
            <div className="grid gap-1 md:grid-cols-2">
              {installed.map((ext) => (
                <div
                  key={extName(ext)}
                  className="flex items-center justify-between gap-2 rounded-md border border-border bg-background p-2"
                >
                  <div className="flex min-w-0 items-center gap-2">
                    <CheckCircle2 className="h-3 w-3 shrink-0 text-success dark:text-success" />
                    <div className="min-w-0">
                      <p className="truncate font-mono text-[12.5px] font-medium text-foreground">
                        {extName(ext)}
                      </p>
                      <p className="text-[10.5px] text-muted-foreground">
                        v{extVersion(ext) || '—'}
                        {ext?.extrelocatable !== undefined
                          ? ` · ${ext.extrelocatable ? 'relocatable' : 'fixed'}`
                          : ''}
                      </p>
                    </div>
                  </div>
                  <Button
                    size="sm"
                    variant="ghost"
                    onClick={() => action(extName(ext), 'uninstall')}
                    disabled={busyName === extName(ext)}
                    className="h-6 gap-0.5.5 rounded-md text-[11px] text-destructive hover:bg-destructive/10"
                  >
                    <Trash2 className="h-2 w-2" />
                    Remove
                  </Button>
                </div>
              ))}
            </div>
          )}
        </Section>

        <Section
          title={`Catalog · ${filteredAvailable.length}`}
          description="Extensions known to the server. Install to enable."
          action={
            <div className="relative w-40">
              <Search className="pointer-events-none absolute left-2.5 top-1/2 h-2.5 w-2.5 -translate-y-1/2 text-muted-foreground" />
              <Input
                value={search}
                onChange={(e) => setSearch(e.target.value)}
                placeholder="Search…"
                className="h-6 rounded-full border-border bg-background pl-4 text-[12px]"
              />
            </div>
          }
        >
          {filteredAvailable.length === 0 ? (
            <p className="text-[12px] text-muted-foreground">No extensions match.</p>
          ) : (
            <div className="grid gap-1 md:grid-cols-2 lg:grid-cols-3">
              {filteredAvailable.map((ext) => {
                const name = extName(ext)
                const alreadyInstalled = installedSet.has(name.toLowerCase())
                return (
                  <div
                    key={name}
                    className={cn(
                      'flex items-center justify-between gap-2 rounded-md border p-2',
                      alreadyInstalled ? 'border-success/30 bg-success/5' : 'border-border bg-background',
                    )}
                  >
                    <div className="min-w-0">
                      <p className="truncate font-mono text-[12px] font-medium text-foreground">{name}</p>
                      <p className="truncate text-[10.5px] text-muted-foreground">
                        {ext?.comment || ext?.description || 'No description'}
                      </p>
                    </div>
                    <Button
                      size="sm"
                      variant={alreadyInstalled ? 'ghost' : 'outline'}
                      disabled={busyName === name || alreadyInstalled}
                      onClick={() => action(name, 'install')}
                      className="h-6 shrink-0 rounded-md text-[11px]"
                    >
                      {alreadyInstalled ? 'Installed' : busyName === name ? 'Installing…' : 'Install'}
                    </Button>
                  </div>
                )
              })}
            </div>
          )}
        </Section>
      </div>
    </div>
  )
}
