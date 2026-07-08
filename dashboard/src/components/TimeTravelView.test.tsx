import { describe, expect, it } from "vitest";
import { fireEvent, render, screen } from "@testing-library/react";
import TimeTravelView, { buildTimeSteps } from "./TimeTravelView";
import type { RecordedSession } from "../types";

const session: RecordedSession = {
  agent_id: "agent_demo",
  agent_name: "Demo Agent",
  prompt: "You are a demo agent.",
  capabilities: [],
  model: "demo-model",
  user_input: "Uppercase hi, please.",
  exchanges: [
    {
      request_fingerprint: "f1",
      checkpoint_id: "ckpt_tool_1",
      response: {
        model: "demo-model",
        content: "",
        tool_calls: [{ id: "call_1", name: "uppercase", arguments: { text: "hi" } }],
        finish_reason: "tool_calls",
      },
    },
    {
      request_fingerprint: "f2",
      checkpoint_id: "ckpt_final",
      response: {
        model: "demo-model",
        content: "Done: HI",
        tool_calls: [],
        finish_reason: "stop",
      },
    },
  ],
  tool_invocations: [
    { name: "uppercase", arguments: { text: "hi" }, success: true, output: "HI" },
  ],
  recorded_at_ms: 1751900000000,
};

describe("buildTimeSteps", () => {
  it("flattens a session into prompt + exchange steps", () => {
    const steps = buildTimeSteps(session);
    expect(steps).toHaveLength(3);
    expect(steps[0]?.kind).toBe("prompt");
    expect(steps[0]?.content).toBe("Uppercase hi, please.");
    expect(steps[1]?.toolCalls).toHaveLength(1);
    expect(steps[1]?.toolResults?.[0]?.output).toBe("HI");
    expect(steps[2]?.content).toBe("Done: HI");
    expect(steps[2]?.finishReason).toBe("stop");
  });

  it("matches tool invocations to their requesting exchange in order", () => {
    const [toolExchange, finalExchange] = session.exchanges;
    if (!toolExchange || !finalExchange) throw new Error("fixture broken");
    const twoRounds: RecordedSession = {
      ...session,
      exchanges: [toolExchange, toolExchange, finalExchange],
      tool_invocations: [
        { name: "uppercase", arguments: {}, success: true, output: "FIRST" },
        { name: "uppercase", arguments: {}, success: false, output: "second failed" },
      ],
    };
    const steps = buildTimeSteps(twoRounds);
    expect(steps[1]?.toolResults?.[0]?.output).toBe("FIRST");
    expect(steps[2]?.toolResults?.[0]?.output).toBe("second failed");
    expect(steps[2]?.toolResults?.[0]?.success).toBe(false);
  });
});

describe("TimeTravelView", () => {
  it("starts at the end of the recording and shows the full transcript", () => {
    render(<TimeTravelView session={session} />);
    expect(screen.getByText("step 3 / 3")).toBeTruthy();
    expect(screen.getByText("Done: HI")).toBeTruthy();
    expect(screen.getByText("Uppercase hi, please.")).toBeTruthy();
  });

  it("scrubs back in time, hiding later steps", () => {
    render(<TimeTravelView session={session} />);
    const prev = screen.getByRole("button", { name: "Previous step" });

    fireEvent.click(prev);
    expect(screen.getByText("step 2 / 3")).toBeTruthy();
    expect(screen.queryByText("Done: HI")).toBeNull();

    fireEvent.click(prev);
    expect(screen.getByText("step 1 / 3")).toBeTruthy();
    expect(screen.queryByText(/uppercase\(/)).toBeNull();
    expect(screen.getByText("Uppercase hi, please.")).toBeTruthy();
  });

  it("steps forward again with the next button", () => {
    render(<TimeTravelView session={session} />);
    const prev = screen.getByRole("button", { name: "Previous step" });
    const next = screen.getByRole("button", { name: "Next step" });

    fireEvent.click(prev);
    fireEvent.click(prev);
    fireEvent.click(next);
    expect(screen.getByText("step 2 / 3")).toBeTruthy();
    expect(screen.getByText(/uppercase\(/)).toBeTruthy();
  });

  it("shows the replay CLI hint for the session", () => {
    render(<TimeTravelView session={session} />);
    expect(screen.getByText(/agentOS replay --session agent_demo/)).toBeTruthy();
  });
});
