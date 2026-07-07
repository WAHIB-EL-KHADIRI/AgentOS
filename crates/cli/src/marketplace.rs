use std::collections::HashMap;
use std::path::{Path, PathBuf};

use anyhow::Context;
use serde::{Deserialize, Serialize};
use tracing::info;

#[derive(Debug, Serialize, Deserialize)]
struct InstalledPlugin {
    name: String,
    version: String,
    source: String,
    installed_at: String,
    path: String,
}

#[derive(Debug, Serialize, Deserialize, Default)]
struct RegistryState {
    plugins: HashMap<String, InstalledPlugin>,
}

fn registry_path() -> anyhow::Result<PathBuf> {
    let home = home_dir().context("cannot determine home directory")?;
    let dir = home.join(".agentos");
    Ok(dir.join("registry.json"))
}

fn home_dir() -> Option<PathBuf> {
    #[cfg(target_os = "windows")]
    {
        std::env::var("USERPROFILE")
            .ok()
            .map(PathBuf::from)
            .or_else(|| {
                let drive = std::env::var("HOMEDRIVE").unwrap_or_default();
                let path = std::env::var("HOMEPATH").unwrap_or_default();
                let full = format!("{drive}{path}");
                if full.is_empty() {
                    None
                } else {
                    Some(PathBuf::from(full))
                }
            })
    }
    #[cfg(not(target_os = "windows"))]
    {
        std::env::var("HOME").ok().map(PathBuf::from)
    }
}

fn load_registry() -> anyhow::Result<RegistryState> {
    let path = registry_path()?;
    if path.exists() {
        let content = std::fs::read_to_string(&path)?;
        Ok(serde_json::from_str(&content)?)
    } else {
        Ok(RegistryState::default())
    }
}

fn save_registry(state: &RegistryState) -> anyhow::Result<()> {
    let path = registry_path()?;
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let content = serde_json::to_string_pretty(state)?;
    std::fs::write(&path, content)?;
    Ok(())
}

pub async fn marketplace_install_command(name: &str, path: &str) -> anyhow::Result<()> {
    let mut registry = load_registry()?;

    if registry.plugins.contains_key(name) {
        anyhow::bail!("plugin '{name}' is already installed");
    }

    let plugin_dir = Path::new(path).join(format!("agentos-plugin-{name}"));
    std::fs::create_dir_all(&plugin_dir)?;

    let now = chrono::Utc::now().to_rfc3339();

    let plugin = InstalledPlugin {
        name: name.to_string(),
        version: "0.1.0".to_string(),
        source: "scaffolded".to_string(),
        installed_at: now,
        path: plugin_dir.display().to_string(),
    };

    registry.plugins.insert(name.to_string(), plugin);
    save_registry(&registry)?;

    info!("Plugin '{name}' installed at {}", plugin_dir.display());
    println!("  [ok] installed plugin '{name}'");
    println!("  path:  {}", plugin_dir.display());
    println!("  next:  cd {}", plugin_dir.display());
    println!("         cargo build --target wasm32-wasip1 --release");

    Ok(())
}

pub async fn marketplace_uninstall_command(name: &str) -> anyhow::Result<()> {
    let mut registry = load_registry()?;

    let plugin = registry
        .plugins
        .remove(name)
        .ok_or_else(|| anyhow::anyhow!("plugin '{name}' is not installed"))?;

    save_registry(&registry)?;
    info!("Plugin '{name}' uninstalled");
    println!("  [ok] uninstalled plugin '{name}'");
    println!("  path:  {}", plugin.path);
    println!("  note:  you may want to remove the directory manually");

    Ok(())
}

pub async fn marketplace_search_command(query: &str) -> anyhow::Result<()> {
    let registry = load_registry()?;

    let local_matches: Vec<&InstalledPlugin> = registry
        .plugins
        .values()
        .filter(|p| p.name.contains(query))
        .collect();

    println!("Search results for '{query}':");
    println!();
    println!("  {} local installed plugin(s)", local_matches.len());
    println!();

    for plugin in &local_matches {
        println!("  {:<30} v{}  {}", plugin.name, plugin.version, plugin.path);
    }

    // Built-in template references
    let builtins = [
        "basic-agent",
        "research-agent",
        "coding-agent",
        "security-agent",
        "ops-agent",
        "api-server",
        "slack-bot",
        "web-scraper",
    ];

    let builtin_matches: Vec<&&str> = builtins.iter().filter(|b| b.contains(query)).collect();
    if !builtin_matches.is_empty() {
        println!();
        println!(
            "  {} template(s) available via `agentOS init-agent`",
            builtin_matches.len()
        );
        for t in &builtin_matches {
            println!(
                "    agentOS init-agent --name {} --template {t}",
                t.replace('-', "_")
            );
        }
    }

    if local_matches.is_empty() && builtin_matches.is_empty() {
        println!("  No results found.");
    }

    Ok(())
}

pub async fn marketplace_list_command() -> anyhow::Result<()> {
    let registry = load_registry()?;

    if registry.plugins.is_empty() {
        println!("  No plugins installed.");
        println!("  Use `agentOS marketplace install <name>` to install one.");
        return Ok(());
    }

    println!("Installed plugins:");
    println!();
    println!(
        "{:<30} {:<10} {:<30} {:<20}",
        "NAME", "VERSION", "PATH", "INSTALLED AT"
    );
    println!("{}", "-".repeat(95));

    for plugin in registry.plugins.values() {
        println!(
            "{:<30} {:<10} {:<30} {:<20}",
            plugin.name,
            plugin.version,
            if plugin.path.len() > 29 {
                format!("{}...", &plugin.path[..26])
            } else {
                plugin.path.clone()
            },
            &plugin.installed_at[..10],
        );
    }

    Ok(())
}
