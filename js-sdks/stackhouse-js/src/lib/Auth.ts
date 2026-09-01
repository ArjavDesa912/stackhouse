/**
 * AuthClient - Authentication and session management
 * Production-ready implementation with JWT token refresh
 */

export interface User {
    id: number;
    email: string;
    created_at: string;
    updated_at: string;
    metadata?: any;
}

export interface AuthSession {
    user: User;
    access_token: string;
    refresh_token: string;
    expires_at: number;
}

export interface AuthTokens {
    access_token: string;
    refresh_token: string;
    expires_in: number;
    token_type: string;
    user: User;
}

export type AuthListener = (session: AuthSession | null) => void;

export class AuthError extends Error {
    constructor(
        message: string,
        public statusCode?: number,
        public details?: any
    ) {
        super(message);
        this.name = 'AuthError';
    }
}

export class AuthClient {
    private listeners: AuthListener[] = [];
    private session: AuthSession | null = null;
    private refreshTimer: NodeJS.Timeout | null = null;
    private refreshPromise: Promise<AuthSession | null> | null = null;

    constructor(
        private url: string,
        private headers: Record<string, string>
    ) {
        this.recoverSession();
    }

    private recoverSession(): void {
        if (typeof window === 'undefined' || !window.localStorage) {
            return;
        }

        try {
            const stored = localStorage.getItem('stackhouse_session');
            if (stored) {
                const session = JSON.parse(stored) as AuthSession;
                // Check if session is not expired
                if (session.expires_at > Date.now()) {
                    this.session = session;
                    this.updateHeaders(session.access_token);
                    this.scheduleTokenRefresh();
                } else {
                    // Session expired, clear it
                    this.clearSession();
                }
            }
        } catch (e) {
            console.error('[Stackhouse] Failed to parse stored session:', e);
            this.clearSession();
        }
    }

    private saveSession(session: AuthSession): void {
        if (typeof window !== 'undefined' && window.localStorage) {
            localStorage.setItem('stackhouse_session', JSON.stringify(session));
        }
    }

    private clearSession(): void {
        if (typeof window !== 'undefined' && window.localStorage) {
            localStorage.removeItem('stackhouse_session');
        }
    }

    private updateHeaders(token: string): void {
        this.headers['Authorization'] = `Bearer ${token}`;
    }

    private notify(): void {
        this.listeners.forEach(listener => listener(this.session));
    }

    private scheduleTokenRefresh(): void {
        if (this.refreshTimer) {
            clearTimeout(this.refreshTimer);
        }

        if (!this.session) {
            return;
        }

        // Refresh 5 minutes before expiration
        const timeUntilExpiry = this.session.expires_at - Date.now();
        const refreshDelay = Math.max(timeUntilExpiry - 5 * 60 * 1000, 0);

        this.refreshTimer = setTimeout(() => {
            this.refreshAccessToken().catch(error => {
                console.error('[Stackhouse] Auto-refresh failed:', error);
                // If refresh fails, sign out the user
                this.signOut();
            });
        }, refreshDelay);
    }

    private async refreshAccessToken(): Promise<AuthSession> {
        // Prevent multiple concurrent refresh attempts
        if (this.refreshPromise) {
            return this.refreshPromise.then(session => {
                if (!session) throw new AuthError('Failed to refresh token');
                return session;
            });
        }

        this.refreshPromise = this.performRefresh();

        try {
            const session = await this.refreshPromise;
            if (!session) {
                throw new AuthError('Failed to refresh token');
            }
            return session;
        } finally {
            this.refreshPromise = null;
        }
    }

    private async performRefresh(): Promise<AuthSession | null> {
        if (!this.session?.refresh_token) {
            return null;
        }

        try {
            const response = await fetch(`${this.url}/v1/auth/refresh`, {
                method: 'POST',
                headers: {
                    ...this.headers,
                    'Content-Type': 'application/json'
                },
                body: JSON.stringify({
                    refresh_token: this.session.refresh_token
                })
            });

            if (!response.ok) {
                throw await this.handleError(response);
            }

            const result = await response.json();
            const tokens = result.data as AuthTokens;

            const newSession: AuthSession = {
                user: tokens.user,
                access_token: tokens.access_token,
                refresh_token: tokens.refresh_token,
                expires_at: Date.now() + (tokens.expires_in * 1000)
            };

            this.session = newSession;
            this.updateHeaders(newSession.access_token);
            this.saveSession(newSession);
            this.scheduleTokenRefresh();
            this.notify();

            return newSession;
        } catch (error) {
            console.error('[Stackhouse] Token refresh failed:', error);
            this.clearSession();
            this.session = null;
            this.notify();
            return null;
        }
    }

    private async handleError(response: Response): Promise<AuthError> {
        let message = `HTTP ${response.status}`;
        let details: any;

        try {
            const data = await response.json();
            message = data.message || data.error || message;
            details = data;
        } catch {
            message = response.statusText || message;
        }

        return new AuthError(message, response.status, details);
    }

