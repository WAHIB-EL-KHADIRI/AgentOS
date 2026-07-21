# AgentOS - Runtime Infrastructure for AI Agents

[![Rust](https://img.shields.io/badge/Rust-1.94%2B-orange)](https://www.rust-lang.org/)
[![License](https://img.shields.io/badge/License-MIT%20OR%20Apache--2.0-blue)](#license)
[![CI](https://github.com/WAHIB-EL-KHADIRI/agentOS/actions/workflows/ci.yml/badge.svg)](https://github.com/WAHIB-EL-KHADIRI/agentOS/actions/workflows/ci.yml)
[![Windows](https://img.shields.io/badge/windows-supported-blue)](scripts/check.ps1)
[![Docs](https://img.shields.io/badge/docs-available-blue)](docs/)
[![Install](https://img.shields.io/badge/install-one--liner-success)](#quick-start)

AgentOS is an open-source runtime layer for AI agents. It focuses on the
infrastructure around agents: lifecycle, supervision, messaging, state, secrets,
observability, and trace replay.

**Created by [WAHIB EL KHADIRI](https://github.com/WAHIB-EL-KHADIRI)** — founder and architect.

Most agent frameworks help you build an agent workflow. AgentOS focuses on what
happens after that workflow needs to run as a long-lived process, fail clearly,
restart carefully, and be inspected after the fact.

## See it run

Real output from a fresh clone — no API key required to bring the runtime up:

```text
$ cargo run -p agentos-cli -- run --agent examples/simple_agent.toml

INFO agentos_kernel::supervisor: agent spawned and running agent_id=agent_simple_agent state=Running name=simple-agent
INFO agentos_kernel::agent: agent loop started agent_id=agent_simple_agent
INFO agentos_kernel::events: system event emitted event=agent.spawned seq=0
INFO agentOS::run: AgentOS runtime started agent_id=agent_simple_agent host=127.0.0.1 http_port=8080 grpc_port=50051 sse_port=8081
INFO agentos_kernel::health: health server listening on 127.0.0.1:8080
INFO agentos_bus::grpc: gRPC bus server listening on 127.0.0.1:50051
INFO agentos_bus::grpc: SSE event stream listening on http://127.0.0.1:8081/events

AgentOS runtime is live
  http:       127.0.0.1:8080
  grpc:       127.0.0.1:50051
  sse:        http://127.0.0.1:8081/events
  auth:       open (set AGENTOS_API_TOKEN to protect)
  vault:      in-memory only (set AGENTOS_VAULT_KEY to persist)
  agent id:   agent_simple_agent
  status:     running
  trace:      294dab79-e626-4e42-97ad-8deee2c43e18
  (press Ctrl+C to stop)
```

One process gives you a supervised agent, a health endpoint, a gRPC message
bus, a live SSE event stream, and a recorded trace you can replay later.

## Time-Travel Debugging

Your agent did something weird on step 7. Reproducing it costs real API calls —
and never behaves the same twice. AgentOS journals every LLM exchange and tool
result at the provider boundary, so any run can be replayed deterministically
and forked into alternate timelines:

```bash
agentOS run --agent my_agent.toml      # every execution step is journaled automatically
agentOS replay --session agent_123    # re-run offline: no API key, no cost, drift-checked
agentOS fork --from ckpt_4 --prompt "try the other path"   # branch from any checkpoint
```

<!-- TODO(launch): demo GIF of the dashboard Recordings scrubber goes here -->

The dashboard's **Recordings** view turns journals into a scrubbable timeline:
step through the prompt, every exchange, tool calls and their results exactly
as they happened, with per-exchange checkpoints as fork anchors.

## What AgentOS Is

- A Rust-first runtime layer for agent processes.
- A CLI-first developer workflow for running, listing, logging, tracing, and
  replaying agents.
- A supervision, bus, state, trace, vault, registry, and dashboard codebase.
- A place to make agent behavior more observable and reproducible.
- Infrastructure that can sit underneath LangGraph, AutoGen, CrewAI, custom
  agents, and other agent frameworks.

## What AgentOS Is Not

- Not another prompt framework.
- Not a chatbot UI.
- Not a replacement for LangGraph, AutoGen, CrewAI, or Semantic Kernel.
- Not a production-hardened distributed control plane yet.
- Not a project that should claim recovery, replay, or security guarantees
  without tests and reproducible demos.

## Official Demo

The official local demo flow is:

```text
run -> ps -> logs -> trace -> replay
```

Start with:

- Demo guide: [`docs/demo.md`](docs/demo.md)
- Demo config: [`.agentos/demo/agentos.demo.toml`](.agentos/demo/agentos.demo.toml)
- Demo script: [`scripts/demo.sh`](scripts/demo.sh)

Smoke-check the demo without inventing output:

```bash
bash scripts/demo.sh --check
```

## Quick Start

The most reliable path during early development is building from source:

```bash
git clone https://github.com/WAHIB-EL-KHADIRI/agentOS
cd agentOS
cargo build --workspace
cargo run -p agentos-cli -- run --agent examples/simple_agent.toml
```

One-liner install scripts ([`install.sh`](install.sh), [`install.ps1`](install.ps1))
activate with the first tagged release. Until then, building from source is the
supported path.

## Current Status

AgentOS is active infrastructure work. It has a working local runtime and
developer workflow, but it is not claiming to be a production-hardened platform.

Stable enough to use locally:

- CLI flows for `run`, `ps`, `logs`, `trace`, and `replay`.
- Rust workspace checks and tests.
- Local state inspection, export, import, and cleanup flows.
- Core crates for kernel, bus, trace, memory, vault, registry, SDK, and CLI.
- SSE event stream started by `agentOS run` (default `127.0.0.1:8081/events`)
  feeding the dashboard live agent and trace events.
- Demo smoke checks that reject known fake-output fallback patterns.

Experimental:

- Dashboard as a live debugging surface.
- WASM plugin runtime and plugin templates.
- Docker Compose packaging.
- LLM provider integrations.
- LLM tool execution loop: tools registered through the SDK are executed
  when the model requests them, with every call and result recorded as
  trace checkpoints, logs, and live dashboard events (capped rounds,
  provider-agnostic result passing).
- Deterministic session replay and fork: every execution step is journaled
  (LLM exchanges + tool results); `agentOS replay --session <agent_id>`
  re-executes it with recorded responses (no API key needed) and reports
  drift, and `agentOS fork` replays a prefix then continues live.
- Dashboard Recordings view: a time-travel scrubber over recorded sessions
  (slider and step controls across the prompt, exchanges, tool calls and
  results, with per-exchange checkpoints shown as fork anchors).
- Python and TypeScript SDK packaging.
- Marketplace commands and plugin distribution ideas.

Planned or still being hardened:

- Stronger restart and recovery guarantees with explicit tests.
- Dashboard diff view between an original run and its forks.
- Published SDK packages.
- More integration examples for existing agent frameworks.

## Architecture

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

Repository layout:

```text
crates/kernel      lifecycle, agent handles, supervisor, system integration
crates/bus         in-memory, gRPC, SSE, and WebSocket messaging
crates/trace       recording, replay, diff, and checkpoint model
crates/memory      memory store and embedding abstraction
crates/vault       secret isolation, encryption, scopes, and audit
crates/registry    service discovery and health metadata
crates/llm         provider abstractions
crates/cli         agentOS command-line interface
crates/sdk         Rust SDK
dashboard/         React dashboard
docs/              architecture, CLI, security, demo, and contributor docs
scripts/           check.sh, check.ps1, and demo scripts
```

Read the deeper architecture guide: [`docs/architecture.md`](docs/architecture.md)

## CLI Shape

Common commands:

```bash
agentOS run --agent my_agent.toml
agentOS ps
agentOS logs --id agent_123
agentOS trace --id agent_123
agentOS replay --session agent_123
agentOS fork --from ckpt_456 --prompt "explore the alternative"
agentOS status
agentOS doctor
agentOS repl
agentOS dev --path examples
```

See the full CLI reference: [`docs/cli-reference.md`](docs/cli-reference.md)

## Development

```bash
cargo fmt --all --check
cargo check --workspace
cargo test --workspace
cargo check --workspace --benches
bash scripts/demo.sh --check
```

Or run the unified check:

```bash
# Linux / macOS
bash scripts/check.sh

# Windows PowerShell
powershell -File scripts/check.ps1
```

## Design Principles

- Runtime first: AgentOS is infrastructure, not a prompt framework.
- Lifecycle correctness before feature volume.
- Replayability and observability over opaque success claims.
- Honest demos over polished fake output.
- Interop over lock-in.
- Small crates with clear ownership.

## Contributing

Start here:

- [`CONTRIBUTING.md`](CONTRIBUTING.md)
- [`AGENTS.md`](AGENTS.md)
- [`docs/ai-development.md`](docs/ai-development.md)
- [`docs/contributor-map.md`](docs/contributor-map.md)
- [`docs/issue-roadmap.md`](docs/issue-roadmap.md)

Good first areas include docs, CLI polish, demo reliability, focused tests,
dashboard inspection views, and SDK examples.

Before opening a large PR, open an issue or discussion so the design can be
aligned with the roadmap.

## Documentation

- Project overview: [`PROJECT_OVERVIEW.md`](PROJECT_OVERVIEW.md)
- Demo: [`docs/demo.md`](docs/demo.md)
- Architecture: [`docs/architecture.md`](docs/architecture.md)
- Runtime walkthrough: [`docs/runtime-walkthrough.md`](docs/runtime-walkthrough.md)
- Security model: [`docs/security-model.md`](docs/security-model.md)
- Trace replay debugging: [`docs/time-travel-debugging.md`](docs/time-travel-debugging.md)
- Glossary: [`docs/project-glossary.md`](docs/project-glossary.md)
- Pitch: [`docs/pitch.md`](docs/pitch.md)
- Roadmap: [`ROADMAP.md`](ROADMAP.md)

## Ownership

AgentOS was created and is led by **WAHIB EL KHADIRI**. Contributions are
welcome and credited, while the project identity and technical direction remain
stewarded by WAHIB EL KHADIRI.

Read more: [`FOUNDER.md`](FOUNDER.md)

## License

Licensed under either of:

- [`MIT License`](LICENSE-MIT)
- [`Apache License, Version 2.0`](LICENSE-APACHE)

at your option.

Copyright (c) 2026 WAHIB EL KHADIRI and contributors.
