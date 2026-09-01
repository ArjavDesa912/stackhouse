/**
 * QueryBuilder - Build and execute queries on Stackhouse collections
 * Production-ready implementation with comprehensive error handling
 */

export interface QueryFilter {
    equals?: Record<string, any>;
    notEquals?: Record<string, any>;
    gt?: Record<string, number>;
    gte?: Record<string, number>;
    lt?: Record<string, number>;
    lte?: Record<string, number>;
    contains?: Record<string, string>;
    startsWith?: Record<string, string>;
    endsWith?: Record<string, string>;
}

export interface QueryOptions {
    filters?: QueryFilter;
    orderBy?: string;
    orderDir?: 'ASC' | 'DESC';
    limit?: number;
    offset?: number;
}

export interface QueryResult {
    success: boolean;
    data: any[];
    count: number;
    total: number;
    collection: string;
}

export class StackhouseError extends Error {
    constructor(
        message: string,
        public statusCode?: number,
        public details?: any
    ) {
        super(message);
        this.name = 'StackhouseError';
    }
}

export class QueryBuilder {
    private aborted = false;
    private controller: AbortController | null = null;

    constructor(
        private url: string,
        private collection: string,
        private headers: Record<string, string>
    ) { }

    /**
     * Query documents with optional filters and pagination
     * Matches backend API: GET /v1/query/:collection
     */
    async select(options?: QueryOptions): Promise<QueryResult> {
        this.ensureNotAborted();
        this.controller = new AbortController();

        try {
            const params = new URLSearchParams();

            // Build query parameters matching backend format
            if (options?.filters) {
                this.applyFilters(params, options.filters);
            }

            if (options?.orderBy) {
                params.append('order_by', options.orderBy);
                params.append('order_dir', options.orderDir || 'ASC');
            }

            if (options?.limit !== undefined) {
                params.append('limit', Math.min(options.limit, 1000).toString());
            }

            if (options?.offset !== undefined) {
                params.append('offset', options.offset.toString());
            }

            const queryString = params.toString();
            const url = `${this.url}/v1/query/${this.collection}${queryString ? `?${queryString}` : ''}`;

            const response = await fetch(url, {
                headers: this.headers,
                signal: this.controller.signal
            });

            if (!response.ok) {
                throw await this.handleError(response);
            }

            const result = await response.json();
            return result;
        } catch (error) {
            if (error instanceof StackhouseError) throw error;
            if (error instanceof Error && error.name === 'AbortError') {
                throw new StackhouseError('Query was aborted');
            }
            throw new StackhouseError(`Query failed: ${error instanceof Error ? error.message : 'Unknown error'}`);
        } finally {
            this.controller = null;
        }
    }

    /**
     * Get a single document by ID
     * Matches backend API: GET /v1/query/:collection/:id
     */
    async getById(id: string | number): Promise<any> {
        this.ensureNotAborted();
        this.controller = new AbortController();

        try {
            const response = await fetch(`${this.url}/v1/query/${this.collection}/${id}`, {
                headers: this.headers,
                signal: this.controller.signal
            });

            if (!response.ok) {
                throw await this.handleError(response);
            }

            const result = await response.json();
            return result.data;
        } catch (error) {
            if (error instanceof StackhouseError) throw error;
            if (error instanceof Error && error.name === 'AbortError') {
                throw new StackhouseError('Request was aborted');
            }
            throw new StackhouseError(`Get by ID failed: ${error instanceof Error ? error.message : 'Unknown error'}`);
        } finally {
            this.controller = null;
        }
    }

    /**
     * Insert a new document
     * Matches backend API: POST /v1/push/:collection
     */
    async insert(data: any): Promise<{ success: boolean; data: { id: number; collection: string; columns_added: string[] }; message?: string }> {
        this.ensureNotAborted();
        this.controller = new AbortController();

        try {
            const response = await fetch(`${this.url}/v1/push/${this.collection}`, {
                method: 'POST',
                headers: this.headers,
                body: JSON.stringify(data),
                signal: this.controller.signal
            });

            if (!response.ok) {
                throw await this.handleError(response);
            }

            return await response.json();
        } catch (error) {
            if (error instanceof StackhouseError) throw error;
            if (error instanceof Error && error.name === 'AbortError') {
                throw new StackhouseError('Insert was aborted');
            }
            throw new StackhouseError(`Insert failed: ${error instanceof Error ? error.message : 'Unknown error'}`);
        } finally {
            this.controller = null;
        }
    }

