# Security Policy

AgentOS includes components for secrets, permissions, agent isolation, runtime
control, and observability. Security reports are important and welcome.

## Supported Versions

| Version | Supported |
| --- | --- |
| `0.1.x` (pre-release) | Yes — fixes land on `main` |
| older / unreleased commits | No |

AgentOS is pre-1.0. Only the latest `main` and the most recent tagged release
receive security fixes.

## Reporting a Vulnerability

Please do not open a public issue, discussion, or pull request for a security
vulnerability.

**Report privately here:**
<https://github.com/WAHIB-EL-KHADIRI/AgentOS/security/advisories/new>

That form is GitHub's private vulnerability reporting. It is visible only to
the maintainer, it keeps the report out of public view until a fix ships, and
it lets us credit you in the resulting advisory.

Include:

- affected component and version or commit
- reproduction steps, ideally a minimal proof of concept
- expected impact, and who is exposed
- suggested fix, if known

### What to expect

| Stage | Target |
| --- | --- |
| Acknowledgement of your report | within 48 hours |
| Initial assessment and severity | within 7 days |
| Fix or documented mitigation | within 90 days of triage |
| Public advisory | after a fix is available, coordinated with you |

If a report goes unanswered past these windows, you are free to disclose
publicly. Silence is not a request for secrecy.

### Safe harbour

Research done in good faith under this policy is welcome. That means: work only
against your own instances, do not access or modify data that is not yours, do
not degrade the service for others, and give the maintainer a reasonable window
to respond before publishing. Reports made under those terms will not be met
with legal action.

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