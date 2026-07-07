use std::collections::HashMap;
use std::path::Path;
use std::sync::Arc;

use tokio::sync::RwLock;
use tracing::{info, warn};
use wasmtime::{Engine, Linker, Module, Store, StoreLimitsBuilder, Val};

use crate::types::{PluginDescriptor, PluginManifest, PluginState};

const MAX_MEMORY_BYTES: u64 = 16 * 1024 * 1024;
const MAX_FUEL_AMOUNT: u64 = 100_000;

#[derive(Debug, thiserror::Error)]
pub enum PluginError {
    #[error("WASM compilation failed: {0}")]
    Compile(String),

    #[error("Plugin not found: {0}")]
    NotFound(String),

    #[error("Plugin execution error: {0}")]
    Execution(String),

    #[error("Manifest parse error: {0}")]
    Manifest(String),

    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
}

#[derive(Clone)]
pub struct WasmPlugin {
    pub descriptor: PluginDescriptor,
    pub module: Arc<Module>,
    state: Arc<RwLock<PluginState>>,
}

impl std::fmt::Debug for WasmPlugin {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("WasmPlugin")
            .field("name", &self.descriptor.manifest.name)
            .field("version", &self.descriptor.manifest.version)
            .field("state", &self.state)
            .finish()
    }
}

impl WasmPlugin {
    pub async fn set_state(&self, new_state: PluginState) {
        let mut s = self.state.write().await;
        *s = new_state;
    }

    pub async fn get_state(&self) -> PluginState {
        let s = self.state.read().await;
        s.clone()
    }
}

fn parse_manifest(_wasm_bytes: &[u8]) -> Result<PluginManifest, PluginError> {
    Ok(PluginManifest {
        name: "wasm-plugin".into(),
        version: "0.1.0".into(),
        description: "WASM-based plugin".into(),
        author: None,
        hooks: vec!["on_think".into()],
        permissions: vec!["log".into()],
    })
}

pub struct PluginRuntime {
    engine: Engine,
    plugins: Arc<RwLock<HashMap<String, WasmPlugin>>>,
    max_memory: u64,
}

impl PluginRuntime {
    pub fn new() -> Result<Self, PluginError> {
        Self::with_max_memory(MAX_MEMORY_BYTES)
    }

    pub fn with_max_memory(max_memory: u64) -> Result<Self, PluginError> {
        let mut engine_config = wasmtime::Config::new();
        engine_config.consume_fuel(true);
        let engine =
            Engine::new(&engine_config).map_err(|e| PluginError::Compile(e.to_string()))?;

        Ok(Self {
            engine,
            plugins: Arc::new(RwLock::new(HashMap::new())),
            max_memory,
        })
    }

    pub async fn load_wasm(
        &self,
        name: &str,
        wasm_bytes: &[u8],
    ) -> Result<WasmPlugin, PluginError> {
        let module = Module::new(&self.engine, wasm_bytes)
            .map_err(|e| PluginError::Compile(e.to_string()))?;

        let manifest = parse_manifest(wasm_bytes)?;
        let descriptor = PluginDescriptor {
            path: format!("memory:{name}"),
            manifest,
            wasm_bytes: wasm_bytes.to_vec(),
        };

        let plugin = WasmPlugin {
            descriptor,
            module: Arc::new(module),
            state: Arc::new(RwLock::new(PluginState::Loaded)),
        };

        let mut plugins = self.plugins.write().await;
        plugins.insert(name.to_string(), plugin.clone());
        info!(plugin = %name, "WASM plugin loaded");

        Ok(plugin)
    }

    pub async fn load_file(&self, path: impl AsRef<Path>) -> Result<WasmPlugin, PluginError> {
        let path = path.as_ref();
        let wasm_bytes = std::fs::read(path)?;
        let name = path
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("unknown");
        self.load_wasm(name, &wasm_bytes).await
    }

    pub async fn load_directory(
        &self,
        dir: impl AsRef<Path>,
    ) -> Result<Vec<WasmPlugin>, PluginError> {
        let dir = dir.as_ref();
        let mut loaded = Vec::new();

        if !dir.exists() {
            warn!(path = %dir.display(), "plugin directory does not exist");
            return Ok(loaded);
        }

        for entry in std::fs::read_dir(dir)? {
            let entry = entry?;
            let path = entry.path();
            if path.extension().is_some_and(|ext| ext == "wasm") {
                match self.load_file(&path).await {
                    Ok(plugin) => loaded.push(plugin),
                    Err(e) => {
                        warn!(path = %path.display(), error = %e, "failed to load plugin")
                    }
                }
            }
        }

        info!(count = loaded.len(), dir = %dir.display(), "plugins loaded");
        Ok(loaded)
    }

    pub fn exports_function(plugin: &WasmPlugin, name: &str) -> bool {
        plugin.module.exports().any(|e| e.name() == name)
    }

