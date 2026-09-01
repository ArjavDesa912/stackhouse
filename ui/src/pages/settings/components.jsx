import React from 'react';
import { Check } from 'lucide-react';

export function Row({ title, description, children }) {
  return (
    <div className="flex flex-col gap-2 border-b border-border py-3 last:border-0 sm:flex-row sm:items-start sm:justify-between">
      <div className="max-w-lg">
        <p className="text-[13px] font-medium tracking-tight text-foreground">{title}</p>
        {description && <p className="mt-0.5 text-[12px] leading-relaxed text-muted-foreground">{description}</p>}
      </div>
      <div className="shrink-0 sm:min-w-[200px] sm:text-right">{children}</div>
    </div>
  );
}

export function SectionHeader({ eyebrow, title, description }) {
  return (
    <div className="mb-3 border-b border-border pb-3">
      <p className="text-[10px] font-medium uppercase tracking-[0.14em] text-muted-foreground">{eyebrow}</p>
      <h2 className="mt-1 font-serif text-[28px] font-medium leading-[1.15] tracking-tight text-foreground">{title}</h2>
      {description && <p className="mt-1 max-w-xl text-[13px] leading-relaxed text-muted-foreground">{description}</p>}
    </div>
  );
}

export function ThemeCard({ value, label, icon: Icon, active, onClick }) {
  return (
    <button
      type="button"
      onClick={onClick}
      className={`group relative flex flex-1 flex-col items-start gap-2 rounded-md border p-3 text-left transition-all ${
        active
          ? 'border-primary bg-primary/5 ring-1 ring-primary'
          : 'border-border bg-card hover:border-foreground/20'
      }`}
    >
      <div className="flex items-center justify-between w-full">
        <div
          className={`flex h-8 w-8 items-center justify-center rounded-md transition-colors ${
            active ? 'bg-primary text-primary-foreground' : 'bg-muted text-muted-foreground'
          }`}
        >
          <Icon className="h-3 w-3" />
        </div>
        {active && <Check className="h-3 w-3 text-primary" />}
      </div>
      <div>
        <p className="text-[13px] font-medium tracking-tight text-foreground">{label}</p>
        <p className="mt-0.5 text-[11px] text-muted-foreground">
          {value === 'light' && 'Parchment daylight'}
          {value === 'dark' && 'Near-black focus'}
          {value === 'system' && 'Matches your OS'}
        </p>
      </div>
    </button>
  );
}
