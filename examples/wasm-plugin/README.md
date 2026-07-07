# WASM Plugin Template for AgentOS

This template shows how to build a WebAssembly plugin for AgentOS.

## Prerequisites

Install the `wasm32-wasi` target:

```bash
rustup target add wasm32-wasi
```

## Build

```bash
cargo build --target wasm32-wasi --release
```

The `.wasm` file will be at `target/wasm32-wasi/release/agentos_plugin_counters.wasm`.

## Use with AgentOS

Copy the plugin to the AgentOS plugins directory:

```bash
cp target/wasm32-wasi/release/agentos_plugin_counters.wasm /path/to/plugins/
```

Then start the AgentOS runtime — it will automatically detect and load the plugin.

## How It Works

The plugin exports two WASM functions that the AgentOS plugin runtime calls:

- `agentos_plugin_init(seed: i32) -> i32` — called once when the plugin loads
- `agentos_plugin_process(input_ptr: i32, input_len: i32) -> i64` — called for each request

The plugin can call host functions imported from the AgentOS runtime:

- `agentos_host_log(ptr: i32, len: i32)` — log a message
- `agentos_host_get_state(key_ptr: i32, key_len: i32) -> i32` — read a key from plugin state
- `agentos_host_set_state(key_ptr: i32, key_len: i32, val_ptr: i32, val_len: i32)` — write to plugin state

## Plugin Protocol

Input and output are JSON-encoded. The plugin receives:

```json
{ "method": "process", "params": { "text": "hello" } }
```

And returns:

```json
{ "ok": true, "data": { "processed": "HELLO", "count": 1 } }
```
