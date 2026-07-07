//! AgentOS SDK Example: Building a custom research agent in Rust.

use agentos_sdk::tool::{Tool, ToolResult};
use agentos_sdk::AgentBuilder;
use async_trait::async_trait;

struct WebSearchTool;

#[async_trait]
impl Tool for WebSearchTool {
    fn name(&self) -> &str {
        "web_search"
    }
    fn description(&self) -> &str {
        "Search the web for current information on a given query"
    }

    async fn run(&self, input: &str) -> ToolResult {
        Ok(format!("[mock] search results for: {input}"))
    }
}

struct CalculatorTool;

#[async_trait]
impl Tool for CalculatorTool {
    fn name(&self) -> &str {
        "calculator"
    }
    fn description(&self) -> &str {
        "Evaluate mathematical expressions (e.g. 2 + 2)"
    }

    async fn run(&self, input: &str) -> ToolResult {
        let parts: Vec<&str> = input.trim().splitn(3, ' ').collect();
        if parts.len() != 3 {
            return Err("Expected format: a op b (e.g. 2 + 2)".into());
        }
        let a: f64 = parts[0].parse().map_err(|_| "Invalid number".to_string())?;
        let b: f64 = parts[2].parse().map_err(|_| "Invalid number".to_string())?;
        let result = match parts[1] {
            "+" => a + b,
            "-" => a - b,
            "*" => a * b,
            "/" => {
                if b == 0.0 {
                    return Err("Division by zero".into());
                }
                a / b
            }
            op => return Err(format!("Unknown operator: {op}")),
        };
        Ok(result.to_string())
    }
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt::init();

    let handle = AgentBuilder::new("research-agent-1")
        .name("Research Assistant")
        .prompt("You are a research assistant that can search the web and calculate.")
        .capability("web_search")
        .capability("calculator")
        .tool(Box::new(WebSearchTool))
        .tool(Box::new(CalculatorTool))
        .max_restarts(3)
        .spawn()
        .await?;

    println!("Agent spawned:");
    println!("  ID:   {}", handle.id);
    println!("  Name: {}", handle.name);
    println!("  State: {:?}", handle.state().await);

    let calc = CalculatorTool;
    let result = calc.run("3 + 4").await.unwrap_or_else(|e| e);
    println!("\nTool test: 3 + 4 = {result}");

    let search = WebSearchTool;
    let result = search.run("latest AI news").await.unwrap_or_else(|e| e);
    println!("Search test: {result}");

    Ok(())
}
