//! AgentOS WASM Plugin Template
//!
//! This plugin implements a simple word counter service.
//! It demonstrates the WASM plugin protocol:
//!   - Host function imports for logging and state
//!   - Plugin function exports for init/process
//!   - JSON-based message passing

use serde::{Deserialize, Serialize};

// ---------------------------------------------------------------------------
// Host function imports (provided by AgentOS runtime)
// ---------------------------------------------------------------------------

extern "C" {
    /// Log a message through the AgentOS runtime.
    fn agentos_host_log(ptr: *const u8, len: i32);
    /// Read a value from plugin state by key. Returns pointer to allocated bytes.
    fn agentos_host_get_state(key_ptr: *const u8, key_len: i32) -> i32;
    /// Write a value to plugin state.
    fn agentos_host_set_state(
        key_ptr: *const u8,
        key_len: i32,
        val_ptr: *const u8,
        val_len: i32,
    );
}

// ---------------------------------------------------------------------------
// Plugin protocol types
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
struct PluginInput {
    method: String,
    #[serde(default)]
    params: serde_json::Value,
}

#[derive(Debug, Serialize)]
struct PluginOutput {
    ok: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    data: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<String>,
}

impl PluginOutput {
    fn success(data: serde_json::Value) -> Self {
        Self { ok: true, data: Some(data), error: None }
    }
    fn error(msg: impl Into<String>) -> Self {
        Self { ok: false, data: None, error: Some(msg.into()) }
    }
}

// ---------------------------------------------------------------------------
// Helper: write to WASM linear memory and return (ptr, len) packed as i64
// ---------------------------------------------------------------------------

fn write_output(value: &PluginOutput) -> i64 {
    let json = serde_json::to_string(value).unwrap_or_default();
    let bytes = json.into_bytes();
    let len = bytes.len() as i32;
    // Allocate memory by leaking — the runtime reads before the next call
    let ptr = Box::into_raw(bytes.into_boxed_slice()) as *mut u8 as i32;
    (ptr as i64) << 32 | (len as i64 & 0xFFFF_FFFF)
}

fn read_string(ptr: i32, len: i32) -> String {
    if ptr == 0 || len <= 0 {
        return String::new();
    }
    let slice = unsafe { std::slice::from_raw_parts(ptr as *const u8, len as usize) };
    String::from_utf8_lossy(slice).to_string()
}

fn host_log(msg: &str) {
    let bytes = msg.as_bytes();
    unsafe {
        agentos_host_log(bytes.as_ptr(), bytes.len() as i32);
    }
}

fn host_get_state(key: &str) -> Option<String> {
    let key_bytes = key.as_bytes();
    let result_ptr = unsafe {
        agentos_host_get_state(key_bytes.as_ptr(), key_bytes.len() as i32)
    };
    if result_ptr == 0 {
        return None;
    }
    // Pointer is 8-byte: [ptr:4][len:4]
    let ptr = result_ptr as *const i32;
    let data_ptr = unsafe { *ptr } as *const u8;
    let data_len = unsafe { *ptr.add(1) } as usize;
    if data_ptr.is_null() || data_len == 0 {
        return None;
    }
    let slice = unsafe { std::slice::from_raw_parts(data_ptr, data_len) };
    Some(String::from_utf8_lossy(slice).to_string())
}

fn host_set_state(key: &str, value: &str) {
    let key_bytes = key.as_bytes();
    let val_bytes = value.as_bytes();
    unsafe {
        agentos_host_set_state(
            key_bytes.as_ptr(),
            key_bytes.len() as i32,
            val_bytes.as_ptr(),
            val_bytes.len() as i32,
        );
    }
}

// ---------------------------------------------------------------------------
// Plugin logic
// ---------------------------------------------------------------------------

fn process_text(text: &str) -> (String, usize, usize) {
    let uppercase = text.to_uppercase();
    let word_count = text.split_whitespace().count();
    let char_count = text.chars().count();
    (uppercase, word_count, char_count)
}

