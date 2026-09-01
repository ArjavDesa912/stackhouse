import { cn } from '@/lib/utils';

export function Table({ className, ...props }) {
  return (
    <div className="relative w-full max-h-[60vh] overflow-auto">
      <table className={cn('w-full caption-bottom text-body-sm', className)} {...props} />
    </div>
  );
}

export function TableHeader({ className, ...props }) {
  return (
    <thead
      className={cn('sticky top-0 z-10 bg-background [&_tr]:border-b [&_tr]:border-border', className)}
      {...props}
    />
  );
}

export function TableBody({ className, ...props }) {
  return <tbody className={cn('[&_tr:last-child]:border-0', className)} {...props} />;
}

export function TableRow({ className, ...props }) {
  return (
    <tr
      className={cn(
        'border-b border-border transition-colors hover:bg-background data-[state=selected]:bg-primary/10',
        className,
      )}
      {...props}
    />
  );
}

export function TableHead({ className, ...props }) {
  return (
    <th
      className={cn(
        'h-8 px-2 text-left align-middle text-caption-mono font-medium',
        className,
      )}
      {...props}
    />
  );
}

export function TableCell({ className, ...props }) {
  return (
    <td
      className={cn('px-2 py-1.5 align-middle text-foreground', className)}
      {...props}
    />
  );
}

export function TableCaption({ className, ...props }) {
  return (
    <caption className={cn('mt-2 text-caption text-muted-foreground/80', className)} {...props} />
  );
}
