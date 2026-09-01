import * as RadioGroupPrimitive from '@radix-ui/react-radio-group';
import { Circle } from 'lucide-react';
import { cn } from '@/lib/utils';

export function RadioGroup({ className, ...props }) {
  return <RadioGroupPrimitive.Root className={cn('grid gap-1', className)} {...props} />;
}

export function RadioGroupItem({ className, ...props }) {
  return (
    <RadioGroupPrimitive.Item
      className={cn(
        'aspect-square h-3 w-3 rounded-full border border-input text-primary',
        'transition-colors',
        'focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring/40',
        'disabled:cursor-not-allowed disabled:opacity-50',
        'data-[state=checked]:border-primary',
        className,
      )}
      {...props}
    >
      <RadioGroupPrimitive.Indicator className="flex items-center justify-center">
        <Circle className="h-1 w-1 fill-primary text-primary" />
      </RadioGroupPrimitive.Indicator>
    </RadioGroupPrimitive.Item>
  );
}
