import { useState, useEffect, useCallback, useRef } from "react";
import type { AgentInfo, ConnectionState, EventEntry, SseEvent, TraceStep } from "./types";
import { createSseConnection } from "./api/sse";
import AgentList from "./components/AgentList";
import AgentDetail from "./components/AgentDetail";
import EventStream from "./components/EventStream";

const SSE_URL = "/api/events";
const connectionLabels: Record<ConnectionState, string> = {
  connecting: "Connecting",
  live: "Live",
  reconnecting: "Reconnecting",
  disconnected: "Disconnected",
};

export default function App() {
  const [agents, setAgents] = useState<AgentInfo[]>([]);
  const [events, setEvents] = useState<EventEntry[]>([]);
  const [trace, setTrace] = useState<TraceStep[]>([]);
  const [selectedAgent, setSelectedAgent] = useState<AgentInfo | null>(null);
  const [connectionState, setConnectionState] = useState<ConnectionState>("connecting");
  const [error, setError] = useState<string | null>(null);
  const connRef = useRef<ReturnType<typeof createSseConnection> | null>(null);
  const maxEvents = 500;

  useEffect(() => {
    const conn = createSseConnection(SSE_URL);
    connRef.current = conn;

    const unsub = conn.subscribe((event: SseEvent) => {
      switch (event.type) {
        case "agent_started":
        case "agent_status":
          setAgents((prev) => {
            const existing = prev.findIndex((a) => a.id === event.data.id);
            if (existing >= 0) {
              const next = [...prev];
              next[existing] = event.data as AgentInfo;
              return next;
            }
            return [...prev, event.data as AgentInfo];
          });
          break;

        case "agent_stopped":
          setAgents((prev) =>
            prev.map((a) =>
              a.id === event.data.agent_id ? { ...a, status: "stopped" as const } : a
            )
          );
          break;

        case "agent_event":
          setEvents((prev) => {
            const next = [event.data as EventEntry, ...prev];
            return next.length > maxEvents ? next.slice(0, maxEvents) : next;
          });
          break;

        case "trace_update":
          setTrace((prev) => [event.data as TraceStep, ...prev]);
          break;

        case "agents_list":
          setAgents(event.data as AgentInfo[]);
          break;

        case "connection_state":
          setConnectionState(event.data);
          if (event.data === "live") {
            setError(null);
          }
          break;

        case "error":
          setError(event.data);
          setConnectionState("reconnecting");
          break;
      }
    });

    conn.connect();

    return () => {
      unsub();
    };
  }, []);

  const handleSelectAgent = useCallback((agent: AgentInfo) => {
    setSelectedAgent(agent);
    setTrace([]);
  }, []);

  return (
    <div className="app">
      <header className="app__header">
        <div className="app__brand">
          <svg
            className="app__logo"
            width="28"
            height="28"
            viewBox="0 0 24 24"
            fill="none"
            stroke="currentColor"
            strokeWidth="2"
            role="img"
            aria-label="AgentOS logo"
          >
            <circle cx="12" cy="12" r="10" />
            <path d="M12 6v6l4 2" />
          </svg>
          <h1>AgentOS</h1>
        </div>

        <div className="app__header-actions">
          <span
            className={`app__connection app__connection--${connectionState}`}
            role="status"
            aria-live="polite"
          >
            <span className="app__connection-dot" aria-hidden="true" />
            {connectionLabels[connectionState]}
          </span>
        </div>
      </header>

      {error && (
        <div className="app__error" role="alert">
          <span>{error}</span>
          <button type="button" onClick={() => setError(null)} aria-label="Dismiss error">
            Dismiss
          </button>
        </div>
      )}

      <div className="app__body">
        <aside className="app__sidebar">
          <AgentList
            agents={agents}
            selectedId={selectedAgent?.id ?? null}
            onSelect={handleSelectAgent}
          />
        </aside>

        <main className="app__main">
          <AgentDetail agent={selectedAgent} trace={trace} />
        </main>

        <aside className="app__events">
          <EventStream events={events} filterAgentId={selectedAgent?.id ?? null} />
        </aside>
      </div>
    </div>
  );
}
