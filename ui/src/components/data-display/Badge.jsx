import { Badge as ShadcnBadge } from '@/components/ui/badge';
import { cn } from '@/lib/utils';

// Tone-based wrapper around the shadcn Badge primitive: shadcn's `variant`
// prop doesn't cover success/warning, so those two tones layer token-based
// color classes on top of the primitive rather than reimplementing it.
const TONE_VARIANT = {
  neutral: 'secondary',
  brand: 'brand',
  success: 'secondary',
  warning: 'secondary',
  destructive: 'destructive',
};

const TONE_CLASSNAME = {
  success: 'bg-success/10 text-success',
  warning: 'bg-warning/10 text-warning',
};

const DOT_CLASSNAME = {
  neutral: 'bg-muted-foreground/80',
  brand: 'bg-primary',
  success: 'bg-success',
  warning: 'bg-warning',
  destructive: 'bg-destructive',
};

export function Badge({ tone = 'neutral', dot = false, children, className, icon: Icon }) {
  return (
    <ShadcnBadge
      variant={TONE_VARIANT[tone] ?? 'secondary'}
      className={cn(TONE_CLASSNAME[tone], className)}
    >
      {dot ? <span className={cn('h-1.5 w-1.5 rounded-full', DOT_CLASSNAME[tone])} /> : null}
      {Icon ? <Icon /> : null}
      {children}
    </ShadcnBadge>
  );
}
