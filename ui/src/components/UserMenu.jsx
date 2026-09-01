import { useAuth } from '../context/AuthContext';
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuSeparator,
  DropdownMenuTrigger,
} from '@/components/ui/dropdown-menu';
import { User, Settings, Shield, LogOut, ChevronsUpDown } from 'lucide-react';

function getInitials(email) {
  if (!email) return '??';
  const parts = email.split('@')[0].split(/[._-]+/);
  if (parts.length > 1) {
    return (parts[0][0] + parts[1][0]).toUpperCase();
  }
  return email.slice(0, 2).toUpperCase();
}

export default function UserMenu({ onNavigate }) {
  const { user, logout, isAdmin } = useAuth();

  if (!user) return null;

  const initials = getInitials(user.email);
  const displayName = user.metadata?.display_name || user.email.split('@')[0];

  const go = (tab) => {
    if (typeof onNavigate === 'function') onNavigate(tab);
    // Also dispatch for any external listeners that rely on the event.
    window.dispatchEvent(new CustomEvent('stackhouse-navigate', { detail: { tab } }));
  };

  return (
    <div className="mt-1 w-full min-w-0 group-data-[collapsible=icon]:hidden">
      <DropdownMenu>
        <DropdownMenuTrigger asChild>
          <button className="flex w-full min-w-0 items-center gap-1 rounded-md border border-sidebar-border bg-background/60 px-1 py-0.5.5 text-left transition-colors hover:bg-muted/50">
            <div className="flex h-6 w-6 shrink-0 items-center justify-center rounded-full bg-secondary text-[11px] font-semibold uppercase tracking-tight text-foreground shadow-none dark:shadow-none">
              {initials}
            </div>
            <div className="flex min-w-0 flex-1 flex-col leading-tight">
              <span className="truncate text-[12px] font-medium text-foreground tracking-tight">{displayName}</span>
              <span className="truncate text-[10px] text-muted-foreground">{user.email}</span>
            </div>
            <ChevronsUpDown className="h-2.5 w-2.5 text-muted-foreground shrink-0" />
          </button>
        </DropdownMenuTrigger>
        <DropdownMenuContent side="top" align="start" className="w-40 p-0.5.5">
          {/* Header card inside menu */}
          <div className="flex items-center gap-1.5 rounded-md px-1 py-1 mb-0.5">
            <div className="flex h-6 w-6 shrink-0 items-center justify-center rounded-full bg-primary text-primary-foreground text-[11px] font-semibold">
              {initials}
            </div>
            <div className="min-w-0 flex-1">
              <p className="truncate text-[13px] font-medium">{displayName}</p>
              <p className="truncate text-[11px] text-muted-foreground">{user.email}</p>
            </div>
          </div>
          <DropdownMenuSeparator />
          <DropdownMenuItem onClick={() => go('settings')} className="gap-1 text-[12.5px]">
            <User className="h-2.5 w-2.5" /> Profile
          </DropdownMenuItem>
          <DropdownMenuItem onClick={() => go('settings')} className="gap-1 text-[12.5px]">
            <Settings className="h-2.5 w-2.5" /> Settings
          </DropdownMenuItem>
          {isAdmin() && (
            <DropdownMenuItem onClick={() => go('admin')} className="gap-1 text-[12.5px]">
              <Shield className="h-2.5 w-2.5" /> Admin Center
            </DropdownMenuItem>
          )}
          <DropdownMenuSeparator />
          <DropdownMenuItem onClick={logout} className="gap-1 text-[12.5px] text-destructive focus:text-destructive focus:bg-destructive/10">
            <LogOut className="h-2.5 w-2.5" /> Sign out
          </DropdownMenuItem>
        </DropdownMenuContent>
      </DropdownMenu>
    </div>
  );
}
