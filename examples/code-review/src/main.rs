use std::sync::Arc;

use agentos_bus::in_memory::InMemoryBus;
use agentos_bus::{AgentBusTrait, AgentEnvelope};
use agentos_sdk::tool::{Tool, ToolResult};
use agentos_sdk::AgentBuilder;
use async_trait::async_trait;
use serde::{Deserialize, Serialize};

/// A code snippet submitted for review.
#[derive(Debug, Clone, Serialize, Deserialize)]
struct CodeSubmission {
    file: String,
    code: String,
    author: String,
}

/// A review finding.
#[derive(Debug, Clone, Serialize, Deserialize)]
struct ReviewFinding {
    agent: String,
    severity: String,
    line: Option<usize>,
    message: String,
}

/// A completed review.
#[derive(Debug, Clone, Serialize, Deserialize)]
struct CodeReview {
    file: String,
    author: String,
    findings: Vec<ReviewFinding>,
    approved: bool,
    summary: String,
}

// ---------------------------------------------------------------------------
// Tools for the code review pipeline
// ---------------------------------------------------------------------------

struct LinterTool;

#[async_trait]
impl Tool for LinterTool {
    fn name(&self) -> &str {
        "lint"
    }
    fn description(&self) -> &str {
        "Lint source code for syntax issues"
    }

    async fn run(&self, input: &str) -> ToolResult {
        let submission: CodeSubmission =
            serde_json::from_str(input).map_err(|e| format!("Failed to parse submission: {e}"))?;

        let mut findings = Vec::new();
        for (i, line) in submission.code.lines().enumerate() {
            let trimmed = line.trim();
            if trimmed.len() > 100 {
                findings.push(ReviewFinding {
                    agent: "linter".into(),
                    severity: "warning".into(),
                    line: Some(i + 1),
                    message: format!("Line {} exceeds 100 characters", i + 1),
                });
            }
            if trimmed.starts_with("dbg!") || trimmed.starts_with("println!") {
                findings.push(ReviewFinding {
                    agent: "linter".into(),
                    severity: "info".into(),
                    line: Some(i + 1),
                    message: format!("Line {} has debug/print statement", i + 1),
                });
            }
        }

        if findings.is_empty() {
            findings.push(ReviewFinding {
                agent: "linter".into(),
                severity: "ok".into(),
                line: None,
                message: "No lint issues found.".into(),
            });
        }

        serde_json::to_string(&findings).map_err(|e| format!("Serialization failed: {e}"))
    }
}

struct StyleCheckerTool;

#[async_trait]
impl Tool for StyleCheckerTool {
    fn name(&self) -> &str {
        "style_check"
    }
    fn description(&self) -> &str {
        "Check code style conventions"
    }

    async fn run(&self, input: &str) -> ToolResult {
        let submission: CodeSubmission =
            serde_json::from_str(input).map_err(|e| format!("Failed to parse submission: {e}"))?;

        let mut findings = Vec::new();
        let lines: Vec<&str> = submission.code.lines().collect();

        for (i, line) in lines.iter().enumerate() {
            if line.contains("let ") && line.contains("CamelCase") {
                findings.push(ReviewFinding {
                    agent: "style-checker".into(),
                    severity: "style".into(),
                    line: Some(i + 1),
                    message: format!("Line {}: variable names should use snake_case", i + 1),
                });
            }
        }

        if let Some(last) = lines.last() {
            if !last.is_empty() {
                findings.push(ReviewFinding {
                    agent: "style-checker".into(),
                    severity: "style".into(),
                    line: Some(lines.len()),
                    message: "File should end with a newline.".into(),
                });
            }
        }

        if findings.is_empty() {
            findings.push(ReviewFinding {
                agent: "style-checker".into(),
                severity: "ok".into(),
                line: None,
                message: "Code style looks good.".into(),
            });
        }

        serde_json::to_string(&findings).map_err(|e| format!("Serialization failed: {e}"))
    }
}

