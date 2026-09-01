import React, { useCallback, useEffect, useState } from 'react'
import { ArrowRight, GitBranch, Minus, Plus, RotateCw, Trash2 } from 'lucide-react'

import {
  Banner,
  Empty,
  InlineSpinner,
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

export default function BranchesModule() {
  const [branches, setBranches] = useState([])
  const [loading, setLoading] = useState(false)
  const [error, setError] = useState('')
  const [message, setMessage] = useState('')
  const [authError, setAuthError] = useState(false)
  const [createOpen, setCreateOpen] = useState(false)
  const [newName, setNewName] = useState('')
  const [newSource, setNewSource] = useState('public')
  const [creating, setCreating] = useState(false)
  const [deletingName, setDeletingName] = useState(null)
  const [diffTarget, setDiffTarget] = useState(null)
  const [diffResult, setDiffResult] = useState(null)
  const [diffLoading, setDiffLoading] = useState(false)

  const load = useCallback(async () => {
    setLoading(true)
    setError('')
    try {
      const payload = await apiFetch('/v1/admin/branches')
      setBranches(Array.isArray(payload?.data) ? payload.data : [])
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

  const handleCreate = async () => {
    if (!newName.trim()) return
    setCreating(true)
    setError('')
    setMessage('')
    try {
      await apiFetch('/v1/admin/branches', {
        method: 'POST',
        body: JSON.stringify({ name: newName.trim(), source: newSource.trim() || 'public' }),
      })
      setMessage(`Branch '${newName.trim()}' created from '${newSource.trim() || 'public'}'.`)
      setNewName('')
      setCreateOpen(false)
      await load()
    } catch (e) {
      setError(e.message)
    } finally {
      setCreating(false)
    }
  }

  const handleDelete = async (name) => {
    setDeletingName(name)
    setError('')
    setMessage('')
    try {
      await apiFetch(`/v1/admin/branches/${encodeURIComponent(name)}`, { method: 'DELETE' })
      setMessage(`Branch '${name}' deleted.`)
      await load()
    } catch (e) {
      setError(e.message)
    } finally {
      setDeletingName(null)
    }
  }

  const openDiff = async (name) => {
    setDiffTarget(name)
    setDiffLoading(true)
    setDiffResult(null)
    try {
      const payload = await apiFetch(`/v1/admin/branches/${encodeURIComponent(name)}/diff`)
      setDiffResult(payload?.data || null)
    } catch (e) {
      setDiffResult({ error: e.message })
    } finally {
      setDiffLoading(false)
    }
  }

  return (
    <div className="h-full overflow-y-auto">
      <div className="space-y-3 p-3">
        <div className="flex flex-wrap items-center justify-between gap-2">
          <div className="flex items-center gap-2">
            <div className="flex h-8 w-8 items-center justify-center rounded-md bg-primary/10 text-primary ring-1 ring-primary/20">
              <GitBranch className="h-3 w-3" />
            </div>
            <div>
              <Overline>Branches</Overline>
              <p className="font-serif text-[20px] font-medium leading-tight tracking-tight text-foreground">
                Schema-level branching
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
              <Plus className="h-2 w-2" />
              New branch
            </Button>
          </div>
        </div>

        {authError ? (
          <Banner kind="auth">
            /v1/admin/branches requires service-admin authentication.
          </Banner>
        ) : null}
        {error ? <Banner kind="error">{error}</Banner> : null}
        {message ? <Banner kind="success">{message}</Banner> : null}

        <Section
          title={`Branches · ${branches.length}`}
          description="Each branch is an isolated PostgreSQL schema cloned from its source."
        >
          {branches.length === 0 && !authError ? (
            <Empty
              icon={GitBranch}
              title="No branches yet"
              description="Create a branch to get an isolated schema you can mutate without touching main."
              action={
                <Button size="sm" onClick={() => setCreateOpen(true)} className="rounded-full">
                  <Plus className="h-2 w-2" /> Create branch
                </Button>
              }
            />
          ) : (
            <div className="space-y-1">
              {branches.map((branch) => (
                <div
                  key={branch.name}
                  className={cn(
                    'flex flex-wrap items-center justify-between gap-2 rounded-md border border-border bg-background p-2',
                  )}
                >
                  <div className="flex min-w-0 items-center gap-2">
                    <div className="flex h-6 w-6 items-center justify-center rounded-md bg-primary/10 text-primary">
                      <GitBranch className="h-3 w-3" />
                    </div>
                    <div className="min-w-0">
                      <p className="truncate font-mono text-[13px] font-medium text-foreground">
                        {branch.name}
                      </p>
                      <p className="mt-0.5 flex items-center gap-0.5.5 text-[11px] text-muted-foreground">
                        from <span className="font-mono">{branch.source || 'public'}</span>
                        <span className="text-muted-foreground/60">·</span>
                        created {formatRelative(branch.created_at)}
                      </p>
                    </div>
                  </div>
                  <div className="flex items-center gap-0.5">
                    <Button
                      size="sm"
                      variant="outline"
                      onClick={() => openDiff(branch.name)}
                      className="h-6 gap-0.5.5 rounded-md text-[11px]"
                    >
                      <ArrowRight className="h-2 w-2" />
                      Diff
                    </Button>
                    <Button
                      size="sm"
                      variant="ghost"
                      onClick={() => handleDelete(branch.name)}
                      disabled={deletingName === branch.name || branch.name === 'main' || branch.name === 'public'}
                      className="h-6 gap-0.5.5 rounded-md text-[11px] text-destructive hover:bg-destructive/10"
                    >
                      <Trash2 className="h-2 w-2" />
                      Delete
                    </Button>
                  </div>
                </div>
              ))}
            </div>
          )}
        </Section>
      </div>

      {/* Create */}
      <Dialog open={createOpen} onOpenChange={setCreateOpen}>
        <DialogContent>
          <DialogHeader>
            <DialogTitle className="font-serif">Create branch</DialogTitle>
            <DialogDescription>
              Clones tables from the source schema into <span className="font-mono">branch_&lt;name&gt;</span>.
            </DialogDescription>
          </DialogHeader>
          <div className="space-y-2">
            <div className="space-y-0.5.5">
              <Overline>Branch name</Overline>
              <Input
                value={newName}
                onChange={(e) => setNewName(e.target.value)}
                placeholder="feature_refactor"
                className="font-mono"
              />
            </div>
            <div className="space-y-0.5.5">
              <Overline>Source schema</Overline>
              <Input
                value={newSource}
                onChange={(e) => setNewSource(e.target.value)}
                placeholder="public"
                className="font-mono"
              />
            </div>
          </div>
          <DialogFooter>
            <Button variant="outline" onClick={() => setCreateOpen(false)} disabled={creating}>
              Cancel
            </Button>
            <Button onClick={handleCreate} disabled={!newName.trim() || creating}>
              {creating ? 'Creating…' : 'Create branch'}
            </Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>

      {/* Diff */}
      <Dialog open={Boolean(diffTarget)} onOpenChange={(v) => !v && setDiffTarget(null)}>
        <DialogContent className="sm:max-w-lg">
          <DialogHeader>
            <DialogTitle className="font-serif">Diff · {diffTarget}</DialogTitle>
            <DialogDescription>
              Table-level differences between the branch and the public schema.
            </DialogDescription>
          </DialogHeader>
          {diffLoading ? (
            <p className="flex items-center gap-1 text-[12.5px] text-muted-foreground">
              <InlineSpinner /> Computing diff…
            </p>
          ) : diffResult?.error ? (
            <Banner kind="error">{diffResult.error}</Banner>
          ) : diffResult ? (
            <div className="space-y-2">
              <DiffGroup
                label="Added"
                tone="success"
                icon={Plus}
                items={diffResult.tables_added || []}
              />
              <DiffGroup
                label="Removed"
                tone="error"
                icon={Minus}
                items={diffResult.tables_removed || []}
              />
              <DiffGroup
                label="Shared"
                tone="muted"
                icon={GitBranch}
                items={diffResult.tables_shared || []}
              />
            </div>
          ) : null}
          <DialogFooter>
            <Button variant="outline" onClick={() => setDiffTarget(null)}>
              Close
            </Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>
    </div>
  )
}

function DiffGroup({ label, icon: Icon, items, tone }) {
  const bg =
    tone === 'success'
      ? 'bg-success/10 text-success dark:text-success'
      : tone === 'error'
      ? 'bg-destructive/10 text-destructive'
      : 'bg-muted/40 text-muted-foreground'
  return (
    <div>
      <div className="mb-0.5.5 flex items-center gap-0.5.5 text-[10.5px] font-medium uppercase tracking-[0.12em] text-muted-foreground">
        <Icon className="h-2 w-2" />
        {label} ({items.length})
      </div>
      {items.length === 0 ? (
        <p className="text-[11.5px] text-muted-foreground/70">None</p>
      ) : (
        <div className="flex flex-wrap gap-0.5.5">
          {items.map((name) => (
            <span
              key={String(name)}
              className={`rounded-md px-1 py-0.5 font-mono text-[11px] ${bg}`}
            >
              {String(name)}
            </span>
          ))}
        </div>
      )}
    </div>
  )
}
