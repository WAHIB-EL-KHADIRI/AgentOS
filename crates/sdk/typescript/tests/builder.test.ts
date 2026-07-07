import { describe, it, expect, vi, beforeEach } from "vitest";
import { AgentBuilder } from "../src/builder.js";
import { AgentClient } from "../src/client.js";

const mockFetch = vi.fn();
vi.stubGlobal("fetch", mockFetch);

describe("AgentBuilder", () => {
  const baseUrl = "http://localhost:50051";

  beforeEach(() => {
    mockFetch.mockReset();
  });

  it("creates builder with name", () => {
    const builder = new AgentBuilder("test-agent");
    expect(builder).toBeInstanceOf(AgentBuilder);
  });

  it("adds single capability", () => {
    const builder = new AgentBuilder("test-agent").withCapability("search");
    expect(builder).toBeInstanceOf(AgentBuilder);
  });

  it("adds multiple capabilities", () => {
    const builder = new AgentBuilder("test-agent").withCapabilities([
      "search",
      "memory",
    ]);
    expect(builder).toBeInstanceOf(AgentBuilder);
  });

  it("chains methods", () => {
    const builder = new AgentBuilder("test-agent")
      .withCapability("search")
      .withCapability("memory");
    expect(builder).toBeInstanceOf(AgentBuilder);
  });

  it("spawn returns agent info", async () => {
    const agent = { id: "a1", name: "test-agent", status: "running", capabilities: [], startedAt: null, traceCount: 0 };
    mockFetch.mockResolvedValue({ ok: true, json: async () => agent });

    const builder = new AgentBuilder("test-agent");
    const client = new AgentClient({ baseUrl });
    const result = await builder.spawn(client);
    expect(result).toHaveProperty("id", "a1");
    expect(result).toHaveProperty("name", "test-agent");
    expect(mockFetch).toHaveBeenCalledWith(
      `${baseUrl}/api/agents`,
      expect.objectContaining({ method: "POST" }),
    );
  });
});
