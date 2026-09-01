import { Suspense, lazy } from 'react';
import { Navigate } from 'react-router';
import { PageHeader } from '@/components/data-display/PageHeader';
import { PageSuspense } from '@/components/feedback/PageSuspense';
import { useAuth } from '@/context/AuthContext';

const AdminCenter = lazy(() => import('@/components/AdminCenter'));

export default function AdminPage() {
  const { isAdmin } = useAuth();
  if (!isAdmin?.()) return <Navigate to="/" replace />;

  return (
    <div className="flex h-full flex-col">
      <PageHeader
        eyebrow="Operate"
        title="Admin Center"
        description="User management, roles, audit logs, system health, and billing."
      />
      <div className="flex-1 overflow-hidden">
        <Suspense fallback={<PageSuspense label="Loading admin center…" />}>
          <AdminCenter />
        </Suspense>
      </div>
    </div>
  );
}
