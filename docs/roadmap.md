# AgentOS Roadmap

AgentOS is being built in public as open infrastructure for AI agents. The
roadmap is intentionally practical: each milestone should make the project more
usable, easier to test, and easier for contributors to join.

## Current Focus

The current focus is the developer experience around the CLI and local runtime
state.

Completed foundations:

- CLI state manager backed by `.agentos/cli-state.json`
- `ps`, `logs`, `trace`, and `replay` connected to local state
- `state doctor`, `state inspect`, `state clean`, `state export`, and
  `state import`
- corruption handling and backup/restore flows
- workspace-wide `cargo check` and `cargo test`

## Milestone 1: Runtime Foundations

- Convert the supervisor into a fully async Tokio runtime component.
- Add heartbeat monitoring.
- Add restart policies.
- Add clear lifecycle events.
- Add graceful shutdown semantics.

Good contributor tasks:

- Improve lifecycle event formatting.
- Add tests for restart behavior.
- Document supervisor states.

## Milestone 2: Durable Persistence

- Move CLI state concepts into a SQLite-backed persistence layer.
- Store memory records, traces, registry entries, and vault metadata.
- Keep JSON export/import for backups.
- Add migrations and compatibility checks.

Good contributor tasks:

- Design migration tests.
- Add state import from JSON into SQLite.
- Write docs for local data layout.

## Milestone 3: Real Agent Bus

- Complete gRPC transport with server and client workflows.
- Add message acknowledgements.
- Add subscription filtering.
- Add protocol compatibility tests.

Good contributor tasks:

- Improve protobuf docs.
- Add bus integration tests.
- Build examples for multi-agent messaging.

## Milestone 4: Trace Time Travel

- Record thoughts, tool calls, tool outputs, and snapshots.
- Replay from checkpoints.
- Compare two trace branches.
- Prepare dashboard APIs for trace visualization.

Good contributor tasks:

- Add richer trace event types.
- Improve replay output.
- Build trace diff examples.

## Milestone 5: Dashboard

- Show live agents.
- Show logs and trace timelines.
- Inspect checkpoints.
- Replay and compare branches.

Good contributor tasks:

- Build the TraceViewer UI.
- Add state summary panels.
- Improve dashboard layout and accessibility.

## Milestone 6: SDKs

- Stabilize the Python SDK.
- Stabilize the TypeScript SDK.
- Add examples for external agents connecting to AgentOS.

Good contributor tasks:

- Add typed SDK events.
- Add README examples.
- Add package publishing docs.

## Contribution Style

AgentOS prefers focused pull requests:

- one command, one crate, or one behavior at a time
- tests for behavior changes
- docs when public workflows change
- clear error messages for CLI and runtime failures

The goal is not to make a giant framework overnight. The goal is to build a
runtime that developers can understand, trust, and improve.
