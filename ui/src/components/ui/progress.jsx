import * as ProgressPrimitive from '@radix-ui/react-progress';
import { cn } from '@/lib/utils';

export function Progress({ value = 0, className, indicatorClassName }) {
  return (
    <ProgressPrimitive.Root
      value={value}
      className={cn(
        'relative h-0.5.5 w-full overflow-hidden rounded-full bg-muted',
        className,
      )}
    >
      <ProgressPrimitive.Indicator
        className={cn(
          'h-full w-full flex-1 bg-primary transition-transform duration-500 ease-out',
          indicatorClassName,
        )}
        style={{ transform: `translateX(-${100 - (value || 0)}%)` }}
      />
    </ProgressPrimitive.Root>
  );
}
