import { useEffect, useState } from 'react';
import { useNavigate } from 'react-router';
import {
  Database, Layers, FileSpreadsheet, Plus, ArrowUpRight, Sparkles,
  Calendar, BarChart3, Trash2, Terminal,
} from 'lucide-react';
import { PageHeader } from '@/components/data-display/PageHeader';
import { StatCard, StatGrid } from '@/components/data-display/StatCard';
import { Badge } from '@/components/data-display/Badge';
import { EmptyState } from '@/components/data-display/EmptyState';
import { SkeletonStatGrid, Skeleton } from '@/components/feedback/Skeleton';
import { Button } from '@/components/ui/button';
import { useTablesMetadata } from '@/hooks/useTablesMetadata';
import { ScrollArea } from '@/components/ui/scroll-area';
import { toast } from '@/components/ui/toast';
import { apiGet, apiPost } from '@/lib/apiClient';

function useDashboards() {
  const [items, setItems] = useState([]);
  const [loading, setLoading] = useState(true);

  const load = async () => {
    setLoading(true);
    try {
      const metaRes = await apiGet('/v1/tables');
      if (metaRes.ok && metaRes.data.success && metaRes.data.tables.includes('stackhouse_dashboards')) {
        const res = await apiGet('/v1/query/stackhouse_dashboards?order_by=created_at&order_dir=DESC');
        if (res.ok && res.data.success) setItems(res.data.data);
      } else {
        setItems([]);
      }
    } catch {
      setItems([]);
    } finally {
      setLoading(false);
    }
  };

  useEffect(() => {
    load();
  }, []);

  const remove = async (id) => {
    try {
      await apiPost(`/v1/delete/stackhouse_dashboards/${id}`);
      toast.success('Worksheet deleted.');
      load();
    } catch {
      toast.error('Failed to delete.');
    }
  };

  return { items, loading, remove, refresh: load };
}

function WorksheetCard({ item, onOpen, onDelete }) {
  return (
    <button
      onClick={onOpen}
      className="group relative flex h-28 flex-col items-start gap-2 overflow-hidden rounded-md border border-border bg-card p-3 text-left transition-all duration-200 ease-out-stackhouse hover:shadow-none hover:-translate-y-0.5"
    >
      <div className="flex w-full items-start justify-between">
        <span className="flex h-6 w-6 items-center justify-center rounded-md bg-primary/10 text-primary">
          <BarChart3 className="h-3 w-3" />
        </span>
        <button
          onClick={(e) => { e.stopPropagation(); onDelete(); }}
          className="flex h-6 w-6 items-center justify-center rounded-md text-muted-foreground/80 opacity-0 transition-opacity hover:bg-destructive/10 hover:text-destructive group-hover:opacity-100"
          aria-label="Delete worksheet"
        >
          <Trash2 className="h-2.5 w-2.5" />
        </button>
      </div>
      <div className="flex-1">
        <p className="text-display-sm truncate text-foreground">{item.name || 'Untitled'}</p>
        <p className="text-caption-mono mt-0.5.5 inline-flex items-center gap-0.5.5">
          <Calendar className="h-2 w-2" />
          {new Date(item.created_at).toLocaleDateString(undefined, {
            month: 'short',
            day: 'numeric',
            year: 'numeric',
          })}
        </p>
      </div>
      <div className="flex w-full items-center justify-between">
        <Badge tone="success" dot>synced</Badge>
        <ArrowUpRight className="h-2.5 w-2.5 text-muted-foreground/80 transition-transform group-hover:-translate-y-0.5 group-hover:translate-x-0.5 group-hover:text-primary" />
      </div>
    </button>
  );
}

function NewCard({ onCreate }) {
  return (
    <button
      onClick={onCreate}
      className="group flex h-28 flex-col items-center justify-center gap-1.5 rounded-md border border-dashed border-input bg-background p-3 text-center transition-colors hover:border-primary hover:bg-primary/5"
    >
      <span className="flex h-8 w-8 items-center justify-center rounded-md bg-card shadow-none transition-transform group-hover:rotate-6 group-hover:scale-105">
        <Plus className="h-3 w-3 text-primary" />
      </span>
      <span className="text-body-sm font-medium text-foreground">New worksheet</span>
      <span className="text-caption text-muted-foreground/80">Start from a blank canvas.</span>
    </button>
  );
}

function QuickAction({ icon: Icon, label, hint, to }) {
  const navigate = useNavigate();
  return (
    <button
      onClick={() => navigate(to)}
      className="group flex items-center gap-2 rounded-md border border-border bg-card p-2 text-left transition-all hover:shadow-none"
    >
      <span className="flex h-6 w-6 items-center justify-center rounded-md bg-muted text-muted-foreground transition-colors group-hover:bg-primary/10 group-hover:text-primary">
        <Icon className="h-3 w-3" />
      </span>
      <div className="flex-1 min-w-0">
        <p className="text-body-sm font-medium text-foreground">{label}</p>
        <p className="text-caption text-muted-foreground/80 truncate">{hint}</p>
      </div>
      <ArrowUpRight className="h-2.5 w-2.5 shrink-0 text-muted-foreground/80 transition-transform group-hover:-translate-y-0.5 group-hover:translate-x-0.5 group-hover:text-primary" />
    </button>
  );
}

