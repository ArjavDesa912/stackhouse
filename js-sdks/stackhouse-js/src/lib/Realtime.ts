/**
 * RealtimeClient - WebSocket-based realtime subscriptions for Stackhouse
 * Subscribe to table-level INSERT/UPDATE/DELETE events
 */

export type RealtimeEventType = 'INSERT' | 'UPDATE' | 'DELETE' | '*';

export interface RealtimeEvent {
    type: string;
    table: string;
    record?: Record<string, any>;
    old_record?: Record<string, any>;
    timestamp: string;
}

export type RealtimeCallback = (event: RealtimeEvent) => void;

interface Subscription {
    table: string;
    event: RealtimeEventType;
    callback: RealtimeCallback;
}

export class RealtimeClient {
    private ws: WebSocket | null = null;
    private subscriptions: Subscription[] = [];
    private reconnectAttempts = 0;
    private maxReconnectAttempts = 10;
    private reconnectDelay = 1000;
    private connected = false;

    constructor(
        private url: string,
        private headers: Record<string, string>
    ) { }

    /**
     * Connect to the Stackhouse realtime WebSocket server
     */
    connect(): Promise<void> {
        return new Promise((resolve, reject) => {
            const wsUrl = this.url
                .replace(/^http:/, 'ws:')
                .replace(/^https:/, 'wss:')
                + '/v1/realtime';

            this.ws = new WebSocket(wsUrl);

            this.ws.onopen = () => {
                this.connected = true;
                this.reconnectAttempts = 0;

                // Re-subscribe to all existing subscriptions
                for (const sub of this.subscriptions) {
                    this.sendSubscribe(sub.table, sub.event);
                }

                resolve();
            };

            this.ws.onmessage = (event) => {
                try {
                    const data = JSON.parse(event.data);

                    // Route event to matching callbacks
                    if (data.type === 'INSERT' || data.type === 'UPDATE' || data.type === 'DELETE') {
                        for (const sub of this.subscriptions) {
                            if (sub.table === data.table && (sub.event === '*' || sub.event === data.type)) {
                                sub.callback(data as RealtimeEvent);
                            }
                        }
                    }
                } catch (e) {
                    // Ignore parse errors
                }
            };

            this.ws.onclose = () => {
                this.connected = false;
                this.attemptReconnect();
            };

            this.ws.onerror = (error) => {
                if (!this.connected) {
                    reject(new Error('WebSocket connection failed'));
                }
            };
        });
    }

    /**
     * Subscribe to changes on a table
     * @param table Table name to subscribe to
     * @param event Event type: 'INSERT' | 'UPDATE' | 'DELETE' | '*'
     * @param callback Function called when an event occurs
     * @returns Unsubscribe function
     */
    on(table: string, event: RealtimeEventType, callback: RealtimeCallback): () => void {
        const sub: Subscription = { table, event, callback };
        this.subscriptions.push(sub);

        // If already connected, subscribe immediately
        if (this.connected && this.ws) {
            this.sendSubscribe(table, event);
        }

        // Return unsubscribe function
        return () => {
            this.subscriptions = this.subscriptions.filter(s => s !== sub);
            if (this.connected && this.ws) {
                // Check if any other subs remain for this table
                const hasOtherSubs = this.subscriptions.some(s => s.table === table);
                if (!hasOtherSubs) {
                    this.sendUnsubscribe(table);
                }
            }
        };
    }

    /**
     * Disconnect from the realtime server
     */
    disconnect(): void {
        this.subscriptions = [];
        this.maxReconnectAttempts = 0; // Prevent reconnection
        if (this.ws) {
            this.ws.close();
            this.ws = null;
        }
        this.connected = false;
    }

    /**
     * Check if currently connected
     */
    isConnected(): boolean {
        return this.connected;
    }

    private sendSubscribe(table: string, event: RealtimeEventType): void {
        if (this.ws && this.ws.readyState === WebSocket.OPEN) {
            this.ws.send(JSON.stringify({
                type: 'subscribe',
                table,
                event,
            }));
        }
    }

    private sendUnsubscribe(table: string): void {
        if (this.ws && this.ws.readyState === WebSocket.OPEN) {
            this.ws.send(JSON.stringify({
                type: 'unsubscribe',
                table,
            }));
        }
    }

    private attemptReconnect(): void {
        if (this.reconnectAttempts >= this.maxReconnectAttempts) return;

        this.reconnectAttempts++;
        const delay = this.reconnectDelay * Math.pow(2, this.reconnectAttempts - 1);

        setTimeout(() => {
            this.connect().catch(() => {
                // Will trigger another reconnect attempt via onclose
            });
        }, Math.min(delay, 30000));
    }
}
