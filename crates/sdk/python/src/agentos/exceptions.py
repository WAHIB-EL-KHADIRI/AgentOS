"""AgentOS SDK exceptions."""


class AgentOSError(Exception):
    """Base exception for AgentOS SDK errors."""


class ConnectionError(AgentOSError):
    """Raised when connection to AgentOS bus fails."""


class PublishError(AgentOSError):
    """Raised when publishing a message fails."""


class SubscribeError(AgentOSError):
    """Raised when subscribing to topics fails."""


class ConfigurationError(AgentOSError):
    """Raised when SDK configuration is invalid."""
