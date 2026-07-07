export type AgentStatus = "running" | "stopped" | "error" | "starting";

export interface AgentInfo {
  id: string;
  name: string;
  status: AgentStatus;
  capabilities: string[];
  startedAt: string | null;
  traceCount: number;
}

export interface EventMessage {
  id: string;
  agentId: string;
  agentName: string;
  eventType: string;
  payload: string;
  timestamp: string;
}

export interface Tool {
  name: string;
  description: string;
  run(input: string): Promise<string>;
}

export type ToolResult = { ok: true; value: string } | { ok: false; error: string };
