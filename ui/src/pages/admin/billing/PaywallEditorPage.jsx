import { useEffect, useState } from 'react'
import { useOutletContext } from 'react-router'

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

function defaultSection() {
  return { type: 'text', title: '', body: '' }
}

export default function PaywallEditorPage() {
  const { appId } = useOutletContext()
  const [offerings, setOfferings] = useState([])
  const [offeringId, setOfferingId] = useState('')
  const [template, setTemplate] = useState('default')
  const [sections, setSections] = useState([defaultSection()])
  const [paywall, setPaywall] = useState(null)
  const [busy, setBusy] = useState(false)
  const [error, setError] = useState(null)

  useEffect(() => {
    if (appId === null) {
      setOfferings([])
      return
    }
    let cancelled = false
    BillingAdminApi.listOfferings(appId)
      .then((rows) => {
        if (!cancelled) setOfferings(rows)
      })
      .catch((err) => {
        if (!cancelled) setError(err instanceof Error ? err.message : String(err))
      })
    return () => {
      cancelled = true
    }
  }, [appId])

  useEffect(() => {
    if (offeringId === '') {
      setPaywall(null)
      setSections([defaultSection()])
      return
    }
    let cancelled = false
    BillingAdminApi.getPaywall(Number(offeringId))
      .then((row) => {
        if (!cancelled) {
          if (row) {
            setPaywall(row)
            setTemplate(row.template || 'default')
            setSections(row.draft_config?.sections || row.config?.sections || [defaultSection()])
          } else {
            setPaywall(null)
            setTemplate('default')
            setSections([defaultSection()])
          }
        }
      })
      .catch((err) => {
        if (!cancelled) setError(err instanceof Error ? err.message : String(err))
      })
    return () => {
      cancelled = true
    }
  }, [offeringId])

  if (appId === null) {
    return <BillingNoApp />
  }

  const config = { template, sections }

  const saveDraft = async () => {
    if (offeringId === '') return
    setBusy(true)
    setError(null)
    try {
      const saved = await BillingAdminApi.upsertPaywall({
        offering_id: Number(offeringId),
        template,
        config: paywall?.config ?? config,
        draft_config: config,
      })
      setPaywall(saved)
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err))
    } finally {
      setBusy(false)
    }
  }

  const publish = async () => {
    if (offeringId === '') return
    setBusy(true)
    setError(null)
    try {
      await saveDraft()
      const saved = await BillingAdminApi.publishPaywall(Number(offeringId))
      setPaywall(saved)
      setSections(saved.config?.sections || sections)
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err))
    } finally {
      setBusy(false)
    }
  }

  const moveSection = (index, direction) => {
    setSections((current) => {
      const next = [...current]
      const target = index + direction
      if (target < 0 || target >= next.length) return current
      const temp = next[index]
      next[index] = next[target]
      next[target] = temp
      return next
    })
  }

  return (
    <div className="space-y-3">
      <section className="rounded-md border border-border bg-card p-3">
        <h3 className="font-serif text-[20px] font-medium tracking-tight">Paywall editor</h3>
        <p className="mt-1 max-w-2xl text-[12.5px] leading-relaxed text-muted-foreground">
          Build a per-offering paywall config. Save a draft and publish it when you are
          ready to serve it through the SDK.
        </p>

        <div className="mt-3 grid grid-cols-1 gap-2 md:grid-cols-2">
          <div>
            <Label htmlFor="paywall-offering">Offering</Label>
            <SelectField
              id="paywall-offering"
              onChange={(e) => setOfferingId(e.target.value)}
              value={offeringId}
            >
              <option value="">- select offering -</option>
              {offerings.map((o) => (
                <option key={o.id} value={o.id}>
                  {o.identifier}
                </option>
              ))}
            </SelectField>
          </div>
          <div>
            <Label htmlFor="paywall-template">Template</Label>
            <Input
              id="paywall-template"
              onChange={(e) => setTemplate(e.target.value)}
              value={template}
            />
          </div>
        </div>

        <div className="mt-3 space-y-2">
          <Label>Sections</Label>
          {sections.map((section, index) => (
            <div key={index} className="space-y-1 rounded-md border border-border bg-background p-2">
              <div className="grid grid-cols-1 gap-1 md:grid-cols-3">
                <SelectField
                  onChange={(e) =>
                    setSections((current) =>
                      current.map((row, i) => (i === index ? { ...row, type: e.target.value } : row)),
                    )
                  }
                  value={section.type}
                >
                  <option value="text">text</option>
                  <option value="feature">feature</option>
                  <option value="cta">cta</option>
                </SelectField>
                <Input
                  onChange={(e) =>
                    setSections((current) =>
                      current.map((row, i) => (i === index ? { ...row, title: e.target.value } : row)),
                    )
                  }
                  placeholder="Title"
                  value={section.title}
                />
                <div className="flex gap-1">
                  <Button
                    disabled={index === 0}
                    onClick={() => moveSection(index, -1)}
                    type="button"
                    variant="outline"
                  >
                    ↑
                  </Button>
                  <Button
                    disabled={index === sections.length - 1}
                    onClick={() => moveSection(index, 1)}
                    type="button"
                    variant="outline"
                  >
                    ↓
                  </Button>
                  <Button
                    onClick={() => setSections((current) => current.filter((_, i) => i !== index))}
                    type="button"
                    variant="outline"
                  >
                    Remove
                  </Button>
                </div>
              </div>
              <Input
                onChange={(e) =>
                  setSections((current) =>
                    current.map((row, i) => (i === index ? { ...row, body: e.target.value } : row)),
                  )
                }
                placeholder="Body text (or comma-separated features for feature blocks)"
                value={section.body}
              />
            </div>
          ))}
          <Button
            onClick={() => setSections((current) => [...current, defaultSection()])}
            type="button"
            variant="outline"
          >
            Add section
          </Button>
        </div>

        <div className="mt-3 flex flex-wrap gap-2">
          <Button disabled={busy || offeringId === ''} onClick={saveDraft}>
            {busy ? 'Saving...' : 'Save draft'}
          </Button>
          <Button disabled={busy || offeringId === ''} onClick={publish} variant="default">
            {busy ? 'Publishing...' : 'Publish'}
          </Button>
          {paywall?.is_published ? (
            <span className="rounded-full bg-primary/10 px-2 py-1 text-[11px] text-primary">
              Published
            </span>
          ) : paywall ? (
            <span className="rounded-full bg-muted px-2 py-1 text-[11px] text-muted-foreground">
              Draft
            </span>
          ) : null}
          {error ? (
            <span className="ml-2 text-[12px] text-destructive">{error}</span>
          ) : null}
        </div>
      </section>

      <section className="overflow-hidden rounded-md border border-border bg-card p-3">
        <h3 className="font-serif text-[20px] font-medium tracking-tight">Preview</h3>
        <div className="mt-2 rounded-md border border-border bg-background p-3">
          <h4 className="font-medium">{template}</h4>
          <div className="mt-2 space-y-2">
            {sections.map((section, index) => (
              <div key={index} className="text-[12px]">
                <strong>{section.title}</strong>
                <span className="ml-2 text-muted-foreground">({section.type})</span>
                <p className="mt-0.5 text-muted-foreground">{section.body}</p>
              </div>
            ))}
          </div>
        </div>
      </section>
    </div>
  )
}
