import { AuthClient, AuthSession, User, AuthError, AuthListener } from './lib/Auth';
import { QueryBuilder, QueryOptions, QueryResult, StackhouseError, QueryFilter } from './lib/QueryBuilder';
import { StorageClient } from './lib/Storage';
import { HostingClient, DeployManifest, Deployment } from './lib/Hosting';
import { VectorClient, VectorCollectionClient, VectorSearchOptions, VectorUpsertOptions, VectorSearchResult, VectorInfo } from './lib/Vectors';
import { RealtimeClient, RealtimeEvent, RealtimeEventType, RealtimeCallback } from './lib/Realtime';
import { RlsClient, RlsTableClient, RlsPolicy, RlsStatus, RlsError } from './lib/Rls';
import { ConnectorsClient, Connector } from './lib/Connectors';
import {
    BillingClient,
    BillingAdminClient,
    BillingError,
    EntitlementInfo,
    CustomerInfo,
    Offering,
    Package as BillingPackage,
    Product as BillingProduct,
    Subscription as BillingSubscription,
    Customer as BillingCustomer,
    App as BillingApp,
    Store as BillingStore,
    Experiment,
    Audience,
    Paywall,
    Variant,
    VariantResult,
    ResolvedOffering,
    ResolvedOfferingContext,
} from './lib/Billing';

/**
 * StackhouseClient - Main client for interacting with Stackhouse
 * Production-ready implementation — better than Supabase
 */
export class StackhouseClient {
    public auth: AuthClient;
    public storage: StorageClient;
    public hosting: HostingClient;
    public realtime: RealtimeClient;
    public connectors: ConnectorsClient;
    public billing: BillingClient;
    private vectorClient: VectorClient;
    private rlsClient: RlsClient;
    private url: string;
    private headers: Record<string, string>;

    /**
     * Create a new StackhouseClient instance
     * @param url Base URL of the Stackhouse server (e.g., 'http://localhost:3000')
     * @param key Optional API key for authentication (not the same as access token)
     */
    constructor(url: string, key?: string) {
        this.url = url.replace(/\/$/, ''); // Remove trailing slash
        this.headers = {
            'Content-Type': 'application/json',
            ...(key && { 'X-API-Key': key })
        };

        // Initialize clients with shared headers (they will be updated by auth)
        this.auth = new AuthClient(this.url, this.headers);
        this.storage = new StorageClient(this.url, this.headers);
        this.hosting = new HostingClient(this.url, this.headers);
        this.vectorClient = new VectorClient(this.url, this.headers);
        this.realtime = new RealtimeClient(this.url, this.headers);
        this.rlsClient = new RlsClient(this.url, this.headers);
        this.connectors = new ConnectorsClient(this.url, this.headers);
        this.billing = new BillingClient(this.url, this.headers);
    }

    /**
     * Get a query builder for a specific collection
     * @param collection Name of the collection
     * @returns QueryBuilder instance
     */
    from(collection: string): QueryBuilder {
        return new QueryBuilder(this.url, collection, this.headers);
    }

    /**
     * Get a vector search client for a specific collection
     * @param collection Name of the collection with vector data
     * @returns VectorCollectionClient instance
     * 
     * @example
     * ```ts
     * // Search for similar documents
     * const results = await stackhouse.vectors('documents').search(queryEmbedding, { topK: 5 });
     * 
     * // Upsert with embedding
     * await stackhouse.vectors('documents').upsert(embedding, { data: { title: 'Hello' } });
     * ```
     */
    vectors(collection: string): VectorCollectionClient {
        return this.vectorClient.collection(collection);
    }

    /**
     * Get the current auth session
     */
    getSession(): AuthSession | null {
        return this.auth.getSession();
    }

    /**
     * Get the current authenticated user
     */
    getUser(): User | null {
        return this.auth.getUser();
    }

    /**
     * Check if user is authenticated
     */
    isAuthenticated(): boolean {
        return this.auth.isAuthenticated();
    }

    /**
     * Subscribe to auth state changes
     * @param callback Function called with current session (null if signed out)
     * @returns Unsubscribe function
     */
    onAuthStateChange(callback: AuthListener): () => void {
        return this.auth.onAuthStateChange(callback);
    }

    /**
     * Sign up a new user
     */
    async signUp(email: string, password: string, metadata?: any): Promise<AuthSession> {
        return this.auth.signUp(email, password, metadata);
    }

    /**
     * Sign in with email and password
     */
    async signIn(email: string, password: string): Promise<AuthSession> {
        return this.auth.signIn(email, password);
    }

    /**
     * Sign out the current user
     */
    async signOut(): Promise<void> {
        return this.auth.signOut();
    }

    /**
     * Change the current user's password
     */
    async changePassword(currentPassword: string, newPassword: string): Promise<void> {
        return this.auth.changePassword(currentPassword, newPassword);
    }

    /**
     * Get an RLS client for a specific table
     * @param tableName Name of the table
     * @returns RlsTableClient instance
     *
     * @example
     * ```ts
     * // Enable RLS on a table
     * await stackhouse.rls('posts').enable();
     *
     * // Create a policy
     * await stackhouse.rls('posts').createPolicy({
     *   name: 'users_own_posts',
     *   operation: 'SELECT',
     *   usingExpression: 'user_id = auth.uid()'
     * });
     * ```
     */
    rls(tableName: string): RlsTableClient {
        return this.rlsClient.table(tableName);
    }
}

/**
 * Create a StackhouseClient instance
 * @param url Base URL of the Stackhouse server
 * @param key Optional API key
 */
export function createClient(url: string, key?: string): StackhouseClient {
    return new StackhouseClient(url, key);
}

// Export all types and classes
export { AuthClient, AuthSession, User, AuthError, AuthListener };
export { QueryBuilder, QueryOptions, QueryResult, StackhouseError, QueryFilter };
export { StorageClient };
export { HostingClient, DeployManifest, Deployment };
export { VectorClient, VectorCollectionClient, VectorSearchOptions, VectorUpsertOptions, VectorSearchResult, VectorInfo };
export { RealtimeClient, RealtimeEvent, RealtimeEventType, RealtimeCallback };
export { RlsClient, RlsTableClient, RlsPolicy, RlsStatus, RlsError };
export { ConnectorsClient, Connector };
export {
    BillingClient,
    BillingAdminClient,
    BillingError,
    EntitlementInfo,
    CustomerInfo,
    Offering,
    BillingPackage,
    BillingProduct,
    BillingSubscription,
    BillingCustomer,
    BillingApp,
    BillingStore,
    Experiment,
    Audience,
    Paywall,
    Variant,
    VariantResult,
    ResolvedOffering,
    ResolvedOfferingContext,
};
