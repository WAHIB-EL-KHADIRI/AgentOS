import type { AgentInfo } from "../types";
import AgentCard from "./AgentCard";

interface AgentListProps {
  agents: AgentInfo[];
  selectedId: string | null;
  onSelect: (agent: AgentInfo) => void;
}

export default function AgentList({ agents, selectedId, onSelect }: AgentListProps) {
  const running = agents.filter((a) => a.status === "running");
  const stopped = agents.filter((a) => a.status !== "running");

  return (
    <div className="agent-list">
      <div className="agent-list__header">
        <h2>Agents</h2>
        <span className="agent-list__count">{agents.length}</span>
      </div>

      {agents.length === 0 && (
        <div className="agent-list__empty">
          <p>No agents running</p>
          <p className="agent-list__hint">
            Start an agent with <code>agentOS run --agent examples/simple_agent.toml</code>
          </p>
        </div>
      )}

      {running.length > 0 && (
        <div className="agent-list__section">
          <h3 className="agent-list__section-title">Running ({running.length})</h3>
          <div className="agent-list__cards">
            {running.map((a) => (
              <AgentCard key={a.id} agent={a} selected={a.id === selectedId} onSelect={onSelect} />
            ))}
          </div>
        </div>
      )}

      {stopped.length > 0 && (
        <div className="agent-list__section">
          <h3 className="agent-list__section-title">Stopped ({stopped.length})</h3>
          <div className="agent-list__cards">
            {stopped.map((a) => (
              <AgentCard key={a.id} agent={a} selected={a.id === selectedId} onSelect={onSelect} />
            ))}
          </div>
        </div>
      )}
    </div>
  );
}
