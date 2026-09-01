import { useCallback, useEffect, useState } from 'react'

import { NavLink, Outlet } from 'react-router';
import {
  BILLING_APP_STORAGE_KEY,
  BillingAdminApi,
  resolveInitialBillingAppId,
} from '../../../lib/billingAdmin'

const TABS = [
  { id: 'apps', label: 'Apps' },
  { id: 'products', label: 'Products' },
  { id: 'entitlements', label: 'Entitlements' },
  { id: 'offerings', label: 'Offerings' },
  { id: 'audiences', label: 'Audiences' },
  { id: 'experiments', label: 'Experiments' },
  { id: 'paywalls', label: 'Paywalls' },
  { id: 'grant', label: 'Grant' },
  { id: 'webhooks', label: 'Webhooks' },
]

export default function BillingLayout() {
  const [apps, setApps] = useState([])
  const [error, setError] = useState(null)
  const [loadingApps, setLoadingApps] = useState(true)
  const [selectedAppId, setSelectedAppId] = useState(null)

  const loadApps = useCallback(async () => {
    setLoadingApps(true)
    setError(null)

    try {
      const rows = await BillingAdminApi.listApps()
      setApps(rows)

      const nextAppId = resolveInitialBillingAppId(
        rows,
        localStorage.getItem(BILLING_APP_STORAGE_KEY),
      )

      setSelectedAppId(nextAppId)

      if (nextAppId !== null) {
        localStorage.setItem(BILLING_APP_STORAGE_KEY, String(nextAppId))
      } else {
        localStorage.removeItem(BILLING_APP_STORAGE_KEY)
      }
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err))
    } finally {
      setLoadingApps(false)
    }
  }, [])

  useEffect(() => {
    loadApps()
  }, [loadApps])



  const handleSelectApp = (value) => {
    const nextAppId = value === '' ? null : Number(value)
    setSelectedAppId(nextAppId)

    if (nextAppId === null) {
      localStorage.removeItem(BILLING_APP_STORAGE_KEY)
      return
    }

    localStorage.setItem(BILLING_APP_STORAGE_KEY, String(nextAppId))
  }
  return (
    <div className="space-y-3">
      <section className="rounded-md border border-border bg-card px-3 py-3">
        <div className="flex flex-col gap-3 lg:flex-row lg:items-end lg:justify-between">
          <div>
            <p className="text-[10px] font-semibold uppercase tracking-[0.16em] text-muted-foreground">
              Platform
            </p>
            <h2 className="mt-1 font-serif text-[28px] font-medium leading-[1.08] tracking-tight">
              Billing
            </h2>
            <p className="mt-1 max-w-2xl text-[12.5px] leading-relaxed text-muted-foreground">
              Manage apps, products, entitlements, offerings, grants, and
              outbound webhooks from the main Admin Center.
            </p>
          </div>

          <div className="min-w-[240px]">
            <label
              className="mb-0.5.5 block text-[10px] font-semibold uppercase tracking-[0.14em] text-muted-foreground"
              htmlFor="billing-app-selector"
            >
              Active app
            </label>
            <select
              id="billing-app-selector"
              className="flex h-8 w-full rounded-md border border-border bg-background px-2 text-[12px] text-foreground outline-none transition-colors focus-visible:ring-2 focus-visible:ring-ring/40"
              disabled={loadingApps}
              onChange={(event) => handleSelectApp(event.target.value)}
              value={selectedAppId ?? ''}
            >
              {apps.length === 0 ? (
                <option value="">
                  {loadingApps ? 'Loading apps...' : 'No apps yet'}
                </option>
              ) : (
                apps.map((app) => (
                  <option key={app.id} value={app.id}>
                    {app.name} (#{app.id})
                  </option>
                ))
              )}
            </select>
          </div>
        </div>

        <div className="mt-3 flex flex-wrap gap-1">
          {TABS.map((tab) => (
            <NavLink
              key={tab.id}
              to={tab.id}
              className={({ isActive }) => `rounded-full border px-2.5 py-0.5.5 text-[12px] font-medium tracking-tight transition-colors ${
                isActive
                  ? 'border-primary bg-primary/5 text-foreground ring-1 ring-primary'
                  : 'border-border bg-background text-muted-foreground hover:bg-muted hover:text-foreground'
              }`}
            >
              {tab.label}
            </NavLink>
          ))}
        </div>
      </section>

      {error ? (
        <div className="rounded-md border border-destructive/30 bg-destructive/5 px-3 py-2 text-[12px] text-destructive">
          {error}
        </div>
      ) : null}

      <Outlet context={{ appId: selectedAppId, apps, loadApps }} />
    </div>
  )
}
