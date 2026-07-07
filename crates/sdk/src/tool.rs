use std::fmt;

use async_trait::async_trait;

/// Result type for tool execution.
pub type ToolResult = Result<String, String>;

/// A tool that an agent can invoke during its reasoning loop.
///
/// Implement this trait to expose custom capabilities to agents.
///
/// # Example
///
/// ```ignore
/// use agentos_sdk::tool::{Tool, ToolResult};
///
/// struct Calculator;
///
/// #[async_trait::async_trait]
/// impl Tool for Calculator {
///     fn name(&self) -> &str { "calculator" }
///     fn description(&self) -> &str { "Simple arithmetic calculator" }
///
///     async fn run(&self, input: &str) -> ToolResult {
///         // parse and evaluate expression...
///         Ok("42".into())
///     }
/// }
/// ```
#[async_trait]
pub trait Tool: Send + Sync {
    fn name(&self) -> &str;
    fn description(&self) -> &str;

    async fn run(&self, input: &str) -> ToolResult;
}

impl fmt::Debug for dyn Tool {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Tool")
            .field("name", &self.name())
            .field("description", &self.description())
            .finish()
    }
}

/// Adapts an SDK [`Tool`] to the kernel's [`RuntimeTool`] so tools built
/// with the SDK execute inside the runtime's LLM tool loop.
pub(crate) struct SdkToolAdapter {
    inner: std::sync::Arc<dyn Tool>,
}

impl SdkToolAdapter {
    pub(crate) fn new(inner: std::sync::Arc<dyn Tool>) -> Self {
        Self { inner }
    }
}

#[async_trait]
impl agentos_kernel::RuntimeTool for SdkToolAdapter {
    fn name(&self) -> &str {
        self.inner.name()
    }

    fn description(&self) -> &str {
        self.inner.description()
    }

    async fn invoke(&self, arguments: &serde_json::Value) -> Result<String, String> {
        // SDK tools take a raw string input: pass string arguments through
        // unchanged, and serialize structured arguments as JSON.
        let input = match arguments {
            serde_json::Value::String(s) => s.clone(),
            other => other.to_string(),
        };
        self.inner.run(&input).await
    }
}
