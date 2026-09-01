import { useEffect } from 'react';

/**
 * Register a global keyboard shortcut.
 * @param {string} combo - e.g. "mod+k", "shift+?", "esc"
 * @param {(e: KeyboardEvent) => void} handler
 * @param {{ enabled?: boolean }} options
 */
export function useShortcut(combo, handler, { enabled = true } = {}) {
  useEffect(() => {
    if (!enabled) return undefined;
    const parts = combo.toLowerCase().split('+').map((s) => s.trim());
    const key = parts[parts.length - 1];
    const wantMod = parts.includes('mod') || parts.includes('cmd') || parts.includes('ctrl');
    const wantShift = parts.includes('shift');
    const wantAlt = parts.includes('alt');

    const onKey = (e) => {
      const isMod = e.metaKey || e.ctrlKey;
      if (wantMod !== isMod) return;
      if (wantShift !== e.shiftKey) return;
      if (wantAlt !== e.altKey) return;
      const k = e.key.toLowerCase();
      if (key === 'esc' && k !== 'escape') return;
      if (key !== 'esc' && k !== key) return;
      handler(e);
    };
    window.addEventListener('keydown', onKey);
    return () => window.removeEventListener('keydown', onKey);
  }, [combo, handler, enabled]);
}
