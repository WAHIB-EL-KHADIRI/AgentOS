import type { AgentInfo, TraceStep } from "../types";
import TraceTimeline from "./TraceTimeline";

interface AgentDetailProps {
  agent: AgentInfo | null;
  trace: TraceStep[];
}

export default function AgentDetail({ agent, trace }: AgentDetailProps) {
  if (!agent) {
    return (
      <div className="agent-detail agent-detail--empty">
        <div className="agent-detail__placeholder">
          <svg
            width="64"
            height="64"
            viewBox="0 0 24 24"
            fill="none"
            stroke="#666"
            strokeWidth="1.5"
          >
            <path d="M20 21v-2a4 4 0 0 0-4-4H8a4 4 0 0 0-4 4v2" />
            <circle cx="12" cy="7" r="4" />
          </svg>
          <p>Select an agent to view details</p>
        </div>
      </div>
    );
  }

  return (
    <div className="agent-detail">
      <div className="agent-detail__header">
        <div>
          <h2>{agent.name}</h2>
          <span className="agent-detail__id">{agent.id}</span>
        </div>
        <span className={`agent-detail__badge agent-detail__badge--${agent.status}`}>
          {agent.status}
        </span>
      </div>

      <div className="agent-detail__stats">
        <div className="agent-detail__stat">
          <span className="agent-detail__stat-value">{agent.trace_count}</span>
          <span className="agent-detail__stat-label">Traces</span>
        </div>
        <div className="agent-detail__stat">
          <span className="agent-detail__stat-value">{agent.capabilities.length}</span>
          <span className="agent-detail__stat-label">Capabilities</span>
        </div>
        <div className="agent-detail__stat">
          <span className="agent-detail__stat-value">
            {agent.started_at ? new Date(agent.started_at).toLocaleDateString() : "-"}
          </span>
          <span className="agent-detail__stat-label">Started</span>
        </div>
      </div>

      {agent.capabilities.length > 0 && (
        <div className="agent-detail__capabilities">
          <h3>Capabilities</h3>
          <div className="agent-detail__tags">
            {agent.capabilities.map((cap) => (
              <span key={cap} className="agent-detail__tag">
                {cap}
              </span>
            ))}
          </div>
        </div>
      )}

      <TraceTimeline trace={trace} />
    </div>
  );
}
