import type { AgentInfo, AgentStoppedPayload, EventEntry, SseEvent, TraceStep } from "../types";

export type SseCallback = (event: SseEvent) => void;

export interface SseConnection {
  subscribe: (cb: SseCallback) => () => void;
  connect: () => void;
  disconnect: () => void;
}

export function createSseConnection(url: string): SseConnection {
  const listeners = new Set<SseCallback>();
  let eventSource: EventSource | null = null;
  let reconnectTimer: ReturnType<typeof setTimeout> | null = null;
  let reconnectAttempts = 0;
  const BASE_RECONNECT_DELAY_MS = 1000;
  const MAX_RECONNECT_DELAY_MS = 30000;

  function notify(event: SseEvent) {
    for (const cb of listeners) cb(event);
  }

  function parseSseData<T>(event: MessageEvent): T | null {
    try {
      return JSON.parse(event.data) as T;
    } catch (err) {
      console.error("Failed to parse SSE payload", err, event.data);
      return null;
    }
  }

  function connect() {
    disconnect();
    notify({ type: "connection_state", data: "connecting" });
    eventSource = new EventSource(url);

    eventSource.onopen = () => {
      reconnectAttempts = 0;
      notify({ type: "connection_state", data: "live" });
    };

    eventSource.addEventListener("agent_started", (e: MessageEvent) => {
      const data = parseSseData<AgentInfo>(e);
      if (data) notify({ type: "agent_started", data });
    });

    eventSource.addEventListener("agent_stopped", (e: MessageEvent) => {
      const data = parseSseData<AgentStoppedPayload>(e);
      if (data) notify({ type: "agent_stopped", data });
    });

    eventSource.addEventListener("agent_status", (e: MessageEvent) => {
      const data = parseSseData<AgentInfo>(e);
      if (data) notify({ type: "agent_status", data });
    });

    eventSource.addEventListener("agent_event", (e: MessageEvent) => {
      const data = parseSseData<EventEntry>(e);
      if (data) notify({ type: "agent_event", data });
    });

    eventSource.addEventListener("trace_update", (e: MessageEvent) => {
      const data = parseSseData<TraceStep>(e);
      if (data) notify({ type: "trace_update", data });
    });

    eventSource.onerror = () => {
      notify({ type: "connection_state", data: "reconnecting" });
      notify({ type: "error", data: "SSE connection lost. Reconnecting..." });
      scheduleReconnect();
    };
  }

  function scheduleReconnect() {
    if (reconnectTimer) clearTimeout(reconnectTimer);
    const delay = Math.min(
      BASE_RECONNECT_DELAY_MS * 2 ** reconnectAttempts,
      MAX_RECONNECT_DELAY_MS
    );
    reconnectAttempts += 1;
    reconnectTimer = setTimeout(() => connect(), delay);
  }

  function disconnect() {
    if (reconnectTimer) {
      clearTimeout(reconnectTimer);
      reconnectTimer = null;
    }
    if (eventSource) {
      eventSource.close();
      eventSource = null;
    }
    notify({ type: "connection_state", data: "disconnected" });
  }

  function subscribe(cb: SseCallback): () => void {
    listeners.add(cb);
    return () => {
      listeners.delete(cb);
      if (listeners.size === 0) disconnect();
    };
  }

  return { subscribe, connect, disconnect };
}
