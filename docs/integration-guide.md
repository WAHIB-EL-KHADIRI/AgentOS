# Integration Guide

AgentOS is designed to work with many kinds of agents.

## Integration Levels

### 1. CLI-Level Integration

An agent is launched from a config file and supervised by AgentOS.

Use this for local workflows and early experiments.

### 2. SDK-Level Integration

An external Python or TypeScript agent connects to AgentOS through an SDK.

Use this for framework integrations and custom agents.

### 3. Bus-Level Integration

A service communicates directly with the AgentOS bus protocol.

Use this for advanced integrations, non-SDK languages, or infrastructure
components.

## Suggested Adapter Pattern

```text
Framework agent
    |
    v
Small adapter
    |
    v
AgentOS SDK or bus
    |
    v
AgentOS runtime
```

## Integration Targets

Useful future integrations:

- LangGraph
- AutoGen
- CrewAI
- Semantic Kernel
- custom Rust agents
- custom Python agents
- custom TypeScript agents

