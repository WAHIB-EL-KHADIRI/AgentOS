# Contributing to AgentOS

AgentOS welcomes contributors who want to build serious infrastructure for AI
agents.

The project is created and led by **WAHIB EL KHADIRI**, developer and founder of
AgentOS. Contributions are welcome with respect for the original vision, project
identity, and technical direction.

## Table of Contents

- [Code of Conduct](#code-of-conduct)
- [Before You Start](#before-you-start)
- [Development Setup](#development-setup)
- [Project Architecture](#project-architecture)
- [Coding Guidelines](#coding-guidelines)
- [Testing](#testing)
- [Pull Request Process](#pull-request-process)
- [Good First Issues](#good-first-issues)

## Code of Conduct

Please read and follow our [Code of Conduct](CODE_OF_CONDUCT.md). We are
committed to providing a welcoming and inclusive environment.

## Before You Start

For large changes, please open an issue first. This keeps the design aligned and
prevents duplicated work.

Good first areas:

- Rust runtime and async supervision
- gRPC and Protobuf transport
- SQLite memory store
- trace recording and replay
- dashboard UI
- Python and TypeScript SDKs
- examples and documentation

## Development Setup

### Prerequisites

- Rust 1.75+ (`rustup install stable`)
- Protobuf compiler (`protoc`) - optional, for gRPC changes
- Node.js 18+ - for the dashboard
- Python 3.10+ - for Python SDK

### Clone and Build

```bash
git clone https://github.com/WAHIB-EL-KHADIRI/agentOS
cd agentOS
cargo build --workspace
```

### Run Tests

```bash
# All tests
cargo test --workspace

# Specific crate
cargo test -p agentos-bus
cargo test -p agentos-kernel
cargo test -p agentos-vault

# With output
cargo test --workspace -- --nocapture
```

### Run Lint Checks

```bash
cargo fmt --all --check
cargo clippy --workspace --all-targets
```

### Run Examples

```bash
# SDK example
cargo run -p agentos-sdk-examples

# Code review example
cargo run -p agentos-code-review-example

# Integration tests
cargo test -p agentos-integration-tests
```

### Install Git Hooks

```bash
make install-hooks
```

This installs pre-commit and pre-push hooks for formatting, linting, and
security checks.

## Project Architecture

AgentOS is organized as a Rust workspace with the following crates:

| Crate | Description |
|-------|-------------|
| `crates/kernel` | Core runtime: agent lifecycle, supervisor, events, health, metrics |
| `crates/bus` | Message bus: in-memory, gRPC, SSE streaming, WebSocket |
| `crates/memory` | Memory store: embeddings, vector search, SQLite |
| `crates/trace` | Trace recording: thought logging, checkpointing, replay, diff |
| `crates/vault` | Secrets management: encrypted storage, scoped access, audit |
| `crates/registry` | Service discovery: capability-based agent registration |
| `crates/llm` | LLM providers: OpenAI, Anthropic, Ollama |
| `crates/plugins` | WASM plugin runtime with sandboxed execution |
| `crates/cli` | CLI binary with subcommands (run, repl, templates, etc.) |
| `crates/sdk` | High-level Rust SDK for building agents |
| `benches` | Criterion benchmarks for bus, memory, vault |
| `tests/integration_tests` | End-to-end integration tests |

### Key Design Decisions

- **Async runtime**: Tokio with multi-threaded scheduler
- **Message bus**: gRPC for production, in-memory for testing and single-node
- **State persistence**: SQLite via rusqlite with JSON export/import
- **LLM communication**: HTTP streaming (SSE) via reqwest
- **Plugin system**: WASM via wasmtime for sandboxed extension
- **Encryption**: AES-256-GCM for secret storage at rest
- **Observability**: Prometheus metrics, structured tracing, event store

## Coding Guidelines

### Rust

- Use `cargo fmt` (stable Rust formatting)
- Clippy must pass without warnings (`cargo clippy --workspace --all-targets`)
- Follow existing patterns in the crate you are modifying
- Prefer `anyhow::Result` for fallible functions
- Prefer `thiserror` for error enums
- Use `async_trait` for async trait definitions
- Add `#[cfg(test)]` modules with unit tests

### TypeScript (Dashboard)

- Use TypeScript strict mode
- Follow existing React component patterns
- Add JSDoc comments for public APIs

### Python (SDK)

- Follow PEP 8
- Add type hints
- Use `httpx` for HTTP communication

## Testing

We aim for meaningful test coverage across the Rust crates and SDKs. Run the
checks locally instead of relying on a static test count:

```bash
cargo test --workspace
cd crates/sdk/python && python -m pytest tests/ -v
cd crates/sdk/typescript && npm test
```

### Running Benchmarks

```bash
make bench
```

Or run individual benchmarks:

```bash
cargo bench -p agentos-benches
```

### Writing Tests

- Unit tests go in a `#[cfg(test)] mod tests` block at the bottom of the file
- Integration tests go in `tests/`
- Benchmarks go in `benches/`
- Test names should describe the scenario: `test_feature_under_condition`

## Pull Request Process

1. Fork the repository and create a feature branch
2. Make your changes following the [coding guidelines](#coding-guidelines)
3. Add or update tests as needed
4. Run `cargo test --workspace` and ensure all tests pass
5. Run `cargo clippy --workspace --all-targets` and fix any warnings
6. Run `cargo fmt --all --check` to ensure formatting
7. Submit a pull request with a clear description of the changes
8. Link related issues in the PR description

### PR Title Convention

```text
<type>(<scope>): <description>
```

Types: `feat`, `fix`, `docs`, `style`, `refactor`, `test`, `chore`

Examples:

- `feat(kernel): add agent heartbeat monitoring`
- `fix(bus): handle connection reset in gRPC client`
- `docs(sdk): add doc comments to public API`

## Good First Issues

Look for issues tagged with `good-first-issue` or `help-wanted` in the
[GitHub issue tracker](https://github.com/WAHIB-EL-KHADIRI/agentOS/issues).

Common entry points:

- Adding a new LLM provider (copy the OpenAI provider pattern)
- Writing a new WASM plugin example
- Improving error messages and diagnostics
- Adding TypeScript type definitions
- Writing documentation and doc comments
- Adding test coverage for edge cases

## Getting Help

- Open a [Discussion](https://github.com/WAHIB-EL-KHADIRI/agentOS/discussions)
- Ask in issues related to your area of interest
- Read the [FAQ](docs/faq.md) and [Project Overview](PROJECT_OVERVIEW.md)
