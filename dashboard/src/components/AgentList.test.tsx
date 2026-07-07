import { render, screen } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import AgentList from "./AgentList";
import type { AgentInfo } from "../types";

function makeAgent(overrides: Partial<AgentInfo>): AgentInfo {
  return {
    id: "agent-1",
    name: "Agent One",
    status: "running",
    capabilities: [],
    started_at: null,
    trace_count: 0,
    ...overrides,
  };
}

describe("AgentList", () => {
  it("shows the empty state when there are no agents", () => {
    render(<AgentList agents={[]} selectedId={null} onSelect={vi.fn()} />);
    expect(screen.getByText("No agents running")).toBeInTheDocument();
  });

  it("splits agents into running and stopped sections", () => {
    const agents = [
      makeAgent({ id: "a", name: "Running Agent", status: "running" }),
      makeAgent({ id: "b", name: "Stopped Agent", status: "stopped" }),
    ];
    render(<AgentList agents={agents} selectedId={null} onSelect={vi.fn()} />);

    expect(screen.getByText("Running (1)")).toBeInTheDocument();
    expect(screen.getByText("Stopped (1)")).toBeInTheDocument();
    expect(screen.getByText("Running Agent")).toBeInTheDocument();
    expect(screen.getByText("Stopped Agent")).toBeInTheDocument();
  });

  it("marks the selected agent's card as pressed", () => {
    const agents = [makeAgent({ id: "a", name: "Running Agent" })];
    render(<AgentList agents={agents} selectedId="a" onSelect={vi.fn()} />);

    expect(screen.getByRole("button", { name: /Running Agent/ })).toHaveAttribute(
      "aria-pressed",
      "true"
    );
  });
});
