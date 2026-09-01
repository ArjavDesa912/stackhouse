import { cn } from '@/lib/utils';

/**
 * PageHeader — the universal top of every page.
 * eyebrow uses caption-mono; title is display-md serif-free; description uses body-md.
 * Right slot for actions.
 */
export function PageHeader({ eyebrow, title, description, actions, children, className }) {
  return (
    <header
      className={cn(
        'shrink-0 border-b border-border bg-card px-3 pt-3 pb-3 sm:px-4',
        className,
      )}
    >
      <div className="flex flex-col gap-3 md:flex-row md:items-start md:justify-between">
        <div className="min-w-0">
          {eyebrow ? (
            <p className="text-caption-mono mb-1">{eyebrow}</p>
          ) : null}
          <h1 className="text-display-sm text-foreground">{title}</h1>
          {description ? (
            <p className="text-body-sm text-muted-foreground mt-0.5.5 max-w-2xl">{description}</p>
          ) : null}
        </div>
        {actions ? (
          <div className="flex shrink-0 flex-wrap items-center gap-1">{actions}</div>
        ) : null}
      </div>
      {children ? <div className="mt-3">{children}</div> : null}
    </header>
  );
}
