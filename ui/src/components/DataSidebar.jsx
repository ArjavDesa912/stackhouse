import React, { useMemo, useState } from 'react';
import {
  Calculator, Calendar, Database, Hash, Plus, Search, Type, GripVertical,
  Sparkles, ChevronDown,
} from 'lucide-react';

import { normalizeField } from '@/lib/worksheet';
import { Badge } from '@/components/ui/badge';
import { Button } from '@/components/ui/button';
import {
  Dialog, DialogContent, DialogFooter, DialogHeader, DialogTitle,
} from '@/components/ui/dialog';
import { Input } from '@/components/ui/input';

function FieldItem({ field, onAddField, onDragStart }) {
  const isDimension = field.type === 'dimension';
  const iconTint = isDimension
    ? 'bg-primary/10 text-primary border-primary/25'
    : 'bg-muted-foreground/10 text-foreground dark:text-muted-foreground border-border/25';

  return (
    <div
      draggable
      onDragStart={(event) => onDragStart(event, field)}
      className="group flex cursor-grab items-center gap-1.5 rounded-md border border-transparent px-1 py-0.5.5 transition-all hover:border-border hover:bg-card active:cursor-grabbing"
    >
      <GripVertical className="h-2.5 w-2.5 shrink-0 text-muted-foreground/30 opacity-0 transition-opacity group-hover:opacity-100" />
      <div className={`flex h-5 w-5 shrink-0 items-center justify-center rounded-md border ${iconTint}`}>
        {isDimension ? (
          field.col_type?.toLowerCase().includes('date')
            ? <Calendar className="h-2 w-2" />
            : <Type className="h-2 w-2" />
        ) : (
          <Hash className="h-2 w-2" />
        )}
      </div>
      <div className="min-w-0 flex-1">
        <p className="truncate text-[12.5px] font-medium tracking-tight text-foreground">{field.name}</p>
        <p className="truncate font-mono text-[10px] text-muted-foreground/80">{field.col_type}</p>
      </div>
      <button
        onClick={() => onAddField(field)}
        className="inline-flex h-5 w-5 shrink-0 items-center justify-center rounded-md text-muted-foreground opacity-0 transition-all hover:bg-muted hover:text-foreground group-hover:opacity-100"
        title="Add to shelf"
      >
        <Plus className="h-2 w-2" />
      </button>
    </div>
  );
}

