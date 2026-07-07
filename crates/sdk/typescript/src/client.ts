import type { AgentInfo, AgentStatus, EventMessage } from "./types.js";

export interface AgentClientOptions {
  baseUrl: string;
  timeout?: number;
}

interface FetchOptions {
  method: string;
  path: string;
  body?: unknown;
}

export class AgentClient {
  private baseUrl: string;
  private timeout: number;

  constructor(options: AgentClientOptions) {
    this.baseUrl = options.baseUrl.replace(/\/$/, "");
    this.timeout = options.timeout ?? 30_000;
  }

  private async request<T>(opts: FetchOptions): Promise<T> {
    const controller = new AbortController();
    const timer = setTimeout(() => controller.abort(), this.timeout);

    try {
      const response = await fetch(`${this.baseUrl}${opts.path}`, {
        method: opts.method,
        headers: { "Content-Type": "application/json" },
        body: opts.body ? JSON.stringify(opts.body) : undefined,
        signal: controller.signal,
      });

      if (!response.ok) {
        throw new Error(`HTTP ${response.status}: ${response.statusText}`);
      }

      return response.json() as Promise<T>;
    } finally {
      clearTimeout(timer);
    }
  }

  async listAgents(): Promise<AgentInfo[]> {
    return this.request<AgentInfo[]>({ method: "GET", path: "/api/agents" });
  }

  async getAgent(id: string): Promise<AgentInfo> {
    return this.request<AgentInfo>({ method: "GET", path: `/api/agents/${id}` });
  }

  async spawnAgent(name: string, capabilities?: string[]): Promise<AgentInfo> {
    return this.request<AgentInfo>({
      method: "POST",
      path: "/api/agents",
      body: { name, capabilities: capabilities ?? [] },
    });
  }

  async stopAgent(id: string): Promise<void> {
    await this.request<void>({ method: "POST", path: `/api/agents/${id}/stop` });
  }

  async publish(topic: string, payload: unknown): Promise<string> {
    const result = await this.request<{ messageId: string }>({
      method: "POST",
      path: "/api/publish",
      body: { topic, payload },
    });
    return result.messageId;
  }
}
