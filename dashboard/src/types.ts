export type AgentStatus = "running" | "stopped" | "error" | "starting";
export type ConnectionState = "connecting" | "live" | "reconnecting" | "disconnected";
export type EventType =
  "thought" | "tool_call" | "tool_result" | "error" | "state_change" | "message";
export type TraceStatus = "complete" | "active" | "pending" | "failed";

export interface AgentInfo {
  id: string;
  name: string;
  status: AgentStatus;
  capabilities: string[];
  started_at: string | null;
  trace_count: number;
}

export interface AgentStoppedPayload {
  agent_id: string;
}

export interface EventEntry {
  id: string;
  agent_id: string;
  agent_name: string;
  event_type: EventType;
  payload: string;
  timestamp: string;
}

export interface TraceStep {
  checkpoint_id: string;
  label: string;
  agent_id: string;
  status: TraceStatus;
  timestamp: string;
}

// ── Recorded sessions (journals) ────────────────────────────────────
// Mirrors crates/kernel/src/journal.rs. Served by /api/v1/journals.

export interface RecordedToolCall {
  id: string;
  name: string;
  arguments: unknown;
}

export interface RecordedResponse {
  model: string;
  content: string;
  tool_calls: RecordedToolCall[];
  finish_reason: string;
}

export interface RecordedExchange {
  request_fingerprint: string;
  checkpoint_id: string;
  response: RecordedResponse;
}

export interface RecordedToolInvocation {
  name: string;
  arguments: unknown;
  success: boolean;
  output: string;
}

export interface RecordedSession {
  agent_id: string;
  agent_name: string;
  prompt: string;
  capabilities: string[];
  model: string;
  user_input: string;
  exchanges: RecordedExchange[];
  tool_invocations: RecordedToolInvocation[];
  recorded_at_ms: number;
}

export type SseEvent =
  | { type: "agent_started"; data: AgentInfo }
  | { type: "agent_stopped"; data: AgentStoppedPayload }
  | { type: "agent_status"; data: AgentInfo }
  | { type: "agent_event"; data: EventEntry }
  | { type: "trace_update"; data: TraceStep }
  | { type: "agents_list"; data: AgentInfo[] }
  | { type: "connection_state"; data: ConnectionState }
  | { type: "error"; data: string };