    /**
     * Subscribe to auth state changes
     * @param callback Function called with current session (null if signed out)
     * @returns Unsubscribe function
     */
    onAuthStateChange(callback: AuthListener): () => void {
        this.listeners.push(callback);
        // Immediate callback with current state
        callback(this.session);
        return () => {
            this.listeners = this.listeners.filter(l => l !== callback);
        };
    }

    /**
     * Get the current session
     */
    getSession(): AuthSession | null {
        return this.session;
    }

    /**
     * Get the current user
     */
    getUser(): User | null {
        return this.session?.user || null;
    }

    /**
     * Check if user is signed in
     */
    isAuthenticated(): boolean {
        return this.session !== null && this.session.expires_at > Date.now();
    }

    /**
     * Get the current access token
     */
    getAccessToken(): string | null {
        return this.session?.access_token || null;
    }

    /**
     * Sign up a new user
     * POST /v1/auth/signup
     */
    async signUp(email: string, password: string, metadata?: any): Promise<AuthSession> {
        const response = await fetch(`${this.url}/v1/auth/signup`, {
            method: 'POST',
            headers: {
                ...this.headers,
                'Content-Type': 'application/json'
            },
            body: JSON.stringify({
                email,
                password,
                metadata
            })
        });

        if (!response.ok) {
            throw await this.handleError(response);
        }

        const result = await response.json();
        const tokens = result.data as AuthTokens;

        const session: AuthSession = {
            user: tokens.user,
            access_token: tokens.access_token,
            refresh_token: tokens.refresh_token,
            expires_at: Date.now() + (tokens.expires_in * 1000)
        };

        this.session = session;
        this.updateHeaders(session.access_token);
        this.saveSession(session);
        this.scheduleTokenRefresh();
        this.notify();

        return session;
    }

    /**
     * Sign in with email and password
     * POST /v1/auth/login
     */
    async signIn(email: string, password: string): Promise<AuthSession> {
        const response = await fetch(`${this.url}/v1/auth/login`, {
            method: 'POST',
            headers: {
                ...this.headers,
                'Content-Type': 'application/json'
            },
            body: JSON.stringify({
                email,
                password
            })
        });

        if (!response.ok) {
            throw await this.handleError(response);
        }

        const result = await response.json();
        const tokens = result.data as AuthTokens;

        const session: AuthSession = {
            user: tokens.user,
            access_token: tokens.access_token,
            refresh_token: tokens.refresh_token,
            expires_at: Date.now() + (tokens.expires_in * 1000)
        };

        this.session = session;
        this.updateHeaders(session.access_token);
        this.saveSession(session);
        this.scheduleTokenRefresh();
        this.notify();

        return session;
    }

    /**
     * Sign out the current user
     * POST /v1/auth/logout
     */
    async signOut(): Promise<void> {
        const refreshToken = this.session?.refresh_token;

        if (refreshToken) {
            try {
                await fetch(`${this.url}/v1/auth/logout`, {
                    method: 'POST',
                    headers: {
                        ...this.headers,
                        'Content-Type': 'application/json'
                    },
                    body: JSON.stringify({
                        refresh_token: refreshToken
                    })
                });
            } catch (error) {
                console.error('[Stackhouse] Logout request failed:', error);
            }
        }

        // Clear local session regardless of logout request success
        if (this.refreshTimer) {
            clearTimeout(this.refreshTimer);
            this.refreshTimer = null;
        }

        this.session = null;
        delete this.headers['Authorization'];
        this.clearSession();
        this.notify();
    }

    /**
     * Update user metadata
     */
    async updateMetadata(metadata: any): Promise<User> {
        if (!this.session) {
            throw new AuthError('Not authenticated');
        }

        const response = await fetch(`${this.url}/v1/auth/user`, {
            method: 'PUT',
            headers: {
                ...this.headers,
                'Content-Type': 'application/json'
            },
            body: JSON.stringify({ metadata })
        });

        if (!response.ok) {
            throw await this.handleError(response);
        }

        const result = await response.json();
        const updatedUser = result.data as User;

        this.session.user = updatedUser;
        this.saveSession(this.session);
        this.notify();

        return updatedUser;
    }

    /**
     * Change the current user's password
     * POST /v1/auth/change-password
     */
    async changePassword(currentPassword: string, newPassword: string): Promise<void> {
        if (!this.session) {
            throw new AuthError('Not authenticated');
        }

        const response = await fetch(`${this.url}/v1/auth/change-password`, {
            method: 'POST',
            headers: {
                ...this.headers,
                'Content-Type': 'application/json'
            },
            body: JSON.stringify({
                current_password: currentPassword,
                new_password: newPassword
            })
        });

        if (!response.ok) {
            throw await this.handleError(response);
        }
    }

    /**
     * Manually refresh the access token
     */
    async refresh(): Promise<AuthSession> {
        const session = await this.refreshAccessToken();
        if (!session) {
            throw new AuthError('Failed to refresh token');
        }
        return session;
    }

    /**
     * Set the session manually (useful for SSR or custom auth flows)
     */
    setSession(session: AuthSession): void {
        if (session.expires_at <= Date.now()) {
            throw new AuthError('Session has expired');
        }

        this.session = session;
        this.updateHeaders(session.access_token);
        this.saveSession(session);
        this.scheduleTokenRefresh();
        this.notify();
    }
}
