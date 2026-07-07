"""Tests for protobuf wire format encoding."""

from agentos.protobuf import (
    build_envelope,
    build_publish_request,
    build_subscribe_request,
    _encode_varint,
    _encode_field,
)


def test_encode_varint_small():
    assert _encode_varint(0) == b"\x00"
    assert _encode_varint(1) == b"\x01"
    assert _encode_varint(127) == b"\x7f"


def test_encode_varint_large():
    assert _encode_varint(128) == b"\x80\x01"
    assert _encode_varint(300) == b"\xac\x02"


def test_encode_field():
    result = _encode_field(1, 2, b"hello")
    assert result.startswith(b"\x0a")


def test_build_envelope():
    env = build_envelope(
        id="test-1",
        source="agent-a",
        target="agent-b",
        topic="test.topic",
        payload=b'{"key":"value"}',
        timestamp_ms=1000,
    )
    assert isinstance(env, bytes)
    assert len(env) > 0
    assert b"test-1" in env
    assert b"agent-a" in env


def test_build_publish_request():
    env = build_envelope(
        id="test-2",
        source="a",
        target="b",
        topic="t",
        payload=b"{}",
        timestamp_ms=2000,
    )
    req = build_publish_request(env)
    assert isinstance(req, bytes)
    assert len(req) > len(env)


def test_build_subscribe_request():
    req = build_subscribe_request("agent-a", ["topic1", "topic2"])
    assert isinstance(req, bytes)
    assert len(req) > 0


def test_build_subscribe_request_empty_topics():
    req = build_subscribe_request("agent-a", [])
    assert isinstance(req, bytes)
    assert len(req) > 0
