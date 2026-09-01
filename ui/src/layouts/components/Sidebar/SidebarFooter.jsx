import { useState } from 'react';
import { useNavigate } from 'react-router';
import { Sun, Moon, Monitor, LogOut, User as UserIcon, ChevronUp } from 'lucide-react';
import { useTheme } from '@/app/ThemeProvider';
import { useAuth } from '@/context/AuthContext';
import { Popover, PopoverContent, PopoverTrigger } from '@/components/ui/popover';
import { cn } from '@/lib/utils';

const THEMES = [
  { value: 'light', label: 'Light', Icon: Sun },
  { value: 'dark', label: 'Dark', Icon: Moon },
  { value: 'system', label: 'System', Icon: Monitor },
];

function ThemeToggle({ collapsed }) {
  const { theme, setTheme } = useTheme();
  if (collapsed) {
    const next = theme === 'light' ? 'dark' : theme === 'dark' ? 'system' : 'light';
    const Current = THEMES.find((t) => t.value === theme)?.Icon || Sun;
    return (
      <button
        onClick={() => setTheme(next)}
        className="flex h-6 w-full items-center justify-center rounded-md text-muted-foreground hover:bg-muted hover:text-foreground"
        aria-label={`Theme: ${theme}`}
      >
        <Current className="h-3 w-3" />
      </button>
    );
  }
  return (
    <div className="flex items-center gap-0.5 rounded-md border border-border bg-background p-0.5">
      {THEMES.map(({ value, label, Icon }) => (
        <button
          key={value}
          onClick={() => setTheme(value)}
          className={cn(
            'flex flex-1 items-center justify-center gap-0.5 rounded-sm px-1 py-0.5 text-caption transition-colors',
            theme === value
              ? 'bg-card text-foreground shadow-none'
              : 'text-muted-foreground/80 hover:text-foreground',
          )}
          aria-label={label}
          title={label}
        >
          <Icon className="h-2 w-2" />
        </button>
      ))}
    </div>
  );
}

function UserMenuTrigger({ collapsed }) {
  const { user, logout } = useAuth();
  const navigate = useNavigate();
  const [open, setOpen] = useState(false);
  const initials = (user?.email || 'U').slice(0, 2).toUpperCase();
  const name = user?.metadata?.full_name || user?.email || 'You';

  return (
    <Popover open={open} onOpenChange={setOpen}>
      <PopoverTrigger asChild>
        <button
          className={cn(
            'flex w-full items-center gap-1 rounded-md px-1 py-0.5.5 transition-colors hover:bg-muted',
            collapsed && 'justify-center px-0',
          )}
        >
          <span className="flex h-6 w-6 shrink-0 items-center justify-center rounded-full bg-primary/10 text-primary text-caption font-medium">
            {initials}
          </span>
          {!collapsed && (
            <>
              <span className="flex min-w-0 flex-col text-left">
                <span className="truncate text-body-sm font-medium text-foreground">{name}</span>
                <span className="truncate text-caption text-muted-foreground/80">{user?.email}</span>
              </span>
              <ChevronUp className="ml-auto h-2.5 w-2.5 text-muted-foreground/80" />
            </>
          )}
        </button>
      </PopoverTrigger>
      <PopoverContent align="end" side="top" className="w-40">
        <div className="border-b border-border px-1 py-1">
          <p className="text-caption-mono">Account</p>
          <p className="mt-0.5 truncate text-body-sm text-foreground">{name}</p>
        </div>
        <div className="flex flex-col py-0.5">
          <button
            className="flex items-center gap-1 rounded-sm px-1 py-0.5.5 text-body-sm text-foreground hover:bg-muted"
            onClick={() => {
              setOpen(false);
              navigate('/settings');
            }}
          >
            <UserIcon className="h-2.5 w-2.5" /> Profile & settings
          </button>
          <button
            className="flex items-center gap-1 rounded-sm px-1 py-0.5.5 text-body-sm text-destructive hover:bg-destructive/10"
            onClick={logout}
          >
            <LogOut className="h-2.5 w-2.5" /> Sign out
          </button>
        </div>
      </PopoverContent>
    </Popover>
  );
}

export function SidebarFooter({ collapsed }) {
  return (
    <div className="border-t border-border px-1 py-1 flex flex-col gap-1">
      <ThemeToggle collapsed={collapsed} />
      <UserMenuTrigger collapsed={collapsed} />
    </div>
  );
}
