import { AlertCircle, CheckCircle2, Info, AlertTriangle } from 'lucide-react';
import { cn } from '@/lib/utils';

const VARIANTS = {
  default: { className: 'bg-card border-border text-foreground', Icon: Info },
  success: { className: 'bg-success/10 border-success/30 text-success', Icon: CheckCircle2 },
  warning: { className: 'bg-warning/10 border-warning/30 text-warning', Icon: AlertTriangle },
  destructive: { className: 'bg-destructive/10 border-destructive/30 text-destructive', Icon: AlertCircle },
};

export function Alert({ variant = 'default', icon: Icon, className, children, ...props }) {
  const v = VARIANTS[variant] || VARIANTS.default;
  const ResolvedIcon = Icon || v.Icon;
  return (
    <div
      role="alert"
      className={cn(
        'relative flex gap-2 rounded-md border px-2.5 py-2 text-body-sm',
        v.className,
        className,
      )}
      {...props}
    >
      {ResolvedIcon ? <ResolvedIcon className="mt-0.5 h-3 w-3 shrink-0" /> : null}
      <div className="flex-1 [&_*]:leading-relaxed">{children}</div>
    </div>
  );
}

export function AlertTitle({ className, ...props }) {
  return <h5 className={cn('mb-0.5 font-medium tracking-tight', className)} {...props} />;
}

export function AlertDescription({ className, ...props }) {
  return <p className={cn('opacity-90', className)} {...props} />;
}
