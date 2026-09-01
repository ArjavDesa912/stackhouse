import React, { useCallback, useEffect, useState } from 'react'
import { Ban, Plus, RotateCw, Shield, ShieldCheck } from 'lucide-react'

import {
  Banner,
  Empty,
  InlineSpinner,
  MetricTile,
  Overline,
  Section,
  apiFetch,
} from './common'
import { Button } from '@/components/ui/button'
import { Input } from '@/components/ui/input'
import { cn } from '@/lib/utils'

export default function NetworkModule() {
  const [rules, setRules] = useState([])
  const [enabled, setEnabled] = useState(false)
  const [loading, setLoading] = useState(false)
  const [error, setError] = useState('')
  const [message, setMessage] = useState('')
  const [authError, setAuthError] = useState(false)
  const [newIp, setNewIp] = useState('')
  const [newDescription, setNewDescription] = useState('')
  const [busy, setBusy] = useState(false)

  const load = useCallback(async () => {
    setLoading(true)
    setError('')
    try {
      const payload = await apiFetch('/v1/admin/network/rules')
      setRules(Array.isArray(payload?.data) ? payload.data : [])
      setEnabled(Boolean(payload?.enabled))
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

  const toggleEnabled = async () => {
    setBusy(true)
    setError('')
    setMessage('')
    try {
      await apiFetch(enabled ? '/v1/admin/network/disable' : '/v1/admin/network/enable', {
        method: 'POST',
      })
      setMessage(enabled ? 'Network restrictions disabled.' : 'Network restrictions enabled.')
      await load()
    } catch (e) {
      setError(e.message)
    } finally {
      setBusy(false)
    }
  }

  const addRule = async () => {
    if (!newIp.trim()) return
    setBusy(true)
    setError('')
    setMessage('')
    try {
      await apiFetch('/v1/admin/network/rules', {
        method: 'POST',
        body: JSON.stringify({ ip: newIp.trim(), description: newDescription.trim() }),
      })
      setMessage(`Rule for '${newIp.trim()}' added.`)
      setNewIp('')
      setNewDescription('')
      await load()
    } catch (e) {
      setError(e.message)
    } finally {
      setBusy(false)
    }
  }

  return (
    <div className="h-full overflow-y-auto">
      <div className="space-y-3 p-3">
        <div className="flex flex-wrap items-center justify-between gap-2">
          <div className="flex items-center gap-2">
            <div className="flex h-8 w-8 items-center justify-center rounded-md bg-primary/10 text-primary ring-1 ring-primary/20">
              <Shield className="h-3 w-3" />
            </div>
            <div>
              <Overline>Network policy</Overline>
              <p className="font-serif text-[20px] font-medium leading-tight tracking-tight text-foreground">
                IP allowlist firewall
              </p>
            </div>
          </div>
          <div className="flex items-center gap-1">
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
              onClick={toggleEnabled}
              disabled={busy || authError}
              variant={enabled ? 'outline' : 'default'}
              className="h-6 gap-0.5.5 rounded-full text-[11.5px]"
            >
              {enabled ? <Ban className="h-2 w-2" /> : <ShieldCheck className="h-2 w-2" />}
              {enabled ? 'Disable enforcement' : 'Enable enforcement'}
            </Button>
          </div>
        </div>

        {authError ? (
          <Banner kind="auth">/v1/admin/network requires service-admin authentication.</Banner>
        ) : null}
        {error ? <Banner kind="error">{error}</Banner> : null}
        {message ? <Banner kind="success">{message}</Banner> : null}

        <div className="grid gap-2 sm:grid-cols-3">
          <MetricTile
            label="Enforcement"
            value={enabled ? 'ACTIVE' : 'OFF'}
            tone={enabled ? 'accent' : 'default'}
          />
          <MetricTile label="Allowlist rules" value={rules.length} />
          <MetricTile label="Mode" value={enabled ? 'deny-by-default' : 'open'} />
        </div>

        <Section
          title="Add rule"
          description="Accepts IPv4, IPv6, or CIDR ranges. Rules take effect immediately once enforcement is enabled."
        >
          <div className="grid gap-1 md:grid-cols-[1fr_1.3fr_auto]">
            <Input
              value={newIp}
              onChange={(e) => setNewIp(e.target.value)}
              placeholder="10.0.0.0/8 or 203.0.113.42"
              className="h-8 font-mono"
            />
            <Input
              value={newDescription}
              onChange={(e) => setNewDescription(e.target.value)}
              placeholder="Optional description"
              className="h-8"
            />
            <Button
              size="sm"
              onClick={addRule}
              disabled={busy || !newIp.trim() || authError}
              className="h-8 gap-0.5.5 rounded-full px-3 text-[12px]"
            >
              <Plus className="h-2 w-2" />
              Add
            </Button>
          </div>
        </Section>

        <Section title={`Allowlist · ${rules.length}`}>
          {rules.length === 0 && !authError ? (
            <Empty
              icon={Shield}
              title="No rules yet"
              description="Add at least one rule before enabling enforcement, or you will lock yourself out."
            />
          ) : (
            <div className="overflow-hidden rounded-md border border-border">
              <table className="min-w-full text-left text-[12px]">
                <thead className="bg-muted/30">
                  <tr className="border-b border-border">
                    {['IP / CIDR', 'Description', 'State'].map((h) => (
                      <th
                        key={h}
                        className="px-2 py-1 text-[10px] font-medium uppercase tracking-[0.12em] text-muted-foreground"
                      >
                        {h}
                      </th>
                    ))}
                  </tr>
                </thead>
                <tbody>
                  {rules.map((rule, index) => (
                    <tr
                      key={`${rule.ip}-${index}`}
                      className={cn(index !== rules.length - 1 && 'border-b border-border/60')}
                    >
                      <td className="px-2 py-1.5 font-mono text-foreground">{rule.ip}</td>
                      <td className="px-2 py-1.5 text-muted-foreground">{rule.description || '—'}</td>
                      <td className="px-2 py-1.5">
                        <span
                          className={cn(
                            'inline-flex items-center gap-0.5.5 rounded-full px-1 py-0.5 text-[10.5px] font-medium',
                            rule.enabled
                              ? 'bg-success/10 text-success dark:text-success'
                              : 'bg-muted text-muted-foreground',
                          )}
                        >
                          <span
                            className={cn(
                              'h-0.5.5 w-0.5.5 rounded-full',
                              rule.enabled ? 'bg-success' : 'bg-muted-foreground/60',
                            )}
                          />
                          {rule.enabled ? 'active' : 'disabled'}
                        </span>
                      </td>
                    </tr>
                  ))}
                </tbody>
              </table>
            </div>
          )}
          <p className="mt-1 text-[11px] text-muted-foreground">
            Note: the current backend does not expose a per-rule delete route; remove rules by restarting
            the allowlist via the enable/disable toggle.
          </p>
        </Section>
      </div>
    </div>
  )
}
