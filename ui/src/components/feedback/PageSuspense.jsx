import { Loader2 } from 'lucide-react';

export function PageSuspense({ label = 'Loading…' }) {
  return (
    <div className="flex h-full w-full flex-1 items-center justify-center bg-background">
      <div className="flex items-center gap-1 text-body-sm text-muted-foreground">
        <Loader2 className="h-3 w-3 animate-spin text-primary" />
        {label}
      </div>
    </div>
  );
}

export function InlineSpinner({ className }) {
  return <Loader2 className={`h-3 w-3 animate-spin text-primary ${className || ''}`} />;
}