export default function DashboardPage() {
  const navigate = useNavigate();
  const { tables, loading: tablesLoading } = useTablesMetadata();
  const { items: dashboards, loading: dashboardsLoading, remove } = useDashboards();

  const totalColumns = tables.reduce((sum, t) => sum + (t.columns?.length || 0), 0);
  const totalRows = tables.reduce((sum, t) => sum + (t.row_count || 0), 0);

  const onCreate = () => navigate('/worksheet');
  const onOpenWorksheet = (dash) => {
    try {
      const config = typeof dash.config === 'string' ? JSON.parse(dash.config) : dash.config;
      sessionStorage.setItem('stackhouse-pending-worksheet', JSON.stringify(config));
      navigate('/worksheet');
    } catch {
      toast.error('Could not parse worksheet config.');
    }
  };

  return (
    <div className="flex h-full flex-col">
      <PageHeader
        eyebrow="Workspace"
        title="Welcome back."
        description="A live view of your data warehouse, recent worksheets, and where your team left off."
        actions={
          <Button variant="default" size="sm" onClick={onCreate}>
            <Plus className="h-2.5 w-2.5" /> New worksheet
          </Button>
        }
      />

      <ScrollArea className="flex-1">
        <div className="mx-auto max-w-7xl space-y-4 px-3 py-3 sm:px-4">
          {/* Stats */}
          {tablesLoading ? (
            <SkeletonStatGrid />
          ) : (
            <StatGrid>
              <StatCard
                label="Tables"
                value={tables.length}
                icon={Database}
                hint={`${totalColumns} columns total`}
              />
              <StatCard
                label="Rows"
                value={totalRows.toLocaleString()}
                icon={Layers}
                hint="Across all tables"
              />
              <StatCard
                label="Worksheets"
                value={dashboardsLoading ? '—' : dashboards.length}
                icon={FileSpreadsheet}
                hint="Saved configurations"
              />
            </StatGrid>
          )}

          {/* Onboarding callout */}
          <EmptyState
            variant="default"
            eyebrow="What's new"
            title="Explore your warehouse with SQL and worksheets."
            description="Run queries, design schemas, and build live visualizations — all backed by Postgres with vector search."
            action={
              <>
                <Button
                  size="sm"
                  className="bg-card text-primary hover:bg-card/90"
                  onClick={() => navigate('/sql')}
                >
                  <Terminal className="h-2.5 w-2.5" /> Run a query
                </Button>
                <Button
                  variant="outline"
                  size="sm"
                  className="border-card/30 bg-transparent text-primary-foreground hover:bg-card/15"
                  onClick={() => navigate('/worksheet')}
                >
                  <FileSpreadsheet className="h-2.5 w-2.5" /> Open worksheet
                </Button>
              </>
            }
          />

          {/* Quick actions */}
          <section>
            <header className="mb-2 flex items-baseline justify-between">
              <h2 className="text-display-sm text-foreground">Quick actions</h2>
              <p className="text-caption text-muted-foreground/80">Jump back into recent contexts.</p>
            </header>
            <div className="grid grid-cols-1 gap-1 sm:grid-cols-2 lg:grid-cols-4">
              <QuickAction icon={Terminal} label="SQL Console" hint="Compose & run queries" to="/sql" />
              <QuickAction icon={Database} label="Schema" hint="Design tables & columns" to="/schema" />
              <QuickAction icon={FileSpreadsheet} label="Worksheet" hint="Drag & drop visualization" to="/worksheet" />
            </div>
          </section>

          {/* Recent worksheets */}
          <section className="pb-4">
            <header className="mb-2 flex items-baseline justify-between">
              <h2 className="text-display-sm text-foreground">Recent worksheets</h2>
              {!dashboardsLoading && dashboards.length > 0 && (
                <button
                  onClick={() => navigate('/worksheet')}
                  className="text-body-sm text-primary hover:underline"
                >
                  Open worksheet builder →
                </button>
              )}
            </header>
            {dashboardsLoading ? (
              <div className="grid grid-cols-1 gap-2 sm:grid-cols-2 lg:grid-cols-3 xl:grid-cols-4">
                {Array.from({ length: 4 }).map((_, i) => (
                  <Skeleton key={i} className="h-28" />
                ))}
              </div>
            ) : dashboards.length === 0 ? (
              <EmptyState
                variant="plain"
                eyebrow="Empty workspace"
                title="No worksheets yet."
                description="Saved worksheets show up here. Create your first chart in the Worksheet builder."
                action={
                  <Button size="sm" onClick={onCreate}>
                    <Plus className="h-2.5 w-2.5" /> Create worksheet
                  </Button>
                }
              />
            ) : (
              <div className="grid grid-cols-1 gap-2 sm:grid-cols-2 lg:grid-cols-3 xl:grid-cols-4">
                <NewCard onCreate={onCreate} />
                {dashboards.map((d) => (
                  <WorksheetCard
                    key={d.id}
                    item={d}
                    onOpen={() => onOpenWorksheet(d)}
                    onDelete={() => remove(d.id)}
                  />
                ))}
              </div>
            )}
          </section>
        </div>
      </ScrollArea>
    </div>
  );
}
