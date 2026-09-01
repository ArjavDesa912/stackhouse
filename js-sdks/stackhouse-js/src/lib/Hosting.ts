export interface DeployManifest {
    files: Record<string, string>; // Path -> Hash
    config?: {
        routes?: string[];
    };
}

export interface Deployment {
    id: string;
    status: 'building' | 'deploying' | 'live' | 'failed';
    created_at: string;
    url?: string;
    manifest: DeployManifest;
}

export class HostingClient {
    constructor(private url: string, private headers: Record<string, string>) { }

    async deploy(manifest: DeployManifest, files: Record<string, Blob | Buffer | string>): Promise<any> {
        // 1. Init Deployment
        const initRes = await fetch(`${this.url}/v1/deploy/init`, {
            method: 'POST',
            headers: { ...this.headers, 'Content-Type': 'application/json' },
            body: JSON.stringify(manifest)
        });
        if (!initRes.ok) throw new Error(`Deploy init failed: ${initRes.statusText}`);
        const { deployment_id, missing_hashes } = await initRes.json();

        // 2. Upload missing chunks
        for (const hash of missing_hashes) {
            const content = this.findContentByHash(hash, manifest, files);
            if (content) {
                const uploadRes = await fetch(`${this.url}/v1/deploy/upload/${deployment_id}`, {
                    method: 'PUT',
                    headers: {
                        ...this.headers,
                        'Content-Type': 'application/octet-stream',
                        'X-File-Hash': hash
                    },
                    body: content
                });
                if (!uploadRes.ok) throw new Error(`Upload failed for hash ${hash}: ${uploadRes.statusText}`);
            }
        }

        // 3. Finalize
        const finalRes = await fetch(`${this.url}/v1/deploy/finalize/${deployment_id}`, {
            method: 'POST',
            headers: this.headers
        });
        if (!finalRes.ok) throw new Error(`Deploy finalize failed: ${finalRes.statusText}`);

        return finalRes.json();
    }

    private findContentByHash(targetHash: string, manifest: DeployManifest, files: Record<string, any>): any {
        for (const [path, hash] of Object.entries(manifest.files)) {
            if (hash === targetHash) {
                return files[path];
            }
        }
        return null;
    }

    /** List all deployments */
    async listDeployments(): Promise<Deployment[]> {
        const res = await fetch(`${this.url}/v1/deploy/list`, {
            method: 'GET',
            headers: this.headers
        });
        if (!res.ok) throw new Error(`Failed to list deployments: ${res.statusText}`);
        const data = await res.json();
        return data.deployments ?? data ?? [];
    }

    /** Get a specific deployment by ID */
    async getDeployment(deploymentId: string): Promise<Deployment> {
        const res = await fetch(`${this.url}/v1/deploy/${deploymentId}`, {
            method: 'GET',
            headers: this.headers
        });
        if (!res.ok) throw new Error(`Failed to get deployment: ${res.statusText}`);
        return res.json();
    }

    /** Delete a deployment */
    async deleteDeployment(deploymentId: string): Promise<void> {
        const res = await fetch(`${this.url}/v1/deploy/${deploymentId}`, {
            method: 'DELETE',
            headers: this.headers
        });
        if (!res.ok) throw new Error(`Failed to delete deployment: ${res.statusText}`);
    }
}
