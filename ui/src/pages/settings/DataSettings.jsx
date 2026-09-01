import React, { useState, useEffect } from 'react';
import { useAuth } from '@/context/AuthContext';
import { apiGet, apiPost } from '@/lib/apiClient';
import { Button } from '@/components/ui/button';
import { Loader2 } from 'lucide-react';
import { Row, SectionHeader } from './components';

export default function DataSettings() {
  const { user } = useAuth();
  const [tableCount, setTableCount] = useState('—');
  const [storageUsed, setStorageUsed] = useState('—');
  const [queriesPerDay, setQueriesPerDay] = useState('—');
  const [statsLoading, setStatsLoading] = useState(false);
  const [banner, setBanner] = useState(null);

  const notify = (type, text) => {
    setBanner({ type, text });
    setTimeout(() => setBanner(null), 4000);
  };

  useEffect(() => {
    loadStats();
  }, []);

  async function loadStats() {
    setStatsLoading(true);
    try {
      const tablesRes = await apiGet('/v1/tables');
      if (tablesRes.ok && tablesRes.data.success) {
        const list = tablesRes.data.data || [];
        setTableCount(list.length.toString());
      }
      setStorageUsed('—');
      setQueriesPerDay('—');
    } catch {
      setTableCount('—');
    }
    setStatsLoading(false);
  }

  async function handleExportWorkspace() {
    const res = await apiPost('/v1/admin/backups', { type: 'full' });
    if (res.ok && res.data.success) notify('ok', 'Backup started. Check the Admin Center for status.');
    else notify('err', res.data?.message || 'Export failed');
  }

  function handleClearCache() {
    const preserve = new Set(['stackhouse_access_token', 'stackhouse_refresh_token']);
    const removed = [];
    for (let i = localStorage.length - 1; i >= 0; i--) {
      const key = localStorage.key(i);
      if (key && key.startsWith('stackhouse_') && !preserve.has(key)) {
        localStorage.removeItem(key);
        removed.push(key);
      }
    }
    sessionStorage.clear();
    notify('ok', `Cleared ${removed.length} cache entries.`);
  }

  return (
    <section>
      {banner && (
        <div className={`mb-3 flex items-start justify-between gap-2 rounded-md border px-3 py-1.5 text-[12px] ${
          banner.type === 'ok'
            ? 'border-success/30 bg-success/5 text-success'
            : 'border-destructive/30 bg-destructive/5 text-destructive'
        }`}>
          <span>{banner.text}</span>
          <button onClick={() => setBanner(null)} className="opacity-60 hover:opacity-100">×</button>
        </div>
      )}

      <SectionHeader
        eyebrow="Data & Storage"
        title="What lives where"
        description="Inspect usage, export snapshots, and clear ephemeral caches."
      />
      <div className="mb-3 grid gap-2 sm:grid-cols-3">
        <div className="rounded-md border border-border bg-card p-3">
          <p className="text-[10px] font-medium uppercase tracking-[0.12em] text-muted-foreground">Tables</p>
          <p className="mt-0.5 font-serif text-[26px] font-medium leading-none tracking-tight text-foreground">
            {statsLoading ? <Loader2 className="h-5 w-5 animate-spin text-muted-foreground" /> : tableCount}
          </p>
          <p className="mt-0.5 text-[11px] text-muted-foreground">discovered</p>
        </div>
        <div className="rounded-md border border-border bg-card p-3">
          <p className="text-[10px] font-medium uppercase tracking-[0.12em] text-muted-foreground">Storage</p>
          <p className="mt-0.5 font-serif text-[26px] font-medium leading-none tracking-tight text-foreground">
            {statsLoading ? <Loader2 className="h-5 w-5 animate-spin text-muted-foreground" /> : (storageUsed === '—' ? '—' : <>{storageUsed}<span className="text-muted-foreground text-[16px] ml-0.5">GB</span></>)}
          </p>
          <p className="mt-0.5 text-[11px] text-muted-foreground">total used</p>
        </div>
        <div className="rounded-md border border-border bg-card p-3">
          <p className="text-[10px] font-medium uppercase tracking-[0.12em] text-muted-foreground">Queries / day</p>
          <p className="mt-0.5 font-serif text-[26px] font-medium leading-none tracking-tight text-foreground">
            {statsLoading ? <Loader2 className="h-5 w-5 animate-spin text-muted-foreground" /> : queriesPerDay}
          </p>
          <p className="mt-0.5 text-[11px] text-muted-foreground">7-day average</p>
        </div>
      </div>
      <Row title="Export workspace" description="Download a zipped archive of tables and worksheets.">
        <Button variant="outline" size="sm" className="text-[12px]" onClick={handleExportWorkspace}>Request export</Button>
      </Row>
      <Row title="Clear query cache" description="Removes cached result sets. Live data is not affected.">
        <Button variant="outline" size="sm" className="text-[12px]" onClick={handleClearCache}>Clear cache</Button>
      </Row>
      <Row title="Retention" description="How long to keep audit trails and agent traces.">
        <select className="h-8 rounded-md border border-border bg-card px-2 text-[12px] text-foreground outline-none focus-visible:ring-2 focus-visible:ring-ring/40 sm:w-[240px]"
          defaultValue={user?.metadata?.retention || '30 days'}>
          <option>30 days</option><option>90 days</option><option>1 year</option><option>Forever</option>
        </select>
      </Row>
    </section>
  );
}
