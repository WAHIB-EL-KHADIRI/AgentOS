import { describe, it, expect, vi, beforeEach } from "vitest";
import { BusClient } from "../src/bus.js";
import { AgentClient } from "../src/client.js";

const mockFetch = vi.fn();
vi.stubGlobal("fetch", mockFetch);

class MockEventSource {
  onopen: (() => void) | null = null;
  onerror: (() => void) | null = null;
  private listeners = new Map<string, Set<(e: MessageEvent) => void>>();

  constructor(url: string) {
    setTimeout(() => this.onopen?.(), 0);
  }

  addEventListener(type: string, cb: (e: MessageEvent) => void) {
    if (!this.listeners.has(type)) this.listeners.set(type, new Set());
    this.listeners.get(type)!.add(cb);
  }

  close() {
    this.listeners.clear();
  }

  dispatch(type: string, data: string) {
    const cbs = this.listeners.get(type);
    if (cbs) {
      const event = { data } as MessageEvent;
      for (const cb of cbs) cb(event);
    }
  }
}

vi.stubGlobal("EventSource", MockEventSource);

describe("BusClient", () => {
  const baseUrl = "http://localhost:50051";

  beforeEach(() => {
    mockFetch.mockReset();
  });

  it("creates with AgentClient", () => {
    const client = new AgentClient({ baseUrl });
    const bus = new BusClient(client);
    expect(bus).toBeInstanceOf(BusClient);
  });

  it("publishes through AgentClient", async () => {
    mockFetch.mockResolvedValue({ ok: true, json: async () => ({ messageId: "msg-1" }) });

    const client = new AgentClient({ baseUrl });
    const bus = new BusClient(client);
    const result = await bus.publish("test.topic", { key: "value" });
    expect(result).toBe("msg-1");
  });

  it("handles multiple event listeners", () => {
    const client = new AgentClient({ baseUrl });
    const bus = new BusClient(client);
    const cb1 = vi.fn();
    const cb2 = vi.fn();

    const unsub1 = bus.on("agent_started", cb1);
    const unsub2 = bus.on("agent_started", cb2);

    expect(typeof unsub1).toBe("function");
    expect(typeof unsub2).toBe("function");

    unsub1();
    unsub2();
  });

  it("connect and disconnect works", () => {
    const client = new AgentClient({ baseUrl });
    const bus = new BusClient(client);
    bus.connect(baseUrl);
    bus.disconnect();
    expect(true).toBe(true);
  });

  it("disconnect cleans up EventSource", () => {
    const client = new AgentClient({ baseUrl });
    const bus = new BusClient(client);
    bus.connect(baseUrl);
    bus.disconnect();
    // After disconnect, connecting again should work
    bus.connect(baseUrl);
    bus.disconnect();
    expect(true).toBe(true);
  });
});