export default function DataSidebar({
  datasets, selectedDataset, onSelectDataset, onDragStart, onAddField,
}) {
  const [searchTerm, setSearchTerm] = useState('');
  const [customFields, setCustomFields] = useState([]);
  const [showCalcModal, setShowCalcModal] = useState(false);
  const [calcName, setCalcName] = useState('');
  const [calcFormula, setCalcFormula] = useState('');
  const [collapsed, setCollapsed] = useState({ dim: false, meas: false });

  const activeDataset = datasets.find((d) => d.id === selectedDataset);

  const normalizedFields = useMemo(() => {
    const source = (activeDataset?.fields || []).map((f) =>
      normalizeField({ ...f, col_type: f.type || f.data_type }),
    );
    const calc = customFields.map((f) => normalizeField(f));
    return [...source, ...calc];
  }, [activeDataset?.fields, customFields]);

  const filteredFields = normalizedFields.filter((field) =>
    field.name.toLowerCase().includes(searchTerm.toLowerCase()),
  );

  const dimensions = filteredFields.filter((f) => f.type === 'dimension');
  const measures = filteredFields.filter((f) => f.type === 'measure');

  const saveCalculation = () => {
    if (!calcName.trim() || !calcFormula.trim()) return;
    setCustomFields((cur) => [
      ...cur,
      { name: calcName.trim(), col_type: 'REAL (calculated)', formula: calcFormula.trim(), type: 'measure' },
    ]);
    setCalcName('');
    setCalcFormula('');
    setShowCalcModal(false);
  };

  return (
    <aside className="flex h-full w-64 shrink-0 flex-col border-r border-sidebar-border bg-sidebar/40">
      {/* Header: table picker */}
      <div className="space-y-2 border-b border-sidebar-border px-3 py-3">
        <div className="flex items-center gap-1">
          <Database className="h-2.5 w-2.5 text-muted-foreground" />
          <span className="text-[9.5px] font-medium uppercase tracking-[0.14em] text-muted-foreground">Data source</span>
        </div>
        <div className="relative">
          <select
            value={selectedDataset || ''}
            onChange={(e) => onSelectDataset(e.target.value)}
            className="h-8 w-full appearance-none rounded-md border border-border bg-card px-2 pr-9 text-[13px] font-medium tracking-tight text-foreground outline-none transition-colors focus-visible:ring-2 focus-visible:ring-ring/40"
          >
            <option value="" disabled>Select a dataset</option>
            {datasets.map((d) => (
              <option key={d.id} value={d.id}>{d.name}</option>
            ))}
          </select>
          <ChevronDown className="pointer-events-none absolute right-3 top-1/2 h-2.5 w-2.5 -translate-y-1/2 text-muted-foreground" />
        </div>
        {activeDataset && (
          <div className="flex items-center gap-2 text-[10.5px] text-muted-foreground">
            <span className="flex items-center gap-0.5"><Hash className="h-1.5 w-1.5" /> {activeDataset.fields?.length || 0} cols</span>
            <span className="h-1.5 w-px bg-border" />
            <span className="flex items-center gap-0.5"><span className="h-1.5 w-1.5 rounded-full bg-success" /> Dataset</span>
          </div>
        )}

        <div className="relative">
          <Search className="pointer-events-none absolute left-2.5 top-1/2 h-2.5 w-2.5 -translate-y-1/2 text-muted-foreground/70" />
          <Input
            value={searchTerm}
            onChange={(e) => setSearchTerm(e.target.value)}
            placeholder="Search fields..."
            className="h-6 rounded-md bg-card pl-4 text-[12px]"
          />
        </div>
      </div>

      {/* Field browser */}
      <div className="flex-1 overflow-y-auto">
        {!selectedDataset ? (
          <div className="m-3 rounded-md border border-dashed border-border bg-muted/20 px-3 py-4 text-center">
            <Database className="mx-auto mb-1 h-4 w-4 text-muted-foreground/50" />
            <p className="text-[12px] font-medium tracking-tight text-foreground">Pick a dataset</p>
            <p className="mt-0.5 text-[11px] text-muted-foreground">Fields will populate once selected.</p>
          </div>
        ) : (
          <>
            {/* Dimensions */}
            <div className="px-1 pt-2">
              <button
                onClick={() => setCollapsed((c) => ({ ...c, dim: !c.dim }))}
                className="flex w-full items-center justify-between px-1 py-0.5 text-[9.5px] font-medium uppercase tracking-[0.14em] text-muted-foreground hover:text-foreground"
              >
                <span className="flex items-center gap-0.5.5">
                  <ChevronDown className={`h-2 w-2 transition-transform ${collapsed.dim ? '-rotate-90' : ''}`} />
                  Dimensions
                </span>
                <Badge variant="outline" className="h-3 px-0.5.5 text-[9px] tracking-tight">{dimensions.length}</Badge>
              </button>
              {!collapsed.dim && (
                <div className="mt-0.5 space-y-0.5">
                  {dimensions.length === 0 ? (
                    <p className="mx-1 my-1 text-[11px] italic text-muted-foreground/70">No matches.</p>
                  ) : (
                    dimensions.map((f) => (
                      <FieldItem key={f.name} field={f} onAddField={onAddField} onDragStart={onDragStart} />
                    ))
                  )}
                </div>
              )}
            </div>

            {/* Measures */}
            <div className="mt-2 border-t border-sidebar-border px-1 pt-2 pb-2">
              <button
                onClick={() => setCollapsed((c) => ({ ...c, meas: !c.meas }))}
                className="flex w-full items-center justify-between px-1 py-0.5 text-[9.5px] font-medium uppercase tracking-[0.14em] text-muted-foreground hover:text-foreground"
              >
                <span className="flex items-center gap-0.5.5">
                  <ChevronDown className={`h-2 w-2 transition-transform ${collapsed.meas ? '-rotate-90' : ''}`} />
                  Measures
                </span>
                <Badge variant="outline" className="h-3 px-0.5.5 text-[9px] tracking-tight">{measures.length}</Badge>
              </button>
              {!collapsed.meas && (
                <div className="mt-0.5 space-y-0.5">
                  {measures.length === 0 ? (
                    <p className="mx-1 my-1 text-[11px] italic text-muted-foreground/70">No matches.</p>
                  ) : (
                    measures.map((f) => (
                      <FieldItem key={f.name} field={f} onAddField={onAddField} onDragStart={onDragStart} />
                    ))
                  )}
                </div>
              )}
            </div>
          </>
        )}
      </div>

      {/* Footer: calculated field */}
      <div className="border-t border-sidebar-border p-2">
        <Button
          variant="outline"
          size="sm"
          className="w-full justify-start gap-1 text-[12px]"
          onClick={() => setShowCalcModal(true)}
        >
          <Calculator className="h-2.5 w-2.5" /> New calculated field
        </Button>
      </div>

      <Dialog open={showCalcModal} onOpenChange={setShowCalcModal}>
        <DialogContent className="sm:max-w-lg">
          <DialogHeader>
            <DialogTitle>Create calculated field</DialogTitle>
          </DialogHeader>
          <div className="space-y-3">
            <div className="space-y-1">
              <label className="text-[12px] font-medium text-foreground">Field name</label>
              <Input value={calcName} onChange={(e) => setCalcName(e.target.value)} placeholder="Profit margin" />
            </div>
            <div className="space-y-1">
              <label className="text-[12px] font-medium text-foreground">Formula</label>
              <textarea
                value={calcFormula}
                onChange={(e) => setCalcFormula(e.target.value)}
                className="min-h-24 w-full rounded-md border border-border bg-card px-2 py-1 font-mono text-[12px] text-foreground outline-none focus-visible:ring-2 focus-visible:ring-ring/40"
                placeholder="SUM(revenue) - SUM(cost)"
              />
              <p className="text-[10.5px] text-muted-foreground">
                <Sparkles className="mr-0.5 inline h-1.5 w-1.5 text-primary" />
                Use SQL-style aggregates. References to other fields are resolved at query time.
              </p>
            </div>
          </div>
          <DialogFooter>
            <Button variant="outline" onClick={() => setShowCalcModal(false)}>Cancel</Button>
            <Button onClick={saveCalculation}>Create field</Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>
    </aside>
  );
}
