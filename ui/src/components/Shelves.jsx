import React, { useState } from 'react';
import { RotateCcw, X, Columns3, Rows3, ChevronDown } from 'lucide-react';

import { addFieldToShelf, removeFieldFromShelf } from '@/lib/worksheet';
import { CHART_CATALOG, CHART_GROUPS, getChartSpec } from '@/lib/chartCatalog';
import { Badge } from '@/components/ui/badge';

function Pill({ field, onRemove }) {
  const isDim = field.type === 'dimension';
  return (
    <div
      className={`group inline-flex h-6 items-center gap-0.5.5 rounded-full border px-1.5 text-[11.5px] font-medium tracking-tight transition-all ${
        isDim
          ? 'border-primary/30 bg-primary/10 text-primary dark:text-primary'
          : 'border-border/30 bg-muted-foreground/15 text-foreground dark:text-muted-foreground'
      }`}
    >
      <span className={`h-0.5 w-0.5 rounded-full ${isDim ? 'bg-primary' : 'bg-foreground'}`} />
      {field.name}
      <button
        onClick={() => onRemove(field.name)}
        className="-mr-0.5 ml-0.5 inline-flex h-3 w-3 items-center justify-center rounded-full text-current opacity-60 transition-all hover:bg-foreground/5 hover:opacity-100"
      >
        <X className="h-1.5 w-1.5" />
      </button>
    </div>
  );
}

function ShelfDropZone({ label, eyebrow, icon: Icon, items, onDrop, onRemove }) {
  const handleDragOver = (e) => e.preventDefault();

  return (
    <div className="group/shelf rounded-md border border-border bg-card transition-colors hover:border-foreground/15">
      <div className="flex items-center gap-1 border-b border-border px-2 py-1">
        <Icon className="h-2.5 w-2.5 text-muted-foreground" />
        <div className="flex-1">
          <p className="text-[9.5px] font-medium uppercase tracking-[0.14em] text-muted-foreground">{eyebrow}</p>
          <p className="text-[12px] font-medium leading-tight tracking-tight text-foreground">{label}</p>
        </div>
        <Badge variant="outline" className="h-3 px-0.5.5 text-[9px] tracking-tight">{items.length}</Badge>
      </div>
      <div
        onDrop={onDrop}
        onDragOver={handleDragOver}
        className="flex min-h-12 flex-wrap items-center gap-0.5.5 rounded-b-lg px-2 py-1.5 transition-colors group-hover/shelf:bg-primary/5"
      >
        {items.length === 0 ? (
          <p className="text-[11.5px] italic text-muted-foreground/70">Drag a field here, or click + in the browser.</p>
        ) : (
          items.map((f) => <Pill key={f.name} field={f} onRemove={onRemove} />)
        )}
      </div>
    </div>
  );
}

export default function Shelves({ config, onUpdateConfig }) {
  const handleDrop = (e, shelfName) => {
    e.preventDefault();
    const serialized = e.dataTransfer.getData('field');
    if (!serialized) return;
    onUpdateConfig(shelfName, addFieldToShelf(config[shelfName] || [], JSON.parse(serialized)));
  };
  const handleRemove = (shelfName, fieldName) => {
    onUpdateConfig(shelfName, removeFieldFromShelf(config[shelfName] || [], fieldName));
  };
  const reset = () => {
    onUpdateConfig('columns', []);
    onUpdateConfig('rows', []);
  };

  return (
    <div className="border-b border-border bg-background px-3 py-2.5">
      <div className="flex flex-col gap-2 xl:flex-row xl:items-stretch">
        {/* Shelves */}
        <div className="grid flex-1 gap-2 sm:grid-cols-2">
          <ShelfDropZone
            eyebrow="Group by"
            label="Dimensions"
            icon={Columns3}
            items={config.columns || []}
            onDrop={(e) => handleDrop(e, 'columns')}
            onRemove={(name) => handleRemove('columns', name)}
          />
          <ShelfDropZone
            eyebrow="Aggregate"
            label="Measures"
            icon={Rows3}
            items={config.rows || []}
            onDrop={(e) => handleDrop(e, 'rows')}
            onRemove={(name) => handleRemove('rows', name)}
          />
        </div>

        {/* Chart control */}
        <ChartPicker
          activeId={config.chartType || 'auto'}
          onPick={(id) => onUpdateConfig('chartType', id)}
          onReset={reset}
        />
      </div>
    </div>
  );
}

