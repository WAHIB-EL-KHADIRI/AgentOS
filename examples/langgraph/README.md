# LangGraph + AgentOS Integration

This example shows how to run a [LangGraph](https://github.com/langchain-ai/langgraph)
agent workflow inside the AgentOS runtime.

## The Idea

AgentOS handles the **runtime layer**: supervision, message bus, secrets vault,
trace recording, and service discovery. LangGraph handles the **agent workflow
logic**: state graphs, tool calling, and LLM orchestration.

By combining them, you get:

- **Time-travel debugging** of LangGraph execution via AgentOS trace recording
- **Secret management** for LangGraph tool credentials via AgentOS vault
- **Agent-to-agent messaging** between LangGraph workflows via AgentOS bus
- **Health monitoring and supervision** via AgentOS kernel

## Prerequisites

```bash
pip install langgraph langchain-openai httpx
```

## Usage

```bash
# Set your API key
export OPENAI_API_KEY="sk-..."

# Run the example
python langgraph_agent.py
```

## How It Works

1. A LangGraph state graph defines the agent workflow (think → act → observe)
2. The graph is wrapped in an `AgentOSRunner` that records checkpoints to
   AgentOS trace and publishes events to the AgentOS bus
3. AgentOS supervises the agent, manages its secrets, and records every step
4. When something goes wrong, you can replay the execution with `agentOS replay`
