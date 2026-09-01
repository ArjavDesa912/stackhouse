import React from 'react';
import { NavLink, Outlet } from 'react-router';
import { useAuth } from '@/context/AuthContext';
import { Button } from '@/components/ui/button';
import {
  User, Palette, Layers, Bell, Keyboard,
  Database, Shield, Globe, LogOut
} from 'lucide-react';

const SECTIONS = [
  { id: 'general', path: '/settings/general', label: 'Account', icon: User },
  { id: 'appearance', path: '/settings/appearance', label: 'Appearance', icon: Palette },
  { id: 'workspace', path: '/settings/workspace', label: 'Workspace', icon: Layers },
  { id: 'notifications', path: '/settings/notifications', label: 'Notifications', icon: Bell },
  { id: 'keyboard', path: '/settings/keyboard', label: 'Keyboard', icon: Keyboard },
  { id: 'data', path: '/settings/data', label: 'Data & Storage', icon: Database },
  { id: 'security', path: '/settings/security', label: 'Security', icon: Shield },
  { id: 'about', path: '/settings/about', label: 'About', icon: Globe },
];

export default function SettingsView() {
  const { logout } = useAuth();

  return (
    <div className="flex h-full w-full overflow-hidden bg-background">
      {/* Sub-navigation rail */}
      <aside className="hidden w-40 shrink-0 border-r border-border bg-sidebar/50 lg:flex lg:flex-col">
        <div className="border-b border-border px-3 py-3">
          <p className="text-[10px] font-medium uppercase tracking-[0.14em] text-muted-foreground">Settings</p>
          <h3 className="mt-0.5 font-serif text-[20px] font-medium tracking-tight text-foreground">Preferences</h3>
        </div>
        <nav className="flex-1 overflow-y-auto p-1">
          {SECTIONS.map((s) => {
            const Icon = s.icon;
            return (
              <NavLink
                key={s.id}
                to={s.path}
                className={({ isActive }) => `group relative flex w-full items-center gap-1.5 rounded-md px-1.5 py-1 text-left text-[13px] tracking-tight transition-colors ${
                  isActive
                    ? 'bg-secondary text-foreground font-medium before:absolute before:left-0 before:top-2 before:bottom-2 before:w-[2px] before:rounded-full before:bg-primary'
                    : 'text-muted-foreground hover:text-foreground hover:bg-muted'
                }`}
              >
                {({ isActive }) => (
                  <>
                    <Icon className={`h-3 w-3 shrink-0 transition-colors ${isActive ? 'text-primary' : 'group-hover:text-foreground'}`} />
                    <span>{s.label}</span>
                  </>
                )}
              </NavLink>
            );
          })}
        </nav>
        <div className="border-t border-border p-2">
          <Button variant="outline" className="w-full justify-start gap-1 text-[12px]" onClick={logout}>
            <LogOut className="h-2.5 w-2.5" /> Sign out
          </Button>
        </div>
      </aside>

      {/* Main pane */}
      <div className="flex min-w-0 flex-1 flex-col overflow-hidden">
        <div className="flex-1 overflow-y-auto">
          <div className="mx-auto w-full max-w-3xl px-4 py-5 lg:px-5 lg:py-14">
            <Outlet />
          </div>
        </div>
      </div>
    </div>
  );
}
