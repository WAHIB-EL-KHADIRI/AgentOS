"""Async context manager for AgentOS sessions."""

from typing import Any

from .client import AgentClient


class AgentSession:
    """Async context manager for AgentOS sessions with auto-connect/disconnect.

    Usage:
        async with AgentSession("my-agent") as client:
            await client.publish("my.topic", {"hello": "world"})
    """

    def __init__(
        self,
        agent_id: str,
        bus_addr: str = "http://localhost:50051",
    ) -> None:
        self.client = AgentClient(bus_addr=bus_addr)
        self.agent_id = agent_id

    async def __aenter__(self) -> AgentClient:
        await self.client.connect(self.agent_id)
        return self.client

    async def __aexit__(self, *args: Any) -> None:
        await self.client.disconnect()
