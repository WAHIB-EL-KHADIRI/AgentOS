# Editions

AgentOS follows an **Open Core** model.

The runtime — the part you build on — is Apache-2.0 open source, permanently.
A separate commercial edition adds the operational layer a company needs when
AgentOS moves from one engineer's laptop to a regulated production estate.

> **Status, stated plainly:** everything in the Community column below exists
> and is released today. Everything in the Enterprise column is **in
> development and not yet purchasable.** This page describes the boundary and
> the direction, not a shipping product. If you need something in the right
> column, that is worth a conversation now — see
> [Design partners](#design-partners).

---

## Community Edition

**Licence:** [Apache-2.0](../LICENSE) · **Price:** free, forever · **Status:** shipping

The complete agent runtime. Not a crippled demo, not time-limited, not
seat-limited. You can run this in production at any scale without paying
anything or telling anyone.

| Capability | Crate |
|---|---|
| Agent kernel, supervision, circuit breaking, health | `agentos-kernel` |
| Message bus (in-process + gRPC) | `agentos-bus` |
| Agent memory | `agentos-memory` |
| **Deterministic record / replay / drift diffing** | `agentos-trace` |
| Secret vault with permission sets and encryption | `agentos-vault` |
| Service registry and capability discovery | `agentos-registry` |
| LLM provider abstraction (OpenAI, Anthropic, custom) | `agentos-llm` |
| WASM plugin sandbox | `agentos-plugins` |
| Rust SDK + Python SDK | `agentos-sdk` |
| CLI, REPL, dev server, time-travel debugging | `agentos-cli` |
| Local dashboard | `dashboard/` |

The headline feature — replaying any agent run offline and deterministically
for $0.00 — is Community. It is the reason to adopt AgentOS, and it is not for
sale.

## Enterprise Edition

**Licence:** proprietary · **Price:** contact · **Status:** in development

Everything in Community, plus the layer that only matters once several teams,
an auditor, and a compliance officer are involved.

| Planned capability | Extends |
|---|---|
| **Hosted trace storage** — S3/Postgres backends, retention policy, cross-run search | `agentos_trace::TraceStore` trait |
| **Multi-tenancy** — tenant isolation, shared state backends, quotas | `StateBackend` (see note below) |
| **RBAC + SSO** — roles, SAML/OIDC, directory sync | `agentos_vault::PermissionSet` |
| **Audit log** — tamper-evident, exportable, retention controls | `agentos_kernel::AgentHooks`, journal |
| **Advanced replay analytics** — fleet-wide drift, regression detection, cost attribution | `agentos_trace::diff`, `compare_replay` |
| **Managed integrations** — supported connectors with an SLA | `agentos_kernel::PluginRegistry` |
| **Cloud control plane** — hosted, managed AgentOS | above the runtime |
| **Commercial support** — SLA, private security advisories, roadmap input | — |

## Where the line is drawn, and why

The rule is deliberate and will not move:

> **If it helps you build and debug agents, it is Community.
> If it helps an organisation operate a fleet of them, it is Enterprise.**

This is why the boundary is honest:

- **No feature is removed from Community to create Enterprise.** Everything
  open today stays open. Apache-2.0 is irrevocable for every released version.
- **Enterprise is additive, not a gate.** Community is not artificially limited
  so that Enterprise looks necessary.
- **The extension points are public.** Every Enterprise capability plugs into a
  trait that is already exported from the open-source core:
  `agentos_trace::TraceStore`, `agentos_vault::PermissionSet`,
  `agentos_kernel::AgentHooks`, and `agentos_kernel::PluginRegistry`. You can
  implement any of them yourself, in-house, and never pay a cent. Several
  people will, and that is fine.

  **One exception, stated honestly:** `StateBackend` currently lives inside
  `agentos-cli`, which is a binary-only crate with no `lib.rs`. It is therefore
  *not* public API today, and multi-tenancy cannot be implemented against it
  from outside. Promoting it to a library crate is a prerequisite for that row
  of the table, and it will be done in the open core — not held back to force
  an Enterprise purchase. Tracked as an open item.

That last point is the test of an honest Open Core: the commercial edition has
to win on being better, not on being the only thing that fits the socket.

## What is not for sale

- The name **AgentOS** — reserved, see [`NOTICE`](../NOTICE).
- Your data. There is no telemetry in Community.
- Your independence. Community has no licence key, no phone-home, no
  expiry, no seat count.

## Design partners

Enterprise features are being built against real deployments rather than
guesses. If you are running agents in production and need any row from the
Enterprise table, early collaboration gets you the feature shaped around your
constraints.

Contact **WAHIB EL KHADIRI** — <wahibmaxim@gmail.com>. Tell me what you are
running, what breaks, and what your auditors ask for.

See also: [`LICENSING.md`](../LICENSING.md) for the licence terms themselves.
