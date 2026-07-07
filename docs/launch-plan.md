# Launch Plan

This file describes how AgentOS can be presented to developers when the project
is ready for wider attention.

## Goal

Make AgentOS easy to understand in the first 60 seconds.

A developer should quickly see:

- what problem AgentOS solves
- why time-travel debugging matters
- how the workspace is organized
- where they can contribute
- who created and leads the project

## Pre-Launch Checklist

- [ ] README has a clear one-sentence pitch
- [ ] project status is honest and current
- [ ] docs explain architecture and design decisions
- [ ] contributor map exists
- [ ] open questions invite discussion
- [ ] examples are small and readable
- [ ] CI passes
- [ ] first issues are labeled
- [ ] founder attribution is visible

## GitHub Repository Setup

Recommended repository settings:

- enable Discussions
- enable Issues
- enable Wiki only if needed later
- add repository topics:
  - `ai-agents`
  - `agent-runtime`
  - `rust`
  - `grpc`
  - `observability`
  - `time-travel-debugging`
  - `developer-tools`
  - `multi-agent`

## Launch Message

Use a short, direct message:

```text
AgentOS is a Rust-first runtime layer for AI agents: supervision, messaging,
memory, vault, trace replay, and time-travel debugging.

Built by WAHIB EL KHADIRI.
```

## What To Ask From Developers

Ask for specific help:

- review the architecture
- challenge the trace/replay model
- suggest framework integrations
- contribute to Rust runtime crates
- improve the dashboard
- improve SDK ergonomics

