import React, { useRef, useState } from 'react'
import Editor from '@monaco-editor/react'
import {
  Activity,
  BookOpen,
  Command as CommandIcon,
  Database,
  Eraser,
  FileCode2,
  Loader2,
  Play,
  TerminalSquare,
  Zap,
} from 'lucide-react'

import { buildSqlRequest, normalizeSqlResponse } from '@/lib/sqlConsole'
import { apiPost } from '@/lib/apiClient'
import { Button } from '@/components/ui/button'
import { useTheme } from './ThemeProvider'
import { TableRail } from './sql-console/TableRail'
import { InspectorRail } from './sql-console/InspectorRail'
import { ResultsGrid } from './sql-console/ResultsGrid'
import { ExplainView } from './sql-console/ExplainView'
import { MessagesView } from './sql-console/MessagesView'
import { SNIPPETS, StatusPill, defineMonacoThemes } from './sql-console/utils'

const STARTER_QUERY = 'SELECT 1 AS hello;'

export default function SqlConsole({ tables = [] }) {
  const { theme } = useTheme()
  const editorRef = useRef(null)
  const monacoRef = useRef(null)

  const [query, setQuery] = useState(STARTER_QUERY)
  const [filter, setFilter] = useState('')
  const [activeTableName, setActiveTableName] = useState('')
  const [history, setHistory] = useState([])
  const [messages, setMessages] = useState([])
  const [tab, setTab] = useState('results')
  const [runState, setRunState] = useState({
    status: 'idle',
    current: null,
    lastResults: null,
    lastExplain: null,
    lastRunAt: null,
    lastDurationMs: null,
  })

  const monacoTheme = theme === 'dark' ? 'stackhouse-dark' : 'stackhouse-light'

  const handleMount = (editor, monaco) => {
    editorRef.current = editor
    monacoRef.current = monaco
    editor.addCommand(monaco.KeyMod.CtrlCmd | monaco.KeyCode.Enter, () => runRequest())
    editor.addCommand(
      monaco.KeyMod.Shift | monaco.KeyMod.CtrlCmd | monaco.KeyCode.Enter,
      () => runRequest({ explain: true }),
    )
  }

  const pushHistory = (entry) =>
    setHistory((current) =>
      [{ id: `${Date.now()}-${current.length}`, timestamp: new Date(), ...entry }, ...current].slice(
        0,
        25,
      ),
    )

  const pushMessage = (entry) =>
    setMessages((current) =>
      [{ id: `${Date.now()}-${current.length}`, timestamp: new Date(), ...entry }, ...current].slice(
        0,
        40,
      ),
    )

  const insertAtCursor = (text) => {
    const editor = editorRef.current
    if (!editor) {
      setQuery((current) => `${current}${text}`)
      return
    }
    const selection = editor.getSelection()
    editor.executeEdits('', [{ range: selection, text, forceMoveMarkers: true }])
    editor.focus()
  }

  const handleFormat = () => {
    const editor = editorRef.current
    if (editor) editor.getAction('editor.action.formatDocument')?.run()
  }

  const runRequest = async (options = {}) => {
    const source = editorRef.current?.getValue() ?? query
    const trimmed = source.trim()
    if (!trimmed) {
      pushMessage({
        status: 'error',
        title: 'Query required',
        description: 'Enter a SQL statement before running the console.',
      })
      setTab('messages')
      return
    }
    const request = buildSqlRequest(trimmed, options)
    const startedAt = performance.now()
    setRunState((s) => ({ ...s, status: 'running' }))

    try {
      const { ok, status, data } = await apiPost(request.endpoint, request.payload)
      let payload = data
      if (!payload || typeof payload !== 'object') {
        payload = { success: false, message: `Request failed with status ${status}` }
      }
      if (!ok) {
        const msg = payload.error?.message || payload.message || `Request failed with status ${status}`
        payload = { success: false, message: msg }
      }
      const model = normalizeSqlResponse(payload, { explain: request.mode === 'explain' })
      const duration = Math.max(1, Math.round(performance.now() - startedAt))

      if (model.kind === 'error') {
        pushMessage({ status: 'error', title: request.mode === 'explain' ? 'Explain failed' : 'Query failed', description: model.message })
        pushHistory({ query: trimmed, status: 'error', mode: request.mode })
        setRunState((s) => ({ ...s, status: 'error', lastRunAt: new Date(), lastDurationMs: duration }))
        setTab('messages')
        return
      }

      pushHistory({ query: trimmed, status: 'success', mode: request.mode })

      if (model.kind === 'message') {
        pushMessage({ status: 'success', title: 'Statement completed', description: model.message })
      }

      setRunState((s) => ({
        ...s,
        status: 'success',
        current: model,
        lastResults: model.kind === 'results' ? model : s.lastResults,
        lastExplain: model.kind === 'explain' ? model : s.lastExplain,
        lastRunAt: new Date(),
        lastDurationMs: duration,
      }))

      if (model.kind === 'results') setTab('results')
      else if (model.kind === 'explain') setTab('explain')
      else setTab('messages')
    } catch (error) {
      pushMessage({ status: 'error', title: 'Network failure', description: error.message })
      pushHistory({ query: trimmed, status: 'error', mode: request.mode })
      setRunState((s) => ({ ...s, status: 'error', lastRunAt: new Date() }))
      setTab('messages')
    }
  }

  const activeResults = runState.current?.kind === 'results' ? runState.current : runState.lastResults
  const activeExplain = runState.current?.kind === 'explain' ? runState.current : runState.lastExplain
  const stale = runState.current?.kind !== 'results' && Boolean(runState.lastResults)

  const TabButton = ({ id, icon: Icon, label, badge }) => {
    const active = tab === id
    return (
      <button
        type="button"
        onClick={() => setTab(id)}
        className={`relative flex items-center gap-0.5.5 px-2 py-1 text-[11.5px] font-medium tracking-tight transition-colors ${
          active ? 'text-foreground' : 'text-muted-foreground hover:text-foreground'
        }`}
      >
        <Icon className="h-2.5 w-2.5" />
        {label}
        {badge ? (
          <span className="ml-0.5 rounded-full bg-muted px-0.5.5 py-0.5 text-[9px] tabular-nums text-muted-foreground">
            {badge}
          </span>
        ) : null}
        {active ? <span className="absolute inset-x-2 bottom-0 h-0.5 rounded-full bg-primary" /> : null}
      </button>
    )
  }

  return (
    <div className="flex h-full min-h-0 bg-background">
      <TableRail
        tables={tables}
        filter={filter}
        onFilter={setFilter}
        onInsert={(columnName) => insertAtCursor(columnName)}
        activeTableName={activeTableName}
        onSelect={setActiveTableName}
        onSetQuery={(q) => {
          setQuery(q)
          editorRef.current?.setValue(q)
          editorRef.current?.focus()
        }}
      />

      <div className="flex min-h-0 flex-1 flex-col">
        {/* Editor toolbar */}
        <div className="flex shrink-0 items-center justify-between gap-2 border-b border-border bg-card/50 px-3 py-1">
          <div className="flex items-center gap-1">
            <div className="flex h-6 w-6 items-center justify-center rounded-md bg-primary/10 text-primary ring-1 ring-primary/20">
              <TerminalSquare className="h-2.5 w-2.5" />
            </div>
            <div className="leading-tight">
              <p className="font-serif text-[14px] font-medium tracking-tight text-foreground">
                Query workbench
              </p>
              <p className="text-[10.5px] text-muted-foreground">
                Compose SQL, inspect plans, audit history.
              </p>
            </div>
          </div>

          <div className="flex items-center gap-0.5.5">
            <StatusPill status={runState.status} />
            <div className="mx-0.5 h-4 w-px bg-border" />
            <Button
              size="sm"
              variant="ghost"
              onClick={handleFormat}
              className="h-6 gap-0.5.5 text-[11.5px]"
              title="Format"
            >
              <FileCode2 className="h-2 w-2" />
              Format
            </Button>
            <Button
              size="sm"
              variant="ghost"
              onClick={() => {
                setQuery('')
                editorRef.current?.setValue('')
              }}
              className="h-6 gap-0.5.5 text-[11.5px]"
              title="Clear"
            >
              <Eraser className="h-2 w-2" />
              Clear
            </Button>
            <Button
              size="sm"
              variant="outline"
              disabled={runState.status === 'running'}
              onClick={() => runRequest({ explain: true })}
              className="h-6 gap-0.5.5 text-[11.5px]"
            >
              <Activity className="h-2 w-2" />
              Explain
              <kbd className="ml-0.5 hidden items-center gap-0.5 rounded border border-border bg-background px-0.5 text-[9px] text-muted-foreground md:inline-flex">
                <CommandIcon className="h-1 w-1" />⇧↵
              </kbd>
            </Button>
            <Button
              size="sm"
              disabled={runState.status === 'running'}
              onClick={() => runRequest()}
              className="h-6 gap-0.5.5 rounded-full px-3 text-[11.5px]"
            >
              {runState.status === 'running' ? (
                <Loader2 className="h-2 w-2 animate-spin" />
              ) : (
                <Play className="h-2 w-2 fill-current" />
              )}
              Run
              <kbd className="ml-0.5 hidden items-center gap-0.5 rounded bg-primary-foreground/15 px-0.5 text-[9px] md:inline-flex">
                <CommandIcon className="h-1 w-1" />↵
              </kbd>
            </Button>
          </div>
        </div>

        {/* Editor */}
        <div className="shrink-0 border-b border-border">
          <Editor
            height="280px"
            defaultLanguage="sql"
            theme={monacoTheme}
            beforeMount={defineMonacoThemes}
            onMount={handleMount}
            value={query}
            onChange={(value) => setQuery(value || '')}
            options={{
              minimap: { enabled: false },
              automaticLayout: true,
              scrollBeyondLastLine: false,
              fontSize: 13,
              lineHeight: 20,
              padding: { top: 12, bottom: 12 },
              lineNumbersMinChars: 3,
              fontFamily: "'JetBrains Mono', ui-monospace, Consolas, monospace",
              renderLineHighlight: 'all',
              smoothScrolling: true,
            }}
          />
        </div>

        {/* Results tab bar */}
        <div className="flex shrink-0 items-center justify-between border-b border-border bg-card/40 pl-1 pr-3">
          <div className="flex items-center">
            <TabButton id="results" icon={Database} label="Results" badge={activeResults?.rowCount ?? 0} />
            <TabButton id="explain" icon={Activity} label="Explain" />
            <TabButton id="messages" icon={TerminalSquare} label="Messages" badge={messages.length || undefined} />
          </div>
          <div className="flex items-center gap-2 text-[10.5px] text-muted-foreground">
            {runState.lastDurationMs ? (
              <span className="inline-flex items-center gap-0.5 tabular-nums">
                <Zap className="h-2 w-2 text-primary/70" />
                {runState.lastDurationMs} ms
              </span>
            ) : null}
            <span className="hidden items-center gap-0.5 md:inline-flex">
              <BookOpen className="h-2 w-2" />
              Read-first routing
            </span>
          </div>
        </div>

        {/* Results panel */}
        <div className="min-h-0 flex-1 overflow-hidden bg-background">
          {tab === 'results' ? (
            <ResultsGrid model={activeResults} stale={stale} />
          ) : tab === 'explain' ? (
            <ExplainView model={activeExplain} />
          ) : (
            <MessagesView entries={messages} />
          )}
        </div>
      </div>

      <InspectorRail
        runState={runState}
        history={history}
        snippets={SNIPPETS}
        onUseSnippet={(q) => {
          setQuery(q)
          editorRef.current?.setValue(q)
        }}
        onReuse={(q) => {
          setQuery(q)
          editorRef.current?.setValue(q)
        }}
      />
    </div>
  )
}
