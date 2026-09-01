import { useState, useEffect } from 'react';
import { NavLink, Outlet, useLocation, useNavigate, useOutletContext } from 'react-router';
import { useAuth } from '../context/AuthContext';
import { apiGet, apiPost, apiDelete } from '../lib/apiClient';
import { Button } from '@/components/ui/button';
import { Input } from '@/components/ui/input';
import { Label } from '@/components/ui/label';
import { Badge } from '@/components/ui/badge';
import { Dialog, DialogContent, DialogDescription, DialogFooter, DialogHeader, DialogTitle } from '@/components/ui/dialog';
import {
  Shield, Users, Puzzle, GitBranch, Network, Database, ScrollText, CreditCard,
  Loader2, Plus, Trash2, Download, Ban, CheckCircle2, ChevronRight,
  UserPlus, Search, RefreshCw, AlertTriangle, RotateCcw
} from 'lucide-react';

/**
 * Admin Center — backed by real backend routes only.
 *   - Overview:    GET  /v1/metrics/summary
 *   - Members:     GET  /v1/teams · GET /v1/teams/:id/members · POST /v1/teams/invite
 *   - Extensions:  GET  /v1/admin/extensions, /extensions/available · POST /install, /uninstall
 *   - Branching:   GET/POST /v1/admin/branches · DELETE /branches/:name
 *   - Network:     GET  /v1/admin/network/rules · POST /enable · POST /disable
 *   - Backups:     GET/POST /v1/admin/backups · DELETE /backups/:id · POST /backups/:id/restore
 *   - Audit:       GET  /v1/admin/audit
 */

const NAV = [
  { id: 'overview',   label: 'Overview',    icon: Shield,     group: 'General' },
  { id: 'users',      label: 'Members',     icon: Users,      group: 'Access' },
  { id: 'extensions', label: 'Extensions',  icon: Puzzle,     group: 'Platform' },
  { id: 'billing',    label: 'Billing',     icon: CreditCard, group: 'Platform' },
  { id: 'branching',  label: 'Branching',   icon: GitBranch,  group: 'Platform' },
  { id: 'network',    label: 'Network',     icon: Network,    group: 'Security' },
  { id: 'backups',    label: 'Backups',     icon: Database,   group: 'Reliability' },
  { id: 'logs',       label: 'Audit Log',   icon: ScrollText, group: 'Reliability' },
];

const ROLE_OPTIONS = [
  { value: 'readonly',  label: 'Read-only',  desc: 'View data and dashboards' },
  { value: 'audit',     label: 'Audit',      desc: 'Read + view audit logs' },
  { value: 'developer', label: 'Developer',  desc: 'Build workflows and queries' },
  { value: 'admin',     label: 'Admin',      desc: 'Manage members and settings' },
];

function initials(email = '') {
  const name = email.split('@')[0] || '';
  const parts = name.split(/[._-]+/);
  if (parts.length > 1) return (parts[0][0] + parts[1][0]).toUpperCase();
  return name.slice(0, 2).toUpperCase() || '??';
}