    /**
     * Insert multiple documents in batch
     * Matches backend API: POST /v1/push/:collection/batch
     */
    async insertBatch(data: any[]): Promise<{ success: boolean; data: { inserted: number; collection: string; columns_added: string[] } }> {
        this.ensureNotAborted();
        this.controller = new AbortController();

        if (!Array.isArray(data) || data.length === 0) {
            throw new StackhouseError('Batch insert requires a non-empty array');
        }

        try {
            const response = await fetch(`${this.url}/v1/push/${this.collection}/batch`, {
                method: 'POST',
                headers: this.headers,
                body: JSON.stringify(data),
                signal: this.controller.signal
            });

            if (!response.ok) {
                throw await this.handleError(response);
            }

            return await response.json();
        } catch (error) {
            if (error instanceof StackhouseError) throw error;
            if (error instanceof Error && error.name === 'AbortError') {
                throw new StackhouseError('Batch insert was aborted');
            }
            throw new StackhouseError(`Batch insert failed: ${error instanceof Error ? error.message : 'Unknown error'}`);
        } finally {
            this.controller = null;
        }
    }

    /**
     * Update a document by ID
     * Matches backend API: POST /v1/update/:collection/:id
     */
    async update(id: string | number, data: any): Promise<{ success: boolean; affected: number; id: number }> {
        this.ensureNotAborted();
        this.controller = new AbortController();

        try {
            const response = await fetch(`${this.url}/v1/update/${this.collection}/${id}`, {
                method: 'POST',
                headers: this.headers,
                body: JSON.stringify(data),
                signal: this.controller.signal
            });

            if (!response.ok) {
                throw await this.handleError(response);
            }

            return await response.json();
        } catch (error) {
            if (error instanceof StackhouseError) throw error;
            if (error instanceof Error && error.name === 'AbortError') {
                throw new StackhouseError('Update was aborted');
            }
            throw new StackhouseError(`Update failed: ${error instanceof Error ? error.message : 'Unknown error'}`);
        } finally {
            this.controller = null;
        }
    }

    /**
     * Delete a document by ID
     * Matches backend API: POST /v1/delete/:collection/:id
     */
    async delete(id: string | number): Promise<{ success: boolean; affected: number; id: number }> {
        this.ensureNotAborted();
        this.controller = new AbortController();

        try {
            const response = await fetch(`${this.url}/v1/delete/${this.collection}/${id}`, {
                method: 'POST',
                headers: this.headers,
                signal: this.controller.signal
            });

            if (!response.ok) {
                throw await this.handleError(response);
            }

            return await response.json();
        } catch (error) {
            if (error instanceof StackhouseError) throw error;
            if (error instanceof Error && error.name === 'AbortError') {
                throw new StackhouseError('Delete was aborted');
            }
            throw new StackhouseError(`Delete failed: ${error instanceof Error ? error.message : 'Unknown error'}`);
        } finally {
            this.controller = null;
        }
    }

    /**
     * Bulk delete with optional filters
     * Matches backend API: POST /v1/delete/:collection
     */
    async bulkDelete(filters?: Record<string, any>): Promise<{ success: boolean; affected: number }> {
        this.ensureNotAborted();
        this.controller = new AbortController();

        try {
            const response = await fetch(`${this.url}/v1/delete/${this.collection}`, {
                method: 'POST',
                headers: this.headers,
                body: JSON.stringify({ filters: filters || {} }),
                signal: this.controller.signal
            });

            if (!response.ok) {
                throw await this.handleError(response);
            }

            return await response.json();
        } catch (error) {
            if (error instanceof StackhouseError) throw error;
            if (error instanceof Error && error.name === 'AbortError') {
                throw new StackhouseError('Bulk delete was aborted');
            }
            throw new StackhouseError(`Bulk delete failed: ${error instanceof Error ? error.message : 'Unknown error'}`);
        } finally {
            this.controller = null;
        }
    }

