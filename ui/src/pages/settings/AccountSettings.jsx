import React, { useState, useEffect } from 'react';
import { useAuth } from '@/context/AuthContext';
import { Check, KeyRound, Loader2 } from 'lucide-react';
import { Input } from '@/components/ui/input';
import { Button } from '@/components/ui/button';
import { Badge } from '@/components/ui/badge';
import { Row, SectionHeader } from './components';

export default function AccountSettings() {
  const { user, updateUser } = useAuth();
  const [displayName, setDisplayName] = useState('');
  const [savingProfile, setSavingProfile] = useState(false);
  const [banner, setBanner] = useState(null);

  const notify = (type, text) => {
    setBanner({ type, text });
    setTimeout(() => setBanner(null), 4000);
  };

  useEffect(() => {
    if (user?.metadata?.display_name) {
      setDisplayName(user.metadata.display_name);
    } else if (user?.email) {
      setDisplayName(user.email.split('@')[0]);
    }
  }, [user]);

  async function handleSaveProfile() {
    setSavingProfile(true);
    const res = await updateUser({ metadata: { ...(user.metadata || {}), display_name: displayName } });
    setSavingProfile(false);
    if (res.success) notify('ok', 'Profile updated.');
    else notify('err', res.message || 'Update failed');
  }

  function getInitials(email) {
    if (!email) return '??';
    const parts = email.split('@')[0].split(/[._-]+/);
    if (parts.length > 1) return (parts[0][0] + parts[1][0]).toUpperCase();
    return email.slice(0, 2).toUpperCase();
  }

  const roleLabel = user?.metadata?.role ? (user.metadata.role.charAt(0).toUpperCase() + user.metadata.role.slice(1)) : 'Member';

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
        eyebrow="Account"
        title="Your profile"
        description="Identity and organization details used across StackhouseBrain workspaces."
      />

      <div className="mb-4 overflow-hidden rounded-lg border border-border bg-card">
        <div className="h-12 bg-muted" />
        <div className="px-3 pb-3">
          <div className="flex flex-col sm:flex-row sm:items-end gap-3 -mt-4">
            <div className="flex h-12 w-12 shrink-0 items-center justify-center rounded-lg bg-primary text-primary-foreground text-[18px] font-semibold tracking-tight ring-2 ring-offset-2 ring-primary">
              {getInitials(user?.email)}
            </div>
            <div className="flex-1 min-w-0 sm:pb-0.5">
              <p className="font-serif text-[22px] font-medium leading-tight tracking-tight">
                {displayName || user?.email?.split('@')[0] || 'User'}
              </p>
              <p className="mt-0.5 flex items-center gap-0.5.5 text-[12px] text-muted-foreground">
                <span className="truncate">{user?.email}</span>
                <span>·</span>
                <Badge variant="outline" className="text-[10px] capitalize">{roleLabel}</Badge>
              </p>
            </div>
          </div>
        </div>
      </div>

      <Row title="Display name" description="Shown across the workspace and audit logs.">
        <div className="flex items-center gap-1 justify-end">
          <Input value={displayName} onChange={(e) => setDisplayName(e.target.value)} className="h-8 sm:w-[220px]" />
          <Button size="sm" onClick={handleSaveProfile} disabled={savingProfile} className="shrink-0">
            {savingProfile ? <Loader2 className="h-2.5 w-2.5 animate-spin" /> : 'Save'}
          </Button>
        </div>
      </Row>
      <Row title="Email" description="Primary address for security alerts. Verified on sign-in.">
        <div className="flex items-center gap-1 justify-end">
          <Input value={user?.email || ''} type="email" readOnly className="h-8 sm:w-[220px] bg-muted/40 font-mono text-[12px]" />
          <Badge variant="outline" className="text-[10px] gap-0.5 shrink-0"><Check className="h-1.5 w-1.5" /> Verified</Badge>
        </div>
      </Row>
      <Row title="Organization" description="Currently active tenant. Only owners can change the slug.">
        <Badge variant="brand">{user?.metadata?.org || 'default'}</Badge>
      </Row>
      <Row title="API access" description="Programmatic tokens for shipping and CI integrations.">
        <Button variant="outline" size="sm" className="gap-0.5.5 text-[12px]">
          <KeyRound className="h-2.5 w-2.5" /> Manage tokens
        </Button>
      </Row>
    </section>
  );
}
