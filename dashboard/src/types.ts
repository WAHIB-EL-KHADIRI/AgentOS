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

export type SseEvent =
  | { type: "agent_started"; data: AgentInfo }
  | { type: "agent_stopped"; data: AgentStoppedPayload }
  | { type: "agent_status"; data: AgentInfo }
  | { type: "agent_event"; data: EventEntry }
  | { type: "trace_update"; data: TraceStep }
  | { type: "agents_list"; data: AgentInfo[] }
  | { type: "connection_state"; data: ConnectionState }
  | { type: "error"; data: string };
