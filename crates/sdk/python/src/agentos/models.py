"""AgentOS data models."""

from dataclasses import dataclass


@dataclass
class AgentEnvelope:
    """Message envelope for agent-to-agent communication."""

    id: str = ""
    source_agent_id: str = ""
    target_agent_id: str = ""
    topic: str = ""
    payload: bytes = b""
    timestamp_ms: int = 0


@dataclass
class PublishedMessage:
    """Result of publishing a message."""

    message_id: str
    topic: str
    timestamp_ms: int


@dataclass
class AgentInfo:
    """Information about a running agent."""

    id: str
    name: str
    state: str
    restarts: int = 0


@dataclass
class SSEMessage:
    """Server-Sent Event received from the AgentOS bus."""

    event: str = ""
    data: str = ""
    id: str = ""
    retry: int | None = None
