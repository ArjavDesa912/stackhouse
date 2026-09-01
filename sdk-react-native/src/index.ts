import { StackhouseClient, AuthSession } from '@stackhouse/js';
import AsyncStorage from '@react-native-async-storage/async-storage';

const TOKEN_STORAGE_KEY = '@stackhouse/auth_tokens';

/**
 * AsyncStorage persistence adapter for React Native.
 * Automatically persists and restores auth tokens.
 */
export class StackhouseRNClient extends StackhouseClient {
    private _initialized = false;
    private _restorePromise: Promise<void> | null = null;

    /**
     * Initialize the client and restore persisted auth tokens.
     * Call this once after creating the client.
     */
    async init(): Promise<void> {
        if (this._initialized) {
            return;
        }

        if (this._restorePromise) {
            return this._restorePromise;
        }

        this._restorePromise = this._performInit();
        await this._restorePromise;
        this._restorePromise = null;
    }

    private async _performInit(): Promise<void> {
        try {
            const stored = await AsyncStorage.getItem(TOKEN_STORAGE_KEY);
            if (stored) {
                const tokens = JSON.parse(stored);
                if (this.isValidSession(tokens)) {
                    // Set session directly on the auth client
                    this.auth.setSession(tokens);
                }
            }
        } catch (e) {
            console.warn('[Stackhouse] Failed to restore tokens from AsyncStorage:', e);
        }

        // Listen for auth state changes and persist
        this.auth.onAuthStateChange(async (session) => {
            try {
                if (session) {
                    await AsyncStorage.setItem(TOKEN_STORAGE_KEY, JSON.stringify(session));
                } else {
                    await AsyncStorage.removeItem(TOKEN_STORAGE_KEY);
                }
            } catch (e) {
                console.warn('[Stackhouse] Failed to persist tokens to AsyncStorage:', e);
            }
        });

        this._initialized = true;
    }

    private isValidSession(session: any): session is AuthSession {
        return (
            session &&
            typeof session === 'object' &&
            typeof session.user === 'object' &&
            typeof session.access_token === 'string' &&
            typeof session.refresh_token === 'string' &&
            typeof session.expires_at === 'number' &&
            session.expires_at > Date.now()
        );
    }

    /**
     * Clear all persisted auth data
     */
    async clearPersistedAuth(): Promise<void> {
        try {
            await AsyncStorage.removeItem(TOKEN_STORAGE_KEY);
        } catch (e) {
            console.warn('[Stackhouse] Failed to clear persisted auth:', e);
        }
    }

    /**
     * Get persisted auth data without restoring it
     */
    async getPersistedAuth(): Promise<AuthSession | null> {
        try {
            const stored = await AsyncStorage.getItem(TOKEN_STORAGE_KEY);
            if (stored) {
                const tokens = JSON.parse(stored);
                if (this.isValidSession(tokens)) {
                    return tokens;
                }
            }
        } catch (e) {
            console.warn('[Stackhouse] Failed to read persisted auth:', e);
        }
        return null;
    }
}

// Re-export everything from the base JS SDK
export { StackhouseClient };
export * from '@stackhouse/js';
