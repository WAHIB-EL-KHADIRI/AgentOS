import { describe, it, expect } from "vitest";
import type { AgentInfo, AgentStatus, EventMessage, Tool, ToolResult } from "../src/types.js";

describe("types", () => {
  it("AgentInfo satisfies interface", () => {
    const info: AgentInfo = {
      id: "agent-1",
      name: "TestAgent",
      status: "running",
      capabilities: ["search"],
      startedAt: "2026-01-01T00:00:00Z",
      traceCount: 5,
    };
    expect(info.id).toBe("agent-1");
    expect(info.status).toBe("running");
  });

  it("AgentStatus accepts all values", () => {
    const statuses: AgentStatus[] = ["running", "stopped", "error", "starting"];
    expect(statuses).toHaveLength(4);
  });

  it("EventMessage satisfies interface", () => {
    const event: EventMessage = {
      id: "evt-1",
      agentId: "agent-1",
      agentName: "TestAgent",
      eventType: "thought",
      payload: '{"key":"value"}',
      timestamp: "2026-01-01T00:00:00Z",
    };
    expect(event.eventType).toBe("thought");
  });

  it("Tool has run function", async () => {
    const tool: Tool = {
      name: "echo",
      description: "Echoes input",
      async run(input: string) {
        return input;
      },
    };
    const result = await tool.run("hello");
    expect(result).toBe("hello");
  });

  it("ToolResult ok variant", () => {
    const result: ToolResult = { ok: true, value: "success" };
    expect(result.ok).toBe(true);
    if (result.ok) {
      expect(result.value).toBe("success");
    }
  });

  it("ToolResult error variant", () => {
    const result: ToolResult = { ok: false, error: "something went wrong" };
    expect(result.ok).toBe(false);
    if (!result.ok) {
      expect(result.error).toBe("something went wrong");
    }
  });
});