// ---------------------------------------------------------------------------
// Exported plugin entry points
// ---------------------------------------------------------------------------

/// Called once when the plugin is loaded.
/// `seed` is an initialization value (0 if none).
/// Returns 0 on success, non-zero on error.
#[no_mangle]
pub extern "C" fn agentos_plugin_init(seed: i32) -> i32 {
    host_log(&format!("[counters-plugin] init with seed={seed}"));
    // Store initial counter
    host_set_state("counter", "0");
    host_set_state("seed", &seed.to_string());
    0
}

/// Called for each request.
/// `input_ptr` and `input_len` describe the JSON input in WASM linear memory.
/// Returns i64 with upper 32 bits = output pointer, lower 32 bits = output length.
#[no_mangle]
pub extern "C" fn agentos_plugin_process(input_ptr: i32, input_len: i32) -> i64 {
    let input_str = read_string(input_ptr, input_len);

    let input: PluginInput = match serde_json::from_str(&input_str) {
        Ok(v) => v,
        Err(e) => {
            return write_output(&PluginOutput::error(format!("Invalid input: {e}")));
        }
    };

    match input.method.as_str() {
        "process" => {
            let text = input.params["text"].as_str().unwrap_or("");
            let (uppercased, word_count, char_count) = process_text(text);

            // Read and increment counter
            let current: i32 = host_get_state("counter")
                .and_then(|v| v.parse().ok())
                .unwrap_or(0);
            host_set_state("counter", &(current + 1).to_string());

            let data = serde_json::json!({
                "processed": uppercased,
                "word_count": word_count,
                "char_count": char_count,
                "invocation": current + 1,
            });

            host_log(&format!(
                "[counters-plugin] processed request #{invocation}: \
                 {word_count} words, {char_count} chars",
                invocation = current + 1,
            ));

            write_output(&PluginOutput::success(data))
        }

        "status" => {
            let counter: i32 = host_get_state("counter")
                .and_then(|v| v.parse().ok())
                .unwrap_or(0);
            let seed: i32 = host_get_state("seed")
                .and_then(|v| v.parse().ok())
                .unwrap_or(0);

            let data = serde_json::json!({
                "plugin": "agentos-plugin-counters",
                "version": "0.1.0",
                "invocations": counter,
                "seed": seed,
            });
            write_output(&PluginOutput::success(data))
        }

        "reset" => {
            host_set_state("counter", "0");
            host_log("[counters-plugin] counter reset");
            write_output(&PluginOutput::success(serde_json::json!({"reset": true})))
        }

        other => {
            write_output(&PluginOutput::error(format!("Unknown method: {other}")))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_process_text_counts() {
        let (upper, words, chars) = process_text("Hello World");
        assert_eq!(upper, "HELLO WORLD");
        assert_eq!(words, 2);
        assert_eq!(chars, 11);
    }

    #[test]
    fn test_process_text_empty() {
        let (upper, words, chars) = process_text("");
        assert_eq!(upper, "");
        assert_eq!(words, 0);
        assert_eq!(chars, 0);
    }

    #[test]
    fn test_process_text_multiline() {
        let text = "hello\nworld\nfoo bar baz";
        let (upper, words, chars) = process_text(text);
        assert_eq!(upper, "HELLO\nWORLD\nFOO BAR BAZ");
        assert_eq!(words, 6);
    }

    #[test]
    fn test_plugin_output_success() {
        let output = PluginOutput::success(serde_json::json!({"key": "value"}));
        assert!(output.ok);
        assert!(output.error.is_none());
        assert_eq!(output.data.unwrap()["key"], "value");
    }

    #[test]
    fn test_plugin_output_error() {
        let output = PluginOutput::error("something went wrong");
        assert!(!output.ok);
        assert!(output.data.is_none());
        assert_eq!(output.error.unwrap(), "something went wrong");
    }
}