struct ReviewCoordinatorTool;

#[async_trait]
impl Tool for ReviewCoordinatorTool {
    fn name(&self) -> &str {
        "coordinate_review"
    }
    fn description(&self) -> &str {
        "Collect all review findings and produce a final summary"
    }

    async fn run(&self, input: &str) -> ToolResult {
        let all_findings: Vec<ReviewFinding> =
            serde_json::from_str(input).map_err(|e| format!("Failed to parse findings: {e}"))?;

        let errors: Vec<_> = all_findings
            .iter()
            .filter(|f| f.severity == "error")
            .collect();
        let warnings: Vec<_> = all_findings
            .iter()
            .filter(|f| f.severity == "warning")
            .collect();
        let approved = errors.is_empty() && warnings.len() <= 3;

        let summary = if approved {
            format!(
                "✓ APPROVED — {} info(s), {} warning(s), {} error(s). All within acceptable thresholds.",
                all_findings.iter().filter(|f| f.severity == "info" || f.severity == "ok").count(),
                warnings.len(),
                errors.len(),
            )
        } else {
            format!(
                "✗ CHANGES REQUESTED — {} error(s), {} warning(s). Please fix before merging.",
                errors.len(),
                warnings.len(),
            )
        };

        serde_json::to_string(&serde_json::json!({
            "approved": approved,
            "summary": summary,
            "total_findings": all_findings.len(),
            "errors": errors.len(),
            "warnings": warnings.len(),
        }))
        .map_err(|e| format!("Serialization failed: {e}"))
    }
}

// ---------------------------------------------------------------------------
// Agent definitions
// ---------------------------------------------------------------------------

async fn run_linter_agent(
    bus: Arc<InMemoryBus>,
    submission: CodeSubmission,
) -> anyhow::Result<Vec<ReviewFinding>> {
    let agent = AgentBuilder::new("linter")
        .name("Linter Agent")
        .capability("lint")
        .tool(Box::new(LinterTool))
        .spawn()
        .await?;

    tracing::info!("{} analyzing {}...", agent.name, submission.file);

    let input = serde_json::to_string(&submission)?;
    let tool = LinterTool;
    let result = tool.run(&input).await.map_err(|e| anyhow::anyhow!(e))?;
    let findings: Vec<ReviewFinding> = serde_json::from_str(&result)?;

    let env = AgentEnvelope::new(
        &agent.id,
        "coordinator",
        "review.linter",
        serde_json::to_vec(&findings)?,
    );
    bus.publish(env).await?;

    Ok(findings)
}

async fn run_style_checker_agent(
    bus: Arc<InMemoryBus>,
    submission: CodeSubmission,
) -> anyhow::Result<Vec<ReviewFinding>> {
    let agent = AgentBuilder::new("style-checker")
        .name("Style Checker Agent")
        .capability("style_check")
        .tool(Box::new(StyleCheckerTool))
        .spawn()
        .await?;

    tracing::info!("{} checking style of {}...", agent.name, submission.file);

    let input = serde_json::to_string(&submission)?;
    let tool = StyleCheckerTool;
    let result = tool.run(&input).await.map_err(|e| anyhow::anyhow!(e))?;
    let findings: Vec<ReviewFinding> = serde_json::from_str(&result)?;

    let env = AgentEnvelope::new(
        &agent.id,
        "coordinator",
        "review.style",
        serde_json::to_vec(&findings)?,
    );
    bus.publish(env).await?;

    Ok(findings)
}

