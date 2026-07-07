# Contributing to AgentOS

Thank you for helping make AgentOS stronger. The project is created and led by
WAHIB EL KHADIRI, Developer, and welcomes contributors who want to build open
runtime infrastructure for AI agents.

## Where to Start

Good first areas:

- CLI command output and error messages
- tests for `crates/cli/src/state.rs`
- documentation examples
- trace replay improvements
- supervisor lifecycle tests

Larger areas:

- async supervisor runtime
- SQLite persistence
- gRPC transport
- dashboard trace viewer
- Python and TypeScript SDKs

## Development Workflow

Run checks before opening a pull request:

```bash
cargo fmt --all
cargo check --workspace
cargo test --workspace
```

For CLI state workflows, useful commands include:

```bash
agentOS state inspect
agentOS state doctor
agentOS state clean --dry-run --status completed
agentOS state export --output backup.json --pretty
agentOS state import --input backup.json --dry-run --merge
```

## Pull Request Guidelines

- Keep the change focused.
- Explain the user-facing behavior.
- Add tests when behavior changes.
- Update docs when commands, architecture, or workflows change.
- Avoid unrelated refactors in the same PR.

## Issue Ideas

Maintainers can label issues with:

- `good first issue`
- `cli`
- `runtime`
- `trace`
- `persistence`
- `dashboard`
- `docs`

Examples:

- Improve `state inspect` formatting.
- Add JSON output for `state clean`.
- Add trace replay examples.
- Document CLI state file format.
- Add supervisor restart policy tests.

## Project Direction

AgentOS should feel useful early and become deeper over time. Small practical
improvements matter: better errors, better docs, better tests, and clearer
runtime behavior all make the project easier for contributors to trust.
