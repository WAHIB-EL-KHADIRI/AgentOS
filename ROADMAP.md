# AgentOS Roadmap

This roadmap is intentionally practical. AgentOS should become useful through
small, measurable milestones. The detailed contributor version lives in
[`docs/roadmap.md`](docs/roadmap.md).

## Current Focus

The current focus is developer trust around the CLI, local runtime state,
documentation, examples, and demo readiness.

Working foundations:

- Cargo workspace with clear crate boundaries
- local CLI commands for `run`, `ps`, `logs`, `trace`, and `replay`
- JSON-backed CLI state with export/import/inspect/doctor/clean workflows
- optional SQLite backend work for local state
- React dashboard build with SSE connection state handling
- Rust, Python, and TypeScript SDK starters
- CI workflow for Rust, SDKs, and dashboard build

## Milestone 1: Trust And Demo Readiness

- Keep README claims aligned with implemented behavior.
- Keep examples and demo scripts honest and runnable.
- Add one canonical demo path based on real CLI output.
- Add screenshots or terminal recordings once the demo is stable.

## Milestone 2: Runtime Reliability

- Harden supervisor restart and heartbeat behavior.
- Add more lifecycle tests.
- Improve logs and trace coverage for failure cases.
- Document clear runtime limitations.

## Milestone 3: Durable Persistence

- Continue hardening SQLite state backend.
- Keep JSON export/import portable.
- Add migration compatibility tests.
- Document local data layout.

## Milestone 4: Agent Bus And Integrations

- Complete production-oriented gRPC/server workflows.
- Add bus integration tests and examples.
- Improve framework integration examples for LangGraph, AutoGen, and CrewAI.

## Milestone 5: Dashboard And SDKs

- Add lightweight dashboard tests.
- Improve trace timeline and checkpoint inspection.
- Stabilize Python and TypeScript SDK packaging.
- Prepare PyPI and npm publishing docs.

## Contribution Style

AgentOS prefers focused pull requests:

- one command, one crate, or one behavior at a time
- tests for behavior changes
- docs when public workflows change
- clear error messages for CLI and runtime failures

The goal is not to make a giant framework overnight. The goal is to build a
runtime that developers can understand, trust, and improve.
