# Changelog

All notable changes to AgentOS will be documented in this file.

The format follows a simple versioned history. Public alpha releases should be
honest about what works, what is experimental, and what is not ready yet.

## v0.1.0-alpha - 2026-05-30

This is the first public alpha release of AgentOS. It is intended for local
evaluation, contributor feedback, and validating the runtime direction. It is
not a production-hardened release.

### What Works Today

- Rust workspace with focused crates for kernel, bus, memory, trace, vault,
  registry, LLM integration, CLI, SDK, plugins, examples, and benchmarks.
- CLI-first local workflow for running and inspecting agents:
  `run -> ps -> logs -> trace -> replay`.
- Local demo script and smoke check based on real command behavior, not fake
  output.
- Agent lifecycle, supervisor, event bus, trace, memory, vault, registry, and
  SDK tests across the workspace.
- Unified repository check script: `bash scripts/check.sh`.
- React dashboard production build check.
- Security, support, contribution, release, and AI-assisted development docs.
- Cross-platform release workflow for CLI binaries.

### Experimental

- Dashboard as a live debugging and inspection surface.
- WASM plugin runtime and plugin templates.
- Docker and Docker Compose packaging.
- LLM provider integrations.
- Python and TypeScript SDK packaging.
- Marketplace commands and plugin distribution ideas.

### Known Limitations

- AgentOS is alpha software. APIs and behavior may change before a stable
  release.
- `agentOS fork` is a placeholder and currently reports that trace forking is
  not implemented yet.
- Restart and recovery behavior should not be described as production-ready
  until stronger tests and public guarantees are in place.
- WASM plugins are experimental and should not be treated as a strong security
  boundary yet.
- Docker artifacts are not published for this alpha release.
- Install scripts can install a tagged alpha release, but source builds remain
  the most reliable path for contributors during early development.

### Release Hygiene

- This release is published as a GitHub prerelease.
- Release notes are stored in `docs/releases/v0.1.0-alpha.md`.
- Docker publishing is intentionally skipped for alpha tags.

## Pre-alpha Development History

The entries below describe internal pre-alpha development snapshots. They are
kept for context, but they are not the public `v0.1.0-alpha` release notes.

### 0.2.0 internal snapshot - 2026-05-28

#### Added

- **AgentOSSystem** - unified system integrating all crates (supervisor + bus + trace + memory + vault + registry)
- **Persistence module** - save/load traces (JSON) and vault (encrypted JSON) to disk
- **Agent logging system** - structured per-agent event log with in-memory ring buffer
- **Integration tests** - 81 total tests across all crates (was 0)
- **System integration tests** - spawn_agent, store_memory, search_memory, vault, registry, logging
- **81 unit/integration tests** across all 7 crates
- **Makefile** - build, test, lint, release, docker commands
- **Dockerfile** - multi-stage build for production
- **pre-commit config** - automatic fmt + clippy checks
- **Clap-based CLI** - proper argument parsing with `fork` command
- **Robust error handling** - `thiserror` in all crates with descriptive messages
- **Comprehensive documentation** - all public items documented

#### Changed

- **Kernel**: Agent lifecycle now derives Clone, supervisor handles timeouts gracefully
- **Bus**: `AgentBusTrait` is fully async, subscription-based routing, BusFull protection
- **Memory**: SQLite backend (`SqliteMemoryStore`), cosine similarity search, L2 normalization
- **Trace**: Parent/fork checkpoint model, branch detection, step_backward replay
- **Vault**: SHA-256 hashing, audit log, access count tracking, secrets hidden in Display
- **Registry**: Service health tracking, register_or_update, capability discovery
- **CLI**: Replaced manual arg parsing with `clap`, added `fork` subcommand
- **CI**: Added fmt check + clippy + test + release build stages
- **Python SDK**: Async context manager, dataclasses, proper error types
- **TypeScript SDK**: Full type definitions, connect/disconnect lifecycle

### 0.1.0 internal snapshot - 2026-05-28

#### Added

- Initial Cargo workspace.
- Kernel crate with agent lifecycle skeleton.
- Bus crate with in-memory bus and Protobuf draft.
- Memory crate with store and embedder abstractions.
- Trace crate with recorder, replayer, and diff skeletons.
- Vault crate with secrets and permissions skeletons.
- Registry crate with service discovery skeleton.
- CLI crate with command entrypoint.
- Python and TypeScript SDK starters.
- React dashboard starter.
- Protocol documentation.
- GitHub Actions CI.