export default function AdminCenter() {
  const { isAdmin } = useAuth();
  const location = useLocation();
  const navigate = useNavigate();
  
  // Extract section from pathname, default to overview
  const pathParts = location.pathname.split('/').filter(Boolean);
  // e.g. /admin/billing/apps -> pathParts = ['admin', 'billing', 'apps']
  const section = pathParts[1] || 'overview';

  const [search, setSearch] = useState('');

  // Real data
  const [metrics, setMetrics] = useState(null);
  const [teams, setTeams] = useState([]);
  const [activeTeamId, setActiveTeamId] = useState(null);
  const [members, setMembers] = useState([]);
  const [installedExts, setInstalledExts] = useState([]);
  const [availableExts, setAvailableExts] = useState([]);
  const [branches, setBranches] = useState([]);
  const [networkRules, setNetworkRules] = useState([]);
  const [backups, setBackups] = useState([]);
  const [auditLogs, setAuditLogs] = useState([]);
  const [banner, setBanner] = useState(null); // {type:'ok'|'err', text}

  // Invite
  const [inviteOpen, setInviteOpen] = useState(false);
  const [inviteEmail, setInviteEmail] = useState('');
  const [inviteRole, setInviteRole] = useState('developer');
  const [inviting, setInviting] = useState(false);

  // Branch create
  const [newBranchName, setNewBranchName] = useState('');
  const [creatingBranch, setCreatingBranch] = useState(false);

  // Backup
  const [triggeringBackup, setTriggeringBackup] = useState(false);

  useEffect(() => {
    if (!isAdmin()) return;
    refreshAll();
  }, []);

  async function refreshAll() {
    const results = await Promise.allSettled([
      apiGet('/v1/metrics/summary'),
      apiGet('/v1/teams'),
      apiGet('/v1/admin/extensions'),
      apiGet('/v1/admin/extensions/available'),
      apiGet('/v1/admin/branches'),
      apiGet('/v1/admin/network/rules'),
      apiGet('/v1/admin/backups'),
      apiGet('/v1/admin/audit'),
    ]);
    const [m, t, ei, ea, b, n, k, a] = results;

    if (m.status === 'fulfilled' && m.value.ok) {
      // metrics summary returns data at root (no .success wrapper)
      setMetrics(m.value.data);
    }
    if (t.status === 'fulfilled' && t.value.ok && t.value.data.success) {
      const list = t.value.data.data || [];
      setTeams(list);
      if (list.length && !activeTeamId) {
        setActiveTeamId(list[0].id);
        loadMembers(list[0].id);
      }
    }
    if (ei.status === 'fulfilled' && ei.value.ok && ei.value.data.success)
      setInstalledExts(ei.value.data.data || []);
    if (ea.status === 'fulfilled' && ea.value.ok && ea.value.data.success)
      setAvailableExts(ea.value.data.data || []);
    if (b.status === 'fulfilled' && b.value.ok && b.value.data.success)
      setBranches(b.value.data.data || []);
    if (n.status === 'fulfilled' && n.value.ok && n.value.data.success)
      setNetworkRules(n.value.data.data || []);
    if (k.status === 'fulfilled' && k.value.ok && k.value.data.success)
      setBackups(k.value.data.data || []);
    if (a.status === 'fulfilled' && a.value.ok && a.value.data.success)
      setAuditLogs(a.value.data.data || []);
  }

  async function loadMembers(teamId) {
    const res = await apiGet(`/v1/teams/${teamId}/members`);
    if (res.ok && res.data.success) setMembers(res.data.data || []);
  }

  async function handleInvite(e) {
    e.preventDefault();
    if (!activeTeamId) { setBanner({ type: 'err', text: 'Create a team first.' }); return; }
    if (!inviteEmail.trim()) return;
    setInviting(true);
    const res = await apiPost('/v1/teams/invite', { team_id: activeTeamId, email: inviteEmail, role: inviteRole });
    setInviting(false);
    if (res.ok && res.data.success) {
      setInviteEmail(''); setInviteOpen(false);
      setBanner({ type: 'ok', text: `Invitation sent to ${inviteEmail}.` });
      loadMembers(activeTeamId);
    } else {
      setBanner({ type: 'err', text: res.data?.message || 'Invite failed' });
    }
  }

  async function installExt(name) {
    const res = await apiPost('/v1/admin/extensions/install', { name });
    if (res.ok && res.data.success) {
      setBanner({ type: 'ok', text: `Installed ${name}.` });
      refreshAll();
    } else {
      setBanner({ type: 'err', text: res.data?.message || 'Install failed' });
    }
  }

  async function uninstallExt(name) {
    if (!confirm(`Uninstall extension "${name}"?`)) return;
    const res = await apiPost('/v1/admin/extensions/uninstall', { name });
    if (res.ok && res.data.success) {
      setBanner({ type: 'ok', text: `Uninstalled ${name}.` });
      refreshAll();
    } else {
      setBanner({ type: 'err', text: res.data?.message || 'Uninstall failed' });
    }
  }

  async function createBranch(e) {
    e.preventDefault();
    if (!newBranchName.trim()) return;
    setCreatingBranch(true);
    const res = await apiPost('/v1/admin/branches', { name: newBranchName });
    setCreatingBranch(false);
    if (res.ok && res.data.success) {
      setNewBranchName('');
      setBanner({ type: 'ok', text: `Branch ${newBranchName} created.` });
      const r = await apiGet('/v1/admin/branches');
      if (r.ok && r.data.success) setBranches(r.data.data || []);
    } else {
      setBanner({ type: 'err', text: res.data?.message || 'Create failed' });
    }
  }

  async function deleteBranch(name) {
    if (!confirm(`Delete branch "${name}"?`)) return;
    const res = await apiDelete(`/v1/admin/branches/${encodeURIComponent(name)}`);
    if (res.ok && res.data.success) {
      setBanner({ type: 'ok', text: `Branch ${name} deleted.` });
      const r = await apiGet('/v1/admin/branches');
      if (r.ok && r.data.success) setBranches(r.data.data || []);
    } else {
      setBanner({ type: 'err', text: res.data?.message || 'Delete failed' });
    }
  }

  async function toggleNetworkEnforcement(enable) {
    const res = await apiPost(enable ? '/v1/admin/network/enable' : '/v1/admin/network/disable', {});
    if (res.ok && res.data.success) {
      setBanner({ type: 'ok', text: `Network enforcement ${enable ? 'enabled' : 'disabled'}.` });
      const r = await apiGet('/v1/admin/network/rules');
      if (r.ok && r.data.success) setNetworkRules(r.data.data || []);
    } else {
      setBanner({ type: 'err', text: res.data?.message || 'Failed' });
    }
  }

  async function triggerBackup() {
    setTriggeringBackup(true);
    const res = await apiPost('/v1/admin/backups', {});
    setTriggeringBackup(false);
    if (res.ok && res.data.success) {
      setBanner({ type: 'ok', text: 'Backup triggered.' });
      const r = await apiGet('/v1/admin/backups');
      if (r.ok && r.data.success) setBackups(r.data.data || []);
    } else {
      setBanner({ type: 'err', text: res.data?.message || 'Backup failed' });
    }
  }

  async function restoreBackup(id) {
    if (!confirm('Restore this backup? This will overwrite current workspace state.')) return;
    const res = await apiPost(`/v1/admin/backups/${id}/restore`, {});
    if (res.ok && res.data.success) {
      setBanner({ type: 'ok', text: 'Restore initiated.' });
    } else {
      setBanner({ type: 'err', text: res.data?.message || 'Restore failed' });
    }
  }

  async function deleteBackup(id) {
    if (!confirm('Delete backup? This cannot be undone.')) return;
    const res = await apiDelete(`/v1/admin/backups/${id}`);
    if (res.ok && res.data.success) {
      const r = await apiGet('/v1/admin/backups');
      if (r.ok && r.data.success) setBackups(r.data.data || []);
    } else {
      setBanner({ type: 'err', text: res.data?.message || 'Delete failed' });
    }
  }

  if (!isAdmin()) return <AccessDenied />;

  const active = NAV.find(n => n.id === section) || NAV[0];
  const groups = NAV.reduce((acc, n) => { (acc[n.group] ||= []).push(n); return acc; }, {});

  return (
    <div className="flex h-full w-full overflow-hidden bg-background">
      {/* ── Left rail ───────────────────────────────────────────── */}
      <aside className="hidden lg:flex lg:w-40 shrink-0 flex-col border-r border-border bg-sidebar/40">
        <div className="border-b border-border px-3 py-3">
          <div className="flex items-center gap-1 text-[10px] font-semibold uppercase tracking-[0.16em] text-muted-foreground">
            <Shield className="h-2 w-2" /> Administration
          </div>
          <h3 className="mt-0.5 font-serif text-[20px] font-medium tracking-tight">Control plane</h3>
        </div>
        <nav className="flex-1 overflow-y-auto p-1">
          {Object.entries(groups).map(([group, items]) => (
            <div key={group} className="mb-2">
              <p className="px-1.5 py-0.5 text-[9.5px] font-semibold uppercase tracking-[0.14em] text-muted-foreground/70">{group}</p>
              {items.map(item => {
                const Icon = item.icon;
                const on = section === item.id;
                return (
                  <NavLink key={item.id} to={`/admin/${item.id}`}
                    className={({ isActive }) => `group relative flex w-full items-center gap-1.5 rounded-md px-1.5 py-1 text-[13px] tracking-tight transition-colors ${
                      isActive ? 'bg-secondary text-foreground font-medium before:absolute before:left-0 before:top-2 before:bottom-2 before:w-[2px] before:rounded-full before:bg-primary'
                         : 'text-muted-foreground hover:text-foreground hover:bg-muted'
                    }`}>
                    <Icon className={`h-3 w-3 shrink-0 ${on ? 'text-primary' : ''}`} />
                    <span className="flex-1 text-left">{item.label}</span>
                    {on && <ChevronRight className="h-2 w-2 text-primary" />}
                  </NavLink>
                );
              })}
            </div>
          ))}
        </nav>
      </aside>

      {/* ── Main pane ───────────────────────────────────────────── */}
      <div className="flex min-w-0 flex-1 flex-col overflow-hidden">
        <header className="shrink-0 border-b border-border bg-card/30 px-4 pt-7 pb-3">
          <div className="flex items-end justify-between gap-3 flex-wrap">
            <div>
              <p className="text-[10px] font-semibold uppercase tracking-[0.16em] text-muted-foreground/70 mb-1">{active.group}</p>
              <h1 className="font-serif text-[30px] font-medium leading-[1.1] tracking-tight">{active.label}</h1>
            </div>
            <div className="flex items-center gap-1">
              <Button variant="outline" size="sm" onClick={refreshAll} className="gap-0.5.5 text-[12px]">
                <RefreshCw className="h-2.5 w-2.5" /> Refresh
              </Button>
              {section === 'users' && activeTeamId && (
                <Button size="sm" onClick={() => setInviteOpen(true)} className="gap-0.5.5 text-[12px]">
                  <UserPlus className="h-2.5 w-2.5" /> Invite
                </Button>
              )}
              {section === 'backups' && (
                <Button size="sm" onClick={triggerBackup} disabled={triggeringBackup} className="gap-0.5.5 text-[12px]">
                  {triggeringBackup ? <Loader2 className="h-2.5 w-2.5 animate-spin" /> : <Download className="h-2.5 w-2.5" />}
                  Trigger backup
                </Button>
              )}
            </div>
          </div>
        </header>

        {banner && (
          <div className={`mx-4 mt-3 flex items-start justify-between gap-2 rounded-md border px-3 py-1.5 text-[12px] ${
            banner.type === 'ok'
              ? 'border-success/30 bg-success/5 text-success'
              : 'border-destructive/30 bg-destructive/5 text-destructive'
          }`}>
            <span>{banner.text}</span>
            <button onClick={() => setBanner(null)} className="opacity-60 hover:opacity-100">×</button>
          </div>
        )}

        <div className="flex-1 overflow-y-auto px-4 py-4">
          <Outlet context={{
            metrics, members, branches, backups,
            teams, activeTeamId, setActiveTeamId: (id) => { setActiveTeamId(id); loadMembers(id); },
            search, setSearch,
            installedExts, availableExts, installExt, uninstallExt,
            newBranchName, setNewBranchName, createBranch, creatingBranch, deleteBranch,
            networkRules, toggleNetworkEnforcement,
            restoreBackup, deleteBackup,
            auditLogs
          }} />
        </div>
      </div>

      {/* Invite dialog */}
      <Dialog open={inviteOpen} onOpenChange={setInviteOpen}>
        <DialogContent className="sm:max-w-md">
          <DialogHeader>
            <DialogTitle className="font-serif text-base">Invite a member</DialogTitle>
            <DialogDescription className="text-[12px]">
              They'll receive an email with a link to join your workspace.
            </DialogDescription>
          </DialogHeader>
          <form onSubmit={handleInvite} className="space-y-3 pt-0.5">
            <div className="space-y-0.5.5">
              <Label className="text-[12px]">Email</Label>
              <Input type="email" placeholder="person@company.com" value={inviteEmail} onChange={e => setInviteEmail(e.target.value)} required />
            </div>
            <div className="space-y-0.5.5">
              <Label className="text-[12px]">Role</Label>
              <div className="grid grid-cols-2 gap-1">
                {ROLE_OPTIONS.map(r => (
                  <button key={r.value} type="button" onClick={() => setInviteRole(r.value)}
                    className={`rounded-md border p-2 text-left transition-all ${
                      inviteRole === r.value
                        ? 'border-primary bg-primary/5 ring-1 ring-primary'
                        : 'border-border bg-card hover:border-foreground/20'
                    }`}>
                    <p className="text-[12px] font-medium">{r.label}</p>
                    <p className="mt-0.5 text-[10.5px] text-muted-foreground leading-snug">{r.desc}</p>
                  </button>
                ))}
              </div>
            </div>
            <DialogFooter>
              <Button type="button" variant="outline" onClick={() => setInviteOpen(false)}>Cancel</Button>
              <Button type="submit" disabled={inviting}>{inviting ? <Loader2 className="h-3 w-3 animate-spin" /> : 'Send invite'}</Button>
            </DialogFooter>
          </form>
        </DialogContent>
      </Dialog>
    </div>
  );
}

