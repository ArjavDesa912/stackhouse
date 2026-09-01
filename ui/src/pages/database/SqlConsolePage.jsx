import { Suspense, lazy } from 'react';
import { PageHeader } from '@/components/data-display/PageHeader';
import { Badge } from '@/components/data-display/Badge';
import { useTablesMetadata } from '@/hooks/useTablesMetadata';
import { PageSuspense } from '@/components/feedback/PageSuspense';

const SqlConsole = lazy(() => import('@/components/SqlConsole'));

export default function SqlConsolePage() {
  const { tables, loading } = useTablesMetadata();
  const totalColumns = tables.reduce((sum, t) => sum + (t.columns?.length || 0), 0);

  return (
    <div className="flex h-full flex-col">
      <PageHeader
        eyebrow="Data Studio"
        title="SQL Console"
        description="Compose, explain and run SQL against your warehouse. History and saved snippets persist to the workspace."
        actions={
          <div className="flex items-center gap-1">
            <Badge tone="neutral">{tables.length} tables</Badge>
            <Badge tone="neutral">{totalColumns} columns</Badge>
            <Badge tone="brand" dot>live</Badge>
          </div>
        }
      />
      <div className="flex-1 overflow-hidden">
        <Suspense fallback={<PageSuspense label="Loading SQL console…" />}>
          <SqlConsole tables={tables} />
        </Suspense>
        {loading ? null : null}
      </div>
    </div>
  );
}
