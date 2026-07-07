# AutoGen + AgentOS Integration

This example shows how to connect [AutoGen](https://github.com/microsoft/autogen)
agents to the AgentOS runtime.

## The Idea

AutoGen provides multi-agent conversation patterns. AgentOS provides the runtime
layer: supervision, message bus, secrets vault, trace recording, and service
discovery between agent groups.

## Prerequisites

```bash
pip install pyautogen httpx
```

## Usage

```bash
python autogen_agents.py
```
