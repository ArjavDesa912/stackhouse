import { NavLink } from 'react-router';
import { cn } from '@/lib/utils';

export function SidebarNavItem({ to, icon: Icon, label, end, badge, collapsed }) {
  return (
    <NavLink
      to={to}
      end={end}
      className={({ isActive }) =>
        cn(
          'group/navitem relative flex h-8 items-center gap-1.5 rounded-md px-1.5 text-body-sm transition-all',
          'text-muted-foreground hover:bg-muted hover:text-foreground',
          isActive && 'bg-muted text-foreground font-medium',
          collapsed && 'justify-center px-0',
        )
      }
      title={collapsed ? label : undefined}
    >
      {({ isActive }) => (
        <>
          {isActive && !collapsed && (
            <span className="absolute left-0 top-2 bottom-2 w-[2px] rounded-full bg-primary" />
          )}
          <Icon
            className={cn(
              'h-3 w-3 shrink-0 transition-colors',
              isActive ? 'text-primary' : 'group-hover/navitem:text-foreground',
            )}
          />
          {!collapsed && (
            <>
              <span className="truncate tracking-tight">{label}</span>
              {badge ? (
                <span
                  className={cn(
                    'ml-auto text-caption-mono',
                    isActive ? 'text-primary' : 'text-muted-foreground/80',
                  )}
                >
                  {badge}
                </span>
              ) : null}
            </>
          )}
        </>
      )}
    </NavLink>
  );
}

export function SidebarNavGroup({ label, collapsed, children }) {
  return (
    <div className="py-0.5">
      {!collapsed && label ? (
        <p className="px-1.5 pb-0.5.5 text-caption-mono">{label}</p>
      ) : null}
      <div className="flex flex-col gap-0.5 px-0.5">{children}</div>
    </div>
  );
}
