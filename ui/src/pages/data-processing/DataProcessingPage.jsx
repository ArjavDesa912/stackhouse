import { useState } from 'react';
import { useNavigate } from 'react-router';
import { Database, Plus, Play, Save, Trash, Loader2 } from 'lucide-react';
import { PageHeader } from '@/components/data-display/PageHeader';
import { Button } from '@/components/ui/button';
import { Input } from '@/components/ui/input';
import { Textarea } from '@/components/ui/textarea';
import { useDatasets } from '@/hooks/useDatasets';
import { apiPost } from '@/lib/apiClient';
import { toast } from '@/components/ui/toast';

export default function DataProcessingPage() {
  const navigate = useNavigate();
  const { datasets, loading, refresh } = useDatasets();

  const [name, setName] = useState('');
  const [description, setDescription] = useState('');
  const [sourceSql, setSourceSql] = useState('SELECT * FROM users LIMIT 100');
  const [preview, setPreview] = useState([]);
  const [previewColumns, setPreviewColumns] = useState([]);
  const [previewLoading, setPreviewLoading] = useState(false);
  const [saving, setSaving] = useState(false);

  const runPreview = async () => {
    if (!sourceSql.trim()) return;
    setPreviewLoading(true);
    try {
      const res = await apiPost('/v1/sql/query', { query: sourceSql });
      if (res.ok && res.data.success) {
        setPreview(res.data.data || []);
        if (res.data.data?.length > 0) {
          setPreviewColumns(Object.keys(res.data.data[0]).map((k) => ({ name: k, data_type: 'text' })));
        } else {
          setPreviewColumns([]);
        }
      } else {
        toast({ variant: 'destructive', title: 'Preview failed', description: res.data.message || 'Unknown error' });
      }
    } catch (e) {
      toast({ variant: 'destructive', title: 'Preview failed', description: e.message });
    } finally {
      setPreviewLoading(false);
    }
  };

  const saveDataset = async () => {
    if (!name.trim()) {
      toast({ variant: 'destructive', title: 'Name required' });
      return;
    }
    if (previewColumns.length === 0) {
      toast({ variant: 'destructive', title: 'Run a preview first' });
      return;
    }
    setSaving(true);
    try {
      const res = await apiPost('/v1/datasets', {
        name: name.trim(),
        description,
        source_sql: sourceSql,
        fields: previewColumns,
      });
      if (res.ok && res.data.success) {
        toast({ title: 'Dataset created' });
        refresh();
        setName('');
        setDescription('');
        setPreview([]);
        setPreviewColumns([]);
      } else {
        toast({ variant: 'destructive', title: 'Save failed', description: res.data.message || 'Unknown error' });
      }
    } catch (e) {
      toast({ variant: 'destructive', title: 'Save failed', description: e.message });
    } finally {
      setSaving(false);
    }
  };

  return (
    <div className="flex h-full flex-col">
      <PageHeader
        title="Data Processing"
        description="Build reusable datasets from SQL queries for worksheets and dashboards."
        actions={
          <Button size="sm" onClick={() => navigate('/worksheet')}>
            <Database className="h-3 w-3 mr-1" /> Open Worksheet
          </Button>
        }
      />

      <div className="flex flex-1 overflow-hidden p-4 gap-4">
        <div className="w-1/3 min-w-64 flex flex-col gap-3 overflow-y-auto border-r pr-4">
          <h3 className="text-[12px] font-semibold uppercase tracking-widest text-muted-foreground">Datasets</h3>
          {loading ? (
            <Loader2 className="h-4 w-4 animate-spin text-muted-foreground" />
          ) : (
            <div className="space-y-2">
              {datasets.map((d) => (
                <div key={d.id} className="rounded-md border border-border bg-card p-2">
                  <p className="text-[13px] font-medium text-foreground">{d.name}</p>
                  <p className="text-[11px] text-muted-foreground line-clamp-2">{d.description}</p>
                  <p className="mt-1 text-[10px] text-muted-foreground font-mono">{d.fields?.length || 0} columns</p>
                </div>
              ))}
              {datasets.length === 0 && (
                <p className="text-[12px] text-muted-foreground">No datasets yet.</p>
              )}
            </div>
          )}
        </div>

        <div className="flex flex-1 flex-col gap-3">
          <div className="space-y-2">
            <Input
              value={name}
              onChange={(e) => setName(e.target.value)}
              placeholder="Dataset name"
              className="h-9"
            />
            <Textarea
              value={description}
              onChange={(e) => setDescription(e.target.value)}
              placeholder="Description (optional)"
              className="min-h-[60px] text-[13px]"
            />
            <label className="text-[11px] font-medium uppercase tracking-widest text-muted-foreground">Source SQL</label>
            <Textarea
              value={sourceSql}
              onChange={(e) => setSourceSql(e.target.value)}
              className="min-h-[140px] font-mono text-[13px]"
            />
            <div className="flex gap-2">
              <Button variant="outline" size="sm" onClick={runPreview} disabled={previewLoading}>
                {previewLoading ? <Loader2 className="h-3 w-3 animate-spin" /> : <Play className="h-3 w-3" />} Preview
              </Button>
              <Button size="sm" onClick={saveDataset} disabled={saving}>
                {saving ? <Loader2 className="h-3 w-3 animate-spin" /> : <Save className="h-3 w-3" />} Save Dataset
              </Button>
            </div>
          </div>

          {preview.length > 0 && (
            <div className="flex-1 overflow-auto rounded-md border border-border bg-card">
              <table className="w-full text-[12px]">
                <thead className="sticky top-0 bg-muted">
                  <tr>
                    {previewColumns.map((c) => (
                      <th key={c.name} className="border-b px-2 py-1 text-left font-medium text-muted-foreground">{c.name}</th>
                    ))}
                  </tr>
                </thead>
                <tbody>
                  {preview.slice(0, 20).map((row, i) => (
                    <tr key={i}>
                      {previewColumns.map((c) => (
                        <td key={c.name} className="border-b px-2 py-1 font-mono">{String(row[c.name] ?? '')}</td>
                      ))}
                    </tr>
                  ))}
                </tbody>
              </table>
            </div>
          )}
        </div>
      </div>
    </div>
  );
}
