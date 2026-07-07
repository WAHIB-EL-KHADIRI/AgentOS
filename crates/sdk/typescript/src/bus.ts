import { AgentClient } from "./client.js";

type EventCallback = (event: unknown) => void;

export class BusClient {
  private client: AgentClient;
  private eventSource: EventSource | null = null;
  private listeners = new Map<string, Set<EventCallback>>();

  constructor(client: AgentClient) {
    this.client = client;
  }

  connect(url: string): void {
    this.disconnect();
    this.eventSource = new EventSource(url);

    for (const [type, cbs] of this.listeners) {
      this.eventSource.addEventListener(type, (e: MessageEvent) => {
        const data = JSON.parse(e.data);
        for (const cb of cbs) cb(data);
      });
    }
  }

  disconnect(): void {
    if (this.eventSource) {
      this.eventSource.close();
      this.eventSource = null;
    }
  }

  on(eventType: string, callback: EventCallback): () => void {
    if (!this.listeners.has(eventType)) {
      this.listeners.set(eventType, new Set());
    }
    this.listeners.get(eventType)!.add(callback);

    if (this.eventSource) {
      this.eventSource.addEventListener(eventType, (e: MessageEvent) => {
        try {
          callback(JSON.parse(e.data));
        } catch { /* ignore malformed */ }
      });
    }

    return () => {
      this.listeners.get(eventType)?.delete(callback);
    };
  }

  async publish(topic: string, payload: unknown): Promise<string> {
    return this.client.publish(topic, payload);
  }
}
