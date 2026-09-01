export interface Connector {
  name: string;
  provider: string;
  config: Record<string, any>;
}

export class ConnectorsClient {
  private url: string;
  private headers: Record<string, string>;

  constructor(url: string, headers: Record<string, string>) {
      this.url = `${url}/v1/connectors`;
      this.headers = headers;
  }

  async list(): Promise<Connector[]> {
      const res = await fetch(this.url, { headers: this.headers });
      if (!res.ok) throw new Error("Failed to list connectors");
      const json = await res.json();
      return json.data;
  }

  async getProviders(): Promise<any[]> {
      const res = await fetch(`${this.url}/providers`, { headers: this.headers });
      if (!res.ok) throw new Error("Failed to list providers");
      const json = await res.json();
      return json.data;
  }

  async register(name: string, provider: string, config: Record<string, any>): Promise<void> {
      const res = await fetch(this.url, {
          method: 'POST',
          headers: this.headers,
          body: JSON.stringify({ name, provider, config })
      });
      if (!res.ok) throw new Error("Failed to register connector");
  }

  async remove(name: string): Promise<void> {
      const res = await fetch(`${this.url}/${name}`, {
          method: 'DELETE',
          headers: this.headers
      });
      if (!res.ok) throw new Error("Failed to remove connector");
  }

  async testHealth(name: string): Promise<any> {
      const res = await fetch(`${this.url}/${name}/test`, {
          method: 'POST',
          headers: this.headers
      });
      if (!res.ok) throw new Error("Failed to test health");
      const json = await res.json();
      return json.data;
  }
}
