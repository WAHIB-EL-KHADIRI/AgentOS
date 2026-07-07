# Development Guide

This guide is for contributors who want to run AgentOS locally and make focused
changes without learning the whole repository first.

## Prerequisites

- Rust stable
- Node.js 20 or newer for the dashboard
- Python 3.10 or newer for the Python SDK
- `protoc` if you are changing Protobuf or gRPC code

## Run The CLI

Build the workspace:

```bash
cargo build --workspace
```

Run the CLI from source:

```bash
cargo run -p agentos-cli -- status
cargo run -p agentos-cli -- run --agent examples/simple_agent.toml
cargo run -p agentos-cli -- ps --all
```

Install the CLI locally from the repository:

```bash
cargo install --path crates/cli
agentOS status
```

## Run Tests

Run all Rust checks:

```bash
cargo check --workspace
cargo test --workspace
```

Run one crate while iterating:

```bash
cargo test -p agentos-kernel
cargo test -p agentos-cli
cargo test -p agentos-trace
```

## Run The Dashboard

Install dependencies and build:

```bash
cd dashboard
npm.cmd install
npm.cmd run build
```

During UI work, use the dev server:

```bash
cd dashboard
npm.cmd run dev
```

The dashboard reads live runtime updates through SSE. UI-only changes should not
change runtime or state backend behavior.

## State Backends

JSON is the default local state backend:

```bash
agentOS ps --all
```

SQLite is optional:

```bash
agentOS --state-backend sqlite ps --all
```

When changing state code, keep JSON backward-compatible and make sure export,
import, inspect, doctor, clean, and migrate still work.

## Good First Areas

- CLI output and help text
- Documentation examples
- Dashboard empty states and tests
- Small trace/replay tests
- SDK examples
- WASM plugin examples

For larger changes, open an issue or discussion first so the design can stay
aligned with the roadmap.

## Before Opening A PR

Run:

```bash
cargo fmt --all
cargo check --workspace
cargo test --workspace
cd dashboard
npm.cmd run build
```
