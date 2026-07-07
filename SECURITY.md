# Security Policy

AgentOS includes components for secrets, permissions, agent isolation, runtime
control, and observability. Security reports are important and welcome.

## Reporting a Vulnerability

Please do not open a public issue for a security vulnerability.

Report privately to the project maintainer:

- Maintainer: WAHIB EL KHADIRI
- GitHub: `WAHIB-EL-KHADIRI`

Include:

- affected component
- reproduction steps
- expected impact
- suggested fix, if known

## Scope

Security-sensitive areas include:

- `crates/vault`
- `crates/kernel`
- `crates/bus`
- `crates/plugins`
- permission checks
- agent isolation
- secret handling
- dashboard access control when implemented
- Docker and Compose deployment defaults

## Secure Defaults

AgentOS defaults to loopback runtime binding through `RuntimeConfig`, uses
bounded protobuf request bodies on the bus, applies defensive HTTP response
headers to built-in JSON/SSE endpoints, and runs the production container images
as a non-root user.

The dashboard production image serves static assets with a restrictive Content
Security Policy and related browser hardening headers. The Compose stack avoids
publishing Redis/Postgres ports by default and requires an explicit
`AGENTOS_POSTGRES_PASSWORD` before enabling the persistence profile.

## Dependency And Build Checks

Run the repository check script before publishing or deploying changes:

```bash
bash scripts/check.sh
```

The script runs Rust formatting, workspace checks, Clippy with all targets,
workspace tests, benches checks, demo smoke checks, dashboard build/lint/format
checks/tests, TypeScript SDK build/tests, and Python SDK pytest/Ruff checks when
the required tools are available.

Node package advisories should be checked with `npm audit --audit-level=high` in
`dashboard` and `crates/sdk/typescript`. Rust advisory checks require installing
`cargo-audit` or an equivalent policy tool in the local/CI environment.

## Current Status

AgentOS has hardened local and container defaults, but production deployments
must still add environment-specific controls such as authentication,
authorization policy, TLS termination, secret injection, backup policy, and
network segmentation. Treat the built-in APIs as trusted-network interfaces
unless an authenticated gateway is placed in front of them.