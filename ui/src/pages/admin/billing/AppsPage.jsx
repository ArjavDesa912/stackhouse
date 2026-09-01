import { useState } from 'react'

import { Button } from '@/components/ui/button'
import { Input } from '@/components/ui/input'
import { Label } from '@/components/ui/label'

import { BillingAdminApi } from '../../../lib/billingAdmin'

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

function TableCell({ align = 'left', children, className = '', ...props }) {
  return (
    <td
      className={`px-3 py-2 text-[12px] ${align === 'right' ? 'text-right' : 'text-left'} ${className}`}
      {...props}
    >
      {children}
    </td>
  )
}

function TableHead({ align = 'left', children }) {
  return (
    <th
      className={`px-3 py-1.5 text-[10px] font-semibold uppercase tracking-[0.12em] text-muted-foreground ${
        align === 'right' ? 'text-right' : 'text-left'
      }`}
    >
      {children}
    </th>
  )
}

import { useOutletContext } from 'react-router';

export default function AppsPage() {
  const { apps, loadApps: onChanged } = useOutletContext();
  const [name, setName] = useState('')
  const [bundleId, setBundleId] = useState('')
  const [platform, setPlatform] = useState('multi')
  const [busy, setBusy] = useState(false)
  const [error, setError] = useState(null)
  const [secretsFor, setSecretsFor] = useState(null)
  const [secrets, setSecrets] = useState({
    apple_shared_secret: '',
    stripe_signing_secret: '',
    outbound_webhook_secret: '',
    google_service_account: '',
  })

  const submit = async (event) => {
    event.preventDefault()
    setBusy(true)
    setError(null)

    try {
      await BillingAdminApi.createApp({
        name,
        bundle_id: bundleId || undefined,
        platform,
      })
      setName('')
      setBundleId('')
      await onChanged()
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err))
    } finally {
      setBusy(false)
    }
  }

  const saveSecrets = async (appId) => {
    setBusy(true)
    setError(null)

    try {
      const payload = {}

      Object.entries(secrets).forEach(([key, value]) => {
        if (value) {
          payload[key] = value
        }
      })

      await BillingAdminApi.updateSecrets(appId, payload)
      setSecretsFor(null)
      setSecrets({
        apple_shared_secret: '',
        stripe_signing_secret: '',
        outbound_webhook_secret: '',
        google_service_account: '',
      })
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err))
    } finally {
      setBusy(false)
    }
  }

  return (
    <div className="space-y-3">
      <section className="rounded-md border border-border bg-card p-3">
        <h3 className="font-serif text-[20px] font-medium tracking-tight">
          Create app
        </h3>
        <p className="mt-1 max-w-2xl text-[12.5px] leading-relaxed text-muted-foreground">
          An app groups products, entitlements, and customers under a single
          identity.
        </p>
        <form
          className="mt-3 grid grid-cols-1 gap-2 md:grid-cols-4"
          onSubmit={submit}
        >
          <div className="md:col-span-2">
            <Label htmlFor="billing-app-name">Name</Label>
            <Input
              id="billing-app-name"
              onChange={(event) => setName(event.target.value)}
              placeholder="My Mobile App"
              required
              value={name}
            />
          </div>
          <div>
            <Label htmlFor="billing-app-bundle">Bundle / package</Label>
            <Input
              id="billing-app-bundle"
              onChange={(event) => setBundleId(event.target.value)}
              placeholder="com.example.app"
              value={bundleId}
            />
          </div>
          <div>
            <Label htmlFor="billing-app-platform">Platform</Label>
            <SelectField
              id="billing-app-platform"
              onChange={(event) => setPlatform(event.target.value)}
              value={platform}
            >
              <option value="multi">multi</option>
              <option value="ios">ios</option>
              <option value="android">android</option>
              <option value="web">web</option>
            </SelectField>
          </div>
          <div className="md:col-span-4">
            <Button disabled={busy} type="submit">
              {busy ? 'Saving...' : 'Create app'}
            </Button>
          </div>
        </form>
        {error ? (
          <p className="mt-1 text-[12px] text-destructive">{error}</p>
        ) : null}
      </section>

      <section className="overflow-hidden rounded-md border border-border bg-card">
        <div className="flex items-center justify-between px-3 py-3">
          <h3 className="font-serif text-[20px] font-medium tracking-tight">
            Apps
          </h3>
          <span className="text-[11px] text-muted-foreground">
            {apps.length} total
          </span>
        </div>
        <table className="w-full border-t border-border">
          <thead className="bg-muted/30">
            <tr>
              <TableHead>#</TableHead>
              <TableHead>Name</TableHead>
              <TableHead>Bundle</TableHead>
              <TableHead>Platform</TableHead>
              <TableHead>Created</TableHead>
              <TableHead align="right">Actions</TableHead>
            </tr>
          </thead>
          <tbody>
            {apps.map((app) => (
              <tr
                key={app.id}
                className="border-t border-border/70 hover:bg-muted/10"
              >
                <TableCell>{app.id}</TableCell>
                <TableCell className="font-medium">{app.name}</TableCell>
                <TableCell>{app.bundle_id || '-'}</TableCell>
                <TableCell>{app.platform}</TableCell>
                <TableCell className="text-muted-foreground">
                  {new Date(app.created_at).toLocaleDateString()}
                </TableCell>
                <TableCell align="right">
                  <Button
                    onClick={() => setSecretsFor(app.id)}
                    size="sm"
                    type="button"
                    variant="outline"
                  >
                    Secrets
                  </Button>
                </TableCell>
              </tr>
            ))}
            {apps.length === 0 ? (
              <tr>
                <TableCell className="text-center text-muted-foreground" colSpan={6}>
                  No apps yet.
                </TableCell>
              </tr>
            ) : null}
          </tbody>
        </table>
      </section>

      {secretsFor !== null ? (
        <section className="rounded-md border border-border bg-card p-3">
          <div className="flex items-center justify-between gap-2">
            <h3 className="font-serif text-[20px] font-medium tracking-tight">
              Store secrets for app #{secretsFor}
            </h3>
            <Button
              onClick={() => setSecretsFor(null)}
              type="button"
              variant="outline"
            >
              Close
            </Button>
          </div>
          <p className="mt-1 text-[12px] leading-relaxed text-muted-foreground">
            Leave a field blank to keep the existing value. Secrets never round
            trip to the browser.
          </p>
          <div className="mt-3 grid grid-cols-1 gap-2 md:grid-cols-2">
            {[
              ['apple_shared_secret', 'Apple shared secret'],
              ['stripe_signing_secret', 'Stripe signing secret'],
              ['outbound_webhook_secret', 'Outbound webhook secret'],
              ['google_service_account', 'Google service-account JSON'],
            ].map(([key, label]) => (
              <div
                key={key}
                className={key === 'google_service_account' ? 'md:col-span-2' : ''}
              >
                <Label htmlFor={`billing-secret-${key}`}>{label}</Label>
                {key === 'google_service_account' ? (
                  <textarea
                    className="flex min-h-20 w-full rounded-md border border-border bg-background px-2 py-1 font-mono text-[12px] text-foreground outline-none transition-colors focus-visible:ring-2 focus-visible:ring-ring/40"
                    id={`billing-secret-${key}`}
                    onChange={(event) =>
                      setSecrets((current) => ({
                        ...current,
                        [key]: event.target.value,
                      }))
                    }
                    value={secrets[key]}
                  />
                ) : (
                  <Input
                    id={`billing-secret-${key}`}
                    onChange={(event) =>
                      setSecrets((current) => ({
                        ...current,
                        [key]: event.target.value,
                      }))
                    }
                    type="password"
                    value={secrets[key]}
                  />
                )}
              </div>
            ))}
          </div>
          <div className="mt-3">
            <Button
              disabled={busy}
              onClick={() => saveSecrets(secretsFor)}
              type="button"
            >
              {busy ? 'Saving...' : 'Save secrets'}
            </Button>
          </div>
        </section>
      ) : null}
    </div>
  )
}

