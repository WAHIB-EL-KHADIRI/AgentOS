import { useEffect, useMemo, useRef, useState } from "react";
import type { RecordedSession, RecordedToolInvocation } from "../types";

interface TimeTravelViewProps {
  session: RecordedSession;
}

interface TimeStep {
  kind: "prompt" | "exchange";
  title: string;
  content: string;
  finishReason?: string;
  toolCalls?: { name: string; args: string }[];
  toolResults?: RecordedToolInvocation[];
  checkpointId?: string;
}

/** Flatten a recorded session into linear, scrubbable time steps. */
export function buildTimeSteps(session: RecordedSession): TimeStep[] {
  const steps: TimeStep[] = [
    {
      kind: "prompt",
      title: "Prompt",
      content: session.user_input,
    },
  ];

  // Tool invocations are recorded in execution order: each exchange with
  // N tool calls consumed the next N invocations.
  let toolCursor = 0;
  for (const [index, exchange] of session.exchanges.entries()) {
    const calls = exchange.response.tool_calls ?? [];
    const results = session.tool_invocations.slice(toolCursor, toolCursor + calls.length);
    toolCursor += calls.length;

    steps.push({
      kind: "exchange",
      title:
        calls.length > 0
          ? `Exchange ${index + 1} — tool round`
          : `Exchange ${index + 1} — response`,
      content: exchange.response.content,
      finishReason: exchange.response.finish_reason,
      toolCalls: calls.map((call) => ({
        name: call.name,
        args: JSON.stringify(call.arguments),
      })),
      toolResults: results,
      checkpointId: exchange.checkpoint_id,
    });
  }
  return steps;
}

export default function TimeTravelView({ session }: TimeTravelViewProps) {
  const steps = useMemo(() => buildTimeSteps(session), [session]);
  const [cursor, setCursor] = useState(steps.length - 1);
  const [renderedSession, setRenderedSession] = useState(session.agent_id);
  const currentRef = useRef<HTMLDivElement | null>(null);

  // A new session selection resets the cursor to the end of the recording.
  // Render-phase reset (not an effect): the sanctioned derived-state pattern.
  if (renderedSession !== session.agent_id) {
    setRenderedSession(session.agent_id);
    setCursor(steps.length - 1);
  }

  useEffect(() => {
    // Guarded: not implemented in all environments (e.g. jsdom).
    if (typeof currentRef.current?.scrollIntoView === "function") {
      currentRef.current.scrollIntoView({ block: "nearest", behavior: "smooth" });
    }
  }, [cursor]);

  const stepBack = () => setCursor((value) => Math.max(0, value - 1));
  const stepForward = () => setCursor((value) => Math.min(steps.length - 1, value + 1));

  // Arrow-key navigation comes from the native range input, which already
  // maps ArrowLeft/ArrowRight to value changes when focused.
  return (
    <section className="timetravel" aria-label="Time travel view">
      <div className="timetravel__header">
        <div>
          <h2>{session.agent_name}</h2>
          <span className="timetravel__meta">
            {session.agent_id}
            {session.model ? ` · ${session.model}` : ""}
            {" · "}
            {new Date(session.recorded_at_ms).toLocaleString()}
          </span>
        </div>
        <code className="timetravel__cli">agentOS replay --session {session.agent_id}</code>
      </div>

      <div className="timetravel__scrubber" role="group" aria-label="Time travel controls">
        <button type="button" onClick={stepBack} disabled={cursor === 0} aria-label="Previous step">
          ◀
        </button>
        <input
          type="range"
          min={0}
          max={steps.length - 1}
          value={cursor}
          onChange={(event) => setCursor(Number(event.target.value))}
          aria-label="Scrub through recorded steps"
        />
        <button
          type="button"
          onClick={stepForward}
          disabled={cursor === steps.length - 1}
          aria-label="Next step"
        >
          ▶
        </button>
        <span className="timetravel__position">
          step {cursor + 1} / {steps.length}
        </span>
      </div>

      <div className="timetravel__transcript">
        {steps.slice(0, cursor + 1).map((step, index) => {
          const isCurrent = index === cursor;
          return (
            <div
              key={index}
              ref={isCurrent ? currentRef : undefined}
              className={`timetravel__step timetravel__step--${step.kind} ${
                isCurrent ? "timetravel__step--current" : ""
              }`}
            >
              <div className="timetravel__step-head">
                <span className="timetravel__step-title">{step.title}</span>
                {step.finishReason && (
                  <span className="timetravel__finish">{step.finishReason}</span>
                )}
              </div>

              {step.content && <p className="timetravel__content">{step.content}</p>}

              {step.toolCalls && step.toolCalls.length > 0 && (
                <div className="timetravel__tools">
                  {step.toolCalls.map((call, callIndex) => {
                    const result = step.toolResults?.[callIndex];
                    return (
                      <div key={callIndex} className="timetravel__tool">
                        <span className="timetravel__tool-call">
                          🔧 {call.name}({call.args})
                        </span>
                        {result && (
                          <span
                            className={`timetravel__tool-result ${
                              result.success
                                ? "timetravel__tool-result--ok"
                                : "timetravel__tool-result--err"
                            }`}
                          >
                            {result.success ? "→" : "✗"} {result.output}
                          </span>
                        )}
                      </div>
                    );
                  })}
                </div>
              )}

              {step.checkpointId && (
                <span className="timetravel__checkpoint" title="Exchange checkpoint (fork anchor)">
                  {step.checkpointId}
                </span>
              )}
            </div>
          );
        })}
      </div>
    </section>
  );
}
