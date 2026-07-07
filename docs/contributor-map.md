# Contributor Map

AgentOS needs different kinds of contributors. This map helps new developers
find a useful starting point.

## Rust Runtime Developers

Good areas:

- `crates/kernel`
- `crates/bus`
- `crates/trace`
- async supervision
- lifecycle events
- restart policies
- runtime health checks

Start by reading:

- `docs/architecture.md`
- `docs/design-decisions.md`

## Storage Developers

Good areas:

- `crates/memory`
- `crates/trace`
- `crates/vault`
- SQLite schema design
- persistence interfaces
- data migration strategy

Start by reading:

- `crates/memory/migrations/001_init.sql`
- `docs/security-model.md`

## Frontend Developers

Good areas:

- `dashboard/src/App.tsx`
- `dashboard/src/TraceViewer.tsx`
- trace timeline
- checkpoint inspector
- replay controls
- fork and compare views

Start by reading:

- `docs/time-travel-debugging.md`

## SDK Developers

Good areas:

- `crates/sdk/python`
- `crates/sdk/typescript`
- framework adapters
- examples
- client documentation

Useful integrations:

- LangGraph
- AutoGen
- CrewAI
- Semantic Kernel
- custom tool-calling agents

## Documentation Contributors

Good areas:

- protocol examples
- architecture diagrams
- CLI usage docs
- integration guides
- troubleshooting docs

Documentation is a first-class contribution because AgentOS is a new runtime
idea and needs to be easy to understand.

## Security Contributors

Good areas:

- `crates/vault`
- permissions
- audit events
- secret storage
- sandbox design
- threat modeling

Start by reading:

- `docs/security-model.md`
- `SECURITY.md`

