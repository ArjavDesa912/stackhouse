import React, { useCallback, useEffect, useMemo, useState } from 'react'
import {
  Archive,
  ArchiveRestore,
  Database,
  Plus,
  RotateCw,
  Trash2,
} from 'lucide-react'

import {
  Banner,
  Empty,
  InlineSpinner,
  MetricTile,
  Overline,
  Section,
  apiFetch,
  formatBytes,
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

export default function BackupsModule() {
  const [backups, setBackups] = useState([])
  const [loading, setLoading] = useState(false)
  const [error, setError] = useState('')
  const [message, setMessage] = useState('')
  const [authError, setAuthError] = useState(false)
  const [creating, setCreating] = useState(false)
  const [createOpen, setCreateOpen] = useState(false)
  const [newName, setNewName] = useState('')
  const [confirmAction, setConfirmAction] = useState(null) // { kind: 'restore'|'delete', backup }
  const [busyId, setBusyId] = useState(null)

  const load = useCallback(async () => {
    setLoading(true)
    setError('')
    try {
      const payload = await apiFetch('/v1/admin/backups')
      setBackups(Array.isArray(payload?.data) ? payload.data : [])
      setAuthError(false)
    } catch (e) {
      if (e.kind === 'auth') setAuthError(true)
      else setError(e.message)
    } finally {
      setLoading(false)
    }
  }, [])

  useEffect(() => {
    load()
  }, [load])

  const totalSize = useMemo(
    () => backups.reduce((sum, b) => sum + (b.size_bytes || 0), 0),
    [backups],
  )

  const handleCreate = async () => {
    if (!newName.trim()) return
    setCreating(true)
    setError('')
    setMessage('')
    try {
      await apiFetch('/v1/admin/backups', {
        method: 'POST',
        body: JSON.stringify({ name: newName.trim() }),
      })
      setMessage(`Backup '${newName.trim()}' created.`)
      setNewName('')
      setCreateOpen(false)
      await load()
    } catch (e) {
      setError(e.message)
    } finally {
      setCreating(false)
    }
  }

  const confirm = async () => {
    if (!confirmAction) return
    const { kind, backup } = confirmAction
    setBusyId(backup.id)
    setError('')
    setMessage('')
    try {
      if (kind === 'restore') {
        await apiFetch(`/v1/admin/backups/${backup.id}/restore`, { method: 'POST' })
        setMessage(`Restored backup '${backup.name}'.`)
      } else if (kind === 'delete') {
        await apiFetch(`/v1/admin/backups/${backup.id}`, { method: 'DELETE' })
        setMessage(`Deleted backup '${backup.name}'.`)
      }
      setConfirmAction(null)
      await load()
    } catch (e) {
      setError(e.message)
    } finally {
      setBusyId(null)
    }
  }

  return (
    <div className="h-full overflow-y-auto">
      <div className="space-y-3 p-3">
        <div className="flex flex-wrap items-center justify-between gap-2">
          <div className="flex items-center gap-2">
            <div className="flex h-8 w-8 items-center justify-center rounded-md bg-primary/10 text-primary ring-1 ring-primary/20">
              <Archive className="h-3 w-3" />
            </div>
            <div>
              <Overline>Backups</Overline>
              <p className="font-serif text-[20px] font-medium leading-tight tracking-tight text-foreground">
                Logical SQL dumps
              </p>
            </div>
          </div>
          <div className="flex items-center gap-1">
            <Button
              size="sm"
              variant="outline"
              onClick={load}
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
              <Plus className="h-2 w-2" /> New backup
            </Button>
          </div>
        </div>

        {authError ? (
          <Banner kind="auth">
            /v1/admin/backups requires service-admin authentication. Sign in with an admin bearer token.
          </Banner>
        ) : null}
        {error ? <Banner kind="error">{error}</Banner> : null}
        {message ? <Banner kind="success">{message}</Banner> : null}

        <div className="grid gap-2 sm:grid-cols-3">
          <MetricTile label="Backups" value={backups.length} tone="accent" />
          <MetricTile label="Total size" value={formatBytes(totalSize)} />
          <MetricTile
            label="Latest"
            value={
              backups.length > 0
                ? formatRelative(
                    backups.reduce((max, b) => (b.created_at > max ? b.created_at : max), 0),
                  )
                : '—'
            }
          />
        </div>

        <Section title="All backups" description="Stored as .sql dumps in the stackhouse backup path.">
          {backups.length === 0 && !authError ? (
            <Empty
              icon={Database}
              title="No backups yet"
              description="Create your first backup to snapshot the current state of every table."
              action={
                <Button size="sm" onClick={() => setCreateOpen(true)} className="rounded-full">
                  <Plus className="h-2 w-2" /> Create backup
                </Button>
              }
            />
          ) : (
            <div className="overflow-hidden rounded-md border border-border">
              <table className="min-w-full text-left text-[12px]">
                <thead className="bg-muted/30">
                  <tr className="border-b border-border">
                    {['Name', 'ID', 'Size', 'Created', 'Actions'].map((h) => (
                      <th
                        key={h}
                        className="px-2 py-1 text-[10px] font-medium uppercase tracking-[0.12em] text-muted-foreground"
                      >
                        {h}
                      </th>
                    ))}
                  </tr>
                </thead>
                <tbody>
                  {backups.map((backup, index) => (
                    <tr
                      key={backup.id}
                      className={cn(index !== backups.length - 1 && 'border-b border-border/60')}
                    >
                      <td className="px-2 py-1.5 font-medium text-foreground">{backup.name}</td>
                      <td className="px-2 py-1.5 font-mono text-[10.5px] text-muted-foreground">
                        {String(backup.id).slice(0, 8)}…
                      </td>
                      <td className="px-2 py-1.5 font-mono text-[11px] text-muted-foreground tabular-nums">
                        {formatBytes(backup.size_bytes)}
                      </td>
                      <td className="px-2 py-1.5 text-[11.5px] text-muted-foreground">
                        {formatRelative(backup.created_at)}
                      </td>
                      <td className="px-2 py-1.5">
                        <div className="flex items-center gap-0.5">
                          <Button
                            size="sm"
                            variant="outline"
                            disabled={busyId === backup.id}
                            onClick={() => setConfirmAction({ kind: 'restore', backup })}
                            className="h-6 gap-0.5.5 rounded-md text-[11px]"
                          >
                            <ArchiveRestore className="h-2 w-2" />
                            Restore
                          </Button>
                          <Button
                            size="sm"
                            variant="ghost"
                            disabled={busyId === backup.id}
                            onClick={() => setConfirmAction({ kind: 'delete', backup })}
                            className="h-6 gap-0.5.5 rounded-md text-[11px] text-destructive hover:bg-destructive/10"
                          >
                            <Trash2 className="h-2 w-2" />
                            Delete
                          </Button>
                        </div>
                      </td>
                    </tr>
                  ))}
                </tbody>
              </table>
            </div>
          )}
        </Section>
      </div>

      {/* Create dialog */}
      <Dialog open={createOpen} onOpenChange={setCreateOpen}>
        <DialogContent>
          <DialogHeader>
            <DialogTitle className="font-serif">Create backup</DialogTitle>
            <DialogDescription>
              Names a new SQL dump. The backup is written to the server backup path immediately.
            </DialogDescription>
          </DialogHeader>
          <div className="space-y-1">
            <Overline>Backup name</Overline>
            <Input
              value={newName}
              onChange={(e) => setNewName(e.target.value)}
              placeholder="nightly-2026-04-19"
              className="font-mono"
            />
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

      {/* Confirm restore/delete */}
      <Dialog open={Boolean(confirmAction)} onOpenChange={(v) => !v && setConfirmAction(null)}>
        <DialogContent>
          <DialogHeader>
            <DialogTitle className="font-serif">
              {confirmAction?.kind === 'restore' ? 'Restore backup' : 'Delete backup'}
            </DialogTitle>
            <DialogDescription>
              {confirmAction?.kind === 'restore'
                ? `Restore '${confirmAction?.backup?.name}'? This will replay the SQL dump into the live database.`
                : `Delete '${confirmAction?.backup?.name}' permanently? The dump file will be removed from disk.`}
            </DialogDescription>
          </DialogHeader>
          <DialogFooter>
            <Button variant="outline" onClick={() => setConfirmAction(null)} disabled={Boolean(busyId)}>
              Cancel
            </Button>
            <Button
              variant={confirmAction?.kind === 'delete' ? 'destructive' : 'default'}
              onClick={confirm}
              disabled={Boolean(busyId)}
            >
              {busyId ? 'Working…' : confirmAction?.kind === 'restore' ? 'Restore' : 'Delete'}
            </Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>
    </div>
  )
}