/* ────────── Sub-views ────────── */

function StatCard({ label, value, hint }) {
  return (
    <div className="rounded-md border border-border bg-card p-3 transition-all hover:border-foreground/15 hover:shadow-none">
      <p className="text-[10px] font-semibold uppercase tracking-[0.14em] text-muted-foreground">{label}</p>
      <p className="mt-1 font-serif text-[30px] font-medium leading-none tracking-tight">{value}</p>
      {hint && <p className="mt-1 text-[11px] text-muted-foreground">{hint}</p>}
    </div>
  );
}

function Overview({ metrics, members, branches, backups }) {
  const uptimeSec = metrics?.uptime_seconds || 0;
  const uptimeFmt = uptimeSec < 3600
    ? `${Math.floor(uptimeSec / 60)}m`
    : uptimeSec < 86400
    ? `${Math.floor(uptimeSec / 3600)}h`
    : `${Math.floor(uptimeSec / 86400)}d`;

  return (
    <div className="space-y-3">
      <div className="grid gap-2 sm:grid-cols-2 lg:grid-cols-4">
        <StatCard label="Members"     value={members.length || '—'}  hint="Across this workspace" />
        <StatCard label="Branches"    value={branches.length || '—'} hint="Including main" />
        <StatCard label="Backups"     value={backups.length || '—'}  hint="Point-in-time snapshots" />
        <StatCard label="Uptime"      value={metrics ? uptimeFmt : '—'} hint="Since last restart" />
      </div>

      {metrics && (
        <div className="rounded-md border border-border bg-card p-3">
          <h3 className="font-serif text-[18px] font-medium tracking-tight mb-3">Live metrics</h3>
          <div className="grid gap-3 sm:grid-cols-2 lg:grid-cols-3">
            <MetricRow label="HTTP active connections" value={metrics.http?.active_connections ?? 0} />
            <MetricRow label="Total signups"           value={metrics.auth?.total_signups ?? 0} />
            <MetricRow label="Total logins"            value={metrics.auth?.total_logins ?? 0} />
            <MetricRow label="Failed logins"           value={metrics.auth?.failed_logins ?? 0} />
            <MetricRow label="Storage uploads"         value={metrics.storage?.total_uploads ?? 0} />
            <MetricRow label="Realtime subscriptions"  value={metrics.realtime?.active_subscriptions ?? 0} />
          </div>
        </div>
      )}
    </div>
  );
}

