import React, { useEffect, useRef } from 'react'

export function ContextMenu({ x, y, items, onClose }) {
  const ref = useRef(null)
  useEffect(() => {
    const handler = (e) => {
      if (ref.current && !ref.current.contains(e.target)) onClose()
    }
    document.addEventListener('mousedown', handler)
    return () => document.removeEventListener('mousedown', handler)
  }, [onClose])

  return (
    <div
      ref={ref}
      className="fixed z-50 min-w-[200px] rounded-md border border-border bg-popover p-0.5 shadow-none animate-in fade-in-0 zoom-in-95"
      style={{ left: x, top: y }}
    >
      {items.map((item, i) =>
        item.separator ? (
          <div key={i} className="my-0.5 h-px bg-border" />
        ) : (
          <button
            key={i}
            type="button"
            className="flex w-full items-center gap-1 rounded-md px-1.5 py-0.5.5 text-left text-[12px] text-foreground transition-colors hover:bg-accent"
            onClick={() => {
              item.action()
              onClose()
            }}
          >
            {item.icon && <item.icon className="h-2.5 w-2.5 text-muted-foreground" />}
            <span className="flex-1">{item.label}</span>
            {item.hint && <span className="text-[10px] text-muted-foreground">{item.hint}</span>}
          </button>
        ),
      )}
    </div>
  )
}
