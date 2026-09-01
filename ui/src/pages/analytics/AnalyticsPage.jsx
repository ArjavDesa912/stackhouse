import { Suspense, lazy } from 'react';
import { PageHeader } from '@/components/data-display/PageHeader';
import { PageSuspense } from '@/components/feedback/PageSuspense';
import { useTablesMetadata } from '@/hooks/useTablesMetadata';

const AnalysisView = lazy(() => import('@/components/AnalysisView'));

export default function AnalyticsPage() {
  const { tables } = useTablesMetadata();
  return (
    <div className="flex h-full flex-col">
      <PageHeader
        eyebrow="Data Studio"
        title="Analytics"
        description="Per-table statistics, distributions, and a live data dictionary across your warehouse."
      />
      <div className="flex-1 overflow-hidden">
        <Suspense fallback={<PageSuspense label="Loading analytics…" />}>
          <AnalysisView tables={tables} />
        </Suspense>
      </div>
    </div>
  );
}
