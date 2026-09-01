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

export default function ProductsPage() {
  const { appId } = useOutletContext();
  const [products, setProducts] = useState([])
  const [busy, setBusy] = useState(false)
  const [error, setError] = useState(null)
  const [form, setForm] = useState({
    currency: 'USD',
    period: 'P1M',
    price_micros: '',
    product_type: 'auto_renewable',
    store: 'app_store',
    store_product_id: '',
  })

  useEffect(() => {
    let cancelled = false

    async function loadProducts() {
      if (appId === null) {
        setProducts([])
        return
      }

      try {
        const rows = await BillingAdminApi.listProducts(appId)
        if (!cancelled) {
          setProducts(rows)
        }
      } catch (err) {
        if (!cancelled) {
          setError(err instanceof Error ? err.message : String(err))
        }
      }
    }

    loadProducts()

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
      await BillingAdminApi.upsertProduct({
        app_id: appId,
        currency: form.currency || undefined,
        period: form.period || undefined,
        price_micros: form.price_micros ? Number(form.price_micros) : undefined,
        product_type: form.product_type,
        store: form.store,
        store_product_id: form.store_product_id,
      })

      setForm((current) => ({ ...current, store_product_id: '' }))
      setProducts(await BillingAdminApi.listProducts(appId))
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
          Create / update product
        </h3>
        <form
          className="mt-3 grid grid-cols-1 gap-2 md:grid-cols-6"
          onSubmit={submit}
        >
          <div className="md:col-span-2">
            <Label htmlFor="billing-product-id">Store product ID</Label>
            <Input
              id="billing-product-id"
              onChange={(event) =>
                setForm((current) => ({
                  ...current,
                  store_product_id: event.target.value,
                }))
              }
              placeholder="pro.monthly"
              required
              value={form.store_product_id}
            />
          </div>
          <div>
            <Label htmlFor="billing-product-store">Store</Label>
            <SelectField
              id="billing-product-store"
              onChange={(event) =>
                setForm((current) => ({ ...current, store: event.target.value }))
              }
              value={form.store}
            >
              <option value="app_store">app_store</option>
              <option value="play_store">play_store</option>
              <option value="stripe">stripe</option>
              <option value="promotional">promotional</option>
            </SelectField>
          </div>
          <div>
            <Label htmlFor="billing-product-type">Type</Label>
            <SelectField
              id="billing-product-type"
              onChange={(event) =>
                setForm((current) => ({
                  ...current,
                  product_type: event.target.value,
                }))
              }
              value={form.product_type}
            >
              <option value="auto_renewable">auto_renewable</option>
              <option value="non_renewing">non_renewing</option>
              <option value="consumable">consumable</option>
              <option value="non_consumable">non_consumable</option>
            </SelectField>
          </div>
          <div>
            <Label htmlFor="billing-product-period">Period (ISO 8601)</Label>
            <Input
              id="billing-product-period"
              onChange={(event) =>
                setForm((current) => ({ ...current, period: event.target.value }))
              }
              placeholder="P1M"
              value={form.period}
            />
          </div>
          <div>
            <Label htmlFor="billing-product-price">Price (micros)</Label>
            <Input
              id="billing-product-price"
              onChange={(event) =>
                setForm((current) => ({
                  ...current,
                  price_micros: event.target.value,
                }))
              }
              placeholder="9990000"
              type="number"
              value={form.price_micros}
            />
          </div>
          <div>
            <Label htmlFor="billing-product-currency">Currency</Label>
            <Input
              id="billing-product-currency"
              onChange={(event) =>
                setForm((current) => ({
                  ...current,
                  currency: event.target.value,
                }))
              }
              value={form.currency}
            />
          </div>
          <div className="md:col-span-6">
            <Button disabled={busy} type="submit">
              {busy ? 'Saving...' : 'Save product'}
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
            Products
          </h3>
          <span className="text-[11px] text-muted-foreground">
            {products.length}
          </span>
        </div>
        <table className="w-full border-t border-border">
          <thead className="bg-muted/30">
            <tr>
              <th className="px-3 py-1.5 text-left text-[10px] font-semibold uppercase tracking-[0.12em] text-muted-foreground">
                #
              </th>
              <th className="px-3 py-1.5 text-left text-[10px] font-semibold uppercase tracking-[0.12em] text-muted-foreground">
                Store
              </th>
              <th className="px-3 py-1.5 text-left text-[10px] font-semibold uppercase tracking-[0.12em] text-muted-foreground">
                Product ID
              </th>
              <th className="px-3 py-1.5 text-left text-[10px] font-semibold uppercase tracking-[0.12em] text-muted-foreground">
                Type
              </th>
              <th className="px-3 py-1.5 text-left text-[10px] font-semibold uppercase tracking-[0.12em] text-muted-foreground">
                Period
              </th>
              <th className="px-3 py-1.5 text-right text-[10px] font-semibold uppercase tracking-[0.12em] text-muted-foreground">
                Price
              </th>
            </tr>
          </thead>
          <tbody>
            {products.map((product) => (
              <tr
                key={product.id}
                className="border-t border-border/70 hover:bg-muted/10"
              >
                <td className="px-3 py-2 text-[12px]">{product.id}</td>
                <td className="px-3 py-2 text-[12px]">{product.store}</td>
                <td className="px-3 py-2 font-mono text-[11px]">
                  {product.store_product_id}
                </td>
                <td className="px-3 py-2 text-[12px]">{product.product_type}</td>
                <td className="px-3 py-2 text-[12px]">{product.period || '-'}</td>
                <td className="px-3 py-2 text-right text-[12px]">
                  {product.price_micros
                    ? `${(product.price_micros / 1000000).toFixed(2)} ${product.currency || ''}`
                    : '-'}
                </td>
              </tr>
            ))}
            {products.length === 0 ? (
              <tr>
                <td
                  className="px-3 py-3 text-center text-[12px] text-muted-foreground"
                  colSpan={6}
                >
                  No products yet.
                </td>
              </tr>
            ) : null}
          </tbody>
        </table>
      </section>
    </div>
  )
}

