import { useRef, useEffect } from "react";
import type { EventEntry, EventType } from "../types";

interface EventStreamProps {
  events: EventEntry[];
  filterAgentId: string | null;
}

const eventColors: Record<EventType, string> = {
  thought: "#8b5cf6",
  tool_call: "#3b82f6",
  tool_result: "#22c55e",
  error: "#ef4444",
  state_change: "#f59e0b",
  message: "#06b6d4",
};

export default function EventStream({ events, filterAgentId }: EventStreamProps) {
  const listRef = useRef<HTMLUListElement>(null);

  useEffect(() => {
    if (listRef.current) {
      listRef.current.scrollTop = listRef.current.scrollHeight;
    }
  }, [events.length]);

  const filtered = filterAgentId ? events.filter((e) => e.agent_id === filterAgentId) : events;

  return (
    <div className="event-stream">
      <div className="event-stream__header">
        <h2>Event Stream</h2>
        <span className="event-stream__count">{filtered.length} events</span>
      </div>

      <ul className="event-stream__list" ref={listRef}>
        {filtered.length === 0 && (
          <li className="event-stream__empty">
            No events yet. Events will appear in real-time as agents run.
          </li>
        )}

        {[...filtered].reverse().map((event) => (
          <li key={event.id} className="event-stream__item">
            <span
              className="event-stream__dot"
              style={{ background: eventColors[event.event_type] }}
              aria-hidden="true"
            />
            <div className="event-stream__content">
              <div className="event-stream__head">
                <span className="event-stream__agent">{event.agent_name}</span>
                <span className="event-stream__type">{event.event_type}</span>
                <span className="event-stream__time">
                  {new Date(event.timestamp).toLocaleTimeString()}
                </span>
              </div>
              <div className="event-stream__payload">{event.payload}</div>
            </div>
          </li>
        ))}
      </ul>
    </div>
  );
}
