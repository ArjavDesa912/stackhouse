import { Link } from 'react-router';
import { cn } from '@/lib/utils';

export function SidebarBrand({ collapsed, environment = 'dev' }) {
  return (
    <Link
      to="/"
      className={cn(
        'flex items-center gap-1.5 rounded-md px-1 py-1 transition-colors hover:bg-muted',
        collapsed && 'justify-center px-0',
      )}
    >
      <span className="relative flex h-6 w-6 shrink-0 items-center justify-center overflow-hidden rounded-md">
        <img src="/logo.svg" alt="Stackhouse" className="h-full w-full" />
        <span className="pointer-events-none absolute inset-0 rounded-md ring-1 ring-inset ring-ring/40" />
      </span>
      {!collapsed && (
        <div className="flex min-w-0 flex-col leading-tight">
          <span className="text-display-sm tracking-tight text-foreground">Stackhouse</span>
          <span className="text-caption-mono mt-0.5 inline-flex items-center gap-0.5.5">
            <span className="pulse-dot scale-[0.6]" />
            {environment}
          </span>
        </div>
      )}
    </Link>
  );
}
