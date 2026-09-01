import { Command as CommandPrimitive } from 'cmdk';
import { Search } from 'lucide-react';
import { Dialog, DialogContent } from '@/components/ui/dialog';
import { cn } from '@/lib/utils';

export function Command({ className, ...props }) {
  return (
    <CommandPrimitive
      className={cn(
        'flex h-full w-full flex-col overflow-hidden rounded-md bg-card text-foreground',
        className,
      )}
      {...props}
    />
  );
}

export function CommandDialog({ open, onOpenChange, children, label = 'Command Palette' }) {
  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent className="overflow-hidden p-0 sm:max-w-[640px]">
        <Command label={label} className="[&_[cmdk-group-heading]]:px-2 [&_[cmdk-group-heading]]:py-1 [&_[cmdk-group-heading]]:text-caption-mono">
          {children}
        </Command>
      </DialogContent>
    </Dialog>
  );
}

export function CommandInput({ className, ...props }) {
  return (
    <div className="flex items-center gap-1 border-b border-border px-2" cmdk-input-wrapper="">
      <Search className="h-3 w-3 shrink-0 text-muted-foreground/80" />
      <CommandPrimitive.Input
        className={cn(
          'flex h-10 w-full rounded-md bg-transparent py-2 text-body-sm text-foreground placeholder:text-muted-foreground/80 outline-none',
          'disabled:cursor-not-allowed disabled:opacity-50',
          className,
        )}
        {...props}
      />
    </div>
  );
}

export function CommandList({ className, ...props }) {
  return (
    <CommandPrimitive.List
      className={cn('max-h-[440px] overflow-y-auto overflow-x-hidden p-0.5', className)}
      {...props}
    />
  );
}

export function CommandEmpty(props) {
  return <CommandPrimitive.Empty className="py-4 text-center text-body-sm text-muted-foreground/80" {...props} />;
}

export function CommandGroup({ className, ...props }) {
  return (
    <CommandPrimitive.Group
      className={cn('overflow-hidden p-0.5 text-foreground', className)}
      {...props}
    />
  );
}

export function CommandSeparator({ className, ...props }) {
  return (
    <CommandPrimitive.Separator className={cn('mx-0.5 h-px bg-border', className)} {...props} />
  );
}

export function CommandItem({ className, ...props }) {
  return (
    <CommandPrimitive.Item
      className={cn(
        'relative flex cursor-pointer select-none items-center gap-1 rounded-md px-1.5 py-1 text-body-sm text-foreground outline-none transition-colors',
        'data-[selected=true]:bg-muted data-[selected=true]:text-foreground',
        'aria-selected:bg-muted',
        'data-[disabled=true]:pointer-events-none data-[disabled=true]:opacity-50',
        className,
      )}
      {...props}
    />
  );
}

export function CommandShortcut({ className, ...props }) {
  return (
    <span
      className={cn('ml-auto text-caption-mono text-muted-foreground/80', className)}
      {...props}
    />
  );
}
