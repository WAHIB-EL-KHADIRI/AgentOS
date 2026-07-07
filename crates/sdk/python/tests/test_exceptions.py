"""Tests for AgentOS SDK exceptions."""

from agentos.exceptions import (
    AgentOSError,
    ConnectionError,
    PublishError,
    SubscribeError,
    ConfigurationError,
)


def test_agentos_error():
    err = AgentOSError("base error")
    assert isinstance(err, Exception)
    assert str(err) == "base error"


def test_connection_error():
    err = ConnectionError("cannot connect")
    assert isinstance(err, AgentOSError)
    assert str(err) == "cannot connect"


def test_publish_error():
    err = PublishError("publish failed")
    assert isinstance(err, AgentOSError)
    assert str(err) == "publish failed"


def test_subscribe_error():
    err = SubscribeError("subscribe failed")
    assert isinstance(err, AgentOSError)
    assert str(err) == "subscribe failed"


def test_configuration_error():
    err = ConfigurationError("invalid config")
    assert isinstance(err, AgentOSError)
    assert str(err) == "invalid config"


def test_error_hierarchy():
    assert issubclass(ConnectionError, AgentOSError)
    assert issubclass(PublishError, AgentOSError)
    assert issubclass(SubscribeError, AgentOSError)
    assert issubclass(ConfigurationError, AgentOSError)
