import React from 'react'
import { Activity } from 'lucide-react'
import { EmptyBlock } from './utils'

export function ExplainView({ model }) {
  if (!model || model.kind !== 'explain') {
    return (
      <EmptyBlock
        icon={Activity}
        title="No plan available"
        description="Click Explain to inspect how the current query will be executed."
      />
    )
  }
  if (!model.plan || model.plan.length === 0) {
    return (
      <EmptyBlock
        icon={Activity}
        title="Empty plan"
        description="The query planner returned no steps."
      />
    )
  }
  return (
    <div className="space-y-1 p-3">
      {model.plan.map((step, index) => (
        <div
          key={index}
          className="rounded-md border border-border bg-background p-2 ring-1 ring-border"
        >
          <div className="mb-0.5.5 flex items-center justify-between gap-1">
            <span className="inline-flex items-center gap-0.5.5 rounded-full bg-primary/10 px-1 py-0.5 text-[10px] font-medium uppercase tracking-[0.1em] text-primary">
              Step {index + 1}
            </span>
          </div>
          <p className="font-mono text-[12px] leading-relaxed text-foreground whitespace-pre-wrap">
            {step.detail}
          </p>
        </div>
      ))}
    </div>
  )
}
