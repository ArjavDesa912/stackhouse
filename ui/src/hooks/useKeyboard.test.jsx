import { describe, it, expect, vi } from 'vitest';
import { renderHook } from '@testing-library/react';
import { useShortcut } from './useKeyboard';

function fireKey({ key, ctrlKey = false, metaKey = false, shiftKey = false, altKey = false }) {
  window.dispatchEvent(new KeyboardEvent('keydown', { key, ctrlKey, metaKey, shiftKey, altKey }));
}

describe('useShortcut', () => {
  it('fires when the bare key matches', () => {
    const handler = vi.fn();
    renderHook(() => useShortcut('k', handler));
    fireKey({ key: 'k' });
    expect(handler).toHaveBeenCalledTimes(1);
  });

  it('requires the mod key when "mod+" is specified', () => {
    const handler = vi.fn();
    renderHook(() => useShortcut('mod+k', handler));
    fireKey({ key: 'k' });
    expect(handler).not.toHaveBeenCalled();
    fireKey({ key: 'k', metaKey: true });
    expect(handler).toHaveBeenCalledTimes(1);
    fireKey({ key: 'k', ctrlKey: true });
    expect(handler).toHaveBeenCalledTimes(2);
  });

  it('matches the "esc" alias to the real Escape key', () => {
    const handler = vi.fn();
    renderHook(() => useShortcut('esc', handler));
    fireKey({ key: 'Escape' });
    expect(handler).toHaveBeenCalledTimes(1);
  });

  it('respects shift / alt requirements', () => {
    const handler = vi.fn();
    renderHook(() => useShortcut('shift+?', handler));
    fireKey({ key: '?' });
    expect(handler).not.toHaveBeenCalled();
    fireKey({ key: '?', shiftKey: true });
    expect(handler).toHaveBeenCalledTimes(1);
  });

  it('does nothing when disabled', () => {
    const handler = vi.fn();
    renderHook(() => useShortcut('k', handler, { enabled: false }));
    fireKey({ key: 'k' });
    expect(handler).not.toHaveBeenCalled();
  });
});