async fn run_reviewer_agent(
    bus: Arc<InMemoryBus>,
    submission: &CodeSubmission,
    linter_findings: &[ReviewFinding],
    style_findings: &[ReviewFinding],
) -> anyhow::Result<()> {
    let coordinator = AgentBuilder::new("coordinator")
        .name("Review Coordinator Agent")
        .capability("coordinate_review")
        .tool(Box::new(ReviewCoordinatorTool))
        .spawn()
        .await?;

    tracing::info!(
        "{} finalizing review for {}...",
        coordinator.name,
        submission.file
    );

    let all_findings: Vec<ReviewFinding> = linter_findings
        .iter()
        .chain(style_findings.iter())
        .cloned()
        .collect();

    let tool = ReviewCoordinatorTool;
    let input = serde_json::to_string(&all_findings)?;
    let result = tool.run(&input).await.map_err(|e| anyhow::anyhow!(e))?;
    let review_result: serde_json::Value = serde_json::from_str(&result)?;

    let approved = review_result["approved"].as_bool().unwrap_or(false);

    // Print the beautiful review report
    println!("\n{}", "=".repeat(60));
    println!("  CODE REVIEW REPORT");
    println!("{}", "=".repeat(60));
    println!("  File:    {}", submission.file);
    println!("  Author:  {}", submission.author);
    println!(
        "  Status:  {}",
        if approved {
            "✓ APPROVED"
        } else {
            "✗ CHANGES REQUESTED"
        }
    );
    println!("{}", "-".repeat(60));

    for finding in &all_findings {
        let icon = match finding.severity.as_str() {
            "error" => "✗",
            "warning" => "⚠",
            "info" => "ℹ",
            "style" => "🎨",
            _ => "·",
        };
        let line_str = finding.line.map(|l| format!(":{}", l)).unwrap_or_default();
        println!("  {icon} [{}{line_str}] {}", finding.agent, finding.message);
    }

    println!("{}", "-".repeat(60));
    println!("  {}", review_result["summary"].as_str().unwrap_or(""));
    println!("{}", "=".repeat(60));

    // Publish final review to bus
    let review = CodeReview {
        file: submission.file.clone(),
        author: submission.author.clone(),
        findings: all_findings,
        approved,
        summary: review_result["summary"].as_str().unwrap_or("").to_string(),
    };

    let env = AgentEnvelope::new(
        &coordinator.id,
        "*",
        "review.complete",
        serde_json::to_vec(&review)?,
    );
    bus.publish(env).await?;

    Ok(())
}

// ---------------------------------------------------------------------------
// Main
// ---------------------------------------------------------------------------

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt::init();

    // Shared in-memory bus
    let bus = Arc::new(InMemoryBus::new());

    // Sample code to review
    let submission = CodeSubmission {
        file: "src/calculator.rs".into(),
        author: "alice".into(),
        code: r#"
fn add(a: i32, b: i32) -> i32 {
    a + b
}

fn complex_calculation(x: f64, y: f64, z: f64, w: f64, v: f64, u: f64) -> f64 {
    // This line is intentionally too long to test the linter
    println!("calculating...");
    (x * y) / (z + w) * (v - u)
}

fn main() {
    let CamelCase = 42;
    dbg!(add(1, 2));
    let result = complex_calculation(1.0, 2.0, 3.0, 4.0, 5.0, 6.0);
    println!("Result: {result}");
}"#
        .trim()
        .into(),
    };

    println!(
        "🚀 Starting multi-agent code review for {} by {}...\n",
        submission.file, submission.author
    );

    // Run linter and style checker in parallel
    let (linter_findings, style_findings) = tokio::join!(
        run_linter_agent(bus.clone(), submission.clone()),
        run_style_checker_agent(bus.clone(), submission.clone()),
    );

    let linter_findings = linter_findings?;
    let style_findings = style_findings?;

    // Coordinator produces final review
    run_reviewer_agent(bus.clone(), &submission, &linter_findings, &style_findings).await?;

    // Drain bus to show all messages
    println!("\n📨 Bus messages exchanged:");
    let all_msgs = bus.drain_for("*").await;
    for msg in all_msgs {
        let topic = &msg.topic;
        let size = msg.payload.len();
        println!(
            "  [{topic}] {} → {} ({} bytes)",
            msg.source_agent_id, msg.target_agent_id, size
        );
    }

    Ok(())
}
