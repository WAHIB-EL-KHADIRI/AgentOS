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
