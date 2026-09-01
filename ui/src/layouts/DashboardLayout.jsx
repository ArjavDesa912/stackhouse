import { Suspense } from 'react';
import { Navigate, Outlet } from 'react-router';
import { useAuth } from '@/context/AuthContext';
import { Sidebar } from './components/Sidebar/Sidebar';
import { Header } from './components/Header/Header';
import { CommandPalette } from './components/CommandPalette';
import { Toaster } from '@/components/ui/toast';
import { ErrorBoundary } from '@/components/feedback/ErrorBoundary';
import { PageSuspense } from '@/components/feedback/PageSuspense';
import { Loader2 } from 'lucide-react';

export default function DashboardLayout() {
  const { isLoading, isAuthenticated } = useAuth();

  if (isLoading) {
    return (
      <div className="flex h-screen w-screen items-center justify-center bg-background">
        <div className="flex items-center gap-1 text-body-sm text-muted-foreground">
          <Loader2 className="h-3 w-3 animate-spin text-primary" />
          Loading…
        </div>
      </div>
    );
  }

  if (!isAuthenticated) {
    return <Navigate to="/login" replace />;
  }

  return (
    <div className="flex h-screen w-screen overflow-hidden bg-background text-foreground">
      <a
        href="#main-content"
        className="sr-only focus:not-sr-only focus:fixed focus:left-3 focus:top-3 focus:z-[100] focus:rounded-md focus:border focus:border-border focus:bg-card focus:px-2 focus:py-0.5.5 focus:text-body-sm focus:text-foreground focus:shadow-none"
      >
        Skip to main content
      </a>
      <Sidebar />
      <div className="flex min-w-0 flex-1 flex-col overflow-hidden">
        <Header />
        <main
          id="main-content"
          tabIndex={-1}
          className="relative flex-1 overflow-hidden bg-background focus:outline-none"
        >
          <ErrorBoundary>
            <Suspense fallback={<PageSuspense />}>
              <Outlet />
            </Suspense>
          </ErrorBoundary>
        </main>
      </div>
      <CommandPalette />
      <Toaster />
    </div>
  );
}
