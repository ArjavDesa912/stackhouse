import { Suspense } from 'react';
import { Navigate, Outlet } from 'react-router';
import { useAuth } from '@/context/AuthContext';
import { ErrorBoundary } from '@/components/feedback/ErrorBoundary';
import { PageSuspense } from '@/components/feedback/PageSuspense';

export default function AuthLayout() {
  const { isAuthenticated, isLoading } = useAuth();
  if (!isLoading && isAuthenticated) return <Navigate to="/" replace />;

  return (
    <div className="relative flex min-h-screen w-screen bg-background">
      <ErrorBoundary>
        <Suspense fallback={<PageSuspense />}>
          <Outlet />
        </Suspense>
      </ErrorBoundary>
    </div>
  );
}