    pub async fn invoke(&self, plugin_name: &str, function: &str) -> Result<String, PluginError> {
        let plugin_name_owned = plugin_name.to_string();
        let plugins = self.plugins.read().await;
        let plugin = plugins
            .get(plugin_name)
            .ok_or_else(|| PluginError::NotFound(plugin_name.to_string()))?;

        let limits = StoreLimitsBuilder::new()
            .memory_size(self.max_memory as usize)
            .build();
        let mut store = Store::new(&self.engine, limits);
        store
            .set_fuel(MAX_FUEL_AMOUNT)
            .map_err(|e| PluginError::Execution(e.to_string()))?;
        store.limiter(|limits| limits as &mut dyn wasmtime::ResourceLimiter);
        let mut linker = Linker::new(&self.engine);

        let pn = plugin_name_owned.clone();
        linker
            .func_wrap("env", "log", move |_msg_ptr: i32, msg_len: i32| {
                info!(plugin = %pn, msg_len, "WASM plugin log:");
            })
            .map_err(|e| PluginError::Execution(format!("linker failed: {e}")))?;

        let instance = linker
            .instantiate(&mut store, &plugin.module)
            .map_err(|e| PluginError::Execution(e.to_string()))?;

        let func = instance
            .get_func(&mut store, function)
            .ok_or_else(|| PluginError::Execution(format!("function `{function}` not found")))?;

        let func_ty = func.ty(&store);
        let param_count = func_ty.params().len();

        let params: Vec<Val> = std::iter::repeat_n(Val::I32(0), param_count).collect();
        let mut results = vec![Val::I32(0); func_ty.results().len()];

        func.call(&mut store, &params, &mut results)
            .map_err(|e| PluginError::Execution(format!("call failed: {e}")))?;

        let result = results
            .first()
            .and_then(|v| v.i32())
            .map(|v| v.to_string())
            .unwrap_or_default();

        Ok(result)
    }

    pub async fn list(&self) -> Vec<PluginDescriptor> {
        let plugins = self.plugins.read().await;
        plugins.values().map(|p| p.descriptor.clone()).collect()
    }

    pub async fn plugin_state(&self, name: &str) -> Option<PluginState> {
        let plugins = self.plugins.read().await;
        if let Some(plugin) = plugins.get(name) {
            Some(plugin.state.read().await.clone())
        } else {
            None
        }
    }
}

// PluginRuntime does not implement Default because construction
// requires WASM engine initialization that can fail on some platforms.

pub struct PluginRuntimeBuilder {
    plugin_dirs: Vec<String>,
    max_memory: u64,
}

impl Default for PluginRuntimeBuilder {
    fn default() -> Self {
        Self::new()
    }
}

impl PluginRuntimeBuilder {
    pub fn new() -> Self {
        Self {
            plugin_dirs: Vec::new(),
            max_memory: MAX_MEMORY_BYTES,
        }
    }

    pub fn with_plugin_dir(mut self, dir: impl Into<String>) -> Self {
        self.plugin_dirs.push(dir.into());
        self
    }

    pub fn max_memory(mut self, bytes: u64) -> Self {
        self.max_memory = bytes;
        self
    }

    pub async fn build(&self) -> Result<PluginRuntime, PluginError> {
        let runtime = PluginRuntime::with_max_memory(self.max_memory)?;

        for dir in &self.plugin_dirs {
            runtime.load_directory(dir).await?;
        }

        Ok(runtime)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_runtime_creation() {
        let runtime = PluginRuntime::new().unwrap();
        let plugins = runtime.list().await;
        assert!(plugins.is_empty());
    }

    #[tokio::test]
    async fn test_load_directory_nonexistent() {
        let runtime = PluginRuntime::new().unwrap();
        let result = runtime.load_directory("/nonexistent/plugins").await;
        assert!(result.is_ok());
        assert!(result.unwrap().is_empty());
    }

    #[tokio::test]
    async fn test_builder_default() {
        let builder = PluginRuntimeBuilder::new();
        let runtime = builder.build().await.unwrap();
        assert!(runtime.list().await.is_empty());
    }

    #[tokio::test]
    async fn test_load_invalid_wasm() {
        let runtime = PluginRuntime::new().unwrap();
        let result = runtime.load_wasm("bad", &[0, 0, 0, 0]).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_plugin_state() {
        let runtime = PluginRuntime::new().unwrap();
        let _plugin = PluginDescriptor {
            path: "test".into(),
            manifest: PluginManifest {
                name: "test".into(),
                version: "1.0".into(),
                description: "".into(),
                author: None,
                hooks: vec![],
                permissions: vec![],
            },
            wasm_bytes: vec![],
        };
        let plugins = runtime.list().await;
        assert!(plugins.is_empty());
    }
}
