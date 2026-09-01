import { useEffect, useState, useCallback, useRef } from 'react';
import { useStackhouse } from './context';
import type { VectorSearchResult, VectorSearchOptions, RealtimeEvent, RealtimeEventType } from '@stackhouse/js';

export function useUser() {
    const stackhouse = useStackhouse();
    const [user, setUser] = useState<any>(null);
    const [loading, setLoading] = useState(true);

    useEffect(() => {
        // Initial state
        const session = stackhouse.auth.getSession();
        setUser(session?.user ?? null);
        setLoading(false);

        // Subscribe to changes
        const unsubscribe = stackhouse.auth.onAuthStateChange((session) => {
            setUser(session?.user ?? null);
            setLoading(false);
        });

        return () => unsubscribe();
    }, [stackhouse]);

    return { user, loading };
}

export function useQuery<T = any>(collection: string, query?: any) {
    const stackhouse = useStackhouse();
    const [data, setData] = useState<T[]>([]);
    const [loading, setLoading] = useState(true);
    const [error, setError] = useState<any>(null);

    useEffect(() => {
        let mounted = true;
        setLoading(true);
        stackhouse.from(collection).select(query)
            .then((res: any) => {
                if (mounted) {
                    setData(res);
                    setLoading(false);
                }
            })
            .catch((err: any) => {
                if (mounted) {
                    setError(err);
                    setLoading(false);
                }
            });

        return () => { mounted = false; };
    }, [stackhouse, collection, JSON.stringify(query)]);

    return { data, loading, error };
}

/**
 * React hook for vector similarity search
 * 
 * @example
 * ```tsx
 * const { results, search, loading } = useVectorSearch('documents');
 * 
 * // Trigger a search
 * await search([0.1, 0.2, 0.3], { topK: 5, metric: 'cosine' });
 * ```
 */
export function useVectorSearch(collection: string) {
    const stackhouse = useStackhouse();
    const [results, setResults] = useState<VectorSearchResult[]>([]);
    const [loading, setLoading] = useState(false);
    const [error, setError] = useState<any>(null);

    const search = useCallback(async (queryVector: number[], options?: VectorSearchOptions) => {
        setLoading(true);
        setError(null);
        try {
            const response = await stackhouse.vectors(collection).search(queryVector, options);
            setResults(response.data);
            return response.data;
        } catch (err) {
            setError(err);
            throw err;
        } finally {
            setLoading(false);
        }
    }, [stackhouse, collection]);

    const reset = useCallback(() => {
        setResults([]);
        setError(null);
    }, []);

    return { results, search, loading, error, reset };
}

/**
 * React hook for realtime subscriptions
 * 
 * @example
 * ```tsx
 * const { events, isConnected } = useRealtime('users', '*', (event) => {
 *     console.log('Change:', event);
 * });
 * ```
 */
export function useRealtime(
    table: string,
    event: RealtimeEventType = '*',
    onEvent?: (event: RealtimeEvent) => void
) {
    const stackhouse = useStackhouse();
    const [events, setEvents] = useState<RealtimeEvent[]>([]);
    const [isConnected, setIsConnected] = useState(false);
    const callbackRef = useRef(onEvent);
    callbackRef.current = onEvent;

    useEffect(() => {
        let unsubscribe: (() => void) | null = null;

        const connect = async () => {
            try {
                if (!stackhouse.realtime.isConnected()) {
                    await stackhouse.realtime.connect();
                }
                setIsConnected(true);

                unsubscribe = stackhouse.realtime.on(table, event, (evt) => {
                    setEvents(prev => [...prev.slice(-99), evt]);
                    callbackRef.current?.(evt);
                });
            } catch (err) {
                console.error('[Stackhouse] Realtime connection failed:', err);
                setIsConnected(false);
            }
        };

        connect();

        return () => {
            unsubscribe?.();
        };
    }, [stackhouse, table, event]);

    const clearEvents = useCallback(() => setEvents([]), []);

    return { events, isConnected, clearEvents };
}
