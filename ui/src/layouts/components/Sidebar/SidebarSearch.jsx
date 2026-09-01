import { Search } from 'lucide-react';
import { useUiStore } from '@/stores/uiStore';
import { Kbd } from '@/components/data-display/KbdShortcut';

export function SidebarSearch({ collapsed }) {
  const setCommandOpen = useUiStore((s) => s.setCommandOpen);
  if (collapsed) {
    return (
      <button
        onClick={() => setCommandOpen(true)}
        className="mx-auto my-1 flex h-8 w-8 items-center justify-center rounded-md text-muted-foreground/80 hover:bg-muted hover:text-foreground"
        aria-label="Open command palette"
      >
        <Search className="h-3 w-3" />
      </button>
    );
  }
  return (
    <button
      onClick={() => setCommandOpen(true)}
      className="group mx-1 my-1 flex items-center gap-1 rounded-md border border-border bg-background px-1.5 py-0.5.5 text-body-sm text-muted-foreground/80 transition-colors hover:border-input hover:text-muted-foreground"
    >
      <Search className="h-2.5 w-2.5" />
      <span className="flex-1 text-left">Quick jump…</span>
      <span className="flex items-center gap-0.5">
        <Kbd>⌘</Kbd>
        <Kbd>K</Kbd>
      </span>
    </button>
  );
}