function MetricRow({ label, value }) {
  return (
    <div className="flex items-baseline justify-between border-b border-border/60 pb-1">
      <span className="text-[12px] text-muted-foreground">{label}</span>
      <span className="font-mono text-[13px] font-medium">{value.toLocaleString?.() ?? value}</span>
    </div>
  );
}

function EmptyState({ icon: Icon, title, desc, cta }) {
  return (
    <div className="rounded-md border-2 border-dashed border-border bg-muted/[0.03] p-5 text-center">
      <div className="mx-auto flex h-10 w-10 items-center justify-center rounded-md bg-background border border-border">
        <Icon className="h-4 w-4 text-muted-foreground/60" />
      </div>
      <h4 className="mt-3 text-[14px] font-medium">{title}</h4>
      <p className="mx-auto mt-0.5 max-w-xs text-[12px] text-muted-foreground">{desc}</p>
      {cta}
    </div>
  );
}

function Members({ teams, activeTeamId, setActiveTeamId, members, search, setSearch }) {
  const filtered = members.filter(u => (u.email || '').toLowerCase().includes(search.toLowerCase()));
  if (teams.length === 0) return <EmptyState icon={Users} title="No teams" desc="Create a team to invite members." />;
  return (
    <div className="space-y-3">
      <div className="flex flex-wrap gap-2 items-center">
        <select
          value={activeTeamId || ''}
          onChange={(e) => setActiveTeamId(Number(e.target.value))}
          className="h-8 rounded-md border border-border bg-card px-2 text-[12px]"
        >
          {teams.map(t => <option key={t.id} value={t.id}>{t.name}</option>)}
        </select>
        <div className="relative flex-1 max-w-sm">
          <Search className="absolute left-3 top-1/2 h-2.5 w-2.5 -translate-y-1/2 text-muted-foreground" />
          <Input placeholder="Search members" value={search} onChange={e => setSearch(e.target.value)} className="pl-9 h-8" />
        </div>
      </div>
      {filtered.length === 0 ? (
        <EmptyState icon={Users} title="No members yet" desc="Invite your first teammate to collaborate." />
      ) : (
        <div className="overflow-hidden rounded-md border border-border">
          <table className="w-full text-left">
            <thead className="bg-muted/40 text-[10px] uppercase tracking-[0.12em] text-muted-foreground">
              <tr>
                <th className="px-3 py-1.5 font-semibold">Member</th>
                <th className="px-3 py-1.5 font-semibold">Role</th>
                <th className="px-3 py-1.5 font-semibold">Joined</th>
              </tr>
            </thead>
            <tbody>
              {filtered.map(u => (
                <tr key={u.user_id || u.id} className="border-t border-border hover:bg-muted/20 transition-colors">
                  <td className="px-3 py-2">
                    <div className="flex items-center gap-2">
                      <div className="flex h-6 w-6 items-center justify-center rounded-full bg-secondary text-[10px] font-semibold">{initials(u.email)}</div>
                      <span className="text-[13px]">{u.email}</span>
                    </div>
                  </td>
                  <td className="px-3 py-2"><Badge variant="outline" className="text-[10px] capitalize">{u.role || 'Member'}</Badge></td>
                  <td className="px-3 py-2 text-[11.5px] text-muted-foreground">{u.joined_at ? new Date(u.joined_at).toLocaleDateString() : '—'}</td>
                </tr>
              ))}
            </tbody>
          </table>
        </div>
      )}
    </div>
  );
}

