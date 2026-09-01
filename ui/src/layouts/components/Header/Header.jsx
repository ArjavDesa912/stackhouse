import { Link, useLocation } from 'react-router';
import { ChevronRight, Sparkles, Menu } from 'lucide-react';
import { useUiStore } from '@/stores/uiStore';
import { Kbd } from '@/components/data-display/KbdShortcut';
import { Badge } from '@/components/data-display/Badge';
import { NotificationCenter } from '@/layouts/components/NotificationCenter';
import { cn } from '@/lib/utils';

const ROUTE_TITLES = {
  '/': 'Overview',
  '/sql': 'SQL Console',
  '/schema': 'Schema',
  '/worksheet': 'Worksheet',
  '/analytics': 'Analytics',
  '/settings': 'Settings',
  '/admin': 'Admin Center',
  '/admin/billing': 'Billing',
};

function buildCrumbs(pathname) {
  if (pathname === '/') return [{ label: 'Overview', to: '/' }];
  const parts = pathname.split('/').filter(Boolean);
  const crumbs = [{ label: 'Workspace', to: '/' }];
  let acc = '';
  parts.forEach((p) => {
    acc += `/${p}`;
    crumbs.push({ label: ROUTE_TITLES[acc] || p[0].toUpperCase() + p.slice(1), to: acc });
  });
  return crumbs;
}

export function Header({ rightSlot }) {
  const { pathname } = useLocation();
  const crumbs = buildCrumbs(pathname);
  const setCommandOpen = useUiStore((s) => s.setCommandOpen);

  return (
    <header
      className={cn(
        'sticky top-0 z-20 flex h-10 shrink-0 items-center justify-between gap-3',
        'border-b border-border bg-card/85 px-3 backdrop-blur-md',
      )}
    >
      <button
        className="flex h-6 w-6 items-center justify-center rounded-md text-muted-foreground hover:bg-muted hover:text-foreground md:hidden"
        aria-label="Open navigation"
        onClick={() => window.dispatchEvent(new CustomEvent('stackhouse-mobile-sidebar-open'))}
      >
        <Menu className="h-3 w-3" />
      </button>
      <nav className="flex min-w-0 items-center gap-0.5.5" aria-label="Breadcrumb">
        {crumbs.map((c, i) => {
          const last = i === crumbs.length - 1;
          return (
            <span key={c.to} className="flex items-center gap-0.5.5">
              {i > 0 && <ChevronRight className="h-2.5 w-2.5 text-muted-foreground/80" />}
              {last ? (
                <span className="text-body-sm font-medium text-foreground">{c.label}</span>
              ) : (
                <Link
                  to={c.to}
                  className="text-body-sm text-muted-foreground hover:text-foreground"
                >
                  {c.label}
                </Link>
              )}
            </span>
          );
        })}
        <Badge tone="brand" dot className="ml-2 hidden lg:inline-flex">live</Badge>
      </nav>

      <div className="flex shrink-0 items-center gap-1">
        {rightSlot}
        <button
          onClick={() => setCommandOpen(true)}
          className="hidden h-6 items-center gap-0.5.5 rounded-md border border-border bg-background px-1 text-body-sm text-muted-foreground transition-colors hover:border-input hover:text-foreground sm:flex"
        >
          <Sparkles className="h-2.5 w-2.5 text-primary" />
          <span>Ask AI</span>
          <Kbd>⌘K</Kbd>
        </button>
        <NotificationCenter />
      </div>
    </header>
  );
}
