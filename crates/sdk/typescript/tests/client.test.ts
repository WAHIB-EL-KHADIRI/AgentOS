import { describe, it, expect, vi, beforeEach } from "vitest";
import { AgentClient } from "../src/client.js";

const mockFetch = vi.fn();
vi.stubGlobal("fetch", mockFetch);

describe("AgentClient", () => {
  const baseUrl = "http://localhost:50051";

  beforeEach(() => {
    mockFetch.mockReset();
  });

  it("creates with default timeout", () => {
    const client = new AgentClient({ baseUrl });
    expect(client).toBeInstanceOf(AgentClient);
  });

  it("creates with custom timeout", () => {
    const client = new AgentClient({ baseUrl, timeout: 10_000 });
    expect(client).toBeInstanceOf(AgentClient);
  });

  it("strips trailing slash from baseUrl", () => {
    const client = new AgentClient({ baseUrl: "http://localhost:50051/" });
    expect(client).toBeInstanceOf(AgentClient);
  });

  it("listAgents returns parsed JSON", async () => {
    const agents = [{ id: "a1", name: "Agent 1", status: "running", capabilities: [], startedAt: null, traceCount: 0 }];
    mockFetch.mockResolvedValue({ ok: true, json: async () => agents });

    const client = new AgentClient({ baseUrl });
    const result = await client.listAgents();
    expect(result).toEqual(agents);
    expect(mockFetch).toHaveBeenCalledWith(`${baseUrl}/api/agents`, expect.any(Object));
  });

  it("publish returns messageId string", async () => {
    mockFetch.mockResolvedValue({ ok: true, json: async () => ({ messageId: "msg-1" }) });

    const client = new AgentClient({ baseUrl });
    const result = await client.publish("test.topic", { hello: "world" });
    expect(result).toBe("msg-1");
    expect(mockFetch).toHaveBeenCalledWith(
      `${baseUrl}/api/publish`,
      expect.objectContaining({ method: "POST" }),
    );
  });

  it("throws on non-ok response", async () => {
    mockFetch.mockResolvedValue({ ok: false, status: 500, statusText: "Internal Server Error" });

    const client = new AgentClient({ baseUrl });
    await expect(client.listAgents()).rejects.toThrow("HTTP 500");
  });
});
