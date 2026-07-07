# AgentOS Project Overview

AgentOS is a Rust-first runtime layer for AI agents.

It focuses on the operational layer around agents:

- supervision
- messaging
- memory
- trace replay
- secrets and permissions
- service discovery
- CLI and dashboard tooling

## One-Line Pitch

AgentOS is the missing runtime layer between AI agent frameworks and production
systems.

## Who It Is For

AgentOS is for developers building:

- long-running AI agents
- multi-agent systems
- tool-calling agents
- agent infrastructure
- debugging tools for agent behavior
- secure runtime environments for agents

## Why It Exists

Agent frameworks help define behavior. AgentOS helps operate that behavior.

The project is designed for the moment when a workflow becomes a system:

- many agents
- persistent state
- tool calls
- secrets
- failures
- debugging needs
- replay needs

## Current Focus

The current focus is the local runtime:

1. kernel supervision
2. event logging
3. bus protocol (gRPC, SSE, WebSocket)
4. trace recording and time-travel debugging
5. persistence boundaries
6. CLI ergonomics and REPL
7. LLM provider abstraction (OpenAI, Anthropic, Ollama)
8. WASM plugin system
9. Multi-agent code review and collaboration demos
10. SDK ecosystem (Rust, Python, TypeScript)
11. Framework integration examples (LangGraph, AutoGen, CrewAI)

## Founder

AgentOS is created and led by **WAHIB EL KHADIRI**, developer and founder of
the project.

