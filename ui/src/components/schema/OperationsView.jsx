import React, { useMemo, useState } from 'react'
import {
  Archive,
  ChevronsLeft,
  ChevronsRight,
  GitBranch,
  HeartPulse,
  Layers,
  Puzzle,
  Scan,
  ScrollText,
  Shield,
  Users,
} from 'lucide-react'

import HealthModule from './ops/HealthModule'
import ProfileModule from './ops/ProfileModule'
import LogsModule from './ops/LogsModule'
import QueryBuilderModule from './ops/QueryBuilderModule'
import BackupsModule from './ops/BackupsModule'
import BranchesModule from './ops/BranchesModule'
import ExtensionsModule from './ops/ExtensionsModule'
import NetworkModule from './ops/NetworkModule'
import TeamsModule from './ops/TeamsModule'
import { cn } from '@/lib/utils'

const GROUPS = [
  {
    id: 'observability',
    label: 'Observability',
    modules: [
      { id: 'health', label: 'Health', icon: HeartPulse, component: HealthModule, needsTables: true },
      { id: 'profile', label: 'Profile', icon: Scan, component: ProfileModule, needsTables: true },
      { id: 'logs', label: 'Logs', icon: ScrollText, component: LogsModule },
    ],
  },
  {
    id: 'data',
    label: 'Data',
    modules: [
      { id: 'query', label: 'Query builder', icon: Layers, component: QueryBuilderModule, needsTables: true },
      { id: 'backups', label: 'Backups', icon: Archive, component: BackupsModule },
    ],
  },
  {
    id: 'governance',
    label: 'Governance',
    modules: [
      { id: 'branches', label: 'Branches', icon: GitBranch, component: BranchesModule },
      { id: 'extensions', label: 'Extensions', icon: Puzzle, component: ExtensionsModule },
      { id: 'network', label: 'Network', icon: Shield, component: NetworkModule },
      { id: 'teams', label: 'Teams', icon: Users, component: TeamsModule },
    ],
  },
]

const ALL_MODULES = GROUPS.flatMap((g) => g.modules.map((m) => ({ ...m, group: g.label })))

function StatusDot({ className }) {
  return (
    <span className={cn('relative flex h-0.5.5 w-0.5.5', className)}>
      <span className="absolute inline-flex h-full w-full animate-ping rounded-full bg-success opacity-60" />
      <span className="relative inline-flex h-0.5.5 w-0.5.5 rounded-full bg-success" />
    </span>
  )
}

export default function OperationsView({ tables = [] }) {
  const [activeId, setActiveId] = useState('health')
  const [collapsed, setCollapsed] = useState(false)

  const active = useMemo(
    () => ALL_MODULES.find((m) => m.id === activeId) || ALL_MODULES[0],
    [activeId],
  )

  const ActiveComponent = active.component
  const activeProps = active.needsTables ? { tables } : {}

  return (
    <div className="flex h-full min-h-0 bg-background">
      {/* Admin rail */}
      <aside
        className={cn(
          'flex shrink-0 flex-col border-r border-border bg-card/60 transition-[width] duration-200',
          collapsed ? 'w-[56px]' : 'w-[232px]',
        )}
      >
        <div
          className={cn(
            'flex items-center border-b border-border px-2 py-2',
            collapsed ? 'justify-center' : 'justify-between',
          )}
        >
          {!collapsed ? (
            <div className="flex items-center gap-1">
              <StatusDot />
              <div className="leading-tight">
                <p className="text-[9.5px] font-medium uppercase tracking-[0.14em] text-muted-foreground/80">
                  Operations
                </p>
                <p className="font-mono text-[11px] font-medium tracking-tight text-foreground">
                  admin.console
                </p>
              </div>
            </div>
          ) : (
            <StatusDot />
          )}
          <button
            type="button"
            onClick={() => setCollapsed((v) => !v)}
            className="rounded-md p-0.5 text-muted-foreground transition-colors hover:bg-muted hover:text-foreground"
            title={collapsed ? 'Expand' : 'Collapse'}
          >
            {collapsed ? (
              <ChevronsRight className="h-2.5 w-2.5" />
            ) : (
              <ChevronsLeft className="h-2.5 w-2.5" />
            )}
          </button>
        </div>

        <nav className="min-h-0 flex-1 overflow-y-auto py-1">
          {GROUPS.map((group, groupIndex) => (
            <div key={group.id} className={cn(groupIndex > 0 && 'mt-1')}>
              {!collapsed ? (
                <p className="px-2 pb-0.5 pt-1 text-[9px] font-medium uppercase tracking-[0.14em] text-muted-foreground/70">
                  {group.label}
                </p>
              ) : groupIndex > 0 ? (
                <div className="mx-2 my-1 h-px bg-border" />
              ) : null}
              <ul className="space-y-0.5 px-1">
                {group.modules.map((module) => {
                  const Icon = module.icon
                  const isActive = module.id === activeId
                  return (
                    <li key={module.id}>
                      <button
                        type="button"
                        onClick={() => setActiveId(module.id)}
                        title={collapsed ? module.label : undefined}
                        className={cn(
                          'group relative flex w-full items-center gap-1.5 rounded-md px-1 py-0.5.5 text-left transition-colors',
                          isActive
                            ? 'bg-secondary text-foreground'
                            : 'text-muted-foreground hover:bg-muted/50 hover:text-foreground',
                          collapsed && 'justify-center px-0',
                        )}
                      >
                        {isActive ? (
                          <span className="absolute left-0 top-1.5 bottom-1.5 w-[2px] rounded-full bg-primary" />
                        ) : null}
                        <Icon
                          className={cn(
                            'h-2.5 w-2.5 shrink-0',
                            isActive ? 'text-primary' : 'text-muted-foreground/80',
                          )}
                        />
                        {!collapsed ? (
                          <span className="flex-1 truncate text-[12px] font-medium tracking-tight">
                            {module.label}
                          </span>
                        ) : null}
                      </button>
                    </li>
                  )
                })}
              </ul>
            </div>
          ))}
        </nav>

        {!collapsed ? (
          <div className="border-t border-border px-2 py-1">
            <div className="flex items-center justify-between text-[10px] text-muted-foreground">
              <span className="font-mono">{ALL_MODULES.length} modules</span>
              <span className="inline-flex items-center gap-0.5 tabular-nums">
                <StatusDot />
                healthy
              </span>
            </div>
          </div>
        ) : null}
      </aside>

      {/* Canvas */}
      <div className="flex min-w-0 flex-1 flex-col">
        <div className="flex shrink-0 items-center justify-between border-b border-border bg-background/95 px-3 py-1 text-[11px] backdrop-blur">
          <div className="flex items-center gap-0.5.5 text-muted-foreground">
            <span>Operations</span>
            <span className="text-muted-foreground/50">/</span>
            <span>{active.group}</span>
            <span className="text-muted-foreground/50">/</span>
            <span className="font-medium text-foreground">{active.label}</span>
          </div>
          <div className="flex items-center gap-2 text-muted-foreground">
            <span className="hidden items-center gap-0.5.5 md:inline-flex">
              <StatusDot />
              <span className="font-mono text-[10.5px]">live</span>
            </span>
            <span className="font-mono text-[10.5px] tabular-nums">
              {tables.length} tables · {tables.reduce((s, t) => s + (t.columns?.length || 0), 0)} cols
            </span>
          </div>
        </div>

        <div className="min-h-0 flex-1 overflow-hidden">
          <ActiveComponent {...activeProps} />
        </div>
      </div>
    </div>
  )
}
