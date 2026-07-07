"""Tests for AgentSession async context manager."""

from unittest.mock import AsyncMock, Mock

import pytest
from agentos.session import AgentSession
from agentos.client import AgentClient


def test_session_creation():
    session = AgentSession("test-agent")
    assert session.agent_id == "test-agent"
    assert isinstance(session.client, AgentClient)


def test_session_default_bus_addr():
    session = AgentSession("test-agent")
    assert session.client.bus_addr == "http://localhost:50051"


def test_session_custom_bus_addr():
    session = AgentSession("test-agent", bus_addr="http://custom:8080")
    assert session.client.bus_addr == "http://custom:8080"


@pytest.mark.asyncio
async def test_session_context_manager():
    mock_response = Mock()
    mock_response.status_code = 200
    mock_response.raise_for_status = Mock()
    mock_http = Mock()
    mock_http.get = AsyncMock(return_value=mock_response)
    mock_http.aclose = AsyncMock()

    session = AgentSession("test-agent")
    session.client._http = mock_http

    client = await session.__aenter__()
    assert isinstance(client, AgentClient)
    assert client.agent_id == "test-agent"
    assert client._connected is True

    await session.__aexit__(None, None, None)
    assert client.agent_id is None
    assert client._connected is False
    mock_http.aclose.assert_awaited_once()
