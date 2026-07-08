import { useCallback, useEffect, useState } from "react";
import type { RecordedSession } from "../types";
import { fetchJournalIds, fetchSession } from "../api/journals";
import TimeTravelView from "./TimeTravelView";

type LoadState = "loading" | "ready" | "error";

export default function SessionExplorer() {
  const [ids, setIds] = useState<string[]>([]);
  const [listState, setListState] = useState<LoadState>("loading");
  const [listError, setListError] = useState<string | null>(null);
  const [selectedId, setSelectedId] = useState<string | null>(null);
  const [session, setSession] = useState<RecordedSession | null>(null);
  const [sessionState, setSessionState] = useState<LoadState>("ready");
  const [sessionError, setSessionError] = useState<string | null>(null);

  const loadList = useCallback(async () => {
    setListState("loading");
    setListError(null);
    try {
      const journals = await fetchJournalIds();
      setIds(journals);
      setListState("ready");
    } catch (err) {
      setListError(err instanceof Error ? err.message : String(err));
      setListState("error");
    }
  }, []);

  useEffect(() => {
    void loadList();
  }, [loadList]);

  const selectSession = useCallback(async (id: string) => {
    setSelectedId(id);
    setSession(null);
    setSessionState("loading");
    setSessionError(null);
    try {
      setSession(await fetchSession(id));
      setSessionState("ready");
    } catch (err) {
      setSessionError(err instanceof Error ? err.message : String(err));
      setSessionState("error");
    }
  }, []);

  return (
    <div className="sessions">
      <aside className="sessions__list">
        <div className="sessions__list-head">
          <h3>Recorded sessions</h3>
          <button type="button" onClick={() => void loadList()} aria-label="Refresh sessions">
            ⟳
          </button>
        </div>

        {listState === "loading" && <div className="sessions__hint">Loading sessions…</div>}

        {listState === "error" && (
          <div className="sessions__hint sessions__hint--error" role="alert">
            <p>Could not load sessions: {listError}</p>
            <button type="button" onClick={() => void loadList()}>
              Retry
            </button>
          </div>
        )}

        {listState === "ready" && ids.length === 0 && (
          <div className="sessions__hint">
            <p>No recorded sessions yet.</p>
            <p>
              Run an agent with an LLM provider configured — every execution
              step is journaled automatically and shows up here.
            </p>
          </div>
        )}

        {listState === "ready" &&
          ids.map((id) => (
            <button
              key={id}
              type="button"
              className={`sessions__item ${id === selectedId ? "sessions__item--selected" : ""}`}
              onClick={() => void selectSession(id)}
            >
              {id}
            </button>
          ))}
      </aside>

      <main className="sessions__detail">
        {!selectedId && (
          <div className="sessions__placeholder">
            <p>Select a recorded session to travel through it.</p>
            <p className="sessions__placeholder-sub">
              Recordings replay deterministically — no API key, no network.
            </p>
          </div>
        )}

        {selectedId && sessionState === "loading" && (
          <div className="sessions__placeholder">Loading {selectedId}…</div>
        )}

        {selectedId && sessionState === "error" && (
          <div className="sessions__placeholder sessions__hint--error" role="alert">
            <p>Could not load session: {sessionError}</p>
            <button type="button" onClick={() => void selectSession(selectedId)}>
              Retry
            </button>
          </div>
        )}

        {session && sessionState === "ready" && <TimeTravelView session={session} />}
      </main>
    </div>
  );
}
