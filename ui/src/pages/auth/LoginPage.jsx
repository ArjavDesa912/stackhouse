import { Suspense, lazy } from 'react';
import { PageSuspense } from '@/components/feedback/PageSuspense';

const AuthPage = lazy(() => import('@/components/AuthPage'));

export default function LoginPage() {
  return (
    <Suspense fallback={<PageSuspense />}>
      <AuthPage />
    </Suspense>
  );
}
