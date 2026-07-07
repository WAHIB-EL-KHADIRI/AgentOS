# Security Model

AgentOS is designed for agent systems where different agents may have different
permissions, secrets, and runtime responsibilities.

This document describes the intended security model.

## Goals

- Keep secrets isolated per agent.
- Make permissions explicit.
- Record sensitive runtime events.
- Prevent accidental access to unrelated agent state.
- Make future sandboxing possible.

## Agent Identity

Every agent should have a stable `agent_id`.

Runtime state, memory, traces, permissions, and secrets should be scoped by that
identity whenever possible.

## Vault Isolation

Secrets should be stored and retrieved by agent id.

An agent should not be able to read another agent's secret without an explicit
permission or trusted system-level operation.

## Permissions

Permissions should be checked before sensitive operations:

- reading files
- writing files
- accessing network resources
- using a secret
- calling a tool
- publishing privileged bus messages

The permission model should stay simple first, then evolve as real integrations
need more detail.

## Auditability

Security-sensitive actions should become traceable events:

- secret accessed
- permission denied
- tool call allowed
- tool call blocked
- agent degraded
- agent failed

The dashboard should eventually make these events visible.

## Current Limitations

AgentOS is still in active development. It should not be treated as a hardened
security boundary yet.

Production-grade isolation will require more work around process boundaries,
tool sandboxing, encrypted storage, and network access policies.
## Implemented Defensive Defaults

- Runtime HTTP/gRPC listeners default to `127.0.0.1` unless explicitly changed.
- Built-in JSON endpoints return `Content-Type: application/json` and defensive
  browser headers such as `X-Content-Type-Options`, `X-Frame-Options`,
  `Referrer-Policy`, and a deny-by-default `Content-Security-Policy`.
- Bus protobuf requests are size-limited before decoding to reduce memory DoS
  risk from oversized request bodies.
- SSE frames prefix every emitted payload line with `data:` so event payloads
  cannot inject SSE control fields. Named `event:` fields are only produced
  from server-side event types, with line breaks stripped defensively.
- Dashboard production Nginx config uses a restrictive CSP and related browser
  hardening headers.
- Container images run as a non-root `agentos` user and use `/data` as the
  writable runtime data directory.
- Docker Compose declares only services the runtime actually uses; external
  storage services will be added together with the code paths that need them.

## Production Deployment Requirements

Before exposing AgentOS outside a trusted local network, add:

- TLS termination with modern cipher policy.
- Authentication and authorization in front of runtime, bus, and dashboard APIs.
- Explicit network policy between dashboard, runtime, bus, cache, and database.
- Managed secret injection instead of plain environment variables where possible.
- Centralized audit logs for secret access, plugin loading, bus publish events,
  and supervisor lifecycle changes.
- Dependency advisory scanning for Rust, Node, Python, and container base images.