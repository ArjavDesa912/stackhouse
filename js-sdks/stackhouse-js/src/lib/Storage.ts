export class StorageClient {
    constructor(private url: string, private headers: Record<string, string>) { }

    async listBuckets(): Promise<any> {
        const res = await fetch(`${this.url}/v1/storage/buckets`, {
            headers: this.headers
        });
        return res.json();
    }

    async upload(bucket: string, path: string, file: File | Blob): Promise<any> {
        const formData = new FormData();
        formData.append('file', file);
        formData.append('path', path);

        const res = await fetch(`${this.url}/v1/storage/files/${bucket}`, {
            method: 'POST',
            headers: {
                ...this.headers,
                // Do not set Content-Type for FormData, browser sets it with boundary
            },
            body: formData
        });
        return res.json();
    }

    async delete(bucket: string, path: string): Promise<any> {
        const res = await fetch(`${this.url}/v1/storage/files/${bucket}/${path}`, {
            method: 'DELETE',
            headers: this.headers
        });
        return res.json();
    }

    async download(bucket: string, path: string): Promise<Blob> {
        const res = await fetch(`${this.url}/v1/storage/files/${bucket}/${path}`, {
            headers: this.headers
        });
        if (!res.ok) throw new Error('Download failed');
        return res.blob();
    }

    getPublicUrl(bucket: string, path: string): string {
        return `${this.url}/v1/storage/public/${bucket}/${path}`;
    }
}