function ChartPicker({ activeId, onPick, onReset }) {
  const [open, setOpen] = useState(false);
  const spec = getChartSpec(activeId);
  const ActiveIcon = spec.icon;

  return (
    <div className="relative flex shrink-0 flex-col rounded-md border border-border bg-card xl:w-64">
      <div className="flex items-center justify-between border-b border-border px-2 py-1">
        <div>
          <p className="text-[9.5px] font-medium uppercase tracking-[0.14em] text-muted-foreground">Render as</p>
          <p className="text-[12px] font-medium leading-tight tracking-tight text-foreground">Visualization</p>
        </div>
        <button
          onClick={onReset}
          className="inline-flex h-5 items-center gap-0.5 rounded-md px-0.5.5 text-[10.5px] font-medium text-muted-foreground transition-colors hover:bg-muted hover:text-foreground"
          title="Clear assignments"
        >
          <RotateCcw className="h-2 w-2" /> Clear
        </button>
      </div>

      <button
        type="button"
        onClick={() => setOpen((v) => !v)}
        className="flex items-center justify-between gap-1 px-2 py-1 text-left transition-colors hover:bg-muted/40"
      >
        <span className="flex min-w-0 items-center gap-1">
          <span className="flex h-6 w-6 shrink-0 items-center justify-center rounded-md bg-primary/10 text-primary">
            <ActiveIcon className="h-2.5 w-2.5" />
          </span>
          <span className="min-w-0">
            <span className="block truncate text-[12.5px] font-medium tracking-tight text-foreground">{spec.label}</span>
            <span className="block truncate text-[10.5px] text-muted-foreground">{spec.group}</span>
          </span>
        </span>
        <ChevronDown className={`h-2.5 w-2.5 text-muted-foreground transition-transform ${open ? 'rotate-180' : ''}`} />
      </button>

      {open && (
        <>
          <div className="fixed inset-0 z-40" onClick={() => setOpen(false)} />
          <div className="absolute left-0 right-0 top-full z-50 mt-0.5 max-h-[60vh] overflow-auto rounded-md border border-border bg-popover p-1 shadow-none">
            {CHART_GROUPS.map((group) => {
              const items = CHART_CATALOG.filter((c) => c.group === group);
              if (items.length === 0) return null;
              return (
                <div key={group} className="mb-1 last:mb-0">
                  <p className="px-1 pb-0.5 pt-0.5.5 text-[9.5px] font-semibold uppercase tracking-[0.16em] text-muted-foreground">{group}</p>
                  <div className="grid grid-cols-2 gap-0.5">
                    {items.map((c) => {
                      const Icon = c.icon;
                      const active = c.id === activeId;
                      return (
                        <button
                          key={c.id}
                          type="button"
                          onClick={() => { onPick(c.id); setOpen(false); }}
                          title={c.description}
                          className={`group flex items-center gap-1 rounded-md px-1 py-0.5.5 text-left transition-colors ${
                            active
                              ? 'bg-primary/15 text-foreground'
                              : 'hover:bg-muted text-foreground/80'
                          }`}
                        >
                          <span className={`flex h-5 w-5 shrink-0 items-center justify-center rounded-md ${active ? 'bg-primary text-primary-foreground' : 'bg-muted text-muted-foreground group-hover:text-foreground'}`}>
                            <Icon className="h-2 w-2" />
                          </span>
                          <span className="min-w-0 flex-1 truncate text-[11.5px] font-medium tracking-tight">{c.label}</span>
                          {active && <Badge variant="outline" className="h-3 px-0.5 text-[8.5px]">Active</Badge>}
                        </button>
                      );
                    })}
                  </div>
                </div>
              );
            })}
          </div>
        </>
      )}
    </div>
  );
}
