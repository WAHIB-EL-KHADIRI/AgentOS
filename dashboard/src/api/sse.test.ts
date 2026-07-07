import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { createSseConnection } from "./sse";
import type { SseEvent } from "../types";

class MockEventSource {
  static instances: MockEventSource[] = [];
  url: string;
  onopen: (() => void) | null = null;
  onerror: (() => void) | null = null;
  listeners = new Map<string, (event: MessageEvent) => void>();
  closed = false;

  constructor(url: string) {
    this.url = url;
    MockEventSource.instances.push(this);
  }

  addEventListener(type: string, cb: (event: MessageEvent) => void) {
    this.listeners.set(type, cb);
  }

  close() {
    this.closed = true;
  }

  emit(type: string, data: unknown) {
    this.listeners.get(type)?.({ data: JSON.stringify(data) } as MessageEvent);
  }

  emitRaw(type: string, data: string) {
    this.listeners.get(type)?.({ data } as MessageEvent);
  }
}

function instanceAt(index: number): MockEventSource {
  const instance = MockEventSource.instances[index];
  if (!instance) throw new Error(`expected MockEventSource at index ${index}`);
  return instance;
}

describe("createSseConnection", () => {
  beforeEach(() => {
    MockEventSource.instances = [];
    vi.stubGlobal("EventSource", MockEventSource);
    vi.useFakeTimers();
  });

  afterEach(() => {
    vi.useRealTimers();
    vi.unstubAllGlobals();
  });

  it("notifies subscribers when a known event parses successfully", () => {
    const events: SseEvent[] = [];
    const conn = createSseConnection("/api/events");
    conn.subscribe((e) => events.push(e));
    conn.connect();

    const source = instanceAt(0);
    source.emit("agent_started", { id: "a1", name: "Agent", status: "running" });

    expect(events.some((e) => e.type === "agent_started")).toBe(true);
  });

  it("logs and drops malformed payloads instead of crashing", () => {
    const errorSpy = vi.spyOn(console, "error").mockImplementation(() => {});
    const events: SseEvent[] = [];
    const conn = createSseConnection("/api/events");
    conn.subscribe((e) => events.push(e));
    conn.connect();

    const source = instanceAt(0);
    source.emitRaw("agent_started", "not json");

    expect(events.some((e) => e.type === "agent_started")).toBe(false);
    expect(errorSpy).toHaveBeenCalled();
    errorSpy.mockRestore();
  });

  it("reconnects with exponential backoff after repeated errors", () => {
    const conn = createSseConnection("/api/events");
    conn.connect();

    const first = instanceAt(0);
    first.onerror?.();
    expect(MockEventSource.instances).toHaveLength(1);

    vi.advanceTimersByTime(1000);
    expect(MockEventSource.instances).toHaveLength(2);

    const second = instanceAt(1);
    second.onerror?.();
    vi.advanceTimersByTime(1999);
    expect(MockEventSource.instances).toHaveLength(2);
    vi.advanceTimersByTime(1);
    expect(MockEventSource.instances).toHaveLength(3);
  });

  it("resets the backoff once the connection opens successfully", () => {
    const conn = createSseConnection("/api/events");
    conn.connect();

    const first = instanceAt(0);
    first.onerror?.();
    vi.advanceTimersByTime(1000);

    const second = instanceAt(1);
    second.onopen?.();
    second.onerror?.();

    vi.advanceTimersByTime(999);
    expect(MockEventSource.instances).toHaveLength(2);
    vi.advanceTimersByTime(1);
    expect(MockEventSource.instances).toHaveLength(3);
  });
});
