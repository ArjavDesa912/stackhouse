import { Suspense, lazy, useEffect, useState } from 'react';
import { Save, Sparkles, Undo, Redo, Plus, X, Database } from 'lucide-react';
import { useNavigate } from 'react-router';
import { PageHeader } from '@/components/data-display/PageHeader';
import { Button } from '@/components/ui/button';
import { Input } from '@/components/ui/input';
import {
  Dialog, DialogContent, DialogHeader, DialogTitle, DialogDescription, DialogFooter,
} from '@/components/ui/dialog';
import { useWorksheetStore } from '@/stores/worksheetStore';
import { useDatasets } from '@/hooks/useDatasets';
import { addFieldToShelf, normalizeField } from '@/lib/worksheet';
import { toast } from '@/components/ui/toast';
import { PageSuspense } from '@/components/feedback/PageSuspense';
import { cn } from '@/lib/utils';
import { apiPost } from '@/lib/apiClient';

const DataSidebar = lazy(() => import('@/components/DataSidebar'));
const Shelves = lazy(() => import('@/components/Shelves'));
const Worksheet = lazy(() => import('@/components/Worksheet'));
const AcceleratorGallery = lazy(() =>
  import('@/components/AcceleratorGallery').then((m) => ({ default: m.AcceleratorGallery })),
);

