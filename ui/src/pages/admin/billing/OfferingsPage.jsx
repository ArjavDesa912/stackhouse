import { useEffect, useState } from 'react'

import { Button } from '@/components/ui/button'
import { Input } from '@/components/ui/input'
import { Label } from '@/components/ui/label'

import { BillingAdminApi } from '../../../lib/billingAdmin'
import BillingNoApp from './BillingNoApp'

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

import { useOutletContext } from 'react-router';

export default function OfferingsPage() {
  const { appId } = useOutletContext();
  const [offerings, setOfferings] = useState([])
  const [products, setProducts] = useState([])
  const [identifier, setIdentifier] = useState('default')
  const [isCurrent, setIsCurrent] = useState(true)
  const [packages, setPackages] = useState([
    { identifier: '$rc_monthly', package_type: 'MONTHLY', product_id: '' },
  ])
  const [busy, setBusy] = useState(false)
  const [error, setError] = useState(null)

  useEffect(() => {
    let cancelled = false

    async function loadData() {
      if (appId === null) {
        setOfferings([])
        setProducts([])
        return
      }

      try {
        const [offeringRows, productRows] = await Promise.all([
          BillingAdminApi.listOfferings(appId),
          BillingAdminApi.listProducts(appId),
        ])

        if (!cancelled) {
          setOfferings(offeringRows)
          setProducts(productRows)
        }
      } catch (err) {
        if (!cancelled) {
          setError(err instanceof Error ? err.message : String(err))
        }
      }
    }

    loadData()

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
      await BillingAdminApi.upsertOffering({
        app_id: appId,
        identifier,
        is_current: isCurrent,
        packages: packages
          .filter((pkg) => pkg.product_id !== '')
          .map((pkg) => ({
            identifier: pkg.identifier,
            package_type: pkg.package_type || undefined,
            product_id: Number(pkg.product_id),
          })),
      })

      setOfferings(await BillingAdminApi.listOfferings(appId))
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
          Create / update offering
        </h3>
        <p className="mt-1 max-w-2xl text-[12.5px] leading-relaxed text-muted-foreground">
          An offering is a named set of packages that your paywall displays.
        </p>
        <form className="mt-3 space-y-3" onSubmit={submit}>
          <div className="grid grid-cols-1 gap-2 md:grid-cols-3">
            <div>
              <Label htmlFor="billing-offering-identifier">Identifier</Label>
              <Input
                id="billing-offering-identifier"
                onChange={(event) => setIdentifier(event.target.value)}
                required
                value={identifier}
              />
            </div>
            <div className="flex items-end">
              <label className="flex items-center gap-1 text-[12px] text-foreground">
                <input
                  checked={isCurrent}
                  onChange={(event) => setIsCurrent(event.target.checked)}
                  type="checkbox"
                />
                Mark as current
              </label>
            </div>
          </div>

          <div className="space-y-1">
            <Label>Packages</Label>
            {packages.map((pkg, index) => (
              <div
                key={`${pkg.identifier}-${index}`}
                className="grid grid-cols-1 gap-1 md:grid-cols-4"
              >
                <Input
                  onChange={(event) =>
                    setPackages((current) =>
                      current.map((row, rowIndex) =>
                        rowIndex === index
                          ? { ...row, identifier: event.target.value }
                          : row,
                      ),
                    )
                  }
                  placeholder="$rc_monthly"
                  value={pkg.identifier}
                />
                <SelectField
                  onChange={(event) =>
                    setPackages((current) =>
                      current.map((row, rowIndex) =>
                        rowIndex === index
                          ? {
                              ...row,
                              product_id:
                                event.target.value === ''
                                  ? ''
                                  : Number(event.target.value),
                            }
                          : row,
                      ),
                    )
                  }
                  value={pkg.product_id}
                >
                  <option value="">- product -</option>
                  {products.map((product) => (
                    <option key={product.id} value={product.id}>
                      {product.store_product_id} ({product.store})
                    </option>
                  ))}
                </SelectField>
                <Input
                  onChange={(event) =>
                    setPackages((current) =>
                      current.map((row, rowIndex) =>
                        rowIndex === index
                          ? { ...row, package_type: event.target.value }
                          : row,
                      ),
                    )
                  }
                  placeholder="MONTHLY"
                  value={pkg.package_type}
                />
                <Button
                  onClick={() =>
                    setPackages((current) =>
                      current.filter((_, rowIndex) => rowIndex !== index),
                    )
                  }
                  type="button"
                  variant="outline"
                >
                  Remove
                </Button>
              </div>
            ))}
            <Button
              onClick={() =>
                setPackages((current) => [
                  ...current,
                  { identifier: '', package_type: '', product_id: '' },
                ])
              }
              type="button"
              variant="outline"
            >
              Add package
            </Button>
          </div>

          <div>
            <Button disabled={busy} type="submit">
              {busy ? 'Saving...' : 'Save offering'}
            </Button>
            {error ? (
              <span className="ml-2 text-[12px] text-destructive">{error}</span>
            ) : null}
          </div>
        </form>
      </section>

      <section className="overflow-hidden rounded-md border border-border bg-card">
        <div className="flex items-center justify-between px-3 py-3">
          <h3 className="font-serif text-[20px] font-medium tracking-tight">
            Offerings
          </h3>
          <span className="text-[11px] text-muted-foreground">
            {offerings.length}
          </span>
        </div>
        <div className="divide-y divide-border">
          {offerings.map((offering) => (
            <div key={offering.id} className="px-3 py-3">
              <div className="flex items-center gap-1">
                <span className="text-[13px] font-medium">
                  {offering.identifier}
                </span>
                {offering.is_current ? (
                  <span className="rounded-full bg-primary/10 px-1 py-0.5 text-[10px] font-medium text-primary">
                    current
                  </span>
                ) : null}
                <span className="ml-auto text-[11px] text-muted-foreground">
                  {offering.packages.length} packages
                </span>
              </div>
              <ul className="mt-1 space-y-0.5 pl-3 text-[12px] text-muted-foreground">
                {offering.packages.map((pkg) => (
                  <li key={pkg.id}>
                    <code>{pkg.identifier}</code>
                    {' -> '}product #{pkg.product_id}
                    {pkg.package_type ? ` - ${pkg.package_type}` : ''}
                  </li>
                ))}
              </ul>
            </div>
          ))}
          {offerings.length === 0 ? (
            <div className="px-3 py-3 text-center text-[12px] text-muted-foreground">
              No offerings yet.
            </div>
          ) : null}
        </div>
      </section>
    </div>
  )
}

