"""AgentOS bus client for connecting to the AgentOS runtime."""

import json
import logging
import time
from typing import Any, AsyncIterator, Callable

import httpx

from .exceptions import ConnectionError, PublishError, SubscribeError
from .models import AgentEnvelope, AgentInfo, PublishedMessage
from .protobuf import build_envelope, build_publish_request, build_subscribe_request

logger = logging.getLogger("agentos")


class AgentClient:
    """Client for connecting to the AgentOS HTTP/gRPC bus.

    Provides publish/subscribe messaging to the AgentOS runtime using
    HTTP/Protobuf for requests and SSE for event streaming.

    Usage:
        client = AgentClient("http://localhost:50051")
        await client.connect("my-agent-id")
        await client.publish("my.topic", {"key": "value"})
        async for msg in client.subscribe(["my.topic"]):
            print(msg)
        await client.disconnect()
    """

    def __init__(
        self,
        bus_addr: str = "http://localhost:50051",
        http_timeout: float = 30.0,
    ) -> None:
        self.bus_addr = bus_addr.rstrip("/")
        self._http = httpx.AsyncClient(timeout=http_timeout)
        self.agent_id: str | None = None
        self._connected = False
        self._message_count = 0

    async def connect(self, agent_id: str) -> None:
        """Connect to the AgentOS bus.

        Args:
            agent_id: Unique identifier for this agent.

        Raises:
            RuntimeError: If already connected.
            ConnectionError: If the bus is unreachable.
        """
        if self._connected:
            raise RuntimeError(f"already connected as '{self.agent_id}'")

        try:
            resp = await self._http.get(f"{self.bus_addr}/health")
            resp.raise_for_status()
        except httpx.RequestError as e:
            raise ConnectionError(
                f"cannot reach AgentOS bus at {self.bus_addr}: {e}"
            ) from e

        self.agent_id = agent_id
        self._connected = True
        logger.info("Agent '%s' connected to AgentOS bus at %s", agent_id, self.bus_addr)

    async def disconnect(self) -> None:
        """Disconnect from the AgentOS bus."""
        self._connected = False
        self.agent_id = None
        await self._http.aclose()
        logger.info("Disconnected from AgentOS bus")

    async def publish(
        self,
        topic: str,
        payload: dict[str, Any] | None = None,
    ) -> PublishedMessage:
        """Publish a message to the AgentOS bus via HTTP/Protobuf.

        Args:
            topic: Message topic (e.g. 'thought.recorded', 'task.created').
            payload: Message payload as a dictionary.

        Returns:
            PublishedMessage with the assigned message ID.

        Raises:
            RuntimeError: If not connected.
            PublishError: If the publish request fails.
        """
        if not self._connected or self.agent_id is None:
            raise RuntimeError("connect() must be called before publish()")

        self._message_count += 1
        now_ms = int(time.time() * 1000)

        envelope_bytes = build_envelope(
            id=f"py_{self._message_count}_{now_ms}",
            source=self.agent_id,
            target="*",
            topic=topic,
            payload=json.dumps(payload or {}).encode("utf-8"),
            timestamp_ms=now_ms,
        )
        request_body = build_publish_request(envelope_bytes)

        try:
            resp = await self._http.post(
                f"{self.bus_addr}/agentos.bus.v1.AgentBus/Publish",
                content=request_body,
                headers={"Content-Type": "application/x-protobuf"},
            )
            resp.raise_for_status()
        except httpx.RequestError as e:
            raise PublishError(f"publish failed: {e}") from e

        logger.debug("Published message on topic '%s'", topic)

        return PublishedMessage(
            message_id=f"py_{self._message_count}_{now_ms}",
            topic=topic,
            timestamp_ms=now_ms,
        )

    async def subscribe(
        self,
        topics: list[str],
        callback: Callable[[AgentEnvelope], None] | None = None,
    ) -> AsyncIterator[AgentEnvelope]:
        """Subscribe to topics via SSE event stream.

        Args:
            topics: List of topic patterns to subscribe to.
            callback: Optional sync callback invoked for each event.

        Yields:
            AgentEnvelope for each received message.

        Raises:
            RuntimeError: If not connected.
            SubscribeError: If the subscription fails.
        """
        if not self._connected or self.agent_id is None:
            raise RuntimeError("connect() must be called before subscribe()")

        body = build_subscribe_request(self.agent_id, topics)

        try:
            async with self._http.stream(
                "POST",
                f"{self.bus_addr}/agentos.bus.v1.AgentBus/Subscribe",
                content=body,
                headers={"Content-Type": "application/x-protobuf"},
            ) as response:
                response.raise_for_status()
                async for line in response.aiter_lines():
                    line = line.strip()
                    if line.startswith("data: "):
                        data = line.removeprefix("data: ")
                        envelope = AgentEnvelope(
                            id=data,
                            source_agent_id="",
                            target_agent_id=self.agent_id,
                            topic=",".join(topics),
                            payload=b"",
                            timestamp_ms=int(time.time() * 1000),
                        )
                        if callback:
                            callback(envelope)
                        yield envelope
        except httpx.RequestError as e:
            raise SubscribeError(f"subscribe failed: {e}") from e

    async def is_connected(self) -> bool:
        """Check if the connection to the bus is alive."""
        try:
            resp = await self._http.get(f"{self.bus_addr}/health")
            return resp.status_code == 200
        except httpx.RequestError:
            return False

    async def get_agent_status(self) -> list[AgentInfo]:
        """Query the AgentOS runtime for agent status."""
        try:
            resp = await self._http.get(f"{self.bus_addr}/health")
            if resp.status_code == 200:
                data = resp.json()
                agents = data.get("agents", [])
                return [
                    AgentInfo(
                        id=a.get("id", "unknown"),
                        name=a.get("name", "unknown"),
                        state=a.get("state", "unknown"),
                        restarts=a.get("restarts", 0),
                    )
                    for a in agents
                ]
        except (httpx.RequestError, json.JSONDecodeError):
            pass
        return []
