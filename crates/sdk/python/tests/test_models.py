"""Tests for AgentOS data models."""

from agentos.models import AgentEnvelope, PublishedMessage, AgentInfo, SSEMessage


def test_agent_envelope_defaults():
    env = AgentEnvelope()
    assert env.id == ""
    assert env.source_agent_id == ""
    assert env.target_agent_id == ""
    assert env.topic == ""
    assert env.payload == b""
    assert env.timestamp_ms == 0


def test_agent_envelope_custom():
    env = AgentEnvelope(
        id="msg-1",
        source_agent_id="agent-a",
        target_agent_id="agent-b",
        topic="test.topic",
        payload=b'{"hello":"world"}',
        timestamp_ms=1234567890,
    )
    assert env.id == "msg-1"
    assert env.source_agent_id == "agent-a"
    assert env.target_agent_id == "agent-b"
    assert env.topic == "test.topic"
    assert env.payload == b'{"hello":"world"}'
    assert env.timestamp_ms == 1234567890


def test_published_message():
    msg = PublishedMessage(message_id="msg-1", topic="test", timestamp_ms=1000)
    assert msg.message_id == "msg-1"
    assert msg.topic == "test"
    assert msg.timestamp_ms == 1000


def test_agent_info():
    info = AgentInfo(id="a1", name="TestAgent", state="running", restarts=2)
    assert info.id == "a1"
    assert info.name == "TestAgent"
    assert info.state == "running"
    assert info.restarts == 2


def test_agent_info_default_restarts():
    info = AgentInfo(id="a1", name="TestAgent", state="stopped")
    assert info.restarts == 0


def test_sse_message_defaults():
    msg = SSEMessage()
    assert msg.event == ""
    assert msg.data == ""
    assert msg.id == ""
    assert msg.retry is None


def test_sse_message_full():
    msg = SSEMessage(event="agent_started", data='{"id":"a1"}', id="evt-1", retry=3000)
    assert msg.event == "agent_started"
    assert msg.data == '{"id":"a1"}'
    assert msg.id == "evt-1"
    assert msg.retry == 3000
