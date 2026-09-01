/**
 * VectorClient - Vector search operations for Stackhouse
 * Powered by pgvector for similarity search with HNSW indexing
 */

export interface VectorSearchOptions {
    /** Number of results to return (default: 10) */
    topK?: number;
    /** Distance metric: 'cosine' | 'l2' | 'inner_product' (default: 'cosine') */
    metric?: 'cosine' | 'l2' | 'inner_product';
    /** Optional metadata filters */
    filters?: Record<string, any>;
    /** Vector column name (default: 'embedding') */
    column?: string;
}

export interface VectorUpsertOptions {
    /** Optional ID for update (omit for insert) */
    id?: number;
    /** Additional data to store alongside the vector */
    data?: Record<string, any>;
    /** Vector column name (default: 'embedding') */
    column?: string;
}

export interface VectorSearchResult {
    id: number;
    similarity: number;
    data: Record<string, any>;
}

export interface VectorInfo {
    table: string;
    column: string;
    dimensions: number;
    index_type: string;
    row_count: number;
}

export class VectorClient {
    constructor(
        private url: string,
        private headers: Record<string, string>
    ) { }

    /**
     * Get a vector query builder for a specific collection
     */
    collection(name: string): VectorCollectionClient {
        return new VectorCollectionClient(this.url, name, this.headers);
    }
}

export class VectorCollectionClient {
    constructor(
        private url: string,
        private collectionName: string,
        private headers: Record<string, string>
    ) { }

    /**
     * Perform a similarity search
     * @param queryVector The query embedding vector
     * @param options Search options (topK, metric, filters)
     */
    async search(queryVector: number[], options?: VectorSearchOptions): Promise<{
        success: boolean;
        data: VectorSearchResult[];
        count: number;
    }> {
        const response = await fetch(
            `${this.url}/v1/vectors/${this.collectionName}/search`,
            {
                method: 'POST',
                headers: this.headers,
                body: JSON.stringify({
                    vector: queryVector,
                    top_k: options?.topK ?? 10,
                    metric: options?.metric ?? 'cosine',
                    filters: options?.filters,
                    column: options?.column ?? 'embedding',
                }),
            }
        );

        if (!response.ok) {
            const error = await response.json().catch(() => ({}));
            throw new Error(error?.error?.message || `Vector search failed: ${response.status}`);
        }

        return response.json();
    }

    /**
     * Insert or update a record with a vector embedding
     * @param embedding The embedding vector
     * @param options Upsert options (id, data, column)
     */
    async upsert(embedding: number[], options?: VectorUpsertOptions): Promise<{
        success: boolean;
        data: { id: number; collection: string; dimensions: number };
        message: string;
    }> {
        const response = await fetch(
            `${this.url}/v1/vectors/${this.collectionName}/upsert`,
            {
                method: 'POST',
                headers: this.headers,
                body: JSON.stringify({
                    embedding,
                    id: options?.id,
                    data: options?.data,
                    column: options?.column ?? 'embedding',
                }),
            }
        );

        if (!response.ok) {
            const error = await response.json().catch(() => ({}));
            throw new Error(error?.error?.message || `Vector upsert failed: ${response.status}`);
        }

        return response.json();
    }

    /**
     * Batch insert/update records with vector embeddings
     */
    async batchUpsert(records: Array<{ embedding: number[]; id?: number; data?: Record<string, any> }>): Promise<{
        success: boolean;
        data: { ids: number[]; collection: string; count: number };
        message: string;
    }> {
        const response = await fetch(
            `${this.url}/v1/vectors/${this.collectionName}/batch`,
            {
                method: 'POST',
                headers: this.headers,
                body: JSON.stringify({
                    records: records.map(r => ({
                        embedding: r.embedding,
                        id: r.id,
                        data: r.data,
                        column: 'embedding',
                    })),
                }),
            }
        );

        if (!response.ok) {
            const error = await response.json().catch(() => ({}));
            throw new Error(error?.error?.message || `Batch upsert failed: ${response.status}`);
        }

        return response.json();
    }

    /**
     * Get vector column metadata for this collection
     */
    async info(): Promise<{ success: boolean; data: VectorInfo[] }> {
        const response = await fetch(
            `${this.url}/v1/vectors/${this.collectionName}/info`,
            { headers: this.headers }
        );

        if (!response.ok) {
            const error = await response.json().catch(() => ({}));
            throw new Error(error?.error?.message || `Vector info failed: ${response.status}`);
        }

        return response.json();
    }
}
