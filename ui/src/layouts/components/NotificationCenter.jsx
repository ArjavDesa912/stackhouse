import { useEffect } from 'react';
import { Bell, Inbox } from 'lucide-react';
import { Popover, PopoverContent, PopoverTrigger } from '@/components/ui/popover';
import { useNotificationStore } from '@/stores/notificationStore';
import { ScrollArea } from '@/components/ui/scroll-area';
import { cn } from '@/lib/utils';

function formatRelative(ts) {
  const diff = (Date.now() - ts) / 1000;
  if (diff < 60) return 'just now';
  if (diff < 3600) return `${Math.floor(diff / 60)}m ago`;
  if (diff < 86400) return `${Math.floor(diff / 3600)}h ago`;
  return new Date(ts).toLocaleDateString();
}

export function NotificationCenter() {
  const { events, unread, markAllRead } = useNotificationStore();

  useEffect(() => {
    // Hook can later subscribe to realtime channels here.
  }, []);

  return (
    <Popover onOpenChange={(o) => o && markAllRead()}>
      <PopoverTrigger asChild>
        <button
          className={cn(
            'relative flex h-6 w-6 items-center justify-center rounded-md text-muted-foreground/80 hover:bg-muted hover:text-foreground',
          )}
          aria-label="Notifications"
        >
          <Bell className="h-3 w-3" />
          {unread > 0 ? (
            <span className="absolute -right-0.5 -top-0.5 inline-flex h-3 min-w-[16px] items-center justify-center rounded-full bg-primary px-0.5 text-[9px] font-medium text-primary-foreground">
              {unread > 9 ? '9+' : unread}
            </span>
          ) : null}
        </button>
      </PopoverTrigger>
      <PopoverContent align="end" className="w-72 p-0">
        <div className="flex items-center justify-between border-b border-border px-2 py-1">
          <p className="text-caption-mono">Activity</p>
          <button
            disabled={!events.length}
            onClick={() => useNotificationStore.getState().clear()}
            className="text-caption text-muted-foreground/80 hover:text-foreground disabled:opacity-50"
          >
            Clear all
          </button>
        </div>
        <ScrollArea className="max-h-72">
          {events.length === 0 ? (
            <div className="flex flex-col items-center gap-1 px-2 py-5 text-center">
              <Inbox className="h-4 w-4 text-muted-foreground/80" />
              <p className="text-body-sm text-muted-foreground">You&rsquo;re all caught up.</p>
              <p className="text-caption text-muted-foreground/80">Real-time events will appear here.</p>
            </div>
          ) : (
            <ul className="divide-y divide-hairline">
              {events.map((e) => (
                <li key={e.id} className="px-2 py-1.5">
                  <p className="text-body-sm text-foreground">{e.title || e.event || 'Event'}</p>
                  {e.message ? <p className="text-caption text-muted-foreground">{e.message}</p> : null}
                  <p className="mt-0.5 text-caption text-muted-foreground/80">{formatRelative(e.ts)}</p>
                </li>
              ))}
            </ul>
          )}
        </ScrollArea>
      </PopoverContent>
    </Popover>
  );
}
