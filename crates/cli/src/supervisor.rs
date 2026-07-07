use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use agentos_kernel::{AgentOSSystem, AgentState, RuntimeConfig};
use tokio::sync::Notify;
use tokio::time::sleep;
use tracing::info;

pub async fn supervisor_command(runtime_config_path: &str) -> anyhow::Result<()> {
    let runtime_config = if std::path::Path::new(runtime_config_path).exists() {
        RuntimeConfig::from_file(runtime_config_path)
            .map_err(|e| anyhow::anyhow!("failed to parse runtime config: {e}"))?
    } else {
        let mut cfg = RuntimeConfig::default();
        cfg.apply_env_overrides();
        info!("Using default runtime config");
        cfg
    };

    let system = Arc::new(AgentOSSystem::with_config(runtime_config.clone()));
    let shutdown = Arc::new(Notify::new());

    let shutdown_signal = shutdown.clone();
    tokio::spawn(async move {
        tokio::signal::ctrl_c().await.ok();
        shutdown_signal.notify_waiters();
    });

    println!("[AgentOS Supervisor] real-time agent health monitor (Ctrl+C to exit)");
    println!();

    let mut tick: u64 = 0;

    loop {
        tokio::select! {
            _ = shutdown.notified() => {
                println!();
                println!("[AgentOS Supervisor] stopped.");
                break;
            }
            _ = sleep(Duration::from_secs(3)) => {
                tick += 1;
                render_dashboard(&system, tick).await;
            }
        }
    }

    Ok(())
}

async fn render_dashboard(system: &AgentOSSystem, tick: u64) {
    let agents = system.supervisor.list().await;

    println!("--- poll #{tick} --- {} agent(s) ---", agents.len());

    if agents.is_empty() {
        println!("  (no agents running)");
        return;
    }

    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);

    println!(
        "  {:<24} {:<14} {:<10} {:<8} HEARTBEAT",
        "ID", "STATUS", "RESTARTS", "UPTIME"
    );

    for handle in &agents {
        let state = handle.state().await;
        let status = status_label(&state);
        let restarts = handle.restart_count();
        let hb = handle.last_heartbeat();
        let uptime = if hb > 0 && state.is_running() {
            now.saturating_sub(hb)
        } else {
            0
        };

        let id_display = if handle.id.len() > 24 {
            format!("{}..", &handle.id[..22])
        } else {
            handle.id.clone()
        };

        println!(
            "  {:<24} {:<14} {:<10} {:<8} {}",
            id_display,
            status,
            restarts,
            format_duration(uptime),
            format_heartbeat(hb, now),
        );
    }
}

fn status_label(state: &AgentState) -> &str {
    match state {
        AgentState::Created => "created",
        AgentState::Running => "running",
        AgentState::Stopped => "stopped",
        AgentState::Degraded(_) => "degraded",
        AgentState::Failed(_) => "failed",
    }
}

fn format_duration(secs: u64) -> String {
    if secs < 60 {
        format!("{secs}s")
    } else if secs < 3600 {
        format!("{}m{}s", secs / 60, secs % 60)
    } else {
        format!("{}h{}m", secs / 3600, (secs % 3600) / 60)
    }
}

fn format_heartbeat(hb_ts: u64, now: u64) -> String {
    if hb_ts == 0 {
        "never".to_string()
    } else {
        let ago = now.saturating_sub(hb_ts);
        if ago < 60 {
            format!("{ago}s ago")
        } else if ago < 3600 {
            format!("{}m ago", ago / 60)
        } else {
            format!("{}h ago", ago / 3600)
        }
    }
}
