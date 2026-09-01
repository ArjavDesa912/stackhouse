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

export default function ExperimentsPage() {
  const { appId } = useOutletContext()
  const [experiments, setExperiments] = useState([])
  const [offerings, setOfferings] = useState([])
  const [audiences, setAudiences] = useState([])
  const [identifier, setIdentifier] = useState('')
  const [metric, setMetric] = useState('purchase')
  const [audienceId, setAudienceId] = useState('')
  const [variants, setVariants] = useState([
    { identifier: 'control', offering_id: '', is_control: true, traffic_weight: 50 },
    { identifier: 'treatment', offering_id: '', is_control: false, traffic_weight: 50 },
  ])
  const [busy, setBusy] = useState(false)
  const [error, setError] = useState(null)
  const [results, setResults] = useState({})

  useEffect(() => {
    if (appId === null) {
      setExperiments([])
      setOfferings([])
      setAudiences([])
      return
    }
    let cancelled = false
    Promise.all([
      BillingAdminApi.listExperiments(appId),
      BillingAdminApi.listOfferings(appId),
      BillingAdminApi.listAudiences(appId),
    ])
      .then(([expRows, offRows, audRows]) => {
        if (!cancelled) {
          setExperiments(expRows)
          setOfferings(offRows)
          setAudiences(audRows)
        }
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
        metric,
        audience_id: audienceId === '' ? undefined : Number(audienceId),
        variants: variants.map((v) => ({
          ...v,
          offering_id: Number(v.offering_id),
          traffic_weight: Number(v.traffic_weight),
        })),
      }
      await BillingAdminApi.upsertExperiment(body)
      setExperiments(await BillingAdminApi.listExperiments(appId))
      setIdentifier('')
      setMetric('purchase')
      setAudienceId('')
      setVariants([
        { identifier: 'control', offering_id: '', is_control: true, traffic_weight: 50 },
        { identifier: 'treatment', offering_id: '', is_control: false, traffic_weight: 50 },
      ])
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err))
    } finally {
      setBusy(false)
    }
  }

  const updateVariant = (index, key, value) => {
    setVariants((current) =>
      current.map((row, i) => (i === index ? { ...row, [key]: value } : row)),
    )
  }

  const setStatus = async (experimentId, status) => {
    setBusy(true)
    setError(null)
    try {
      await BillingAdminApi.updateExperimentStatus(experimentId, status)
      setExperiments(await BillingAdminApi.listExperiments(appId))
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err))
    } finally {
      setBusy(false)
    }
  }

  const loadResults = async (experimentId) => {
    try {
      const rows = await BillingAdminApi.getExperimentResults(experimentId)
      setResults((current) => ({ ...current, [experimentId]: rows }))
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err))
    }
  }

  return (
    <div className="space-y-3">
      <section className="rounded-md border border-border bg-card p-3">
        <h3 className="font-serif text-[20px] font-medium tracking-tight">
          Create / update experiment
        </h3>
        <p className="mt-1 max-w-2xl text-[12.5px] leading-relaxed text-muted-foreground">
          Experiments assign customers to offering variants. Weights must sum to 100
          and exactly one variant must be marked control.
        </p>
        <form className="mt-3 space-y-3" onSubmit={submit}>
          <div className="grid grid-cols-1 gap-2 md:grid-cols-3">
            <div>
              <Label htmlFor="exp-identifier">Identifier</Label>
              <Input
                id="exp-identifier"
                onChange={(e) => setIdentifier(e.target.value)}
                required
                value={identifier}
              />
            </div>
            <div>
              <Label htmlFor="exp-metric">Metric</Label>
              <Input
                id="exp-metric"
                onChange={(e) => setMetric(e.target.value)}
                value={metric}
              />
            </div>
            <div>
              <Label htmlFor="exp-audience">Audience (optional)</Label>
              <SelectField
                id="exp-audience"
                onChange={(e) => setAudienceId(e.target.value)}
                value={audienceId}
              >
                <option value="">All users</option>
                {audiences.map((a) => (
                  <option key={a.id} value={a.id}>
                    {a.display_name || a.identifier}
                  </option>
                ))}
              </SelectField>
            </div>
          </div>

          <div className="space-y-1">
            <Label>Variants</Label>
            {variants.map((variant, index) => (
              <div
                key={index}
                className="grid grid-cols-1 gap-1 md:grid-cols-6"
              >
                <Input
                  onChange={(e) => updateVariant(index, 'identifier', e.target.value)}
                  placeholder="control"
                  value={variant.identifier}
                />
                <SelectField
                  onChange={(e) => updateVariant(index, 'offering_id', e.target.value)}
                  value={variant.offering_id}
                >
                  <option value="">- offering -</option>
                  {offerings.map((o) => (
                    <option key={o.id} value={o.id}>
                      {o.identifier}
                    </option>
                  ))}
                </SelectField>
                <div className="flex items-center gap-1 text-[12px]">
                  <input
                    checked={variant.is_control}
                    onChange={(e) =>
                      setVariants((current) =>
                        current.map((row, i) =>
                          i === index ? { ...row, is_control: e.target.checked } : row,
                        ),
                      )
                    }
                    type="checkbox"
                  />
                  Control
                </div>
                <Input
                  onChange={(e) =>
                    updateVariant(index, 'traffic_weight', Number(e.target.value))
                  }
                  placeholder="50"
                  type="number"
                  value={variant.traffic_weight}
                />
                <Button
                  onClick={() => setVariants((current) => current.filter((_, i) => i !== index))}
                  type="button"
                  variant="outline"
                >
                  Remove
                </Button>
              </div>
            ))}
            <Button
              onClick={() =>
                setVariants((current) => [
                  ...current,
                  { identifier: '', offering_id: '', is_control: false, traffic_weight: 0 },
                ])
              }
              type="button"
              variant="outline"
            >
              Add variant
            </Button>
          </div>

          <div>
            <Button disabled={busy} type="submit">
              {busy ? 'Saving...' : 'Save experiment'}
            </Button>
            {error ? (
              <span className="ml-2 text-[12px] text-destructive">{error}</span>
            ) : null}
          </div>
        </form>
      </section>

      <section className="overflow-hidden rounded-md border border-border bg-card">
        <div className="flex items-center justify-between px-3 py-3">
          <h3 className="font-serif text-[20px] font-medium tracking-tight">Experiments</h3>
          <span className="text-[11px] text-muted-foreground">{experiments.length}</span>
        </div>
        <div className="divide-y divide-border">
          {experiments.map((exp) => (
            <div key={exp.experiment.id} className="px-3 py-3">
              <div className="flex items-center gap-2">
                <span className="text-[13px] font-medium">{exp.experiment.identifier}</span>
                <span className="rounded-full bg-primary/10 px-1.5 py-0.5 text-[10px] text-primary">
                  {exp.experiment.status}
                </span>
                <span className="ml-auto text-[11px] text-muted-foreground">
                  metric: {exp.experiment.metric || '-'}
                </span>
              </div>
              <div className="mt-2 flex flex-wrap gap-1 text-[12px]">
                {exp.variants.map((v) => (
                  <span
                    key={v.id}
                    className="rounded-md border border-border bg-background px-2 py-0.5"
                  >
                    {v.identifier} ({v.traffic_weight}%)
                    {v.is_control ? ' · control' : ''}
                  </span>
                ))}
              </div>
              <div className="mt-2 flex flex-wrap gap-2">
                {exp.experiment.status === 'draft' ? (
                  <Button onClick={() => setStatus(exp.experiment.id, 'running')} size="sm" variant="outline">
                    Start
                  </Button>
                ) : null}
                {exp.experiment.status === 'running' ? (
                  <Button onClick={() => setStatus(exp.experiment.id, 'paused')} size="sm" variant="outline">
                    Pause
                  </Button>
                ) : null}
                {exp.experiment.status !== 'completed' ? (
                  <Button onClick={() => setStatus(exp.experiment.id, 'completed')} size="sm" variant="outline">
                    Complete
                  </Button>
                ) : null}
                <Button onClick={() => loadResults(exp.experiment.id)} size="sm" variant="outline">
                  Show results
                </Button>
              </div>

              {results[exp.experiment.id] ? (
                <div className="mt-2 overflow-hidden rounded-md border border-border bg-background">
                  <table className="w-full text-[12px]">
                    <thead className="bg-muted text-muted-foreground">
                      <tr>
                        <th className="px-2 py-1 text-left">Variant</th>
                        <th className="px-2 py-1 text-left">Impressions</th>
                        <th className="px-2 py-1 text-left">Conversions</th>
                        <th className="px-2 py-1 text-left">Rate</th>
                        <th className="px-2 py-1 text-left">z-score</th>
                      </tr>
                    </thead>
                    <tbody>
                      {results[exp.experiment.id].map((row) => (
                        <tr key={row.variant_id}>
                          <td className="px-2 py-1">{row.identifier}</td>
                          <td className="px-2 py-1">{row.impressions}</td>
                          <td className="px-2 py-1">{row.conversions}</td>
                          <td className="px-2 py-1">{(row.conversion_rate * 100).toFixed(1)}%</td>
                          <td className="px-2 py-1">
                            {row.z_score === null ? '-' : row.z_score.toFixed(2)}
                          </td>
                        </tr>
                      ))}
                    </tbody>
                  </table>
                </div>
              ) : null}
            </div>
          ))}
          {experiments.length === 0 ? (
            <div className="px-3 py-3 text-center text-[12px] text-muted-foreground">
              No experiments yet.
            </div>
          ) : null}
        </div>
      </section>
    </div>
  )
}
