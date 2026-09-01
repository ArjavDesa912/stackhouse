import { useNavigate } from 'react-router';
import {
  LayoutDashboard, Database, Table as TableIcon, FileSpreadsheet,
  ChartLine, Settings, Shield, Sun, Moon, Monitor, LogOut, Palette,
} from 'lucide-react';
import { useUiStore } from '@/stores/uiStore';
import { useShortcut } from '@/hooks/useKeyboard';
import { useTheme } from '@/app/ThemeProvider';
import { useAuth } from '@/context/AuthContext';
import {
  CommandDialog, CommandInput, CommandList, CommandEmpty,
  CommandGroup, CommandItem, CommandSeparator, CommandShortcut,
} from '@/components/ui/command';

export function CommandPalette() {
  const navigate = useNavigate();
  const open = useUiStore((s) => s.commandOpen);
  const setOpen = useUiStore((s) => s.setCommandOpen);
  const { setTheme } = useTheme();
  const { logout } = useAuth();

  useShortcut('mod+k', (e) => {
    e.preventDefault();
    useUiStore.getState().toggleCommand();
  });

  const go = (to) => {
    setOpen(false);
    navigate(to);
  };

  return (
    <CommandDialog open={open} onOpenChange={setOpen}>
      <CommandInput placeholder="Search pages, tables, queries…" />
      <CommandList>
        <CommandEmpty>No results.</CommandEmpty>
        <CommandGroup heading="Navigate">
          <CommandItem onSelect={() => go('/')}>
            <LayoutDashboard className="h-3 w-3 text-muted-foreground" /> Overview
            <CommandShortcut>G O</CommandShortcut>
          </CommandItem>
          <CommandItem onSelect={() => go('/sql')}>
            <Database className="h-3 w-3 text-muted-foreground" /> SQL Console
            <CommandShortcut>G S</CommandShortcut>
          </CommandItem>
          <CommandItem onSelect={() => go('/schema')}>
            <TableIcon className="h-3 w-3 text-muted-foreground" /> Schema
          </CommandItem>
          <CommandItem onSelect={() => go('/worksheet')}>
            <FileSpreadsheet className="h-3 w-3 text-muted-foreground" /> Worksheet
          </CommandItem>
          <CommandItem onSelect={() => go('/analytics')}>
            <ChartLine className="h-3 w-3 text-muted-foreground" /> Analytics
          </CommandItem>
          <CommandItem onSelect={() => go('/settings')}>
            <Settings className="h-3 w-3 text-muted-foreground" /> Settings
          </CommandItem>
          <CommandItem onSelect={() => go('/admin')}>
            <Shield className="h-3 w-3 text-muted-foreground" /> Admin Center
          </CommandItem>
          {import.meta.env.DEV && (
            <CommandItem onSelect={() => go('/__design')}>
              <Palette className="h-3 w-3 text-muted-foreground" /> Design Catalog
              <CommandShortcut>dev</CommandShortcut>
            </CommandItem>
          )}
        </CommandGroup>
        <CommandSeparator />
        <CommandSeparator />
        <CommandGroup heading="Theme">
          <CommandItem onSelect={() => { setTheme('light'); setOpen(false); }}>
            <Sun className="h-3 w-3 text-muted-foreground" /> Light mode
          </CommandItem>
          <CommandItem onSelect={() => { setTheme('dark'); setOpen(false); }}>
            <Moon className="h-3 w-3 text-muted-foreground" /> Dark mode
          </CommandItem>
          <CommandItem onSelect={() => { setTheme('system'); setOpen(false); }}>
            <Monitor className="h-3 w-3 text-muted-foreground" /> Match system
          </CommandItem>
        </CommandGroup>
        <CommandSeparator />
        <CommandGroup heading="Account">
          <CommandItem onSelect={() => { setOpen(false); logout(); }}>
            <LogOut className="h-3 w-3 text-destructive" /> Sign out
          </CommandItem>
        </CommandGroup>
      </CommandList>
    </CommandDialog>
  );
}
