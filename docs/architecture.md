# AgentOS Architecture

AgentOS is a Rust runtime layer for AI agents. The codebase is split into small
crates so lifecycle, messaging, state, trace replay, secrets, registry, SDK, and
developer tooling can evolve without turning the runtime into one large module.

The architectural goal is practical infrastructure: a developer should be able
to run an agent, inspect its lifecycle, read its logs, view its trace, and replay
from recorded checkpoints without relying on invented demo output.

## System Flow

```text
CLI / SDK / Dashboard
        |
        v
Runtime
        |
        v
Supervisor
        |
        v
Bus / State / Trace
        |
        v
Agents / Tools
```

## Runtime Boundary

The runtime boundary is where AgentOS turns agent definitions and commands into
managed processes and recorded state. Runtime-facing changes should be treated
as reliability work, not UI polish.

Important runtime questions:

- Did spawn actually succeed?
- Is the lifecycle event order consistent?
- Can multiple watchers observe lifecycle events safely?
- Does stop behavior have a clear timeout path?
- Are logs, traces, and replay data tied to the correct agent id?

## Kernel And Supervisor

`crates/kernel` owns agent lifecycle and supervision. It creates agent handles,
starts and stops agents, tracks agent state, emits lifecycle events, and connects
the supervisor to system-level services.

Future work in this area should remain narrow and test-driven. Restart and
recovery behavior should not be described as production-ready until there are
clear tests and documented guarantees.

## CLI And Local State

`crates/cli` is the primary developer entry point. It should make the runtime
easy to try without hiding what is experimental.

Common inspection flows:

- `agentOS ps --all`
- `agentOS logs --id <agent_id>`
- `agentOS trace --id <agent_id>`
- `agentOS replay --checkpoint <checkpoint_id>`
- `agentOS state doctor`
- `agentOS state inspect`
- `agentOS state export --output backup.json --pretty`
- `agentOS state import --input backup.json --merge`

The local state layer supports inspection, backup, import, cleanup, and
corruption recovery workflows for local development. Durable production storage
guarantees should be documented only when the backing tests and migration story
are in place.

## Bus

`crates/bus` is the messaging boundary between agents and runtime components. It
contains local and networked transports, including in-memory messaging and
protocol-backed transports.

The bus should prioritize message correctness, clear errors, and stable protocol
types over transport-specific shortcuts.

## State, Memory, Trace

AgentOS separates related but different concerns:

- State: operational records used by CLI and runtime workflows.
- Memory: agent-owned records and embedding-backed search.
- Trace: ordered execution history used for replay and debugging.

Trace is the foundation for time-travel debugging. Replay should be
deterministic when possible and explicit when a step depends on external state.

## Vault

`crates/vault` isolates secrets and permission-sensitive access. Treat vault
changes as security-sensitive. Do not weaken validation, access checks, audit
records, or encryption behavior without an explicit design reason.

## Registry

`crates/registry` tracks service discovery and health metadata. It matters when
AgentOS grows from a single local agent to multiple cooperating agents and
tools.

## Dashboard

`dashboard/` is a visual debugging surface. It should help developers inspect
agents, lifecycle state, logs, traces, checkpoints, and replay information. It
should not imply production guarantees that the runtime has not yet earned.

## SDKs

SDKs should make AgentOS usable from existing ecosystems while keeping the core
runtime contracts clear. The Rust SDK is part of the workspace; Python and
TypeScript SDKs are package-oriented surfaces that should stay aligned with the
CLI and protocol behavior.

## Contributor Entry Points

- Runtime contributors: lifecycle correctness, supervision, stop paths, restart
  guarantees, and event consistency.
- CLI contributors: clearer commands, safer state flows, better diagnostics.
- Storage contributors: state, memory, trace, vault, and registry persistence.
- Protocol contributors: bus schema, transport behavior, and compatibility.
- Frontend contributors: dashboard inspection and replay views.
- Documentation contributors: examples, diagrams, glossary, and honest status
  notes.

See [`CONTRIBUTING.md`](../CONTRIBUTING.md), [`docs/contributor-map.md`](contributor-map.md),
and [`docs/issue-roadmap.md`](issue-roadmap.md) for scoped contribution areas.