export default function WorksheetPage() {
  const navigate = useNavigate();
  const { datasets, activeDataset, setActiveDataset } = useDatasets();
  const {
    sheets, activeSheetId, setActiveSheetId, addSheet, deleteSheet,
    updateActiveConfig, undo, redo, getActiveSheet, loadConfig, renameActive,
  } = useWorksheetStore();

  const [showSave, setShowSave] = useState(false);
  const [showAccelerators, setShowAccelerators] = useState(false);
  const [saveName, setSaveName] = useState('');
  const [saveError, setSaveError] = useState('');
  const [isSaving, setIsSaving] = useState(false);

  const activeSheet = getActiveSheet();
  const vizConfig = activeSheet.config;

  // Pull pending dashboard config from sessionStorage if Dashboard navigated here
  useEffect(() => {
    const pending = sessionStorage.getItem('stackhouse-pending-worksheet');
    if (pending) {
      try {
        loadConfig(JSON.parse(pending));
      } catch { /* ignore */ }
      sessionStorage.removeItem('stackhouse-pending-worksheet');
    }
  }, [loadConfig]);

  const onDragStart = (e, field) => {
    e.dataTransfer.setData('field', JSON.stringify(normalizeField(field)));
  };

  const onAddField = (field) => {
    const f = normalizeField(field);
    const shelf = f.type === 'dimension' ? 'columns' : 'rows';
    updateActiveConfig(shelf, addFieldToShelf(vizConfig[shelf] || [], f));
  };

  const openSave = () => {
    setSaveName(activeSheet.name);
    setSaveError('');
    setShowSave(true);
  };

  const onSave = async () => {
    if (!saveName.trim()) {
      setSaveError('Name is required.');
      return;
    }
    setIsSaving(true);
    try {
      const res = await apiPost('/v1/push/stackhouse_dashboards', {
        name: saveName.trim(),
        config: JSON.stringify(vizConfig),
        created_at: new Date().toISOString(),
      });
      if (!res.ok) throw new Error('Failed to save.');
      renameActive(saveName.trim());
      setShowSave(false);
      toast.success('Worksheet saved.');
    } catch (e) {
      setSaveError(e.message || 'Error saving.');
      toast.error('Could not save worksheet.');
    } finally {
      setIsSaving(false);
    }
  };

  const onApplyTemplate = (config) => {
    if (config) {
      loadConfig(config);
      setShowAccelerators(false);
      toast.success('Template applied.');
    }
  };

  return (
    <div className="flex h-full flex-col">
      <PageHeader
        eyebrow="Data Studio"
        title="Worksheet"
        description="Drag dimensions and measures onto shelves to build live visualizations of your data."
        actions={
          <>
            <div className="flex items-center gap-0.5 rounded-md border border-border bg-card p-0.5 shadow-none">
              <button onClick={undo} aria-label="Undo" className="rounded p-0.5.5 text-muted-foreground hover:bg-muted hover:text-foreground">
                <Undo className="h-2.5 w-2.5" />
              </button>
              <button onClick={redo} aria-label="Redo" className="rounded p-0.5.5 text-muted-foreground hover:bg-muted hover:text-foreground">
                <Redo className="h-2.5 w-2.5" />
              </button>
            </div>
            <Button variant="outline" size="sm" onClick={() => setShowAccelerators(true)}>
              <Sparkles className="h-2.5 w-2.5 text-primary" /> Templates
            </Button>
            <Button variant="default" size="sm" onClick={openSave}>
              <Save className="h-2.5 w-2.5" /> Save
            </Button>
          </>
        }
      />

      <div className="flex flex-1 overflow-hidden">
        <Suspense fallback={<PageSuspense label="Loading worksheet…" />}>
          <DataSidebar
            datasets={datasets}
            selectedDataset={activeDataset}
            onSelectDataset={setActiveDataset}
            onDragStart={onDragStart}
            onAddField={onAddField}
          />
          <div className="flex min-w-0 flex-1 flex-col">
            <Shelves config={vizConfig} onUpdateConfig={updateActiveConfig} />
            <div className="relative flex-1 overflow-hidden bg-background">
              <Worksheet datasetId={activeDataset} shelves={vizConfig} />
            </div>
            {/* Sheet tabs */}
            <div className="flex h-8 items-center overflow-x-auto border-t border-border bg-card">
              <button
                onClick={() => navigate('/sql')}
                className="flex h-full items-center gap-0.5.5 border-r border-border px-2 text-caption text-muted-foreground hover:bg-muted hover:text-foreground"
              >
                <Database className="h-2 w-2" /> SQL
              </button>
              {sheets.map((sheet) => (
                <div
                  key={sheet.id}
                  onClick={() => setActiveSheetId(sheet.id)}
                  className={cn(
                    'group flex h-full cursor-pointer items-center border-r border-border px-2 text-caption font-medium border-t-2 transition-colors',
                    activeSheetId === sheet.id
                      ? 'bg-background border-t-flux text-foreground'
                      : 'border-t-transparent text-muted-foreground hover:bg-muted',
                  )}
                >
                  <span className="mr-1">{sheet.name}</span>
                  {sheets.length > 1 && (
                    <button
                      onClick={(e) => { e.stopPropagation(); deleteSheet(sheet.id); }}
                      className="opacity-0 hover:text-destructive group-hover:opacity-100"
                      aria-label="Close sheet"
                    >
                      <X className="h-2 w-2" />
                    </button>
                  )}
                </div>
              ))}
              <button
                onClick={addSheet}
                className="flex h-full items-center px-1.5 text-muted-foreground/80 hover:bg-muted hover:text-foreground"
                aria-label="New sheet"
              >
                <Plus className="h-2.5 w-2.5" />
              </button>
            </div>
          </div>
        </Suspense>
      </div>

      {showAccelerators ? (
        <Suspense fallback={null}>
          <AcceleratorGallery onClose={() => setShowAccelerators(false)} onApply={onApplyTemplate} />
        </Suspense>
      ) : null}

      <Dialog open={showSave} onOpenChange={setShowSave}>
        <DialogContent className="sm:max-w-md">
          <DialogHeader>
            <DialogTitle>Save worksheet</DialogTitle>
            <DialogDescription>Store the current configuration as a reusable artifact.</DialogDescription>
          </DialogHeader>
          <div className="space-y-1">
            <label className="text-body-sm font-medium text-foreground">Name</label>
            <Input
              value={saveName}
              onChange={(e) => setSaveName(e.target.value)}
              placeholder="Executive pipeline view"
            />
            {saveError ? <p className="text-body-sm text-destructive">{saveError}</p> : null}
          </div>
          <DialogFooter>
            <Button variant="outline" onClick={() => setShowSave(false)} disabled={isSaving}>
              Cancel
            </Button>
            <Button onClick={onSave} disabled={isSaving}>
              {isSaving ? 'Saving…' : 'Save worksheet'}
            </Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>
    </div>
  );
}
