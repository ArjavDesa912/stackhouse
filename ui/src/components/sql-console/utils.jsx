import { CheckCircle2, Clock3, Loader2, XCircle } from 'lucide-react'

export const SNIPPETS = [
  {
    id: 'recent-users',
    label: 'Recent users',
    tag: 'read',
    description: 'Inspect the latest 25 user records.',
    query: 'SELECT * FROM users ORDER BY created_at DESC LIMIT 25;',
  },
  {
    id: 'order-status',
    label: 'Orders by status',
    tag: 'aggregate',
    description: 'Summarize orders grouped by their status.',
    query: 'SELECT status, COUNT(*) AS total\nFROM orders\nGROUP BY status\nORDER BY total DESC;',
  },
  {
    id: 'inactive-accounts',
    label: 'Inactive accounts',
    tag: 'audit',
    description: 'Find inactive users ready for cleanup.',
    query:
      "SELECT id, email, last_login_at\nFROM users\nWHERE active = false\nORDER BY last_login_at ASC\nLIMIT 50;",
  },
  {
    id: 'row-counts',
    label: 'Row counts per table',
    tag: 'meta',
    description: 'Introspect row volumes using information_schema.tables.',
    query: "SELECT table_name FROM information_schema.tables WHERE table_schema = 'public' ORDER BY table_name;",
  },
]

// Monaco's defineTheme API needs literal hex — it can't resolve CSS custom
// properties. Values below are mirrors of the real tokens in styles/tokens.css
// (--canvas*, --ink*, --hairline*, --flux, --success/--warning/--info) so the
// SQL editor uses the same color language as the rest of the app instead of
// an unrelated palette. Keep in sync if those tokens change.
export const defineMonacoThemes = (monaco) => {
  monaco.editor.defineTheme('stackhouse-light', {
    base: 'vs',
    inherit: true,
    rules: [
      { token: 'keyword', foreground: '0fb5a6', fontStyle: 'bold' }, // --flux
      { token: 'keyword.sql', foreground: '0fb5a6', fontStyle: 'bold' },
      { token: 'string', foreground: '1f9d6e' }, // --success
      { token: 'string.sql', foreground: '1f9d6e' },
      { token: 'number', foreground: '2e6bd6' }, // --info
      { token: 'comment', foreground: '8a8e97', fontStyle: 'italic' }, // --ink-3
      { token: 'operator', foreground: '4b4f58' }, // --ink-2
      { token: 'identifier', foreground: '15171c' }, // --ink
    ],
    colors: {
      'editor.background': '#fbfbfa', // --canvas-1
      'editor.foreground': '#15171c', // --ink
      'editor.lineHighlightBackground': '#f4f4f2', // --canvas-2
      'editor.selectionBackground': '#ececec', // --canvas-3
      'editorLineNumber.foreground': '#8a8e97', // --ink-3
      'editorLineNumber.activeForeground': '#4b4f58', // --ink-2
      'editorCursor.foreground': '#0fb5a6', // --flux
      'editor.inactiveSelectionBackground': '#f4f4f2', // --canvas-2
      'editorIndentGuide.background': '#e8e7e3', // --hairline
      'editorIndentGuide.activeBackground': '#c8c7c2', // --hairline-strong
      'editorGutter.background': '#fbfbfa', // --canvas-1
    },
  })
  monaco.editor.defineTheme('stackhouse-dark', {
    base: 'vs-dark',
    inherit: true,
    rules: [
      { token: 'keyword', foreground: '1ac9b8', fontStyle: 'bold' }, // --flux (dark)
      { token: 'keyword.sql', foreground: '1ac9b8', fontStyle: 'bold' },
      { token: 'string', foreground: '2bb47e' }, // --success (dark)
      { token: 'string.sql', foreground: '2bb47e' },
      { token: 'number', foreground: '4f84e0' }, // --info (dark)
      { token: 'comment', foreground: '7a7e88', fontStyle: 'italic' }, // --ink-3 (dark)
      { token: 'operator', foreground: 'b5b8c0' }, // --ink-2 (dark)
      { token: 'identifier', foreground: 'ecedf0' }, // --ink (dark)
    ],
    colors: {
      'editor.background': '#16181d', // --canvas
      'editor.foreground': '#ecedf0', // --ink
      'editor.lineHighlightBackground': '#23262e', // --canvas-2
      'editor.selectionBackground': '#444751', // --hairline-strong
      'editorLineNumber.foreground': '#7a7e88', // --ink-3
      'editorLineNumber.activeForeground': '#b5b8c0', // --ink-2
      'editorCursor.foreground': '#1ac9b8', // --flux
      'editor.inactiveSelectionBackground': '#1b1e24', // --canvas-1
      'editorIndentGuide.background': '#2c2f36', // --canvas-3
      'editorIndentGuide.activeBackground': '#444751', // --hairline-strong
      'editorGutter.background': '#16181d', // --canvas
    },
  })
}

export function formatTime(date) {
  if (!date) return '—'
  return new Intl.DateTimeFormat('en-US', {
    hour: 'numeric',
    minute: '2-digit',
    second: '2-digit',
  }).format(date)
}

export function condenseQuery(q) {
  return q.replace(/\s+/g, ' ').trim()
}

export function Overline({ children, className = '' }) {
  return (
    <p className={`text-[9.5px] font-medium uppercase tracking-[0.14em] text-muted-foreground/80 ${className}`}>
      {children}
    </p>
  )
}

export function StatusPill({ status }) {
  const map = {
    running: {
      icon: Loader2,
      spin: true,
      label: 'Running',
      cls: 'border-primary/30 bg-primary/10 text-primary',
    },
    success: {
      icon: CheckCircle2,
      label: 'Ready',
      cls: 'border-success/35 bg-success/10 text-success dark:text-success',
    },
    error: {
      icon: XCircle,
      label: 'Failed',
      cls: 'border-destructive/30 bg-destructive/10 text-destructive',
    },
    idle: { icon: Clock3, label: 'Idle', cls: 'border-border bg-muted/40 text-muted-foreground' },
  }
  const entry = map[status] || map.idle
  const Icon = entry.icon
  return (
    <span
      className={`inline-flex items-center gap-0.5.5 rounded-full border px-1.5 py-0.5 text-[10.5px] font-medium tracking-tight ${entry.cls}`}
    >
      <Icon className={`h-2 w-2 ${entry.spin ? 'animate-spin' : ''}`} />
      {entry.label}
    </span>
  )
}

export function EmptyBlock({ icon: Icon, title, description }) {
  return (
    <div className="flex h-full min-h-32 flex-col items-center justify-center gap-2 rounded-md border border-dashed border-border bg-muted/10 p-4 text-center">
      <div className="flex h-10 w-10 items-center justify-center rounded-full bg-background ring-1 ring-border">
        <Icon className="h-4 w-4 text-muted-foreground" />
      </div>
      <div>
        <p className="font-serif text-[17px] font-medium tracking-tight text-foreground">{title}</p>
        <p className="mt-0.5 text-[12.5px] text-muted-foreground">{description}</p>
      </div>
    </div>
  )
}
