import { App, inject, ref, onMounted, onUnmounted, reactive } from 'vue';
import { StackhouseClient } from '@stackhouse/js';
import type { VectorSearchResult, VectorSearchOptions, RealtimeEvent, RealtimeEventType } from '@stackhouse/js';

const StackhouseSymbol = Symbol('StackhouseClient');

export const StackhousePlugin = {
    install(app: App, client: StackhouseClient) {
        app.provide(StackhouseSymbol, client);
    }
};

export function useStackhouse(): StackhouseClient {
    const client = inject<StackhouseClient>(StackhouseSymbol);
    if (!client) {
        throw new Error('Stackhouse client not provided. Did you install StackhousePlugin?');
    }
    return client;
}

export function useUser() {
    const stackhouse = useStackhouse();
    const user = ref<any>(null);
    const loading = ref(true);

    // Initial state
    const session = stackhouse.auth.getSession();
    user.value = session?.user ?? null;
    loading.value = false;

    let unsubscribe: (() => void) | null = null;

    onMounted(() => {
        unsubscribe = stackhouse.auth.onAuthStateChange((session) => {
            user.value = session?.user ?? null;
            loading.value = false;
        });
    });

    onUnmounted(() => {
        if (unsubscribe) {
            unsubscribe();
            unsubscribe = null;
        }
    });

    return { user, loading };
}

export function useQuery(collection: string, query?: any) {
    const stackhouse = useStackhouse();
    const data = ref<any[]>([]);
    const loading = ref(true);
    const error = ref<any>(null);

    onMounted(async () => {
        try {
            loading.value = true;
            const res = await stackhouse.from(collection).select(query);
            data.value = res;
        } catch (e) {
            error.value = e;
        } finally {
            loading.value = false;
        }
    });

    return { data, loading, error };
}

/**
 * Vue composable for vector similarity search
 * 
 * @example
 * ```vue
 * const { results, search, loading } = useVectorSearch('documents');
 * await search([0.1, 0.2, 0.3], { topK: 5 });
 * ```
 */
export function useVectorSearch(collection: string) {
    const stackhouse = useStackhouse();
    const results = ref<VectorSearchResult[]>([]);
    const loading = ref(false);
    const error = ref<any>(null);

    const search = async (queryVector: number[], options?: VectorSearchOptions) => {
        loading.value = true;
        error.value = null;
        try {
            const response = await stackhouse.vectors(collection).search(queryVector, options);
            results.value = response.data;
            return response.data;
        } catch (e) {
            error.value = e;
            throw e;
        } finally {
            loading.value = false;
        }
    };

    const reset = () => {
        results.value = [];
        error.value = null;
    };

    return { results, search, loading, error, reset };
}

/**
 * Vue composable for realtime subscriptions
 * 
 * @example
 * ```vue
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
    const events = ref<RealtimeEvent[]>([]);
    const isConnected = ref(false);

    let unsubscribe: (() => void) | null = null;

    onMounted(async () => {
        try {
            if (!stackhouse.realtime.isConnected()) {
                await stackhouse.realtime.connect();
            }
            isConnected.value = true;

            unsubscribe = stackhouse.realtime.on(table, event, (evt) => {
                events.value = [...events.value.slice(-99), evt];
                onEvent?.(evt);
            });
        } catch (err) {
            console.error('[Stackhouse] Realtime connection failed:', err);
            isConnected.value = false;
        }
    });

    onUnmounted(() => {
        unsubscribe?.();
    });

    const clearEvents = () => { events.value = []; };

    return { events, isConnected, clearEvents };
}
