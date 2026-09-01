import * as TogglePrimitive from '@radix-ui/react-toggle';
import { cn } from '@/lib/utils';

export function Toggle({ className, ...props }) {
  return (
    <TogglePrimitive.Root
      className={cn(
        'inline-flex h-6 items-center justify-center gap-0.5.5 rounded-md px-1.5 text-body-sm font-medium text-muted-foreground',
        'transition-colors hover:bg-muted hover:text-foreground',
        'focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring/40',
        'data-[state=on]:bg-muted data-[state=on]:text-foreground',
        'disabled:pointer-events-none disabled:opacity-50',
        className,
      )}
      {...props}
    />
  );
}
