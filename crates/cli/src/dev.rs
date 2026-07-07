use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use anyhow::Context;
use notify::{Config, Event, EventKind, RecommendedWatcher, RecursiveMode, Watcher};
use tokio::sync::Mutex;
use tokio::time::sleep;
use tracing::{error, info, warn};

use agentos_kernel::{AgentConfig, AgentOSSystem, RuntimeConfig};

struct ManagedAgent {
    config_path: PathBuf,
}

struct DevState {
    agents: HashMap<String, ManagedAgent>,
}

impl DevState {
    fn new() -> Self {
        Self {
            agents: HashMap::new(),
        }
    }
}

pub async fn dev_command(
    watch_dir: &str,
    runtime_config_path: &str,
    _watch_patterns: &[String],
) -> anyhow::Result<()> {
    let dir = PathBuf::from(watch_dir);
    if !dir.is_dir() {
        anyhow::bail!("watch path is not a directory: {watch_dir}");
    }

    let runtime_config = if Path::new(runtime_config_path).exists() {
        RuntimeConfig::from_file(runtime_config_path)
            .map_err(|e| anyhow::anyhow!("failed to parse runtime config: {e}"))?
    } else {
        let mut cfg = RuntimeConfig::default();
        cfg.apply_env_overrides();
        info!("Using default runtime config (no {runtime_config_path} found)");
        cfg
    };

    let system = Arc::new(AgentOSSystem::with_config(runtime_config.clone()));
    let state = Arc::new(Mutex::new(DevState::new()));

    let starting = discover_agents(&dir, &state, &system).await?;
    println!(
        "  [dev] {} agent(s) started. Watching for changes...",
        starting
    );
    if starting > 0 {
        let s = state.lock().await;
        for (id, agent) in &s.agents {
            println!("  [ok] {} ({})", id, agent.config_path.display());
        }
    }

    let (tx, rx) = std::sync::mpsc::channel::<Result<Event, notify::Error>>();

    let mut watcher = RecommendedWatcher::new(
        move |res| {
            let _ = tx.send(res);
        },
        Config::default(),
    )
    .context("failed to create file watcher")?;

    watcher
        .watch(&dir, RecursiveMode::Recursive)
        .context("failed to start watching directory")?;

    info!("Watching {} for agent config changes", dir.display());

    let (shutdown_tx, mut shutdown_rx) = tokio::sync::mpsc::channel::<()>(1);

    tokio::spawn(async move {
        tokio::signal::ctrl_c().await.ok();
        info!("Shutdown signal received, stopping dev mode...");
        let _ = shutdown_tx.send(()).await;
    });

    let mut last_content: HashMap<PathBuf, String> = HashMap::new();

    {
        let s = state.lock().await;
        for agent in s.agents.values() {
            if let Ok(content) = tokio::fs::read_to_string(&agent.config_path).await {
                last_content.insert(agent.config_path.clone(), content);
            }
        }
    }

    loop {
        tokio::select! {
            _ = shutdown_rx.recv() => {
                break;
            }
            _ = sleep(Duration::from_millis(250)) => {
                while let Ok(Ok(event)) = rx.try_recv() {
                    process_event(&event, &dir, &state, &system, &mut last_content).await;
                }
                scan_for_changes(&dir, &state, &system, &mut last_content).await;
            }
        }
    }

    info!("Shutting down dev mode, stopping all agents...");
    drop(system);
    info!("All agents stopped. Goodbye!");

    Ok(())
}

fn is_toml(path: &Path) -> bool {
    path.extension()
        .is_some_and(|ext| ext == "toml" || ext == "yaml" || ext == "yml")
}

async fn process_event(
    event: &Event,
    watch_dir: &Path,
    state: &Arc<Mutex<DevState>>,
    system: &Arc<AgentOSSystem>,
    last_content: &mut HashMap<PathBuf, String>,
) {
    match &event.kind {
        EventKind::Create(_) | EventKind::Modify(_) => {
            for path in &event.paths {
                if is_toml(path) && path.starts_with(watch_dir) {
                    if let Ok(content) = tokio::fs::read_to_string(path).await {
                        let changed = last_content.get(path) != Some(&content);
                        if changed {
                            last_content.insert(path.clone(), content);
                            handle_config_change(path, state, system).await;
                        }
                    }
                }
            }
        }
        EventKind::Remove(_) => {
            for path in &event.paths {
                if is_toml(path) && path.starts_with(watch_dir) {
                    last_content.remove(path);
                    handle_config_removal(path, state).await;
                }
            }
        }
        _ => {}
    }
}

