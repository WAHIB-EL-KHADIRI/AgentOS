#![forbid(unsafe_code)]

pub mod runtime;
pub mod types;

pub use runtime::{PluginRuntime, PluginRuntimeBuilder, WasmPlugin};
pub use types::{PluginDescriptor, PluginEvent, PluginManifest, PluginState};
