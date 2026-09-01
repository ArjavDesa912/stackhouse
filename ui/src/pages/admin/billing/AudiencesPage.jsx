import { useEffect, useState } from 'react'
import { useOutletContext } from 'react-router'

import { Button } from '@/components/ui/button'
import { Input } from '@/components/ui/input'
import { Label } from '@/components/ui/label'

import { BillingAdminApi } from '../../../lib/billingAdmin'
import BillingNoApp from './BillingNoApp'

const OPERATORS = ['eq', 'neq', 'gt', 'gte', 'lt', 'lte', 'in', 'exists']

function SelectField({ children, ...props }) {
  return (
    <select
      className="flex h-8 w-full rounded-md border border-border bg-background px-2 text-[12px] text-foreground outline-none transition-colors focus-visible:ring-2 focus-visible:ring-ring/40"
      {...props}
    >
      {children}
    </select>
  )
}

export default function AudiencesPage() {
  const { appId } = useOutletContext()
  const [audiences, setAudiences] = useState([])
  const [identifier, setIdentifier] = useState('')
  const [displayName, setDisplayName] = useState('')
  const [rules, setRules] = useState([{ field: 'country', op: 'eq', value: '' }])
  const [busy, setBusy] = useState(false)
  const [error, setError] = useState(null)

  useEffect(() => {
    if (appId === null) {
      setAudiences([])
      return
    }
    let cancelled = false
    BillingAdminApi.listAudiences(appId)
      .then((rows) => {
        if (!cancelled) setAudiences(rows)
      })
      .catch((err) => {
        if (!cancelled) setError(err instanceof Error ? err.message : String(err))
      })
    return () => {
      cancelled = true
    }
  }, [appId])

  if (appId === null) {
    return <BillingNoApp />
  }

  const submit = async (event) => {
    event.preventDefault()
    setBusy(true)
    setError(null)
    try {
      const body = {
        app_id: appId,
        identifier,
        display_name: displayName || undefined,
        rules: rules.map((r) => ({
          field: r.field,
          op: r.op,
          value: r.value,
        })),
      }
      await BillingAdminApi.upsertAudience(body)
      setAudiences(await BillingAdminApi.listAudiences(appId))
      setIdentifier('')
      setDisplayName('')
      setRules([{ field: 'country', op: 'eq', value: '' }])
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err))
    } finally {
      setBusy(false)
    }
  }

  const updateRule = (index, key, value) => {
    setRules((current) =>
      current.map((row, i) => (i === index ? { ...row, [key]: value } : row)),
    )
  }

  return (
    <div className="space-y-3">
      <section className="rounded-md border border-border bg-card p-3">
        <h3 className="font-serif text-[20px] font-medium tracking-tight">
          Create / update audience
        </h3>
        <p className="mt-1 max-w-2xl text-[12.5px] leading-relaxed text-muted-foreground">
          Audiences match customers by attributes and request context. Offerings and
          experiments can target an audience.
        </p>
        <form className="mt-3 space-y-3" onSubmit={submit}>
          <div className="grid grid-cols-1 gap-2 md:grid-cols-2">
            <div>
              <Label htmlFor="audience-identifier">Identifier</Label>
              <Input
                id="audience-identifier"
                onChange={(e) => setIdentifier(e.target.value)}
                required
                value={identifier}
              />
            </div>
            <div>
              <Label htmlFor="audience-display-name">Display name</Label>
              <Input
                id="audience-display-name"
                onChange={(e) => setDisplayName(e.target.value)}
                value={displayName}
              />
            </div>
          </div>

          <div className="space-y-1">
            <Label>Rules</Label>
            {rules.map((rule, index) => (
              <div
                key={index}
                className="grid grid-cols-1 gap-1 md:grid-cols-5"
              >
                <Input
                  onChange={(e) => updateRule(index, 'field', e.target.value)}
                  placeholder="country"
                  value={rule.field}
                />
                <SelectField
                  onChange={(e) => updateRule(index, 'op', e.target.value)}
                  value={rule.op}
                >
                  {OPERATORS.map((op) => (
                    <option key={op} value={op}>
                      {op}
                    </option>
                  ))}
                </SelectField>
                <Input
                  className="md:col-span-2"
                  onChange={(e) => updateRule(index, 'value', e.target.value)}
                  placeholder="US"
                  value={rule.value}
                />
                <Button
                  onClick={() => setRules((current) => current.filter((_, i) => i !== index))}
                  type="button"
                  variant="outline"
                >
                  Remove
                </Button>
              </div>
            ))}
            <Button
              onClick={() =>
                setRules((current) => [...current, { field: '', op: 'eq', value: '' }])
              }
              type="button"
              variant="outline"
            >
              Add rule
            </Button>
          </div>

          <div>
            <Button disabled={busy} type="submit">
              {busy ? 'Saving...' : 'Save audience'}
            </Button>
            {error ? (
              <span className="ml-2 text-[12px] text-destructive">{error}</span>
            ) : null}
          </div>
        </form>
      </section>

      <section className="overflow-hidden rounded-md border border-border bg-card">
        <div className="flex items-center justify-between px-3 py-3">
          <h3 className="font-serif text-[20px] font-medium tracking-tight">Audiences</h3>
          <span className="text-[11px] text-muted-foreground">{audiences.length}</span>
        </div>
        <div className="divide-y divide-border">
          {audiences.map((audience) => (
            <div key={audience.id} className="px-3 py-3">
              <div className="text-[13px] font-medium">
                {audience.display_name || audience.identifier}
                <span className="ml-2 text-[11px] text-muted-foreground">#{audience.id}</span>
              </div>
              <div className="mt-1 text-[12px] text-muted-foreground">
                {audience.rules.length} rule{audience.rules.length === 1 ? '' : 's'}
              </div>
            </div>
          ))}
          {audiences.length === 0 ? (
            <div className="px-3 py-3 text-center text-[12px] text-muted-foreground">
              No audiences yet.
            </div>
          ) : null}
        </div>
      </section>
    </div>
  )
}
