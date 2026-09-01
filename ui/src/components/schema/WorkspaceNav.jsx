import React, { useState } from 'react'
import {
  ChevronDown,
  Database,
  FileSearch,
  GitBranch,
  Import,
  LayoutPanelLeft,
  Network,
  Table2,
  TableProperties,
} from 'lucide-react'

import { cn } from '@/lib/utils'
import { Button } from '@/components/ui/button'
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuLabel,
  DropdownMenuSeparator,
  DropdownMenuTrigger,
} from '@/components/ui/dropdown-menu'

const WORKSPACES = [
  { id: 'catalog', label: 'Catalog', description: 'Browse tables', icon: LayoutPanelLeft },
  { id: 'designer', label: 'Designer', description: 'Structure & columns', icon: TableProperties },
  { id: 'explorer', label: 'Explorer', description: 'Rows & edits', icon: FileSearch },
  { id: 'model', label: 'Model', description: 'Relationships', icon: Network },
  { id: 'import', label: 'Import', description: 'CSV / Excel', icon: Import },
  { id: 'operations', label: 'Ops', description: 'Advanced modules', icon: GitBranch },
]

function TablePicker({ tables, selectedTableName, onSelectTable }) {
  const [open, setOpen] = useState(false)
  return (
    <DropdownMenu open={open} onOpenChange={setOpen}>
      <DropdownMenuTrigger asChild>
        <Button
          variant="outline"
          size="sm"
          className="h-6 max-w-[240px] gap-1 rounded-full border-border bg-background px-2 text-[12px] font-medium"
        >
          <Table2 className="h-2.5 w-2.5 text-primary/80" />
          <span className="truncate">{selectedTableName || 'Select a table'}</span>
          <ChevronDown className="h-2 w-2 text-muted-foreground" />
        </Button>
      </DropdownMenuTrigger>
      <DropdownMenuContent align="end" className="w-40">
        <DropdownMenuLabel className="text-[10px] font-medium uppercase tracking-[0.14em] text-muted-foreground">
          Tables · {tables.length}
        </DropdownMenuLabel>
        <DropdownMenuSeparator />
        <div className="max-h-72 overflow-y-auto">
          {tables.length === 0 ? (
            <div className="px-2 py-3 text-center text-[12px] text-muted-foreground">
              No tables yet
            </div>
          ) : (
            tables.map((table) => (
              <DropdownMenuItem
                key={table.name}
                onSelect={() => onSelectTable(table.name)}
                className={cn(
                  'flex items-center justify-between gap-2 text-[12px]',
                  table.name === selectedTableName && 'bg-secondary text-foreground',
                )}
              >
                <div className="flex items-center gap-1">
                  <Table2 className="h-2.5 w-2.5 text-muted-foreground" />
                  <span className="font-medium">{table.name}</span>
                </div>
                <span className="tabular-nums text-[10.5px] text-muted-foreground">
                  {table.row_count?.toLocaleString?.() ?? 0} rows
                </span>
              </DropdownMenuItem>
            ))
          )}
        </div>
      </DropdownMenuContent>
    </DropdownMenu>
  )
}

export default function WorkspaceNav({
  activeWorkspace,
  onChange,
  tables = [],
  selectedTableName,
  onSelectTable,
}) {
  return (
    <div className="shrink-0 border-b border-border bg-card/50">
      <div className="flex items-center justify-between gap-3 px-3 py-2">
        <div className="flex items-center gap-2">
          <div className="flex h-6 w-6 items-center justify-center rounded-md bg-primary/10 text-primary ring-1 ring-primary/20">
            <Database className="h-3 w-3" />
          </div>
          <div className="leading-tight">
            <p className="text-[9.5px] font-medium uppercase tracking-[0.14em] text-muted-foreground/80">
              Schema Workspace
            </p>
            <p className="font-serif text-[15px] font-medium tracking-tight text-foreground">
              {WORKSPACES.find((w) => w.id === activeWorkspace)?.label ?? 'Catalog'}
            </p>
          </div>
        </div>

        <TablePicker
          tables={tables}
          selectedTableName={selectedTableName}
          onSelectTable={onSelectTable}
        />
      </div>

      <div className="flex items-center gap-0.5 overflow-x-auto border-t border-border px-3 py-0.5.5">
        {WORKSPACES.map((workspace) => {
          const Icon = workspace.icon
          const active = workspace.id === activeWorkspace
          return (
            <button
              key={workspace.id}
              type="button"
              onClick={() => onChange(workspace.id)}
              className={cn(
                'group relative flex shrink-0 items-center gap-1 rounded-full px-2.5 py-0.5.5 text-[12px] font-medium tracking-tight transition-colors',
                active
                  ? 'bg-secondary text-foreground ring-1 ring-border dark:ring-1 ring-border'
                  : 'text-muted-foreground hover:bg-muted/50 hover:text-foreground',
              )}
              title={workspace.description}
            >
              <Icon className={cn('h-2.5 w-2.5', active ? 'text-primary' : 'text-muted-foreground/80')} />
              {workspace.label}
            </button>
          )
        })}
      </div>
    </div>
  )
}
