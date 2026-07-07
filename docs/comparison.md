# AgentOS Comparison

AgentOS is not a replacement for existing agent frameworks. It is a runtime and
debugging layer that can sit around them.

This comparison is meant to clarify scope.

## Quick Summary

| Project | Main Focus | AgentOS Relationship |
| --- | --- | --- |
| LangGraph | Graph-based agent workflows | AgentOS can supervise and debug LangGraph agents |
| AutoGen | Multi-agent conversation patterns | AgentOS can provide runtime, bus, trace, and permissions |
| CrewAI | Role-based agent teams | AgentOS can operate and observe CrewAI-style agents |
| Semantic Kernel | AI orchestration and skills | AgentOS can add runtime services and replay tooling |
| AgentOS | Runtime, supervision, trace, replay, isolation | Complements agent frameworks |

## Feature Scope

| Capability | AgentOS | LangGraph | AutoGen | CrewAI | Semantic Kernel |
| --- | --- | --- | --- | --- | --- |
| Define agent workflow | Partial | Strong | Strong | Strong | Strong |
| Run long-lived agents | Goal | Partial | Partial | Partial | Partial |
| Agent supervision | Core goal | Limited | Limited | Limited | Limited |
| Agent-to-agent bus | Core goal | Not primary | Partial | Partial | Not primary |
| Time-travel replay | Core goal | Not primary | Not primary | Not primary | Not primary |
| Fork and compare runs | Planned | Not primary | Not primary | Not primary | Not primary |
| Per-agent vault | Core goal | Not primary | Not primary | Not primary | Not primary |
| Service discovery | Core goal | Not primary | Not primary | Not primary | Not primary |
| Runtime dashboard | Goal | Ecosystem-dependent | Ecosystem-dependent | Ecosystem-dependent | Ecosystem-dependent |

## Where AgentOS Fits

Use an agent framework to define what an agent does.

Use AgentOS to manage how that agent runs, communicates, stores memory, handles
secrets, records trace data, and gets debugged after something goes wrong.

## Example Integration Model

```text
LangGraph / AutoGen / CrewAI / custom agent
        |
        +-- AgentOS SDK (Rust, Python, TypeScript)
        +-- AgentOS Bus (gRPC, SSE, WebSocket)
        +-- AgentOS Kernel (supervision, lifecycle)
        +-- AgentOS Trace (recording, replay, diff)
        +-- AgentOS Vault (secrets, encryption, audit)
        +-- AgentOS Memory (embeddings, SQLite)
        +-- AgentOS Plugins (WASM sandboxed extensions)
        |
        v
Dashboard (React, SSE) + CLI (REPL, diagnostics, export/import)
```

## Positioning

AgentOS should be judged as infrastructure.

The question is not "Can AgentOS build a prompt workflow better than X?"

The better question is:

> When an agent system becomes complex, can AgentOS help developers operate,
> inspect, replay, secure, and evolve it?

That is the product direction.
