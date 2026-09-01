import { cn } from '@/lib/utils';
import { Card, CardContent } from '@/components/ui/card';

/**
 * EmptyState — accent card for empty/zero/onboarding states.
 * Built on the shadcn <Card> primitive.
 * variants: 'default' | 'plain'
 */
const VARIANTS = {
  default: 'bg-muted text-foreground border-transparent',
  plain:   'bg-background text-foreground',
};

export function EmptyState({
  variant = 'default',
  eyebrow,
  title,
  description,
  action,
  illustration,
  className,
}) {
  const isAccent = variant === 'default';
  return (
    <Card className={cn('relative overflow-hidden', VARIANTS[variant], className)}>
      <CardContent className="relative z-10 flex flex-col gap-3 p-3 sm:max-w-xl sm:p-3">
        {eyebrow ? (
          <p className={cn('text-caption-mono', isAccent ? 'opacity-75' : '')}>{eyebrow}</p>
        ) : null}
        <h2 className={cn('text-display-sm', isAccent ? '' : 'text-foreground')}>{title}</h2>
        {description ? (
          <p className={cn('text-body-sm max-w-prose', isAccent ? 'opacity-85' : 'text-muted-foreground')}>
            {description}
          </p>
        ) : null}
        {action ? <div className="mt-1 flex flex-wrap gap-1">{action}</div> : null}
      </CardContent>
      {illustration ? (
        <div className="pointer-events-none absolute inset-y-0 right-0 hidden w-0.5/2 items-center justify-end pr-4 opacity-90 sm:flex">
          {illustration}
        </div>
      ) : null}
    </Card>
  );
}
