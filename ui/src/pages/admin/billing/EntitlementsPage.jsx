import { useEffect, useState } from 'react'

import { Button } from '@/components/ui/button'
import { Input } from '@/components/ui/input'
import { Label } from '@/components/ui/label'

import { BillingAdminApi } from '../../../lib/billingAdmin'
import BillingNoApp from './BillingNoApp'

import { useOutletContext } from 'react-router';

export default function EntitlementsPage() {
  const { appId } = useOutletContext();
  const [rows, setRows] = useState([])
  const [products, setProducts] = useState([])
  const [identifier, setIdentifier] = useState('')
  const [displayName, setDisplayName] = useState('')
  const [selected, setSelected] = useState([])
  const [busy, setBusy] = useState(false)
  const [error, setError] = useState(null)

  useEffect(() => {
    let cancelled = false

    async function loadData() {
      if (appId === null) {
        setRows([])
        setProducts([])
        return
      }

      try {
        const [entitlements, productRows] = await Promise.all([
          BillingAdminApi.listEntitlements(appId),
          BillingAdminApi.listProducts(appId),
        ])

        if (!cancelled) {
          setRows(entitlements)
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

  const toggleProduct = (id) => {
    setSelected((current) =>
      current.includes(id)
        ? current.filter((value) => value !== id)
        : [...current, id],
    )
  }

  const submit = async (event) => {
    event.preventDefault()
    setBusy(true)
    setError(null)

    try {
      await BillingAdminApi.upsertEntitlement({
        app_id: appId,
        display_name: displayName || undefined,
        identifier,
        product_ids: selected,
      })
      setIdentifier('')
      setDisplayName('')
      setSelected([])

      const [entitlements, productRows] = await Promise.all([
        BillingAdminApi.listEntitlements(appId),
        BillingAdminApi.listProducts(appId),
      ])
      setRows(entitlements)
      setProducts(productRows)
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
          Create / update entitlement
        </h3>
        <form
          className="mt-3 grid grid-cols-1 gap-2 md:grid-cols-2"
          onSubmit={submit}
        >
          <div>
            <Label htmlFor="billing-entitlement-identifier">Identifier</Label>
            <Input
              id="billing-entitlement-identifier"
              onChange={(event) => setIdentifier(event.target.value)}
              placeholder="pro"
              required
              value={identifier}
            />
          </div>
          <div>
            <Label htmlFor="billing-entitlement-display-name">Display name</Label>
            <Input
              id="billing-entitlement-display-name"
              onChange={(event) => setDisplayName(event.target.value)}
              placeholder="Pro"
              value={displayName}
            />
          </div>
          <div className="md:col-span-2">
            <Label>Attach products</Label>
            <div className="mt-1 flex flex-wrap gap-1">
              {products.length === 0 ? (
                <span className="text-[12px] text-muted-foreground">
                  No products yet - create some on the Products tab.
                </span>
              ) : null}
              {products.map((product) => (
                <button
                  key={product.id}
                  className={`rounded-full border px-2 py-0.5 text-[12px] transition-colors ${
                    selected.includes(product.id)
                      ? 'border-primary bg-primary/5 text-foreground'
                      : 'border-border bg-background text-muted-foreground hover:bg-muted hover:text-foreground'
                  }`}
                  onClick={() => toggleProduct(product.id)}
                  type="button"
                >
                  {product.store_product_id}{' '}
                  <span className="opacity-70">({product.store})</span>
                </button>
              ))}
            </div>
          </div>
          <div className="md:col-span-2">
            <Button disabled={busy} type="submit">
              {busy ? 'Saving...' : 'Save entitlement'}
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
            Entitlements
          </h3>
          <span className="text-[11px] text-muted-foreground">{rows.length}</span>
        </div>
        <table className="w-full border-t border-border">
          <thead className="bg-muted/30">
            <tr>
              <th className="px-3 py-1.5 text-left text-[10px] font-semibold uppercase tracking-[0.12em] text-muted-foreground">
                #
              </th>
              <th className="px-3 py-1.5 text-left text-[10px] font-semibold uppercase tracking-[0.12em] text-muted-foreground">
                Identifier
              </th>
              <th className="px-3 py-1.5 text-left text-[10px] font-semibold uppercase tracking-[0.12em] text-muted-foreground">
                Display
              </th>
              <th className="px-3 py-1.5 text-left text-[10px] font-semibold uppercase tracking-[0.12em] text-muted-foreground">
                Products
              </th>
            </tr>
          </thead>
          <tbody>
            {rows.map((row) => (
              <tr
                key={row.entitlement.id}
                className="border-t border-border/70 hover:bg-muted/10"
              >
                <td className="px-3 py-2 text-[12px]">{row.entitlement.id}</td>
                <td className="px-3 py-2 text-[12px] font-medium">
                  {row.entitlement.identifier}
                </td>
                <td className="px-3 py-2 text-[12px] text-muted-foreground">
                  {row.entitlement.display_name || '-'}
                </td>
                <td className="px-3 py-2 text-[12px]">
                  {row.product_ids.length === 0
                    ? '-'
                    : row.product_ids
                        .map(
                          (productId) =>
                            products.find((product) => product.id === productId)
                              ?.store_product_id || `#${productId}`,
                        )
                        .join(', ')}
                </td>
              </tr>
            ))}
            {rows.length === 0 ? (
              <tr>
                <td
                  className="px-3 py-3 text-center text-[12px] text-muted-foreground"
                  colSpan={4}
                >
                  No entitlements yet.
                </td>
              </tr>
            ) : null}
          </tbody>
        </table>
      </section>
    </div>
  )
}

