import React from 'react';
import { Button } from '@/components/ui/button';
import { Row, SectionHeader } from './components';

export default function WorkspaceSettings() {
  return (
    <section>
      <SectionHeader
        eyebrow="Workspace"
        title="Canvas defaults"
        description="Govern how new worksheets and agent sessions initialize."
      />
      <Row title="Default chart type" description="Applied when the worksheet cannot infer a sensible default.">
        <select className="h-8 rounded-md border border-border bg-card px-2 text-[12px] text-foreground outline-none focus-visible:ring-2 focus-visible:ring-ring/40 sm:w-[240px]">
          <option>Auto</option><option>Bar</option><option>Line</option><option>Area</option><option>Pie</option>
        </select>
      </Row>
      <Row title="Grid lines" description="Display faint cartesian grids behind charts by default.">
        <Button variant="outline" size="sm" className="text-[12px]">On</Button>
      </Row>
      <Row title="Color palette" description="Five-step palette used for categorical encodings.">
        <div className="flex items-center justify-end gap-0.5.5">
          {['var(--chart-1)', 'var(--chart-2)', 'var(--chart-3)', 'var(--chart-4)', 'var(--chart-5)'].map((c) => (
            <span key={c} className="h-4 w-4 rounded-full border border-border shadow-none" style={{ background: c }} />
          ))}
        </div>
      </Row>
      <Row title="Auto-save worksheets" description="Drafts persist to local storage and the connected workspace.">
        <Button variant="outline" size="sm" className="text-[12px]">Every 30s</Button>
      </Row>
    </section>
  );
}