    /**
     * Bulk update with data and optional filters
     * Matches backend API: POST /v1/update/:collection
     */
    async bulkUpdate(data: Record<string, any>, filters?: Record<string, any>): Promise<{ success: boolean; affected: number }> {
        this.ensureNotAborted();
        this.controller = new AbortController();

        try {
            const response = await fetch(`${this.url}/v1/update/${this.collection}`, {
                method: 'POST',
                headers: this.headers,
                body: JSON.stringify({ data, filters: filters || {} }),
                signal: this.controller.signal
            });

            if (!response.ok) {
                throw await this.handleError(response);
            }

            return await response.json();
        } catch (error) {
            if (error instanceof StackhouseError) throw error;
            if (error instanceof Error && error.name === 'AbortError') {
                throw new StackhouseError('Bulk update was aborted');
            }
            throw new StackhouseError(`Bulk update failed: ${error instanceof Error ? error.message : 'Unknown error'}`);
        } finally {
            this.controller = null;
        }
    }

    /**
     * Drop the entire table
     * Matches backend API: DELETE /v1/tables/:collection
     */
    async dropTable(): Promise<{ success: boolean; message: string }> {
        this.ensureNotAborted();
        this.controller = new AbortController();

        try {
            const response = await fetch(`${this.url}/v1/tables/${this.collection}`, {
                method: 'DELETE',
                headers: this.headers,
                signal: this.controller.signal
            });

            if (!response.ok) {
                throw await this.handleError(response);
            }

            return await response.json();
        } catch (error) {
            if (error instanceof StackhouseError) throw error;
            if (error instanceof Error && error.name === 'AbortError') {
                throw new StackhouseError('Drop table was aborted');
            }
            throw new StackhouseError(`Drop table failed: ${error instanceof Error ? error.message : 'Unknown error'}`);
        } finally {
            this.controller = null;
        }
    }

    /**
     * Cancel the current query
     */
    abort(): void {
        if (this.controller) {
            this.controller.abort();
            this.aborted = true;
        }
    }

    /**
     * Reset the aborted state
     */
    reset(): void {
        this.aborted = false;
        this.controller = null;
    }

    private applyFilters(params: URLSearchParams, filters: QueryFilter): void {
        // Equals filters (field=value)
        if (filters.equals) {
            Object.entries(filters.equals).forEach(([field, value]) => {
                params.append(field, String(value));
            });
        }

        // Not equals (field.neq=value)
        if (filters.notEquals) {
            Object.entries(filters.notEquals).forEach(([field, value]) => {
                params.append(`${field}.neq`, String(value));
            });
        }

        // Greater than (field.gt=value)
        if (filters.gt) {
            Object.entries(filters.gt).forEach(([field, value]) => {
                params.append(`${field}.gt`, String(value));
            });
        }

        // Greater than or equal (field.gte=value)
        if (filters.gte) {
            Object.entries(filters.gte).forEach(([field, value]) => {
                params.append(`${field}.gte`, String(value));
            });
        }

        // Less than (field.lt=value)
        if (filters.lt) {
            Object.entries(filters.lt).forEach(([field, value]) => {
                params.append(`${field}.lt`, String(value));
            });
        }

        // Less than or equal (field.lte=value)
        if (filters.lte) {
            Object.entries(filters.lte).forEach(([field, value]) => {
                params.append(`${field}.lte`, String(value));
            });
        }

        // Contains - partial string match (backend will need to support this)
        if (filters.contains) {
            Object.entries(filters.contains).forEach(([field, value]) => {
                params.append(`${field}.contains`, String(value));
            });
        }

        // Starts with (backend will need to support this)
        if (filters.startsWith) {
            Object.entries(filters.startsWith).forEach(([field, value]) => {
                params.append(`${field}.startsWith`, String(value));
            });
        }

        // Ends with (backend will need to support this)
        if (filters.endsWith) {
            Object.entries(filters.endsWith).forEach(([field, value]) => {
                params.append(`${field}.endsWith`, String(value));
            });
        }
    }

    private async handleError(response: Response): Promise<StackhouseError> {
        let message = `HTTP ${response.status}`;
        let details: any;

        try {
            const data = await response.json();
            message = data.message || data.error || message;
            details = data;
        } catch {
            message = response.statusText || message;
        }

        return new StackhouseError(message, response.status, details);
    }

    private ensureNotAborted(): void {
        if (this.aborted) {
            throw new StackhouseError('QueryBuilder has been aborted. Create a new instance to continue.');
        }
    }
}
