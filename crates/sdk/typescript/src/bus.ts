import { EventSource as EventSourcePolyfill } from "eventsource";

import { AgentClient } from "./client.js";

/** The shape of an SSE event this client actually reads. */
interface SseMessage {
  data: string;
}

/** The surface of an SSE connection this client actually uses. */
interface SseConnection {
  addEventListener(type: string, listener: (event: SseMessage) => void): void;
  close(): void;
}

interface SseConstructor {
  new (url: string): SseConnection;
}

/**
 * Browsers, Deno and Bun expose `EventSource` as a global. Node does not --
 * it exists only behind `--experimental-eventsource` -- so calling the bare
 * global there threw `ReferenceError: EventSource is not defined` the moment
 * a consumer connected. Prefer the platform implementation where there is one
 * and fall back to the polyfill, so one build works in both places without
 * asking the caller to pass a flag or inject a constructor.
 *
 * Resolved per connect rather than once at module load: a global installed
 * after this module is evaluated -- a browser shim, or a test substituting a
 * fake -- must still win, and binding at import time would silently ignore it.
 *
 * The two implementations are not assignable to each other (the polyfill
 * carries private fields the DOM type does not), so both are narrowed to the
 * structural contract above, which is also the only part of either that this
 * client depends on.
 */
function resolveEventSource(): SseConstructor {
  const platform = (globalThis as { EventSource?: SseConstructor }).EventSource;
  return platform ?? (EventSourcePolyfill as unknown as SseConstructor);
}

type EventCallback = (event: unknown) => void;

export class BusClient {
  private client: AgentClient;
  private eventSource: SseConnection | null = null;
  private listeners = new Map<string, Set<EventCallback>>();

  constructor(client: AgentClient) {
    this.client = client;
  }

  connect(url: string): void {
    this.disconnect();
    this.eventSource = new (resolveEventSource())(url);

    for (const [type, cbs] of this.listeners) {
      this.eventSource.addEventListener(type, (e: SseMessage) => {
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
      this.eventSource.addEventListener(eventType, (e: SseMessage) => {
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
