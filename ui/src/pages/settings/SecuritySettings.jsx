import React, { useState, useEffect } from 'react';
import { useAuth } from '@/context/AuthContext';
import { apiGet, apiPost, apiDelete } from '@/lib/apiClient';
import { Shield, Check, AlertTriangle, Trash2, Loader2 } from 'lucide-react';
import { Button } from '@/components/ui/button';
import { Input } from '@/components/ui/input';
import { Label } from '@/components/ui/label';
import { Badge } from '@/components/ui/badge';
import { Separator } from '@/components/ui/separator';
import { Dialog, DialogContent, DialogHeader, DialogTitle, DialogFooter } from '@/components/ui/dialog';
import { Row, SectionHeader } from './components';

export default function SecuritySettings() {
  const { logout } = useAuth();
  const [banner, setBanner] = useState(null);

  const [sessions, setSessions] = useState([]);
  const [showSessions, setShowSessions] = useState(false);

  const [auditLogs, setAuditLogs] = useState([]);
  const [showAudit, setShowAudit] = useState(false);

  const [showMfa, setShowMfa] = useState(false);
  const [mfaStatus, setMfaStatus] = useState(null);
  const [mfaEnroll, setMfaEnroll] = useState(null);
  const [mfaCode, setMfaCode] = useState('');
  const [mfaLoading, setMfaLoading] = useState(false);
  const [mfaError, setMfaError] = useState('');

  const [showDelete, setShowDelete] = useState(false);
  const [deleteConfirm, setDeleteConfirm] = useState('');

  const notify = (type, text) => {
    setBanner({ type, text });
    setTimeout(() => setBanner(null), 4000);
  };

  useEffect(() => {
    loadSessions();
    loadMfaStatus();
  }, []);

  async function loadMfaStatus() {
    const res = await apiGet('/v1/auth/mfa/status');
    if (res.ok && res.data.success) setMfaStatus(res.data.data);
  }

  async function loadSessions() {
    try {
      const res = await apiGet('/v1/auth/sessions');
      if (res.ok && res.data.success) {
        setSessions(res.data.data || []);
      }
    } catch {
      setSessions([]);
    }
  }

  async function handleRevokeSession(id) {
    const res = await apiDelete(`/v1/auth/sessions/${id}`);
    if (res.ok && res.data.success) notify('ok', 'Session revoked.');
    else notify('err', res.data?.message || 'Revoke failed');
    loadSessions();
  }

  async function openMfaDialog() {
    setShowMfa(true);
    setMfaError('');
    setMfaCode('');
    loadMfaStatus();
  }

  async function startMfaEnroll() {
    setMfaLoading(true); setMfaError('');
    const res = await apiPost('/v1/auth/mfa/enroll', {});
    setMfaLoading(false);
    if (res.ok && res.data.success) setMfaEnroll(res.data.data);
    else setMfaError(res.data?.message || 'Enrollment failed');
  }

  async function verifyMfaEnroll(e) {
    e.preventDefault();
    setMfaLoading(true); setMfaError('');
    const res = await apiPost('/v1/auth/mfa/verify', { code: mfaCode });
    setMfaLoading(false);
    if (res.ok && res.data.success) {
      notify('ok', 'Two-factor auth enabled.');
      setShowMfa(false);
      setMfaEnroll(null);
      setMfaStatus({ enabled: true });
    } else {
      setMfaError(res.data?.message || 'Invalid code');
    }
  }

  async function disableMfa() {
    if (!confirm('Disable two-factor auth? Your account will be less secure.')) return;
    const res = await apiDelete('/v1/auth/mfa');
    if (res.ok && res.data.success) {
      notify('ok', 'Two-factor auth disabled.');
      setMfaStatus({ enabled: false });
    } else {
      notify('err', res.data?.message || 'Disable failed');
    }
  }

  async function confirmDeleteWorkspace() {
    const res = await apiPost('/v1/admin/workspace/delete', { confirm: 'DELETE' });
    if (res.ok && res.data.success) {
      notify('ok', 'Deletion scheduled. You will be signed out.');
      setTimeout(() => logout(), 1500);
    } else {
      notify('err', res.data?.message || 'Deletion requires a workspace owner.');
    }
    setShowDelete(false);
    setDeleteConfirm('');
  }

  async function handleLoadAudit() {
    setShowAudit(true);
    try {
      const res = await apiGet('/v1/admin/audit');
      if (res.ok && res.data.success) {
        setAuditLogs(res.data.data || []);
      }
    } catch {
      setAuditLogs([]);
    }
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
        eyebrow="Security"
        title="Access & integrity"
        description="Protect the workspace with strong auth, audit logs and destructive confirmations."
      />
      <Row title="Two-factor auth" description="Require a TOTP app at every sign-in.">
        {mfaStatus?.enabled ? (
          <div className="flex items-center gap-1 justify-end">
            <Badge variant="outline" className="gap-0.5 text-[10px]"><Check className="h-1.5 w-1.5" /> Enabled</Badge>
            <Button variant="outline" size="sm" onClick={disableMfa}>Disable</Button>
          </div>
        ) : (
          <Button variant="outline" size="sm" onClick={openMfaDialog}>Set up MFA</Button>
        )}
      </Row>
      <Row title="Active sessions" description={`${sessions.length} active session${sessions.length === 1 ? '' : 's'}. Review and revoke individually.`}>
        <Button variant="outline" size="sm" className="text-[12px]" onClick={() => setShowSessions(true)}>Review sessions</Button>
      </Row>
      <Row title="Audit log" description="Immutable record of every mutation made to the workspace.">
        <Button variant="outline" size="sm" className="text-[12px]" onClick={handleLoadAudit}>View log</Button>
      </Row>

      <Dialog open={showSessions} onOpenChange={setShowSessions}>
        <DialogContent className="sm:max-w-lg p-0 overflow-hidden">
          <DialogHeader className="px-3 pt-3 pb-2 border-b border-border">
            <DialogTitle className="font-serif text-[18px] font-medium tracking-tight">Active sessions</DialogTitle>
            <p className="text-[12px] text-muted-foreground">Revoke any session to force re-authentication on that device.</p>
          </DialogHeader>
          <div className="max-h-[360px] overflow-y-auto px-3 py-2 space-y-1">
            {sessions.length === 0 && (
              <div className="py-5 text-center">
                <Shield className="mx-auto h-5 w-5 text-muted-foreground/50" />
                <p className="mt-1 text-[12px] text-muted-foreground">No active sessions.</p>
              </div>
            )}
            {sessions.map((s) => (
              <div key={s.id || s.token_hash} className="flex items-center justify-between gap-2 rounded-md border border-border bg-card p-2.5">
                <div className="flex items-center gap-2 min-w-0">
                  <div className="flex h-6 w-6 shrink-0 items-center justify-center rounded-md bg-muted">
                    <Shield className="h-2.5 w-2.5 text-muted-foreground" />
                  </div>
                  <div className="min-w-0">
                    <p className="text-[13px] font-medium">{s.device || 'Unknown device'}</p>
                    <p className="text-[11px] text-muted-foreground truncate">
                      <span className="font-mono">{s.ip || '—'}</span> · {s.created_at ? new Date(s.created_at).toLocaleString() : 'unknown time'}
                    </p>
                  </div>
                </div>
                <Button variant="ghost" size="sm" className="text-destructive hover:bg-destructive/10 shrink-0" onClick={() => handleRevokeSession(s.id || s.token_hash)}>
                  Revoke
                </Button>
              </div>
            ))}
          </div>
        </DialogContent>
      </Dialog>

      <Dialog open={showAudit} onOpenChange={setShowAudit}>
        <DialogContent className="sm:max-w-2xl p-0 overflow-hidden">
          <DialogHeader className="px-3 pt-3 pb-2 border-b border-border">
            <DialogTitle className="font-serif text-[18px] font-medium tracking-tight">Audit log</DialogTitle>
            <p className="text-[12px] text-muted-foreground">Immutable record of privileged actions across the workspace.</p>
          </DialogHeader>
          <div className="max-h-[440px] overflow-y-auto">
            {auditLogs.length === 0 ? (
              <div className="py-5 text-center">
                <Shield className="mx-auto h-5 w-5 text-muted-foreground/50" />
                <p className="mt-1 text-[12px] text-muted-foreground">No audit events found.</p>
              </div>
            ) : (
              <table className="w-full text-left">
                <thead className="sticky top-0 bg-muted/40 backdrop-blur text-[10px] uppercase tracking-[0.12em] text-muted-foreground">
                  <tr>
                    <th className="px-3 py-1 font-semibold">When</th>
                    <th className="px-3 py-1 font-semibold">Action</th>
                    <th className="px-3 py-1 font-semibold">Resource</th>
                    <th className="px-3 py-1 font-semibold">Outcome</th>
                  </tr>
                </thead>
                <tbody>
                  {auditLogs.map((log, i) => (
                    <tr key={i} className="border-t border-border hover:bg-muted/20">
                      <td className="px-3 py-1.5 text-[11px] text-muted-foreground font-mono whitespace-nowrap">
                        {new Date(log.occurred_at).toLocaleString()}
                      </td>
                      <td className="px-3 py-1.5 text-[12.5px] font-medium">{log.action}</td>
                      <td className="px-3 py-1.5 text-[11.5px] font-mono text-muted-foreground">
                        {log.resource_type}{log.resource_id ? ` · ${log.resource_id}` : ''}
                      </td>
                      <td className="px-3 py-1.5">
                        <Badge variant={log.outcome === 'success' ? 'default' : 'destructive'} className="text-[10px]">{log.outcome}</Badge>
                      </td>
                    </tr>
                  ))}
                </tbody>
              </table>
            )}
          </div>
        </DialogContent>
      </Dialog>

      <div className="mt-5 rounded-md border border-destructive/30 bg-destructive/5 p-3">
        <div className="flex items-start gap-2">
          <AlertTriangle className="mt-0.5 h-3 w-3 shrink-0 text-destructive" />
          <div className="flex-1">
            <p className="font-serif text-[17px] font-medium tracking-tight text-foreground">Danger zone</p>
            <p className="mt-0.5 text-[12px] leading-relaxed text-muted-foreground">These actions are irreversible. Proceed with care.</p>
          </div>
        </div>
        <Separator className="my-3 bg-destructive/20" />
        <div className="flex flex-col gap-2 sm:flex-row sm:items-center sm:justify-between">
          <div>
            <p className="text-[13px] font-medium tracking-tight text-foreground">Delete workspace</p>
            <p className="mt-0.5 text-[11px] text-muted-foreground">Permanently removes all data and members.</p>
          </div>
          <Button variant="outline" size="sm" className="gap-0.5.5 border-destructive/40 text-destructive hover:bg-destructive/10 hover:text-destructive" onClick={() => setShowDelete(true) }>
            <Trash2 className="h-2.5 w-2.5" /> Delete
          </Button>
        </div>
      </div>

      <Dialog open={showMfa} onOpenChange={(open) => { setShowMfa(open); if (!open) { setMfaEnroll(null); setMfaCode(''); setMfaError(''); } }}>
        <DialogContent className="sm:max-w-md">
          <DialogHeader>
            <DialogTitle className="font-serif text-base">Two-factor authentication</DialogTitle>
            <p className="text-[12px] text-muted-foreground">
              Scan the QR code with an authenticator app, then enter the 6-digit code to confirm.
            </p>
          </DialogHeader>
          {!mfaEnroll ? (
            <div className="space-y-3 py-1">
              <p className="text-[13px] text-muted-foreground">
                You'll need an authenticator app like 1Password, Authy, or Google Authenticator.
              </p>
              {mfaError && <div className="text-[12px] text-destructive">{mfaError}</div>}
              <Button onClick={startMfaEnroll} disabled={mfaLoading} className="w-full">
                {mfaLoading ? <Loader2 className="h-3 w-3 animate-spin" /> : 'Begin enrollment'}
              </Button>
            </div>
          ) : (
            <form onSubmit={verifyMfaEnroll} className="space-y-3 py-1">
              <div className="rounded-md border border-border bg-muted/30 p-3 text-center">
                <p className="text-[11px] uppercase tracking-[0.14em] text-muted-foreground mb-1">Secret</p>
                <p className="font-mono text-[13px] break-all select-all">{mfaEnroll.secret}</p>
                <p className="mt-2 text-[11px] text-muted-foreground">or use URL</p>
                <p className="mt-0.5 font-mono text-[10px] break-all text-muted-foreground select-all">{mfaEnroll.otpauth_url}</p>
              </div>
              {mfaEnroll.recovery_codes?.length > 0 && (
                <div className="rounded-md border border-success/30 bg-success/5 p-2">
                  <p className="text-[11px] font-medium text-success mb-1">Save these recovery codes in a safe place:</p>
                  <div className="grid grid-cols-2 gap-0.5 font-mono text-[11px] text-foreground">
                    {mfaEnroll.recovery_codes.map((code) => <span key={code} className="select-all">{code}</span>)}
                  </div>
                </div>
              )}
              <div className="space-y-0.5.5">
                <Label className="text-[12px]">6-digit code from app</Label>
                <Input
                  type="text" inputMode="numeric" maxLength={6}
                  value={mfaCode}
                  onChange={(e) => setMfaCode(e.target.value.replace(/\D/g, ''))}
                  placeholder="000000"
                  className="h-10 text-center text-base font-mono tracking-[0.4em]"
                  required
                />
              </div>
              {mfaError && <div className="text-[12px] text-destructive">{mfaError}</div>}
              <DialogFooter>
                <Button type="button" variant="outline" onClick={() => setShowMfa(false)}>Cancel</Button>
                <Button type="submit" disabled={mfaLoading || mfaCode.length < 6}>
                  {mfaLoading ? <Loader2 className="h-3 w-3 animate-spin" /> : 'Verify & enable'}
                </Button>
              </DialogFooter>
            </form>
          )}
        </DialogContent>
      </Dialog>

      <Dialog open={showDelete} onOpenChange={(open) => { setShowDelete(open); if (!open) setDeleteConfirm(''); }}>
        <DialogContent className="sm:max-w-md">
          <DialogHeader>
            <DialogTitle className="font-serif text-base text-destructive">Delete workspace</DialogTitle>
            <p className="text-[12px] text-muted-foreground">
              This permanently removes all data and members. This action cannot be undone.
            </p>
          </DialogHeader>
          <div className="space-y-2 py-1">
            <div className="rounded-md border border-destructive/30 bg-destructive/5 p-2 text-[12px] text-destructive">
              <AlertTriangle className="inline h-2.5 w-2.5 mr-0.5" />
              Only workspace owners can perform this action.
            </div>
            <div className="space-y-0.5.5">
              <Label className="text-[12px]">Type <span className="font-mono font-medium text-foreground">DELETE</span> to confirm</Label>
              <Input value={deleteConfirm} onChange={(e) => setDeleteConfirm(e.target.value)} className="font-mono" />
            </div>
          </div>
          <DialogFooter>
            <Button variant="outline" onClick={() => setShowDelete(false)}>Cancel</Button>
            <Button
              onClick={confirmDeleteWorkspace}
              disabled={deleteConfirm !== 'DELETE'}
              className="bg-destructive text-destructive-foreground hover:bg-destructive/90"
            >
              <Trash2 className="h-2.5 w-2.5 mr-0.5.5" /> Delete forever
            </Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>
    </section>
  );
}
