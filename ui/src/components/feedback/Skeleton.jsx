import { cn } from '@/lib/utils';
import { Skeleton as ShadcnSkeleton } from '@/components/ui/skeleton';

/**
 * Re-export the shadcn Skeleton primitive for the canonical single-element
 * skeleton, then layer composite skeleton shapes on top of it.
 */
export function Skeleton({ className }) {
  return <ShadcnSkeleton className={cn('skeleton-shimmer rounded-md', className)} />;
}

export function SkeletonText({ lines = 3, className }) {
  return (
    <div className={cn('flex flex-col gap-1', className)}>
      {Array.from({ length: lines }).map((_, i) => (
        <Skeleton
          key={i}
          className={cn('h-2', i === lines - 1 ? 'w-1/3' : 'w-full')}
        />
      ))}
    </div>
  );
}

export function SkeletonCard({ className }) {
  return (
    <div className={cn('rounded-md border border-border bg-card p-3', className)}>
      <Skeleton className="mb-2 h-2 w-16" />
      <Skeleton className="mb-1 h-6 w-24" />
      <Skeleton className="h-1 w-full" />
    </div>
  );
}

export function SkeletonStatGrid({ count = 4 }) {
  return (
    <div className="grid grid-cols-1 gap-2 sm:grid-cols-2 xl:grid-cols-4">
      {Array.from({ length: count }).map((_, i) => (
        <SkeletonCard key={i} />
      ))}
    </div>
  );
}

export function SkeletonTable({ rows = 6, cols = 4 }) {
  return (
    <div className="overflow-hidden rounded-md border border-border bg-card">
      <div className="flex gap-3 border-b border-border bg-background px-3 py-1.5">
        {Array.from({ length: cols }).map((_, i) => (
          <Skeleton key={i} className="h-2 flex-1" />
        ))}
      </div>
      {Array.from({ length: rows }).map((_, r) => (
        <div key={r} className="flex gap-3 border-b border-border px-3 py-2 last:border-b-0">
          {Array.from({ length: cols }).map((_, c) => (
            <Skeleton key={c} className="h-2 flex-1" />
          ))}
        </div>
      ))}
    </div>
  );
}
