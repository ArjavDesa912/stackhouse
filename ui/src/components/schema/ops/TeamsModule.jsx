import React, { useCallback, useEffect, useState } from 'react'
import { ChevronDown, Crown, Mail, Plus, RotateCw, Shield, UserPlus, Users } from 'lucide-react'

import {
  Banner,
  Empty,
  InlineSpinner,
  MetricTile,
  Overline,
  Section,
  apiFetch,
  formatRelative,
} from './common'
import { Button } from '@/components/ui/button'
import { Input } from '@/components/ui/input'
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from '@/components/ui/dialog'
import { cn } from '@/lib/utils'

const ROLES = ['readonly', 'audit', 'developer', 'admin', 'owner']

const ROLE_STYLES = {
  owner: 'bg-primary/15 text-primary',
  admin: 'bg-primary/10 text-primary',
  developer: 'bg-success/10 text-success dark:text-success',
  audit: 'bg-muted text-muted-foreground',
  readonly: 'bg-muted text-muted-foreground',
}

export default function TeamsModule() {
  const [teams, setTeams] = useState([])
  const [loading, setLoading] = useState(false)
  const [error, setError] = useState('')
  const [message, setMessage] = useState('')
  const [authError, setAuthError] = useState(false)
  const [selectedTeam, setSelectedTeam] = useState(null)
  const [members, setMembers] = useState([])
  const [membersLoading, setMembersLoading] = useState(false)
  const [membersError, setMembersError] = useState('')
  const [createOpen, setCreateOpen] = useState(false)
  const [newName, setNewName] = useState('')
  const [creating, setCreating] = useState(false)
  const [inviteOpen, setInviteOpen] = useState(false)
  const [inviteEmail, setInviteEmail] = useState('')
  const [inviteRole, setInviteRole] = useState('developer')
  const [inviting, setInviting] = useState(false)
  const [inviteToken, setInviteToken] = useState('')

  const loadTeams = useCallback(async () => {
    setLoading(true)
    setError('')
    try {
      const payload = await apiFetch('/v1/teams')
      const list = Array.isArray(payload?.data) ? payload.data : []
      setTeams(list)
      setAuthError(false)
      if (list.length > 0 && !selectedTeam) setSelectedTeam(list[0])
    } catch (e) {
      if (e.kind === 'auth') setAuthError(true)
      else setError(e.message)
    } finally {
      setLoading(false)
    }
  }, [selectedTeam])

  useEffect(() => {
    loadTeams()
  }, [loadTeams])

  const loadMembers = useCallback(async (team) => {
    if (!team) return
    setMembersLoading(true)
    setMembersError('')
    setMembers([])
    try {
      const payload = await apiFetch(`/v1/teams/${team.id}/members`)
      setMembers(Array.isArray(payload?.data) ? payload.data : [])
    } catch (e) {
      setMembersError(e.message)
    } finally {
      setMembersLoading(false)
    }
  }, [])

  useEffect(() => {
    if (selectedTeam) loadMembers(selectedTeam)
  }, [selectedTeam, loadMembers])

  const handleCreate = async () => {
    if (!newName.trim()) return
    setCreating(true)
    setError('')
    setMessage('')
    try {
      await apiFetch('/v1/teams', {
        method: 'POST',
        body: JSON.stringify({ name: newName.trim() }),
      })
      setMessage(`Team '${newName.trim()}' created.`)
      setNewName('')
      setCreateOpen(false)
      await loadTeams()
    } catch (e) {
      setError(e.message)
    } finally {
      setCreating(false)
    }
  }

  const handleInvite = async () => {
    if (!selectedTeam || !inviteEmail.trim()) return
    setInviting(true)
    setError('')
    setMessage('')
    setInviteToken('')
    try {
      const payload = await apiFetch('/v1/teams/invite', {
        method: 'POST',
        body: JSON.stringify({
          team_id: selectedTeam.id,
          email: inviteEmail.trim(),
          role: inviteRole,
        }),
      })
      setInviteToken(payload?.data?.invite_token || '')
      setMessage(`Invitation for ${inviteEmail.trim()} generated.`)
      setInviteEmail('')
    } catch (e) {
      setError(e.message)
    } finally {
      setInviting(false)
    }
  }

  return (
    <div className="h-full overflow-y-auto">
      <div className="space-y-3 p-3">
        <div className="flex flex-wrap items-center justify-between gap-2">
          <div className="flex items-center gap-2">
            <div className="flex h-8 w-8 items-center justify-center rounded-md bg-primary/10 text-primary ring-1 ring-primary/20">
              <Users className="h-3 w-3" />
            </div>
            <div>
              <Overline>Teams</Overline>
              <p className="font-serif text-[20px] font-medium leading-tight tracking-tight text-foreground">
                Membership & roles
              </p>
            </div>
          </div>
          <div className="flex items-center gap-1">
            <Button
              size="sm"
              variant="outline"
              onClick={loadTeams}
              disabled={loading}
              className="h-6 gap-0.5.5 rounded-full text-[11.5px]"
            >
              {loading ? <InlineSpinner /> : <RotateCw className="h-2 w-2" />}
              Refresh
            </Button>
            <Button
              size="sm"
              onClick={() => setCreateOpen(true)}
              disabled={authError}
              className="h-6 gap-0.5.5 rounded-full text-[11.5px]"
            >
              <Plus className="h-2 w-2" /> New team
            </Button>
          </div>
        </div>

        {authError ? (
          <Banner kind="auth">/v1/teams requires an authenticated bearer token.</Banner>
        ) : null}
        {error ? <Banner kind="error">{error}</Banner> : null}
        {message ? <Banner kind="success">{message}</Banner> : null}

        <div className="grid gap-2 sm:grid-cols-3">
          <MetricTile label="Teams" value={teams.length} tone="accent" />
          <MetricTile label="Active team" value={selectedTeam?.name ?? '—'} />
          <MetricTile label="Members (active team)" value={members.length} />
        </div>

        <div className="grid gap-3 lg:grid-cols-[1fr_1.6fr]">
          <Section title="Your teams" description="Teams you are a member of.">
            {teams.length === 0 && !authError ? (
              <Empty
                icon={Users}
                title="Not in any teams"
                description="Create a team to organize access and workspaces."
              />
            ) : (
              <div className="space-y-0.5">
                {teams.map((team) => {
                  const active = selectedTeam?.id === team.id
                  return (
                    <button
                      key={team.id}
                      type="button"
                      onClick={() => setSelectedTeam(team)}
                      className={cn(
                        'flex w-full items-center justify-between gap-2 rounded-md border px-2 py-1 text-left transition-colors',
                        active
                          ? 'border-primary/30 bg-primary/5'
                          : 'border-border bg-background hover:bg-muted/30',
                      )}
                    >
                      <div className="min-w-0">
                        <p className="truncate text-[13px] font-medium text-foreground">{team.name}</p>
                        <p className="truncate font-mono text-[10.5px] text-muted-foreground">
                          {team.slug || `team-${team.id}`}
                        </p>
                      </div>
                      <span
                        className={cn(
                          'shrink-0 rounded-full px-1 py-0.5 text-[10px] font-medium uppercase tracking-[0.1em]',
                          ROLE_STYLES[team.role] || 'bg-muted text-muted-foreground',
                        )}
                      >
                        {team.role || 'member'}
                      </span>
                    </button>
                  )
                })}
              </div>
            )}
          </Section>

          <Section
            title={selectedTeam ? `Members · ${selectedTeam.name}` : 'Members'}
            description="All users with access to the selected team."
            action={
              selectedTeam ? (
                <Button
                  size="sm"
                  variant="outline"
                  onClick={() => setInviteOpen(true)}
                  className="h-6 gap-0.5.5 rounded-full text-[11.5px]"
                >
                  <UserPlus className="h-2 w-2" />
                  Invite
                </Button>
              ) : null
            }
          >
            {membersError ? <Banner kind="error">{membersError}</Banner> : null}
            {!selectedTeam ? (
              <Empty icon={Users} title="Select a team" description="Pick a team on the left to view its members." />
            ) : membersLoading ? (
              <p className="flex items-center gap-1 text-[12.5px] text-muted-foreground">
                <InlineSpinner /> Loading members…
              </p>
            ) : members.length === 0 ? (
              <Empty icon={Users} title="No members" description="Invite someone to get started." />
            ) : (
              <div className="space-y-1">
                {members.map((member) => (
                  <div
                    key={`${member.id}-${member.email}`}
                    className="flex items-center justify-between gap-2 rounded-md border border-border bg-background p-2"
                  >
                    <div className="flex min-w-0 items-center gap-2">
                      <div className="flex h-6 w-6 shrink-0 items-center justify-center rounded-full bg-muted font-medium text-[11px] uppercase text-muted-foreground">
                        {(member.email || 'user').slice(0, 2)}
                      </div>
                      <div className="min-w-0">
                        <p className="truncate text-[13px] font-medium text-foreground">
                          {member.email || `user-${member.id}`}
                        </p>
                        <p className="truncate text-[10.5px] text-muted-foreground">
                          joined {formatRelative(member.joined_at)}
                        </p>
                      </div>
                    </div>
                    <span
                      className={cn(
                        'inline-flex items-center gap-0.5 rounded-full px-1 py-0.5 text-[10.5px] font-medium uppercase tracking-[0.1em]',
                        ROLE_STYLES[member.role] || 'bg-muted text-muted-foreground',
                      )}
                    >
                      {member.role === 'owner' ? <Crown className="h-2 w-2" /> : member.role === 'admin' ? <Shield className="h-2 w-2" /> : null}
                      {member.role || 'member'}
                    </span>
                  </div>
                ))}
              </div>
            )}
          </Section>
        </div>
      </div>

      {/* Create team */}
      <Dialog open={createOpen} onOpenChange={setCreateOpen}>
        <DialogContent>
          <DialogHeader>
            <DialogTitle className="font-serif">Create team</DialogTitle>
            <DialogDescription>
              You will be added as the owner automatically.
            </DialogDescription>
          </DialogHeader>
          <div className="space-y-1">
            <Overline>Team name</Overline>
            <Input value={newName} onChange={(e) => setNewName(e.target.value)} placeholder="Engineering" />
          </div>
          <DialogFooter>
            <Button variant="outline" onClick={() => setCreateOpen(false)} disabled={creating}>
              Cancel
            </Button>
            <Button onClick={handleCreate} disabled={!newName.trim() || creating}>
              {creating ? 'Creating…' : 'Create'}
            </Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>

      {/* Invite */}
      <Dialog
        open={inviteOpen}
        onOpenChange={(v) => {
          setInviteOpen(v)
          if (!v) setInviteToken('')
        }}
      >
        <DialogContent>
          <DialogHeader>
            <DialogTitle className="font-serif">Invite to {selectedTeam?.name}</DialogTitle>
            <DialogDescription>
              Generates a signed invite token valid for 7 days.
            </DialogDescription>
          </DialogHeader>
          <div className="space-y-2">
            <div className="space-y-0.5.5">
              <Overline>Email</Overline>
              <div className="relative">
                <Mail className="pointer-events-none absolute left-2.5 top-1/2 h-2.5 w-2.5 -translate-y-1/2 text-muted-foreground" />
                <Input
                  value={inviteEmail}
                  onChange={(e) => setInviteEmail(e.target.value)}
                  placeholder="teammate@company.com"
                  className="pl-4"
                />
              </div>
            </div>
            <div className="space-y-0.5.5">
              <Overline>Role</Overline>
              <div className="relative">
                <select
                  value={inviteRole}
                  onChange={(e) => setInviteRole(e.target.value)}
                  className="h-8 w-full appearance-none rounded-md border border-border bg-background pl-2 pr-4 font-mono text-[12px] text-foreground outline-none focus-visible:ring-2 focus-visible:ring-primary/30"
                >
                  {ROLES.map((role) => (
                    <option key={role} value={role}>
                      {role}
                    </option>
                  ))}
                </select>
                <ChevronDown className="pointer-events-none absolute right-2.5 top-1/2 h-2 w-2 -translate-y-1/2 text-muted-foreground" />
              </div>
            </div>
            {inviteToken ? (
              <div className="space-y-0.5.5">
                <Overline>Invite token</Overline>
                <div className="flex items-center gap-1">
                  <code className="flex-1 truncate rounded-md border border-border bg-muted/30 px-1 py-0.5.5 font-mono text-[11px] text-foreground">
                    {inviteToken}
                  </code>
                  <Button
                    size="sm"
                    variant="outline"
                    onClick={() => navigator.clipboard?.writeText(inviteToken)}
                    className="h-6 text-[11px]"
                  >
                    Copy
                  </Button>
                </div>
              </div>
            ) : null}
          </div>
          <DialogFooter>
            <Button variant="outline" onClick={() => setInviteOpen(false)} disabled={inviting}>
              Close
            </Button>
            <Button onClick={handleInvite} disabled={!inviteEmail.trim() || inviting}>
              {inviting ? 'Generating…' : 'Send invite'}
            </Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>
    </div>
  )
}