function Extensions({ installed, available, onInstall, onUninstall }) {
  const installedNames = new Set(installed.map(e => e.name));
  const toInstall = available.filter(e => !installedNames.has(e.name));
  return (
    <div className="space-y-4">
      <section>
        <h3 className="mb-2 text-[11px] font-semibold uppercase tracking-[0.14em] text-muted-foreground">Installed</h3>
        {installed.length === 0 ? (
          <EmptyState icon={Puzzle} title="No extensions installed" desc="Browse the catalog below to add functionality." />
        ) : (
          <div className="grid gap-2 md:grid-cols-2">
            {installed.map(ext => (
              <div key={ext.id || ext.name} className="rounded-md border border-border bg-card p-3 flex items-start justify-between gap-2">
                <div className="flex gap-2">
                  <div className="flex h-8 w-8 items-center justify-center rounded-md bg-primary/10 text-primary">
                    <Puzzle className="h-3 w-3" />
                  </div>
                  <div>
                    <p className="text-[13px] font-medium">{ext.name}</p>
                    <p className="text-[11px] text-muted-foreground">{ext.version || 'v1.0'}</p>
                    {ext.description && <p className="mt-0.5 text-[11.5px] text-muted-foreground">{ext.description}</p>}
                  </div>
                </div>
                <Button variant="ghost" size="sm" className="text-muted-foreground hover:text-destructive" onClick={() => onUninstall(ext.name)}>
                  <Trash2 className="h-2.5 w-2.5" />
                </Button>
              </div>
            ))}
          </div>
        )}
      </section>

      {toInstall.length > 0 && (
        <section>
          <h3 className="mb-2 text-[11px] font-semibold uppercase tracking-[0.14em] text-muted-foreground">Available</h3>
          <div className="grid gap-2 md:grid-cols-2">
            {toInstall.map(ext => (
              <div key={ext.name} className="rounded-md border border-border bg-card p-3 flex items-start justify-between gap-2">
                <div className="flex gap-2">
                  <div className="flex h-8 w-8 items-center justify-center rounded-md bg-muted text-muted-foreground">
                    <Puzzle className="h-3 w-3" />
                  </div>
                  <div>
                    <p className="text-[13px] font-medium">{ext.name}</p>
                    {ext.description && <p className="mt-0.5 text-[11.5px] text-muted-foreground">{ext.description}</p>}
                  </div>
                </div>
                <Button size="sm" onClick={() => onInstall(ext.name)} className="gap-0.5">
                  <Plus className="h-2.5 w-2.5" /> Install
                </Button>
              </div>
            ))}
          </div>
        </section>
      )}
    </div>
  );
}

