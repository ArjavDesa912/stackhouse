import { useState } from 'react'

import { Button } from '@/components/ui/button'
import { Input } from '@/components/ui/input'
import { Label } from '@/components/ui/label'

import { BillingAdminApi } from '../../../lib/billingAdmin'
import BillingNoApp from './BillingNoApp'

const KNOWN_EVENTS = [
  'INITIAL_PURCHASE',
  'RENEWAL',
  'CANCELLATION',
  'UNCANCELLATION',
  'NON_RENEWING_PURCHASE',
  'EXPIRATION',
  'BILLING_ISSUE',
  'PRODUCT_CHANGE',
  'TRANSFER',
  'SUBSCRIPTION_PAUSED',
  'REFUND',
]

import { useOutletContext } from 'react-router';

export default function WebhooksPage() {
  const { appId } = useOutletContext();
  const [url, setUrl] = useState('')
  const [secret, setSecret] = useState('')
  const [events, setEvents] = useState([])
  const [status, setStatus] = useState(null)
  const [error, setError] = useState(null)
  const [busy, setBusy] = useState(false)

  if (appId === null) {
    return <BillingNoApp />
  }

  const toggle = (eventName) => {
    setEvents((current) =>
      current.includes(eventName)
        ? current.filter((value) => value !== eventName)
        : [...current, eventName],
    )
  }

  const submit = async (event) => {
    event.preventDefault()
    setBusy(true)
    setError(null)
    setStatus(null)

    try {
      const response = await BillingAdminApi.addWebhookEndpoint({
        app_id: appId,
        events,
        secret,
        url,
      })
      setStatus(`Registered endpoint #${response.id}.`)
      setUrl('')
      setSecret('')
      setEvents([])
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err))
    } finally {
      setBusy(false)
    }
  }

  return (
    <section className="rounded-md border border-border bg-card p-3">
      <h3 className="font-serif text-[20px] font-medium tracking-tight">
        Add outbound webhook endpoint
      </h3>
      <p className="mt-1 max-w-2xl text-[12.5px] leading-relaxed text-muted-foreground">
        Stackhouse signs each delivery with
        <code className="mx-0.5 rounded bg-muted px-0.5.5 py-0.5 text-[11px]">
          X-StackhouseBilling-Signature
        </code>
        and retries failed deliveries with exponential backoff.
      </p>
      <form className="mt-3 grid grid-cols-1 gap-2" onSubmit={submit}>
        <div>
          <Label htmlFor="billing-webhook-url">URL</Label>
          <Input
            id="billing-webhook-url"
            onChange={(event) => setUrl(event.target.value)}
            placeholder="https://example.com/stackhouse/billing"
            required
            type="url"
            value={url}
          />
        </div>
        <div>
          <Label htmlFor="billing-webhook-secret">Shared secret</Label>
          <Input
            id="billing-webhook-secret"
            onChange={(event) => setSecret(event.target.value)}
            required
            type="password"
            value={secret}
          />
        </div>
        <div>
          <Label>Event filter</Label>
          <div className="mt-1 flex flex-wrap gap-1">
            {KNOWN_EVENTS.map((eventName) => (
              <button
                key={eventName}
                className={`rounded-full border px-2 py-0.5 text-[12px] transition-colors ${
                  events.includes(eventName)
                    ? 'border-primary bg-primary/5 text-foreground'
                    : 'border-border bg-background text-muted-foreground hover:bg-muted hover:text-foreground'
                }`}
                onClick={() => toggle(eventName)}
                type="button"
              >
                {eventName}
              </button>
            ))}
          </div>
        </div>
        <div>
          <Button disabled={busy} type="submit">
            {busy ? 'Saving...' : 'Register endpoint'}
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

