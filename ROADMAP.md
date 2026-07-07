# AgentOS Roadmap

This is the single canonical roadmap for AgentOS. It is intentionally
practical: AgentOS should become useful through small, measurable milestones,
and each milestone lists contributor-friendly tasks.

## Current Focus

The current focus is developer trust around the CLI, local runtime state,
documentation, examples, and demo readiness.

Working foundations:

- Cargo workspace with clear crate boundaries
- local CLI commands for `run`, `ps`, `logs`, `trace`, and `replay`
- JSON-backed CLI state with export/import/inspect/doctor/clean workflows
- optional SQLite backend work for local state
- SSE event stream started by `agentOS run`, feeding the dashboard
- React dashboard build with SSE connection state handling
- Rust, Python, and TypeScript SDK starters
- CI workflow for Rust, SDKs, and dashboard build

## Milestone 1: Trust And Demo Readiness

- Keep README claims aligned with implemented behavior.
- Keep examples and demo scripts honest and runnable.
- Add one canonical demo path based on real CLI output.
- Add screenshots or terminal recordings once the demo is stable.

Good contributor tasks:

- Improve lifecycle event formatting.
- Record a real terminal demo once the flow is stable.
- Document supervisor states.

## Milestone 2: Runtime Reliability

- Harden supervisor restart and heartbeat behavior.
- Add more lifecycle tests.
- Improve logs and trace coverage for failure cases.
- Document clear runtime limitations.

Good contributor tasks:

- Add tests for restart behavior.
- Improve failure-path log messages.

## Milestone 3: Durable Persistence

- Continue hardening SQLite state backend.
- Keep JSON export/import portable.
- Add migration compatibility tests.
- Document local data layout.

Good contributor tasks:

- Design migration tests.
- Add state import from JSON into SQLite.
- Write docs for local data layout.

## Milestone 4: Agent Bus And Integrations

- Complete production-oriented gRPC/server workflows.
- Add message acknowledgements and subscription filtering.
- Add bus integration tests and protocol compatibility tests.
- Improve framework integration examples for LangGraph, AutoGen, and CrewAI.

Good contributor tasks:

- Improve protobuf docs.
- Add bus integration tests.
- Build examples for multi-agent messaging.

## Milestone 5: Trace Replay

- Record thoughts, tool calls, tool outputs, and snapshots.
- Replay from checkpoints.
- Compare two trace branches.
- Prepare dashboard APIs for trace visualization.

Good contributor tasks:

- Add richer trace event types.
- Improve replay output.
- Build trace diff examples.

## Milestone 6: Dashboard And SDKs

- Improve trace timeline and checkpoint inspection views.
- Add lightweight dashboard tests.
- Stabilize Python and TypeScript SDK packaging.
- Prepare PyPI and npm publishing docs.

Good contributor tasks:

- Build the TraceViewer UI.
- Add state summary panels.
- Add typed SDK events and README examples.

## Contribution Style

AgentOS prefers focused pull requests:

- one command, one crate, or one behavior at a time
- tests for behavior changes
- docs when public workflows change
- clear error messages for CLI and runtime failures

The goal is not to make a giant framework overnight. The goal is to build a
runtime that developers can understand, trust, and improve.
