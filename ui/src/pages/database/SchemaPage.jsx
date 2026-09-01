import { Suspense, lazy } from 'react';
import { Database } from 'lucide-react';
import { PageHeader } from '@/components/data-display/PageHeader';
import { Button } from '@/components/ui/button';
import { useTablesMetadata } from '@/hooks/useTablesMetadata';
import { PageSuspense } from '@/components/feedback/PageSuspense';

const SchemaManager = lazy(() => import('@/components/SchemaManager'));

export default function SchemaPage() {
  const { tables, refresh } = useTablesMetadata();

  return (
    <div className="flex h-full flex-col">
      <PageHeader
        eyebrow="Data Studio"
        title="Schema"
        description="Browse catalogs, design tables, model relationships and import data. All operations are audited."
        actions={
          <Button variant="outline" size="sm" onClick={refresh}>
            <Database className="h-2.5 w-2.5" /> Refresh
          </Button>
        }
      />
      <div className="flex-1 overflow-hidden">
        <Suspense fallback={<PageSuspense label="Loading schema…" />}>
          <SchemaManager tables={tables} onRefresh={refresh} />
        </Suspense>
      </div>
    </div>
  );
}