async fn handle_config_change(
    config_path: &Path,
    state: &Arc<Mutex<DevState>>,
    system: &Arc<AgentOSSystem>,
) {
    let exists = {
        let s = state.lock().await;
        s.agents.values().any(|a| a.config_path == config_path)
    };

    if exists {
        let agent_id = {
            let s = state.lock().await;
            s.agents
                .iter()
                .find(|(_, a)| a.config_path == config_path)
                .map(|(id, _)| id.clone())
        };

        if let Some(id) = agent_id {
            info!("Config change detected for {id}, restarting...");
            println!("  [..] restarting {id}...");
            let mut s = state.lock().await;
            s.agents.remove(&id);
            drop(s);
            start_agent(config_path, state, system).await;
        }
    } else {
        info!("New config detected: {}", config_path.display());
        println!("  [new] config: {}", config_path.display());
        start_agent(config_path, state, system).await;
    }
}

async fn handle_config_removal(config_path: &Path, state: &Arc<Mutex<DevState>>) {
    let agent_id = {
        let s = state.lock().await;
        s.agents
            .iter()
            .find(|(_, a)| a.config_path == config_path)
            .map(|(id, _)| id.clone())
    };

    if let Some(id) = agent_id {
        info!("Config removed for {id}, stopping agent");
        println!("  [--] stopped {id} (config removed)");
        let mut s = state.lock().await;
        s.agents.remove(&id);
    }
}

async fn discover_agents(
    dir: &Path,
    state: &Arc<Mutex<DevState>>,
    system: &Arc<AgentOSSystem>,
) -> anyhow::Result<usize> {
    let mut entries = Vec::new();
    let mut read_dir = tokio::fs::read_dir(dir).await?;
    while let Some(entry) = read_dir.next_entry().await? {
        let path = entry.path();
        if is_toml(&path) {
            entries.push(path);
        }
    }

    let mut count = 0usize;
    for config_path in entries {
        if start_agent(&config_path, state, system).await {
            count += 1;
        }
    }

    Ok(count)
}

async fn start_agent(
    config_path: &Path,
    state: &Arc<Mutex<DevState>>,
    system: &Arc<AgentOSSystem>,
) -> bool {
    let config = match AgentConfig::from_file(config_path.to_str().unwrap_or("")) {
        Ok(c) => c,
        Err(e) => {
            warn!("Skipping {}: {e}", config_path.display());
            return false;
        }
    };

    let agent_id = format!("agent_{}", config.name.replace('-', "_"));

    {
        let s = state.lock().await;
        if s.agents.contains_key(&agent_id) {
            return false;
        }
    }

    let spec = config.into_spec(agent_id.clone());

    match system.spawn_agent(spec).await {
        Ok(_handle) => {
            info!("Started agent {agent_id} from {}", config_path.display());
            let mut s = state.lock().await;
            s.agents.insert(
                agent_id.clone(),
                ManagedAgent {
                    config_path: config_path.to_path_buf(),
                },
            );
            println!("  [ok] {} ({})", agent_id, config_path.display());
            true
        }
        Err(e) => {
            error!("Failed to start agent {agent_id}: {e}");
            false
        }
    }
}

async fn scan_for_changes(
    dir: &Path,
    state: &Arc<Mutex<DevState>>,
    system: &Arc<AgentOSSystem>,
    last_content: &mut HashMap<PathBuf, String>,
) {
    let mut current_tomls = Vec::new();
    if let Ok(mut read_dir) = tokio::fs::read_dir(dir).await {
        while let Ok(Some(entry)) = read_dir.next_entry().await {
            let path = entry.path();
            if is_toml(&path) {
                current_tomls.push(path);
            }
        }
    }

    for path in &current_tomls {
        if let Ok(content) = tokio::fs::read_to_string(path).await {
            let changed = last_content.get(path) != Some(&content);

            if changed {
                last_content.insert(path.clone(), content);
                handle_config_change(path, state, system).await;
            }
        }
    }

    let known_paths: Vec<PathBuf> = {
        let s = state.lock().await;
        s.agents.values().map(|a| a.config_path.clone()).collect()
    };

    for config_path in known_paths {
        if !config_path.exists() {
            last_content.remove(&config_path);
            handle_config_removal(&config_path, state).await;
        }
    }
}
