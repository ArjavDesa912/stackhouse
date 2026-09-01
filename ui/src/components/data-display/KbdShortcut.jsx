import { cn } from '@/lib/utils';

export function Kbd({ children, className }) {
  return (
    <kbd
      className={cn(
        'inline-flex h-4 min-w-[20px] items-center justify-center rounded border border-border bg-muted px-0.5.5 font-mono text-[10px] font-medium text-muted-foreground',
        className,
      )}
    >
      {children}
    </kbd>
  );
}

export function ShortcutGroup({ keys, className }) {
  return (
    <span className={cn('inline-flex items-center gap-0.5', className)}>
      {keys.map((k, i) => (
        <Kbd key={`${k}-${i}`}>{k}</Kbd>
      ))}
    </span>
  );
}
