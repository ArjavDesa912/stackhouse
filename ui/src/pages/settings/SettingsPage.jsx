import { Suspense, lazy } from 'react';
import { PageHeader } from '@/components/data-display/PageHeader';
import { PageSuspense } from '@/components/feedback/PageSuspense';

const SettingsView = lazy(() => import('@/components/SettingsView'));

export default function SettingsPage() {
  return (
    <div className="flex h-full flex-col">
      <PageHeader
        eyebrow="Operate"
        title="Settings"
        description="Workspace, security, integrations, and team — all in one place."
      />
      <div className="flex-1 overflow-hidden">
        <Suspense fallback={<PageSuspense label="Loading settings…" />}>
          <SettingsView />
        </Suspense>
      </div>
    </div>
  );
}
