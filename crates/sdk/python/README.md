# AgentOS Python SDK

Python client library for [AgentOS](https://github.com/WAHIB-EL-KHADIRI/agentOS) — the open-source runtime layer for AI agents.

## Installation

```bash
pip install agentos-sdk
```

## Quick Start

```python
import asyncio
from agentos import AgentSession


async def main():
    async with AgentSession("my-agent") as client:
        # Publish a message to the bus
        result = await client.publish(
            "task.created",
            {"task": "research AI agents", "priority": "high"},
        )
        print(f"Published: {result}")

        # Subscribe to events
        async for msg in client.subscribe(["task.*"]):
            print(f"Received: {msg}")


asyncio.run(main())
```

## Features

- **Publish/Subscribe** — send and receive messages via the AgentOS bus
- **SSE Streaming** — real-time event streaming from the runtime
- **Async Native** — built on `httpx` for full async/await support
- **Type Hints** — fully typed API for IDE autocompletion

## Requirements

- Python 3.10+
- httpx 0.27+

## Documentation

Full documentation: https://github.com/WAHIB-EL-KHADIRI/agentOS
