# AgentOS - Runtime Infrastructure for AI Agents

[![Rust](https://img.shields.io/badge/Rust-1.75%2B-orange)](https://www.rust-lang.org/)
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

Created and led by **WAHIB EL KHADIRI**.

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

Install scripts are available for tagged releases. For the first public alpha,
pin the alpha tag explicitly:

```bash
# Linux / macOS
export AGENTOS_VERSION=v0.1.0-alpha
curl -fsSL https://raw.githubusercontent.com/WAHIB-EL-KHADIRI/agentOS/main/install.sh | bash

# Windows PowerShell
$env:AGENTOS_VERSION = "v0.1.0-alpha"
iwr -useb https://raw.githubusercontent.com/WAHIB-EL-KHADIRI/agentOS/main/install.ps1 | iex
```

After stable releases exist, the install scripts can resolve the latest GitHub
release automatically. During alpha, source builds remain the most reliable
path for contributors.

Run the repository check before opening a PR:

```bash
# Linux / macOS
bash scripts/check.sh

# Windows PowerShell
powershell -File scripts/check.ps1
```

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
- Python and TypeScript SDK packaging.
- Marketplace commands and plugin distribution ideas.

Planned or still being hardened:

- Stronger restart and recovery guarantees with explicit tests.
- More complete dashboard time-travel views.
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
agentOS replay --checkpoint ckpt_456
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
