import type { TraceStatus, TraceStep } from "../types";

interface TraceTimelineProps {
  trace: TraceStep[];
}

const statusColors: Record<TraceStatus, string> = {
  complete: "#22c55e",
  active: "#3b82f6",
  pending: "#a3a3a3",
  failed: "#ef4444",
};

export default function TraceTimeline({ trace }: TraceTimelineProps) {
  return (
    <div className="trace-timeline">
      <h3>Trace Timeline</h3>

      {trace.length === 0 && (
        <div className="trace-timeline__empty">No trace data available for this agent.</div>
      )}

      <div className="trace-timeline__list">
        {trace.map((step, i) => (
          <div
            key={step.checkpoint_id}
            className={`trace-timeline__step ${
              step.status === "active" ? "trace-timeline__step--active" : ""
            }`}
          >
            <div className="trace-timeline__connector">
              <span
                className="trace-timeline__dot"
                style={{ background: statusColors[step.status] }}
              />
              {i < trace.length - 1 && (
                <div
                  className="trace-timeline__line"
                  style={{
                    background: step.status === "complete" ? statusColors.complete : "#333",
                  }}
                />
              )}
            </div>
            <div className="trace-timeline__content">
              <div className="trace-timeline__label">{step.label}</div>
              <div className="trace-timeline__meta">
                <span className={`trace-timeline__status trace-timeline__status--${step.status}`}>
                  {step.status}
                </span>
                {step.timestamp && (
                  <span className="trace-timeline__timestamp">
                    {new Date(step.timestamp).toLocaleTimeString()}
                  </span>
                )}
              </div>
            </div>
          </div>
        ))}
      </div>
    </div>
  );
}
