#![no_main]

//! Fuzz the bus WebSocket message parser.
//!
//! `WsMessage` is deserialized directly from text received on the socket, so
//! every byte here is attacker-controlled: anyone who can reach the bus port
//! reaches this parser first, before any authentication or topic check.
//!
//! The target asserts nothing about *what* is parsed. It asserts that parsing
//! never panics, never aborts, and never runs out of memory - a panic in a
//! network-facing parser is a remote denial of service.

use libfuzzer_sys::fuzz_target;

use agentos_bus::websocket::WsMessage;

fuzz_target!(|data: &[u8]| {
    // The socket hands us text, so mirror that: skip non-UTF-8 inputs rather
    // than fuzzing a code path the real server never reaches.
    if let Ok(text) = std::str::from_utf8(data) {
        let _ = serde_json::from_str::<WsMessage>(text);
    }
});