function Branches({ branches, newName, setNewName, onCreate, creating, onDelete }) {
  return (
    <div className="space-y-3">
      <form onSubmit={onCreate} className="rounded-md border border-border bg-card p-3">
        <p className="text-[10px] font-semibold uppercase tracking-[0.14em] text-muted-foreground mb-1">Create branch</p>
        <div className="flex gap-1">
          <Input placeholder="feature/new-pipeline" value={newName} onChange={e => setNewName(e.target.value)} className="font-mono text-[13px]" required />
          <Button type="submit" disabled={creating} className="gap-0.5.5">
            {creating ? <Loader2 className="h-3 w-3 animate-spin" /> : <Plus className="h-3 w-3" />}
            Create
          </Button>
        </div>
      </form>
      {branches.length === 0 ? (
        <EmptyState icon={GitBranch} title="No branches yet" desc="Create a branch to iterate without affecting production." />
      ) : (
        <div className="space-y-1">
          {branches.map(b => (
            <div key={b.id || b.name} className="flex items-center justify-between rounded-md border border-border bg-card p-2.5">
              <div className="flex items-center gap-2">
                <GitBranch className="h-3 w-3 text-muted-foreground" />
                <span className="font-mono text-[13px]">{b.name}</span>
                {b.parent && <span className="text-[11px] text-muted-foreground">← {b.parent}</span>}
              </div>
              <div className="flex items-center gap-1">
                <Badge variant={b.active ? 'default' : 'secondary'} className="text-[10px]">{b.active ? 'Active' : 'Inactive'}</Badge>
                <Button variant="ghost" size="sm" className="text-muted-foreground hover:text-destructive" onClick={() => onDelete(b.name)}>
                  <Trash2 className="h-2.5 w-2.5" />
                </Button>
              </div>
            </div>
          ))}
        </div>
      )}
    </div>
  );
}

