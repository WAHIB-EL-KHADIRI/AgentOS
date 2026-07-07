# Design Decisions

This file explains the technical choices behind AgentOS. It helps contributors
understand the direction before proposing large changes.

## Rust First

AgentOS is runtime infrastructure. Rust is a good fit because it gives the
project:

- predictable performance
- strong type safety
- good async support
- reliable binaries for CLI and services
- a strong ecosystem for networking and storage

Python and TypeScript remain important through SDKs, but the core runtime should
stay Rust-first.

## Cargo Workspace

AgentOS is split into crates so each boundary stays clear:

- `kernel` owns lifecycle and supervision
- `bus` owns messaging
- `trace` owns replay and debugging data
- `memory` owns stored agent memory
- `vault` owns secrets and permissions
- `registry` owns service discovery
- `cli` owns developer commands

This keeps the project easier to review and lets contributors work on one area
without understanding the entire runtime.

## Protobuf And gRPC

AgentOS agents may run in different processes or languages. Protobuf gives the
bus a stable protocol, while gRPC gives a practical transport for production
systems.

The in-memory bus should remain useful for local development and lightweight
integration.

## SQLite First

SQLite is the best first persistence layer because it is:

- local
- embedded
- easy to inspect
- strong enough for single-node runtime data
- simple to ship with examples and local development

The design should not prevent future Postgres or distributed storage backends.

## Time-Travel As A Core Primitive

Trace data is not an optional logging feature. It is one of the reasons AgentOS
exists.

The trace model should influence the kernel, bus, memory, and dashboard design.
AgentOS should make agent behavior replayable today, with fork and comparison
workflows added carefully as the trace model matures.

## Small Core, Extensible Edges

The kernel should stay focused. Integrations, framework adapters, dashboard
features, and provider-specific logic should live at the edges.

This protects AgentOS from becoming a large framework with unclear boundaries.
