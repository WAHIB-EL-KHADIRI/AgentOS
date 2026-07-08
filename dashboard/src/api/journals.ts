import type { RecordedSession } from "../types";

async function getJson<T>(path: string): Promise<T> {
  const response = await fetch(path, {
    headers: { Accept: "application/json" },
  });
  if (!response.ok) {
    throw new Error(`HTTP ${response.status}: ${response.statusText}`);
  }
  return (await response.json()) as T;
}

/** Recorded session ids available for replay (journal filenames). */
export function fetchJournalIds(): Promise<string[]> {
  return getJson<string[]>("/api/v1/journals");
}

/** One recorded execution session, as journaled by the runtime. */
export function fetchSession(agentId: string): Promise<RecordedSession> {
  return getJson<RecordedSession>(`/api/v1/journals/${encodeURIComponent(agentId)}`);
}