function NetworkPane({ rules, onEnable, onDisable }) {
  return (
    <div className="space-y-3">
      <div className="rounded-md border border-border bg-card p-3 flex items-center justify-between">
        <div>
          <p className="text-[13px] font-medium">Network enforcement</p>
          <p className="text-[11.5px] text-muted-foreground">Apply IP allow/deny rules at the request middleware.</p>
        </div>
        <div className="flex gap-1">
          <Button variant="outline" size="sm" className="gap-0.5.5" onClick={onDisable}>
            <Ban className="h-2.5 w-2.5" /> Disable
          </Button>
          <Button size="sm" className="gap-0.5.5" onClick={onEnable}>
            <CheckCircle2 className="h-2.5 w-2.5" /> Enable
          </Button>
        </div>
      </div>
      {rules.length === 0 ? (
        <EmptyState icon={Network} title="No rules configured" desc="Traffic is unrestricted until rules are added via the API." />
      ) : (
        <div className="space-y-1">
          {rules.map(r => (
            <div key={r.id} className="flex items-center justify-between rounded-md border border-border bg-card p-2.5">
              <div className="flex items-center gap-2">
                <div className={`h-1 w-1 rounded-full ${r.enabled ? 'bg-success' : 'bg-muted-foreground/40'}`} />
                <div>
                  <p className="text-[13px] font-medium">{r.name || r.pattern}</p>
                  <p className="text-[11px] text-muted-foreground">
                    <span className="font-mono">{r.action || 'allow'}</span> · <span className="font-mono">{r.pattern}</span>
                  </p>
                </div>
              </div>
            </div>
          ))}
        </div>
      )}
    </div>
  );
}

function Backups({ backups, onRestore, onDelete }) {
  if (backups.length === 0)
    return <EmptyState icon={Database} title="No backups yet" desc="Trigger your first backup to enable point-in-time recovery." />;
  return (
    <div className="overflow-hidden rounded-md border border-border">
      <table className="w-full text-left">
        <thead className="bg-muted/40 text-[10px] uppercase tracking-[0.12em] text-muted-foreground">
          <tr>
            <th className="px-3 py-1.5 font-semibold">Name</th>
            <th className="px-3 py-1.5 font-semibold">Created</th>
            <th className="px-3 py-1.5 font-semibold">Size</th>
            <th className="px-3 py-1.5"></th>
          </tr>
        </thead>
        <tbody>
          {backups.map(b => (
            <tr key={b.id} className="border-t border-border hover:bg-muted/20">
              <td className="px-3 py-2 text-[13px] font-mono">{b.name || b.id}</td>
              <td className="px-3 py-2 text-[11.5px] text-muted-foreground">{b.created_at ? new Date(b.created_at).toLocaleString() : '—'}</td>
              <td className="px-3 py-2 text-[12px]">{b.size || '—'}</td>
              <td className="px-3 py-2 text-right">
                <div className="flex items-center justify-end gap-0.5">
                  <Button variant="ghost" size="sm" onClick={() => onRestore(b.id)} title="Restore">
                    <RotateCcw className="h-2.5 w-2.5" />
                  </Button>
                  <Button variant="ghost" size="sm" className="text-muted-foreground hover:text-destructive" onClick={() => onDelete(b.id)}>
                    <Trash2 className="h-2.5 w-2.5" />
                  </Button>
                </div>
              </td>
            </tr>
          ))}
        </tbody>
      </table>
    </div>
  );
}

