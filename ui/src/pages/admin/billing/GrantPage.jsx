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

export default function GrantPage() {
  const { appId } = useOutletContext();
  const [products, setProducts] = useState([])
  const [appUserId, setAppUserId] = useState('')
  const [productId, setProductId] = useState('')
  const [days, setDays] = useState(30)
  const [status, setStatus] = useState(null)
  const [error, setError] = useState(null)
  const [busy, setBusy] = useState(false)

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
      } catch {
        if (!cancelled) {
          setProducts([])
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
    setStatus(null)

    try {
      await BillingAdminApi.grant({
        app_id: appId,
        app_user_id: appUserId,
        duration_days: days,
        product_id: Number(productId),
      })
      setStatus(`Granted ${days}d of product #${productId} to ${appUserId}.`)
      setAppUserId('')
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err))
    } finally {
      setBusy(false)
    }
  }

  return (
    <section className="rounded-md border border-border bg-card p-3">
      <h3 className="font-serif text-[20px] font-medium tracking-tight">
        Grant promotional entitlement
      </h3>
      <p className="mt-1 max-w-2xl text-[12.5px] leading-relaxed text-muted-foreground">
        Use this for customer support comps, refund recovery, or promotional
        launches.
      </p>
      <form
        className="mt-3 grid grid-cols-1 gap-2 md:grid-cols-4"
        onSubmit={submit}
      >
        <div className="md:col-span-2">
          <Label htmlFor="billing-grant-user">App user ID</Label>
          <Input
            id="billing-grant-user"
            onChange={(event) => setAppUserId(event.target.value)}
            placeholder="user-42"
            required
            value={appUserId}
          />
        </div>
        <div>
          <Label htmlFor="billing-grant-product">Product</Label>
          <SelectField
            id="billing-grant-product"
            onChange={(event) => setProductId(event.target.value)}
            required
            value={productId}
          >
            <option value="">-</option>
            {products.map((product) => (
              <option key={product.id} value={product.id}>
                {product.store_product_id} ({product.store})
              </option>
            ))}
          </SelectField>
        </div>
        <div>
          <Label htmlFor="billing-grant-days">Duration (days)</Label>
          <Input
            id="billing-grant-days"
            min={1}
            onChange={(event) => setDays(Number(event.target.value))}
            type="number"
            value={days}
          />
        </div>
        <div className="md:col-span-4">
          <Button disabled={busy} type="submit">
            {busy ? 'Granting...' : 'Grant'}
          </Button>
          {status ? (
            <span className="ml-2 text-[12px] text-primary">{status}</span>
          ) : null}
          {error ? (
            <span className="ml-2 text-[12px] text-destructive">{error}</span>
          ) : null}
        </div>
      </form>
    </section>
  )
}

