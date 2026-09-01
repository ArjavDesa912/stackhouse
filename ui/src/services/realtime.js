/**
 * Stackhouse realtime — thin SSE client for /v1/stream/:collection.
 * Falls back to polling if EventSource isn't available.
 */

import { getToken } from '@/lib/apiClient';

const API_BASE = import.meta.env.DEV ? 'http://localhost:3000' : window.location.origin;

export function subscribeCollection(collection, onEvent, { signal } = {}) {
  if (!collection) return () => {};
  if (typeof EventSource === 'undefined') return () => {};

  let es;
  try {
    const token = getToken();
    const url = new URL(`${API_BASE}/v1/stream/${collection}`);
    if (token) url.searchParams.set('access_token', token);
    es = new EventSource(url.toString());
  } catch {
    return () => {};
  }

  const close = () => {
    try { es.close(); } catch { /* ignore */ }
  };

  es.onmessage = (e) => {
    try {
      const data = JSON.parse(e.data);
      onEvent?.(data);
    } catch {
      onEvent?.({ raw: e.data });
    }
  };

  es.onerror = () => {
    // EventSource auto-reconnects; nothing else to do.
  };

  if (signal) {
    if (signal.aborted) close();
    else signal.addEventListener('abort', close, { once: true });
  }

  return close;
}
