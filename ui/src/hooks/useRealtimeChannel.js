import { useEffect } from 'react';
import { subscribeCollection } from '@/services/realtime';

export function useRealtimeChannel(collection, onEvent, { enabled = true } = {}) {
  useEffect(() => {
    if (!enabled || !collection) return undefined;
    const close = subscribeCollection(collection, onEvent);
    return close;
  }, [collection, enabled, onEvent]);
}
