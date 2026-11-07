# `agentos-state`

`agentos-state` defines AgentOS's operational state snapshot and the storage
contract used by the CLI. It owns the stored agent, log, and checkpoint record
types so third-party backends do not depend on CLI or trace implementation
details. A backend stores a complete `StateSnapshot`; it does
not need a local path or CLI-specific import, export, cleanup, or diagnostic
operations.

Implement `StateBackend` for a custom store and return a stable
`StateBackendKind` identity. The crate's integration test in
`tests/external_backend.rs` demonstrates this from a separate crate boundary.

The API is synchronous and saves whole snapshots. Callers must serialize
read-modify-write operations: this version has no revision check or
compare-and-swap operation, so stale concurrent writers can overwrite newer
state. The built-in CLI and file backends are intended for single-writer use.

The portable JSON `StateSnapshot::version` and SQLite schema version are
separate compatibility versions. Adding fields to the model or changing a
storage schema requires updating its corresponding compatibility path.
