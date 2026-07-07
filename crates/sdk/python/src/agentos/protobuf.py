"""Protobuf wire format helpers for AgentOS bus protocol.

These functions manually encode protobuf messages without requiring the
protobuf compiler or runtime library. The encoding follows the standard
protobuf binary wire format as defined in the AgentOS bus protocol spec.
"""

from typing import Sequence

# ---------------------------------------------------------------------------
# Protobuf wire format primitives
# ---------------------------------------------------------------------------

VARINT_MAX = 1 << 63


def _encode_varint(value: int) -> bytes:
    """Encode a 64-bit integer as a protobuf varint."""
    buf = []
    while value > 0x7F:
        buf.append((value & 0x7F) | 0x80)
        value >>= 7
    buf.append(value & 0x7F)
    return bytes(buf)


def _encode_field(field_num: int, wire_type: int, payload: bytes) -> bytes:
    """Encode a protobuf field with tag + payload."""
    return _encode_varint((field_num << 3) | wire_type) + payload


def _encode_string(value: str) -> bytes:
    """Encode a protobuf string field."""
    encoded = value.encode("utf-8")
    return _encode_varint(len(encoded)) + encoded


def _encode_uint64(value: int) -> bytes:
    """Encode a uint64 protobuf field."""
    return _encode_varint(value)


def _encode_bytes(value: bytes) -> bytes:
    """Encode a bytes protobuf field."""
    return _encode_varint(len(value)) + value


# ---------------------------------------------------------------------------
# Message builders
# ---------------------------------------------------------------------------


def build_envelope(
    id: str,
    source: str,
    target: str,
    topic: str,
    payload: bytes,
    timestamp_ms: int,
) -> bytes:
    """Manually encode an AgentEnvelope protobuf message.

    Schema:
        message AgentEnvelope {
            string id = 1;
            string source_agent_id = 2;
            string target_agent_id = 3;
            string topic = 4;
            bytes  payload = 5;
            uint64 timestamp_ms = 6;
        }
    """
    buf = bytearray()
    buf.extend(_encode_field(1, 2, _encode_string(id)))
    buf.extend(_encode_field(2, 2, _encode_string(source)))
    buf.extend(_encode_field(3, 2, _encode_string(target)))
    buf.extend(_encode_field(4, 2, _encode_string(topic)))
    buf.extend(_encode_field(5, 2, _encode_bytes(payload)))
    buf.extend(_encode_field(6, 0, _encode_uint64(timestamp_ms)))
    return bytes(buf)


def build_publish_request(envelope_bytes: bytes) -> bytes:
    """Wrap an AgentEnvelope in a PublishRequest.

    Schema:
        message PublishRequest { AgentEnvelope envelope = 1; }
    """
    return _encode_field(1, 2, _encode_varint(len(envelope_bytes)) + envelope_bytes)


def build_subscribe_request(agent_id: str, topics: Sequence[str]) -> bytes:
    """Build a SubscribeRequest protobuf message.

    Schema:
        message SubscribeRequest {
            string agent_id = 1;
            repeated string topics = 2;
        }
    """
    buf = bytearray()
    buf.extend(_encode_field(1, 2, _encode_string(agent_id)))
    for topic in topics:
        buf.extend(_encode_field(2, 2, _encode_string(topic)))
    return bytes(buf)
