"""AgentOS Python SDK - Client library for the AgentOS runtime.

Provides real HTTP/Protobuf communication with the AgentOS bus server,
including publish/subscribe, SSE event streaming, and runtime management.
"""

from .client import AgentClient
from .exceptions import AgentOSError, ConnectionError, ConfigurationError, PublishError, SubscribeError
from .models import AgentEnvelope, AgentInfo, PublishedMessage
from .session import AgentSession

__all__ = [
    "AgentClient",
    "AgentSession",
    "AgentEnvelope",
    "AgentInfo",
    "PublishedMessage",
    "AgentOSError",
    "ConnectionError",
    "ConfigurationError",
    "PublishError",
    "SubscribeError",
]

__version__ = "0.1.0"
