import { cn } from '@/lib/utils';
import { TrendingDown, TrendingUp, Minus } from 'lucide-react';
import { Card, CardContent } from '@/components/ui/card';

/**
 * StatCard — a key metric tile built on the shadcn Card primitive.
 * Composed entirely from <Card>+<CardContent> so theming stays consistent.
 */
export function StatCard({
  label,
  value,
  delta,
  icon: Icon,
  hint,
  trailing,
  className,
}) {
  const direction = delta?.direction;
  const trendColor =
    direction === 'up'
      ? 'text-success'
      : direction === 'down'
      ? 'text-destructive'
      : 'text-muted-foreground/80';

  const TrendIcon =
    direction === 'up' ? TrendingUp : direction === 'down' ? TrendingDown : Minus;

  return (
    <Card
      size="sm"
      className={cn(
        'group transition-shadow-none duration-200 ease-out hover:shadow-none',
        className,
      )}
    >
      <CardContent className="flex flex-col gap-1">
        <div className="flex items-start justify-between gap-1">
          <p className="text-caption-mono">{label}</p>
          {Icon ? (
            <div className="flex h-6 w-6 items-center justify-center rounded-md bg-muted text-muted-foreground">
              <Icon className="h-2.5 w-2.5" />
            </div>
          ) : null}
        </div>
        <div className="flex items-baseline gap-1">
          <span className="text-display-md tabular text-foreground">{value}</span>
          {delta ? (
            <span className={cn('inline-flex items-center gap-0.5 text-body-sm tabular', trendColor)}>
              <TrendIcon className="h-2 w-2" />
              {delta.value}
              {delta.label ? <span className="ml-0.5 text-muted-foreground/80">{delta.label}</span> : null}
            </span>
          ) : null}
        </div>
        {hint ? <p className="text-caption text-muted-foreground/80">{hint}</p> : null}
        {trailing ? <div className="mt-auto">{trailing}</div> : null}
      </CardContent>
    </Card>
  );
}

export function StatGrid({ children, className }) {
  return (
    <div
      className={cn(
        'grid grid-cols-1 gap-2 sm:grid-cols-2 xl:grid-cols-4',
        className,
      )}
    >
      {children}
    </div>
  );
}
