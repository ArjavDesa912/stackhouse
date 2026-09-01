/**
 * RlsClient - Row Level Security management for Stackhouse
 * Manage RLS policies on database tables
 */

export interface RlsPolicy {
    name: string;
    table: string;
    operation: string;
    permissive: boolean;
    using_expression?: string;
    check_expression?: string;
}

export interface RlsStatus {
    table: string;
    enabled: boolean;
    policies: RlsPolicy[];
}

export class RlsError extends Error {
    constructor(
        message: string,
        public statusCode?: number,
        public details?: any
    ) {
        super(message);
        this.name = 'RlsError';
    }
}

export class RlsTableClient {
    constructor(
        private url: string,
        private table: string,
        private headers: Record<string, string>
    ) { }

    /**
     * Enable RLS on this table
     */
    async enable(): Promise<void> {
        const response = await fetch(`${this.url}/v1/rls/${this.table}/enable`, {
            method: 'POST',
            headers: this.headers
        });

        if (!response.ok) {
            throw await this.handleError(response);
        }
    }

    /**
     * Disable RLS on this table
     */
    async disable(): Promise<void> {
        const response = await fetch(`${this.url}/v1/rls/${this.table}/disable`, {
            method: 'POST',
            headers: this.headers
        });

        if (!response.ok) {
            throw await this.handleError(response);
        }
    }

    /**
     * Create an RLS policy
     */
    async createPolicy(options: {
        name: string;
        operation?: string;
        permissive?: boolean;
        usingExpression?: string;
        checkExpression?: string;
    }): Promise<void> {
        const response = await fetch(`${this.url}/v1/rls/${this.table}/policies`, {
            method: 'POST',
            headers: this.headers,
            body: JSON.stringify({
                name: options.name,
                operation: options.operation || 'ALL',
                permissive: options.permissive ?? true,
                ...(options.usingExpression && { using_expression: options.usingExpression }),
                ...(options.checkExpression && { check_expression: options.checkExpression })
            })
        });

        if (!response.ok) {
            throw await this.handleError(response);
        }
    }

    /**
     * List all RLS policies on this table
     */
    async listPolicies(): Promise<RlsPolicy[]> {
        const response = await fetch(`${this.url}/v1/rls/${this.table}/policies`, {
            headers: this.headers
        });

        if (!response.ok) {
            throw await this.handleError(response);
        }

        const result = await response.json();
        return result.data;
    }

    /**
     * Drop an RLS policy by name
     */
    async dropPolicy(policyName: string): Promise<void> {
        const response = await fetch(`${this.url}/v1/rls/${this.table}/policies/${policyName}`, {
            method: 'DELETE',
            headers: this.headers
        });

        if (!response.ok) {
            throw await this.handleError(response);
        }
    }

    /**
     * Get RLS status for this table
     */
    async getStatus(): Promise<RlsStatus> {
        const response = await fetch(`${this.url}/v1/rls/${this.table}/status`, {
            headers: this.headers
        });

        if (!response.ok) {
            throw await this.handleError(response);
        }

        const result = await response.json();
        return result.data;
    }

    private async handleError(response: Response): Promise<RlsError> {
        let message = `HTTP ${response.status}`;
        let details: any;

        try {
            const data = await response.json();
            message = data.message || data.error || message;
            details = data;
        } catch {
            message = response.statusText || message;
        }

        return new RlsError(message, response.status, details);
    }
}

/**
 * RlsClient provides access to RLS operations for any table
 */
export class RlsClient {
    constructor(
        private url: string,
        private headers: Record<string, string>
    ) { }

    /**
     * Get an RLS client for a specific table
     */
    table(tableName: string): RlsTableClient {
        return new RlsTableClient(this.url, tableName, this.headers);
    }
}
