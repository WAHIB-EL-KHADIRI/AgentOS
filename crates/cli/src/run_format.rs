use std::fmt::Display;

use crate::state::{StateInspectAgentSummary, StoredLogEntry};

pub(crate) fn format_timestamp_ms(timestamp_ms: u64) -> String {
    if timestamp_ms == 0 {
        return "-".to_string();
    }

    match chrono::DateTime::from_timestamp_millis(timestamp_ms as i64) {
        Some(value) => value.format("%Y-%m-%d %H:%M:%S UTC").to_string(),
        None => format!("{timestamp_ms}ms"),
    }
}

pub(crate) fn print_section(title: &str) {
    println!("{title}");
    println!("{}", "-".repeat(title.len().max(32)));
}

pub(crate) fn print_success(message: &str) {
    println!("{} {message}", color_status("[ok]"));
}

pub(crate) fn print_empty_state(message: &str) {
    println!("{} {message}", color_status("[empty]"));
}

pub(crate) fn print_hint(message: &str) {
    println!("hint: {message}");
}

pub(crate) fn print_kv(label: &str, value: impl Display) {
    println!("{label:<18} {value}");
}

pub(crate) fn print_table_header(columns: &[(&str, usize)]) {
    let header = columns
        .iter()
        .map(|(name, width)| table_cell(name, *width))
        .collect::<Vec<_>>()
        .join(" ");
    let width = columns.iter().map(|(_, width)| width + 1).sum::<usize>();
    println!("{header}");
    println!("{}", "-".repeat(width.saturating_sub(1)));
}

pub(crate) fn table_cell(value: &str, width: usize) -> String {
    format!("{:<width$}", truncate_cell(value, width), width = width)
}

pub(crate) fn color_status_cell(status: &str, width: usize) -> String {
    color_status(&table_cell(status, width))
}

pub(crate) fn color_status(value: &str) -> String {
    let lower = value.trim().to_ascii_lowercase();
    let color = if lower.contains("running") || lower.contains("healthy") || lower.contains("[ok]")
    {
        "\x1b[32m"
    } else if lower.contains("failed") || lower.contains("corrupt") || lower.contains("invalid") {
        "\x1b[31m"
    } else if lower.contains("degraded") || lower.contains("warning") {
        "\x1b[33m"
    } else if lower.contains("stopped") || lower.contains("completed") || lower.contains("[empty]")
    {
        "\x1b[36m"
    } else {
        "\x1b[0m"
    };
    format!("{color}{value}\x1b[0m")
}

pub(crate) fn truncate_cell(value: &str, width: usize) -> String {
    if value.chars().count() <= width {
        value.to_string()
    } else if width <= 1 {
        ".".to_string()
    } else {
        format!("{}.", value.chars().take(width - 1).collect::<String>())
    }
}

pub(crate) fn short_checkpoint(checkpoint_id: &str) -> String {
    if checkpoint_id.len() <= 36 {
        checkpoint_id.to_string()
    } else {
        format!("{}...", &checkpoint_id[..35])
    }
}

pub(crate) fn print_logs(agent_id: &str, logs: &[StoredLogEntry]) {
    print_section(&format!("Logs for {agent_id}"));
    print_table_header(&[("TIME", 20), ("EVENT", 14), ("MESSAGE", 62)]);
    for entry in logs {
        println!(
            "{} {} {}",
            table_cell(&format_timestamp_ms(entry.timestamp_ms), 20),
            table_cell(&entry.event_type, 14),
            table_cell(&entry.message, 62)
        );
    }
}

pub(crate) fn format_status_summary(
    statuses: &std::collections::BTreeMap<String, usize>,
) -> String {
    if statuses.is_empty() {
        return "-".to_string();
    }

    statuses
        .iter()
        .map(|(status, count)| format!("{status}={count}"))
        .collect::<Vec<_>>()
        .join(", ")
}

pub(crate) fn format_agent_summary(agent: &Option<StateInspectAgentSummary>) -> String {
    match agent {
        Some(agent) => format!(
            "{} ({}, {}, {})",
            agent.agent_id,
            agent.name,
            agent.status,
            format_timestamp_ms(agent.updated_at_ms)
        ),
        None => "-".to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::truncate_cell;

    #[test]
    fn truncate_cell_keeps_values_within_width() {
        assert_eq!(truncate_cell("short", 8), "short");
        assert_eq!(truncate_cell("exact", 5), "exact");
        assert_eq!(truncate_cell("too long", 5), "too .");
    }

    #[test]
    fn truncate_cell_handles_tiny_widths() {
        assert_eq!(truncate_cell("abc", 1), ".");
        assert_eq!(truncate_cell("abc", 0), ".");
    }
}
