import { useEffect, useState } from 'react';
import { useLocation } from 'react-router';
import {
  LayoutDashboard, Database, Table as TableIcon,
  ChartLine, Settings, Shield, FileSpreadsheet, ChevronLeft, ChevronRight, CreditCard,
} from 'lucide-react';
import { useUiStore } from '@/stores/uiStore';
import { useAuth } from '@/context/AuthContext';
import { cn } from '@/lib/utils';
import { SidebarBrand } from './SidebarBrand';
import { SidebarSearch } from './SidebarSearch';
import { SidebarNavGroup, SidebarNavItem } from './SidebarNav';
import { SidebarFooter } from './SidebarFooter';

function SidebarBody({ collapsed }) {
  const { isAdmin } = useAuth();
  return (
    <>
      <div className="px-1 pt-2">
        <SidebarBrand collapsed={collapsed} />
      </div>
      <SidebarSearch collapsed={collapsed} />
      <nav className="flex-1 overflow-y-auto pb-1">
        <SidebarNavGroup label="Workspace" collapsed={collapsed}>
          <SidebarNavItem to="/" end icon={LayoutDashboard} label="Overview" collapsed={collapsed} />
        </SidebarNavGroup>
        <div className="mx-2 my-0.5 h-px bg-border" />
        <SidebarNavGroup label="Data Studio" collapsed={collapsed}>
          <SidebarNavItem to="/sql" icon={Database} label="SQL Console" collapsed={collapsed} />
          <SidebarNavItem to="/schema" icon={TableIcon} label="Schema" collapsed={collapsed} />
          <SidebarNavItem to="/data-processing" icon={Database} label="Data Processing" collapsed={collapsed} />
          <SidebarNavItem to="/worksheet" icon={FileSpreadsheet} label="Worksheet" collapsed={collapsed} />
          <SidebarNavItem to="/analytics" icon={ChartLine} label="Analytics" collapsed={collapsed} />
        </SidebarNavGroup>
        <div className="mx-2 my-0.5 h-px bg-border" />
        <SidebarNavGroup label="Operate" collapsed={collapsed}>
          <SidebarNavItem to="/billing" icon={CreditCard} label="Billing" collapsed={collapsed} />
          <SidebarNavItem to="/settings" icon={Settings} label="Settings" collapsed={collapsed} />
          {isAdmin?.() && (
            <SidebarNavItem to="/admin" icon={Shield} label="Admin Center" collapsed={collapsed} />
          )}
        </SidebarNavGroup>
      </nav>
      <SidebarFooter collapsed={collapsed} />
    </>
  );
}

export function Sidebar() {
  const collapsed = useUiStore((s) => s.sidebarCollapsed);
  const toggle = useUiStore((s) => s.toggleSidebar);
  const [mobileOpen, setMobileOpen] = useState(false);
  const { pathname } = useLocation();

  // Close mobile drawer on route change
  useEffect(() => { setMobileOpen(false); }, [pathname]);

  // Listen for global "open mobile sidebar" events from the Header hamburger
  useEffect(() => {
    const handler = () => setMobileOpen(true);
    window.addEventListener('stackhouse-mobile-sidebar-open', handler);
    return () => window.removeEventListener('stackhouse-mobile-sidebar-open', handler);
  }, []);

  // Close mobile drawer on Escape
  useEffect(() => {
    if (!mobileOpen) return undefined;
    const onKey = (e) => { if (e.key === 'Escape') setMobileOpen(false); };
    window.addEventListener('keydown', onKey);
    return () => window.removeEventListener('keydown', onKey);
  }, [mobileOpen]);

  return (
    <>
      {/* Desktop / tablet sidebar */}
      <aside
        className={cn(
          'relative z-30 hidden shrink-0 flex-col border-r border-border bg-background transition-[width] duration-200 ease-out md:flex',
          collapsed ? 'w-12' : 'w-40',
        )}
        aria-label="Primary navigation"
      >
        <SidebarBody collapsed={collapsed} />
        <button
          type="button"
          onClick={toggle}
          aria-label={collapsed ? 'Expand sidebar' : 'Collapse sidebar'}
          className="absolute -right-3 top-12 flex h-5 w-5 items-center justify-center rounded-full border border-border bg-card text-muted-foreground/80 shadow-none transition-colors hover:text-foreground"
        >
          {collapsed ? <ChevronRight className="h-2 w-2" /> : <ChevronLeft className="h-2 w-2" />}
        </button>
      </aside>

      {/* Mobile drawer */}
      {mobileOpen && (
        <div className="fixed inset-0 z-40 md:hidden" role="dialog" aria-modal="true" aria-label="Mobile navigation">
          <button
            type="button"
            aria-label="Close navigation"
            className="absolute inset-0 bg-foreground/50 backdrop-blur-sm"
            onClick={() => setMobileOpen(false)}
          />
          <aside
            className="absolute left-0 top-0 flex h-full w-40 flex-col border-r border-border bg-background shadow-none animate-in slide-in-from-left-2"
            aria-label="Primary navigation"
          >
            <SidebarBody collapsed={false} />
          </aside>
        </div>
      )}
    </>
  );
}
