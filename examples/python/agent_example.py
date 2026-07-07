"""AgentOS Python SDK Example.

Demonstrates building a research agent with custom tools using the Python SDK.
Requires: pip install agentos-sdk  (or run from repo root)
"""

import asyncio
import json
import sys
import os

from agentos import AgentClient, AgentSession


async def main():
    # Connect to the AgentOS runtime.
    async with AgentSession("research-agent") as session:
        client = session.client

        # Publish a research query.
        query = json.dumps({
            "action": "search",
            "query": "latest developments in AI agents",
        })
        result = await client.publish("research.query", json.loads(query))
        print(f"Published research query [id={result.message_id}]")


if __name__ == "__main__":
    asyncio.run(main())
