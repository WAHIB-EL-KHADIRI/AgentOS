# AgentOS Examples

This directory shows how to use AgentOS in different scenarios.

## Getting Started

- [`simple_agent.toml`](simple_agent.toml) - minimal TOML agent configuration
- [`simple_agent.yaml`](simple_agent.yaml) - minimal YAML-style agent manifest

## Rust Examples

| Example | Description |
|---------|-------------|
| [`sdk/`](sdk/) | Build a research agent with custom tools |
| [`code-review/`](code-review/) | Multi-agent code review with linter, style checker, and coordinator |
| [`wasm-plugin/`](wasm-plugin/) | Build a WASM plugin with host function imports |

## Python Examples

| Example | Description |
|---------|-------------|
| [`python/`](python/) | Python SDK with `AgentSession` |
| [`langgraph/`](langgraph/) | LangGraph workflow with AgentOS runtime instrumentation |
| [`autogen/`](autogen/) | AutoGen multi-agent conversation with AgentOS bus logging |
| [`crewai/`](crewai/) | CrewAI role-based crew with AgentOS observability |

## TypeScript Examples

| Example | Description |
|---------|-------------|
| [`typescript/`](typescript/) | TypeScript SDK with `AgentClient` and `BusClient` |

## Purpose

Examples should stay small and readable. They are meant to show how an agent or
SDK connects to the AgentOS runtime without hiding the important details.
