import { Suspense } from 'react';
import { RouterProvider } from 'react-router';
import { AppProviders } from './app/providers';
import { router } from './app/router';
import { PageSuspense } from '@/components/feedback/PageSuspense';
import { ErrorBoundary } from '@/components/feedback/ErrorBoundary';

export default function App() {
  return (
    <AppProviders>
      <ErrorBoundary>
        <Suspense fallback={<PageSuspense />}>
          <RouterProvider router={router} />
        </Suspense>
      </ErrorBoundary>
    </AppProviders>
  );
}
