import type { AgentInfo, AgentStatus } from "../types";

interface AgentCardProps {
  agent: AgentInfo;
  selected: boolean;
  onSelect: (agent: AgentInfo) => void;
}

const statusColors: Record<AgentStatus, string> = {
  running: "#22c55e",
  stopped: "#a3a3a3",
  error: "#ef4444",
  starting: "#f59e0b",
};

export default function AgentCard({ agent, selected, onSelect }: AgentCardProps) {
  return (
    <button
      type="button"
      className={`agent-card ${selected ? "agent-card--selected" : ""}`}
      onClick={() => onSelect(agent)}
      aria-pressed={selected}
    >
      <div className="agent-card__header">
        <span
          className="agent-card__dot"
          style={{ background: statusColors[agent.status] }}
          aria-hidden="true"
        />
        <span className="agent-card__name">{agent.name}</span>
        <span className={`agent-card__status agent-card__status--${agent.status}`}>
          {agent.status}
        </span>
      </div>
      <div className="agent-card__meta">
        <span>{agent.trace_count} traces</span>
        <span>{agent.capabilities.length} capabilities</span>
      </div>
    </button>
  );
}