function AuditLog({ logs }) {
  if (logs.length === 0)
    return <EmptyState icon={ScrollText} title="No audit events" desc="Privileged actions will appear here as they occur." />;
  return (
    <div className="overflow-hidden rounded-md border border-border">
      <table className="w-full text-left">
        <thead className="bg-muted/40 text-[10px] uppercase tracking-[0.12em] text-muted-foreground">
          <tr>
            <th className="px-3 py-1.5 font-semibold">When</th>
            <th className="px-3 py-1.5 font-semibold">Action</th>
            <th className="px-3 py-1.5 font-semibold">Resource</th>
            <th className="px-3 py-1.5 font-semibold">Outcome</th>
          </tr>
        </thead>
        <tbody>
          {logs.map((log, i) => (
            <tr key={i} className="border-t border-border hover:bg-muted/20">
              <td className="px-3 py-2 text-[11.5px] text-muted-foreground font-mono whitespace-nowrap">{new Date(log.occurred_at).toLocaleString()}</td>
              <td className="px-3 py-2 text-[13px] font-medium">{log.action}</td>
              <td className="px-3 py-2 text-[12px] font-mono text-muted-foreground">
                {log.resource_type}{log.resource_id ? ` · ${log.resource_id}` : ''}
              </td>
              <td className="px-3 py-2">
                <Badge variant={log.outcome === 'success' ? 'default' : 'destructive'} className="text-[10px]">{log.outcome}</Badge>
              </td>
            </tr>
          ))}
        </tbody>
      </table>
    </div>
  );
}

function AccessDenied() {
  return (
    <div className="flex h-full items-center justify-center bg-background p-4">
      <div className="max-w-sm text-center">
        <div className="mx-auto flex h-12 w-12 items-center justify-center rounded-lg bg-destructive/10">
          <AlertTriangle className="h-5 w-5 text-destructive" />
        </div>
        <h2 className="mt-3 font-serif text-[22px] font-medium tracking-tight">Access denied</h2>
        <p className="mt-1 text-[13px] text-muted-foreground leading-relaxed">
          You need administrator privileges to view the control plane. Ask a workspace owner to elevate your role.
        </p>
      </div>
    </div>
  );
}
export function OverviewRoute() {
  const { metrics, members, branches, backups } = useOutletContext();
  return <Overview metrics={metrics} members={members} branches={branches} backups={backups} />;
}

export function MembersRoute() {
  const { teams, activeTeamId, setActiveTeamId, members, search, setSearch } = useOutletContext();
  return <Members teams={teams} activeTeamId={activeTeamId} setActiveTeamId={setActiveTeamId} members={members} search={search} setSearch={setSearch} />;
}

export function ExtensionsRoute() {
  const { installedExts, availableExts, installExt, uninstallExt } = useOutletContext();
  return <Extensions installed={installedExts} available={availableExts} onInstall={installExt} onUninstall={uninstallExt} />;
}

export function BranchesRoute() {
  const { branches, newName, setNewName, createBranch, creatingBranch, deleteBranch } = useOutletContext();
  return <Branches branches={branches} newName={newName} setNewName={setNewName} onCreate={createBranch} creating={creatingBranch} onDelete={deleteBranch} />;
}

export function NetworkRoute() {
  const { networkRules, toggleNetworkEnforcement } = useOutletContext();
  return <NetworkPane rules={networkRules} onEnable={() => toggleNetworkEnforcement(true)} onDisable={() => toggleNetworkEnforcement(false)} />;
}

export function BackupsRoute() {
  const { backups, restoreBackup, deleteBackup } = useOutletContext();
  return <Backups backups={backups} onRestore={restoreBackup} onDelete={deleteBackup} />;
}

export function AuditLogRoute() {
  const { auditLogs } = useOutletContext();
  return <AuditLog logs={auditLogs} />;
}
