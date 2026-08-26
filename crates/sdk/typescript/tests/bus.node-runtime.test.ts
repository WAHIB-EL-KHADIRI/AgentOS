import http from "node:http";
import type { AddressInfo } from "node:net";

import { describe, it, expect, afterEach } from "vitest";

import { BusClient } from "../src/bus.js";
import { AgentClient } from "../src/client.js";

/**
 * This file deliberately does NOT stub `EventSource`.
 *
 * `bus.test.ts` installs `vi.stubGlobal("EventSource", MockEventSource)`, which
 * is useful for exercising listener bookkeeping but means the suite says
 * nothing about whether a real connection can be opened. It stayed green while
 * `connect()` threw `ReferenceError: EventSource is not defined` for every Node
 * consumer, because the stub supplied the global Node does not have.
 *
 * So these tests run against a real `http` server speaking real SSE, on the
 * real runtime. If the resolution of `EventSource` regresses, this fails.
 */

let server: http.Server | null = null;

function startSseServer(
  onRequest: (res: http.ServerResponse) => void,
): Promise<string> {
  server = http.createServer((_req, res) => {
    res.writeHead(200, {
      "Content-Type": "text/event-stream",
      "Cache-Control": "no-cache",
      Connection: "keep-alive",
    });
    onRequest(res);
  });

  return new Promise((resolve) => {
    server!.listen(0, "127.0.0.1", () => {
      const { port } = server!.address() as AddressInfo;
      resolve(`http://127.0.0.1:${port}/events`);
    });
  });
}

afterEach(async () => {
  if (server) {
    await new Promise<void>((resolve) => server!.close(() => resolve()));
    server = null;
  }
});

describe("BusClient against a real SSE server", () => {
  it("resolves an EventSource implementation on this runtime", () => {
    // The regression in its smallest form: on Node there is no global
    // EventSource, so the bare `new EventSource(url)` this client used to call
    // threw before a single byte was read.
    expect(() => new BusClient(new AgentClient({ baseUrl: "http://127.0.0.1:1" }))).not.toThrow();
  });

  it("connects and delivers a real server-sent event", async () => {
    const url = await startSseServer((res) => {
      setTimeout(() => {
        res.write('event: agent.started\ndata: {"agentId":"a1"}\n\n');
      }, 20);
    });

    const bus = new BusClient(new AgentClient({ baseUrl: url }));

    const received = new Promise<unknown>((resolve, reject) => {
      const timer = setTimeout(
        () => reject(new Error("no event arrived within 5s")),
        5000,
      );
      bus.on("agent.started", (event) => {
        clearTimeout(timer);
        resolve(event);
      });
    });

    // Throws ReferenceError on Node without the fix -- before any I/O happens.
    bus.connect(url);

    try {
      expect(await received).toEqual({ agentId: "a1" });
    } finally {
      bus.disconnect();
    }
  });

  it("delivers to a listener registered after connect", async () => {
    // `connect()` wires the listeners it knows about; `on()` wires late ones
    // itself. Both paths touch the connection object, so both need a real one.
    const url = await startSseServer((res) => {
      setTimeout(() => {
        res.write('event: agent.stopped\ndata: {"agentId":"a2"}\n\n');
      }, 40);
    });

    const bus = new BusClient(new AgentClient({ baseUrl: url }));
    bus.connect(url);

    const received = new Promise<unknown>((resolve, reject) => {
      const timer = setTimeout(
        () => reject(new Error("no event arrived within 5s")),
        5000,
      );
      bus.on("agent.stopped", (event) => {
        clearTimeout(timer);
        resolve(event);
      });
    });

    try {
      expect(await received).toEqual({ agentId: "a2" });
    } finally {
      bus.disconnect();
    }
  });

  it("disconnect closes the connection without throwing", async () => {
    const url = await startSseServer(() => {
      /* hold the stream open */
    });

    const bus = new BusClient(new AgentClient({ baseUrl: url }));
    bus.connect(url);

    expect(() => bus.disconnect()).not.toThrow();
    // Idempotent: a second disconnect must not reach into a closed handle.
    expect(() => bus.disconnect()).not.toThrow();
  });
});
