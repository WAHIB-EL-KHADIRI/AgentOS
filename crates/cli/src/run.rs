use std::path::Path;
use std::sync::Arc;

use agentos_bus::{grpc, SseEvent};
use agentos_kernel::{AgentConfig, AgentOSSystem, HealthServer, LifecycleEvent, RuntimeConfig};
use agentos_trace::TraceReplayer;
use tracing::info;

use crate::run_format::*;
use crate::sse_bridge::{agent_info_json, agent_status_label, DashboardSseBridge};
use crate::state::{
    append_checkpoint, append_log, checkpoints_count_for_agent, current_time_millis, doctor_state,
    export_state, has_agent, import_state, inspect_state, list_agents, load_state,
    logs_count_for_agent, logs_for_agent, migrate_state, new_checkpoint, new_log_entry,
    parse_duration_ms, save_state, trace_containing_checkpoint, trace_for_agent, upsert_agent,
    StateBackendKind, StateCleanOptions, StateDoctorStatus, StateImportMode,
};
use crate::{AgentTemplate, GraphFormat, OutputFormat};

pub async fn run_command(agent_path: &str, runtime_config_path: &str) -> anyhow::Result<()> {
    // Load runtime config (or use defaults)
    let runtime_config = if std::path::Path::new(runtime_config_path).exists() {
        RuntimeConfig::from_file(runtime_config_path)
            .map_err(|e| anyhow::anyhow!("failed to parse runtime config: {e}"))?
    } else {
        let mut cfg = RuntimeConfig::default();
        cfg.apply_env_overrides();
        cfg
    };

    // Load agent config (auto-detect .toml, .yaml, .yml)
    let agent_config = AgentConfig::from_file(agent_path)
        .map_err(|e| anyhow::anyhow!("failed to parse agent config: {e}"))?;

    let agent_id = format!("agent_{}", agent_config.name.replace('-', "_"));
    let agent_name = agent_config.name.clone();
    let spec = agent_config.into_spec(agent_id.clone());
    let capabilities = spec.capabilities.clone();
    let started_at = current_time_millis();

    // Create the full AgentOS system
    let system = Arc::new(AgentOSSystem::with_config(runtime_config.clone()));

    // SSE event stream for the dashboard. The bridge subscribes before the
    // agent is spawned so the AgentSpawned event is forwarded too.
    let (sse_tx, sse_rx) = tokio::sync::broadcast::channel::<SseEvent>(1024);
    let bridge = DashboardSseBridge::new(
        sse_tx.clone(),
        agent_id.clone(),
        agent_name.clone(),
        capabilities.clone(),
        started_at,
    );
    let trace_counter = bridge.trace_counter();
    system.event_bus.subscribe(Box::new(bridge)).await;

    let sse_addr: std::net::SocketAddr = runtime_config
        .sse_addr()
        .parse()
        .map_err(|e| anyhow::anyhow!("invalid SSE address: {e}"))?;
    let sse_handle = tokio::spawn(async move {
        agentos_bus::start_sse_server(sse_addr, sse_rx).await;
    });

    let agent_handle = system.spawn_agent(spec).await?;

    // Dashboards that connect after spawn missed the agent_started event;
    // a periodic agent_status frame lets them converge on current state.
    let status_handle = {
        let status_tx = sse_tx.clone();
        let agent_handle = agent_handle.clone();
        let agent_id = agent_id.clone();
        let agent_name = agent_name.clone();
        let capabilities = capabilities.clone();
        tokio::spawn(async move {
            let mut ticker = tokio::time::interval(std::time::Duration::from_secs(5));
            ticker.tick().await;
            loop {
                ticker.tick().await;
                let state = agent_handle.state().await;
                let _ = status_tx.send(SseEvent::named(
                    "agent_status",
                    agent_info_json(
                        &agent_id,
                        &agent_name,
                        agent_status_label(&state),
                        &capabilities,
                        started_at,
                        trace_counter.load(std::sync::atomic::Ordering::Relaxed),
                    ),
                ));
            }
        })
    };

    info!(
        agent_id = %agent_id,
        host = %runtime_config.host,
        http_port = %runtime_config.http_port,
        grpc_port = %runtime_config.grpc_port,
        sse_port = %runtime_config.sse_port,
        "AgentOS runtime started"
    );

    // Start health server
    let health_addr: std::net::SocketAddr =
        format!("{}:{}", runtime_config.host, runtime_config.http_port)
            .parse()
            .map_err(|e| anyhow::anyhow!("invalid health address: {e}"))?;
    let health_server = HealthServer::new(Arc::clone(&system), health_addr);
    let health_handle = tokio::spawn(async move {
        health_server.start().await;
    });

    // Start gRPC bus server
    let grpc_addr: std::net::SocketAddr =
        format!("{}:{}", runtime_config.host, runtime_config.grpc_port)
            .parse()
            .map_err(|e| anyhow::anyhow!("invalid gRPC address: {e}"))?;
    let bus = system.bus.clone() as Arc<dyn agentos_bus::AgentBusTrait + Send + Sync>;
    let grpc_handle = tokio::spawn(async move {
        grpc::start_grpc_server(grpc_addr, bus).await;
    });

    let mut cli_state = load_state()?;
    upsert_agent(
        &mut cli_state,
        &agent_id,
        &agent_name,
        agent_path,
        "running",
        started_at,
    );
    append_log(
        &mut cli_state,
        &agent_id,
        new_log_entry(
            "spawned",
            format!("Agent '{agent_name}' started"),
            started_at,
        ),
    );
    let start_checkpoint = append_checkpoint(
        &mut cli_state,
        &agent_id,
        new_checkpoint(
            &agent_id,
            format!("Agent '{agent_name}' spawned"),
            started_at,
        ),
    );

    // A failed LLM step must not take the runtime down: the agent stays
    // supervised and the failure is recorded instead of propagated.
    let execution_checkpoint = if system.has_llm_provider().await {
        match system
            .run_agent_once(&agent_id, "Begin executing the configured agent prompt.")
            .await
        {
            Ok(step) => {
                let executed_at = current_time_millis();
                append_log(
                    &mut cli_state,
                    &agent_id,
                    new_log_entry(
                        "llm_response",
                        format!(
                            "{}:{} finished {}",
                            step.provider, step.model, step.finish_reason
                        ),
                        executed_at,
                    ),
                );
                Some(append_checkpoint(
                    &mut cli_state,
                    &agent_id,
                    new_checkpoint(
                        &agent_id,
                        format!(
                            "LLM response ({}:{}): {}",
                            step.provider, step.model, step.content
                        ),
                        executed_at,
                    ),
                ))
            }
            Err(error) => {
                tracing::warn!(agent_id = %agent_id, %error, "LLM execution step failed; continuing without it");
                eprintln!("warning: LLM execution step failed: {error}");
                eprintln!("         the agent keeps running; check your provider credentials");
                append_log(
                    &mut cli_state,
                    &agent_id,
                    new_log_entry(
                        "llm_error",
                        format!("LLM execution step failed: {error}"),
                        current_time_millis(),
                    ),
                );
                None
            }
        }
    } else {
        None
    };

    save_state(&cli_state)?;

    println!("AgentOS runtime is live");
    println!(
        "  http:       {}:{}",
        runtime_config.host, runtime_config.http_port
    );
    println!(
        "  grpc:       {}:{}",
        runtime_config.host, runtime_config.grpc_port
    );
    println!(
        "  sse:        http://{}:{}/events",
        runtime_config.host, runtime_config.sse_port
    );
    println!("  agent id:   {agent_id}");
    let st = agent_handle.state().await;
    let status_str = if st.is_running() {
        "running"
    } else {
        "not running"
    };
    println!("  status:     {status_str}");
    println!("  trace:      {start_checkpoint}");
    if let Some(checkpoint) = &execution_checkpoint {
        println!("  execution:  {checkpoint}");
    }
    println!("  (press Ctrl+C to stop)");

    let lifecycle_result = monitor_lifecycle(Arc::clone(&system), agent_id.clone()).await;

    // Graceful shutdown
    info!("shutting down AgentOS runtime");
    system.shutdown_all().await;

    let finished_at = current_time_millis();
    let mut cli_state = load_state()?;
    let (final_state, event_type, reason) = match &lifecycle_result {
        LifecycleExit::Stopped => ("stopped", "stopped", "normal shutdown".to_string()),
        LifecycleExit::Failed(reason) => ("failed", "failed", reason.clone()),
    };
    upsert_agent(
        &mut cli_state,
        &agent_id,
        &agent_name,
        agent_path,
        final_state,
        finished_at,
    );
    append_log(
        &mut cli_state,
        &agent_id,
        new_log_entry(
            event_type,
            format!("Agent '{agent_name}' {final_state}: {reason}"),
            finished_at,
        ),
    );
    let stop_checkpoint = append_checkpoint(
        &mut cli_state,
        &agent_id,
        new_checkpoint(
            &agent_id,
            format!("Agent '{agent_name}' {final_state}: {reason}"),
            finished_at,
        ),
    );
    save_state(&cli_state)?;

    // Abort server handles
    health_handle.abort();
    grpc_handle.abort();
    sse_handle.abort();
    status_handle.abort();

    println!("agent '{agent_id}' stopped");
    println!("  final status: {:?}", agent_handle.state().await);
    println!("  trace:        {stop_checkpoint}");

    match lifecycle_result {
        LifecycleExit::Stopped => {
            println!("  reason: normal shutdown");
            Ok(())
        }
        LifecycleExit::Failed(reason) => {
            println!("  reason: failed - {reason}");
            Ok(())
        }
    }
}

pub async fn status_command(format: OutputFormat) -> anyhow::Result<()> {
    let system = AgentOSSystem::new();
    let agents = system.supervisor.list().await;
    let logs = system.get_all_logs().await;

    if format == OutputFormat::Json {
        println!(
            "{}",
            serde_json::json!({
                "version": env!("CARGO_PKG_VERSION"),
                "host": system.config.host,
                "http": system.config.listen_addr(),
                "grpc": system.config.grpc_addr(),
                "max_agents": system.config.max_agents,
                "data_dir": system.config.data_dir,
                "agents": agents.len(),
                "log_entries": logs.len(),
                "components": {
                    "kernel": "ready",
                    "bus": "in-memory",
                    "trace": "ready",
                    "memory": "in-memory",
                    "vault": "ready",
                    "registry": "ready"
                }
            })
        );
        return Ok(());
    }

    println!("AgentOS local runtime status");
    println!("{}", "-".repeat(32));
    println!("version:       {}", env!("CARGO_PKG_VERSION"));
    println!("host:          {}", system.config.host);
    println!("http:          {}", system.config.listen_addr());
    println!("grpc:          {}", system.config.grpc_addr());
    println!("max agents:    {}", system.config.max_agents);
    println!("data dir:      {}", system.config.data_dir);
    println!("agents:        {}", agents.len());
    println!("log entries:   {}", logs.len());
    println!("components:");
    println!("  kernel:      ready");
    println!("  bus:         in-memory");
    println!("  trace:       ready");
    println!("  memory:      in-memory");
    println!("  vault:       ready");
    println!("  registry:    ready");

    Ok(())
}

pub async fn quickstart_command(
    format: OutputFormat,
    path: &str,
    template: AgentTemplate,
) -> anyhow::Result<()> {
    let agent_name = format!("{}-agent", template_name(template));
    let agent_path = format!("{path}/agents/{agent_name}.toml");
    let runtime_path = format!("{path}/agentos.toml");
    let steps = [
        QuickstartStep::new("Check the local workspace", "agentOS doctor"),
        QuickstartStep::new(
            "Create a starter AgentOS workspace",
            format!(
                "agentOS init-workspace --path {path} --template {} --agent-name {agent_name}",
                template_name(template)
            ),
        ),
        QuickstartStep::new(
            "Inspect the generated runtime config",
            format!("agentOS inspect-runtime --config {runtime_path}"),
        ),
        QuickstartStep::new(
            "Inspect the generated agent config",
            format!("agentOS inspect-config --agent {agent_path} --strict-capabilities"),
        ),
        QuickstartStep::new(
            "Run the starter agent",
            format!("agentOS run --agent {agent_path}"),
        ),
    ];

    if format == OutputFormat::Json {
        let values = steps
            .iter()
            .enumerate()
            .map(|(index, step)| {
                serde_json::json!({
                    "index": index + 1,
                    "title": step.title,
                    "command": step.command,
                })
            })
            .collect::<Vec<_>>();
        println!(
            "{}",
            serde_json::json!({
                "path": path,
                "template": template_name(template),
                "steps": values,
            })
        );
        return Ok(());
    }

    println!("AgentOS quickstart");
    println!("{}", "-".repeat(32));
    println!("workspace: {path}");
    println!("template:  {}", template_name(template));
    println!();

    for (index, step) in steps.iter().enumerate() {
        println!("{}. {}", index + 1, step.title);
        println!("   {}", step.command);
    }

    Ok(())
}

pub async fn docs_command(format: OutputFormat) -> anyhow::Result<()> {
    let docs = docs_catalog();

    if format == OutputFormat::Json {
        let values = docs
            .iter()
            .map(|doc| {
                serde_json::json!({
                    "path": doc.path,
                    "title": doc.title,
                    "description": doc.description,
                })
            })
            .collect::<Vec<_>>();
        println!("{}", serde_json::json!({ "docs": values }));
        return Ok(());
    }

    println!("AgentOS documentation");
    println!("{}", "-".repeat(32));
    for doc in docs {
        println!("{}", doc.title);
        println!("  path: {}", doc.path);
        println!("  {}", doc.description);
        println!();
    }

    Ok(())
}

pub async fn check_links_command(root_path: &str, format: OutputFormat) -> anyhow::Result<()> {
    validate_cli_path(root_path)?;
    let root = Path::new(root_path);
    let markdown_files = collect_markdown_files(root)?;
    let mut links_checked = 0usize;
    let mut broken_links = Vec::new();

    for file in &markdown_files {
        let content = std::fs::read_to_string(file)?;
        for link in extract_markdown_links(&content) {
            if should_skip_link(&link) {
                continue;
            }

            links_checked += 1;
            let target = normalize_markdown_link(&link);
            if target.is_empty() {
                continue;
            }

            let resolved = file
                .parent()
                .unwrap_or(root)
                .join(target.replace('/', std::path::MAIN_SEPARATOR_STR));

            if !resolved.exists() {
                broken_links.push(BrokenLink {
                    file: file.display().to_string(),
                    target: link,
                });
            }
        }
    }

    if format == OutputFormat::Json {
        let broken = broken_links
            .iter()
            .map(|link| {
                serde_json::json!({
                    "file": link.file,
                    "target": link.target,
                })
            })
            .collect::<Vec<_>>();
        println!(
            "{}",
            serde_json::json!({
                "root": root_path,
                "files_scanned": markdown_files.len(),
                "links_checked": links_checked,
                "broken_count": broken_links.len(),
                "broken": broken,
            })
        );
    } else {
        println!("AgentOS markdown link check");
        println!("{}", "-".repeat(32));
        println!("root:          {root_path}");
        println!("files scanned: {}", markdown_files.len());
        println!("links checked: {links_checked}");
        println!("broken links:  {}", broken_links.len());

        if !broken_links.is_empty() {
            println!();
            for broken in &broken_links {
                println!("[broken] {} -> {}", broken.file, broken.target);
            }
        }
    }

    if !broken_links.is_empty() {
        anyhow::bail!("{} broken Markdown link(s)", broken_links.len());
    }

    Ok(())
}

pub async fn doctor_command(format: OutputFormat) -> anyhow::Result<()> {
    let checks = [
        check_path("Cargo.toml", "workspace manifest"),
        check_path("README.md", "project README"),
        check_path("docs/architecture.md", "architecture docs"),
        check_path("docs/security-model.md", "security model"),
        check_path("examples/simple_agent.toml", "sample agent config"),
        check_path("crates/bus/proto/agent_bus.proto", "bus protocol"),
    ];

    let failed = checks.iter().filter(|check| !check.ok).count();

    let mut config = RuntimeConfig::default();
    config.apply_env_overrides();

    if format == OutputFormat::Json {
        let checks_json = checks
            .iter()
            .map(|check| {
                serde_json::json!({
                    "name": check.name,
                    "detail": check.detail,
                    "ok": check.ok,
                })
            })
            .collect::<Vec<_>>();

        println!(
            "{}",
            serde_json::json!({
                "status": if failed == 0 { "ok" } else { "failed" },
                "failed_checks": failed,
                "checks": checks_json,
                "runtime": {
                    "listen": config.listen_addr(),
                    "grpc": config.grpc_addr(),
                    "max_agents": config.max_agents,
                    "data_dir": config.data_dir,
                }
            })
        );

        if failed > 0 {
            anyhow::bail!("{failed} doctor check(s) failed");
        }

        return Ok(());
    }

    println!("AgentOS doctor");
    println!("{}", "-".repeat(32));

    for check in &checks {
        if check.ok {
            println!("[ok]   {:<22} {}", check.name, check.detail);
        } else {
            println!("[miss] {:<22} {}", check.name, check.detail);
        }
    }

    println!();
    println!("runtime defaults");
    println!("  listen:       {}", config.listen_addr());
    println!("  grpc:         {}", config.grpc_addr());
    println!("  max agents:   {}", config.max_agents);
    println!("  data dir:     {}", config.data_dir);

    if failed > 0 {
        anyhow::bail!("{failed} doctor check(s) failed");
    }

    println!();
    println!("AgentOS workspace looks ready.");
    Ok(())
}

pub async fn env_command(format: OutputFormat) -> anyhow::Result<()> {
    let mut config = RuntimeConfig::default();
    config.apply_env_overrides();

    let http_port = config.http_port.to_string();
    let grpc_port = config.grpc_port.to_string();
    let sse_port = config.sse_port.to_string();
    let max_agents = config.max_agents.to_string();
    let vars = [
        EnvVar::new("AGENTOS_HOST", &config.host, "Runtime bind host"),
        EnvVar::new("AGENTOS_HTTP_PORT", &http_port, "HTTP health and API port"),
        EnvVar::new("AGENTOS_GRPC_PORT", &grpc_port, "gRPC bus port"),
        EnvVar::new("AGENTOS_SSE_PORT", &sse_port, "SSE event stream port"),
        EnvVar::new(
            "AGENTOS_MAX_AGENTS",
            &max_agents,
            "Maximum supervised agents",
        ),
        EnvVar::new("AGENTOS_LOG_LEVEL", &config.log_level, "Runtime log level"),
        EnvVar::new(
            "AGENTOS_DATA_DIR",
            &config.data_dir,
            "Runtime data directory",
        ),
    ];

    if format == OutputFormat::Json {
        let values = vars
            .iter()
            .map(|var| {
                serde_json::json!({
                    "name": var.name,
                    "value": var.value,
                    "description": var.description,
                    "is_set": std::env::var(var.name).is_ok(),
                })
            })
            .collect::<Vec<_>>();
        println!("{}", serde_json::json!({ "env": values }));
        return Ok(());
    }

    println!("AgentOS environment");
    println!("{}", "-".repeat(32));
    for var in &vars {
        let marker = if std::env::var(var.name).is_ok() {
            "set"
        } else {
            "default"
        };
        println!(
            "{:<22} {:<8} {:<18} {}",
            var.name, marker, var.value, var.description
        );
    }

    println!();
    println!("PowerShell example:");
    println!("  $env:AGENTOS_LOG_LEVEL='debug'");
    println!("  $env:AGENTOS_HTTP_PORT='9090'");

    Ok(())
}

pub async fn capabilities_command(format: OutputFormat) -> anyhow::Result<()> {
    let capabilities = capability_catalog();

    if format == OutputFormat::Json {
        let values = capabilities
            .iter()
            .map(|capability| {
                serde_json::json!({
                    "name": capability.name,
                    "category": capability.category,
                    "description": capability.description,
                })
            })
            .collect::<Vec<_>>();
        println!("{}", serde_json::json!({ "capabilities": values }));
        return Ok(());
    }

    println!("AgentOS capabilities");
    println!("{}", "-".repeat(32));
    println!("{:<22} {:<12} DESCRIPTION", "NAME", "CATEGORY");
    println!("{}", "-".repeat(72));
    for capability in capabilities {
        println!(
            "{:<22} {:<12} {}",
            capability.name, capability.category, capability.description
        );
    }

    println!();
    println!("Example:");
    println!("  agentOS init-agent --name research-agent \\");
    println!("    --capability memory_write --capability trace_record");

    Ok(())
}

pub async fn templates_command(format: OutputFormat) -> anyhow::Result<()> {
    let templates = [
        AgentTemplate::Basic,
        AgentTemplate::Research,
        AgentTemplate::Coding,
        AgentTemplate::Security,
        AgentTemplate::Ops,
    ];

    if format == OutputFormat::Json {
        let values = templates
            .iter()
            .map(|template| {
                serde_json::json!({
                    "name": template_name(*template),
                    "prompt": template_prompt(*template),
                    "capabilities": template_capabilities(*template),
                })
            })
            .collect::<Vec<_>>();
        println!("{}", serde_json::json!({ "templates": values }));
        return Ok(());
    }

    println!("AgentOS agent templates");
    println!("{}", "-".repeat(32));
    for template in templates {
        println!("{}", template_name(template));
        println!("  prompt: {}", template_prompt(template));
        println!("  capabilities:");
        for capability in template_capabilities(template) {
            println!("    - {capability}");
        }
        println!(
            "  create: agentOS init-agent --name {}-agent --template {} --output agents/{}.toml",
            template_name(template),
            template_name(template),
            template_name(template)
        );
        println!();
    }

    Ok(())
}

pub async fn inspect_config_command(
    agent_path: &str,
    format: OutputFormat,
    strict_capabilities: bool,
) -> anyhow::Result<()> {
    let config = AgentConfig::from_toml(agent_path)
        .map_err(|e| anyhow::anyhow!("failed to parse agent config: {e}"))?;
    let unknown = unknown_capabilities(&config.capabilities);
    let known_count = config.capabilities.len().saturating_sub(unknown.len());

    if strict_capabilities && !unknown.is_empty() {
        anyhow::bail!(
            "unknown capabilities in '{}': {}. Run `agentOS capabilities` to see supported names",
            agent_path,
            unknown.join(", ")
        );
    }

    if format == OutputFormat::Json {
        println!(
            "{}",
            serde_json::json!({
                "path": agent_path,
                "name": config.name,
                "prompt_chars": config.prompt.chars().count(),
                "capabilities": config.capabilities,
                "known_capabilities": known_count,
                "unknown_capabilities": unknown,
            })
        );
        return Ok(());
    }

    println!("Agent config");
    println!("{}", "-".repeat(32));
    println!("path:          {agent_path}");
    println!("name:          {}", config.name);
    println!("prompt chars:  {}", config.prompt.chars().count());
    println!("capabilities:  {}", config.capabilities.len());
    println!("known:         {known_count}");
    println!("unknown:       {}", unknown.len());

    if config.capabilities.is_empty() {
        println!("  - none");
    } else {
        for capability in &config.capabilities {
            println!("  - {capability}");
        }
    }

    if !unknown.is_empty() {
        println!();
        println!("warning: unknown capabilities detected");
        for capability in &unknown {
            println!("  - {capability}");
        }
        println!("run `agentOS capabilities` to see known capability names");
    }

    Ok(())
}

pub async fn init_agent_command(
    name: &str,
    template: AgentTemplate,
    output: &str,
    capabilities: &[String],
    force: bool,
    strict_capabilities: bool,
) -> anyhow::Result<()> {
    validate_cli_path(output)?;
    let resolved_capabilities = resolve_template_capabilities(template, capabilities);
    let unknown = unknown_capabilities(&resolved_capabilities);
    if strict_capabilities && !unknown.is_empty() {
        anyhow::bail!(
            "unknown capabilities: {}. Run `agentOS capabilities` to see supported names",
            unknown.join(", ")
        );
    }

    let output_path = Path::new(output);
    if output_path.exists() && !force {
        anyhow::bail!(
            "refusing to overwrite '{}'; pass --force to replace it",
            output
        );
    }

    if let Some(parent) = output_path.parent() {
        if !parent.as_os_str().is_empty() {
            std::fs::create_dir_all(parent)?;
        }
    }

    let config = render_agent_config(name, template, &resolved_capabilities);
    std::fs::write(output_path, config)?;

    println!("created agent config");
    println!("  path:         {output}");
    println!("  name:         {name}");
    println!("  template:     {}", template_name(template));
    println!("  capabilities: {}", resolved_capabilities.len());
    if resolved_capabilities.is_empty() {
        println!("  next:         agentOS inspect-config --agent {output}");
    } else {
        for capability in &resolved_capabilities {
            println!("    - {capability}");
        }
        if !unknown.is_empty() {
            println!();
            println!("warning: unknown capabilities detected");
            for capability in &unknown {
                println!("  - {capability}");
            }
            println!("run `agentOS capabilities` to see known capability names");
        }
        println!("  next:         agentOS inspect-config --agent {output}");
    }

    Ok(())
}

#[allow(clippy::too_many_arguments)]
pub async fn init_runtime_command(
    output: &str,
    host: &str,
    http_port: u16,
    grpc_port: u16,
    sse_port: u16,
    max_agents: usize,
    force: bool,
) -> anyhow::Result<()> {
    validate_cli_path(output)?;
    let output_path = Path::new(output);
    if output_path.exists() && !force {
        anyhow::bail!(
            "refusing to overwrite '{}'; pass --force to replace it",
            output
        );
    }

    if let Some(parent) = output_path.parent() {
        if !parent.as_os_str().is_empty() {
            std::fs::create_dir_all(parent)?;
        }
    }

    let config = render_runtime_config(host, http_port, grpc_port, sse_port, max_agents);
    std::fs::write(output_path, config)?;

    println!("created AgentOS runtime config");
    println!("  path:       {output}");
    println!("  host:       {host}");
    println!("  http:       {host}:{http_port}");
    println!("  grpc:       {host}:{grpc_port}");
    println!("  sse:        {host}:{sse_port}/events");
    println!("  max agents: {max_agents}");
    println!("  next:       agentOS doctor");

    Ok(())
}

pub async fn init_workspace_command(
    path: &str,
    template: AgentTemplate,
    agent_name: &str,
    force: bool,
) -> anyhow::Result<()> {
    let root = Path::new(path);
    let agents_dir = root.join("agents");
    let runtime_path = root.join("agentos.toml");
    let agent_file_name = format!("{}.toml", slugify(agent_name));
    let agent_path = agents_dir.join(agent_file_name);

    if !force {
        let mut existing = Vec::new();
        if runtime_path.exists() {
            existing.push(runtime_path.display().to_string());
        }
        if agent_path.exists() {
            existing.push(agent_path.display().to_string());
        }
        if !existing.is_empty() {
            anyhow::bail!(
                "refusing to overwrite existing files: {}. Pass --force to replace them",
                existing.join(", ")
            );
        }
    }

    std::fs::create_dir_all(&agents_dir)?;

    let runtime_config = render_workspace_runtime_config(&agent_path, agent_name, template);
    std::fs::write(&runtime_path, runtime_config)?;

    let capabilities = resolve_template_capabilities(template, &[]);
    let agent_config = render_agent_config(agent_name, template, &capabilities);
    std::fs::write(&agent_path, agent_config)?;

    println!("created AgentOS workspace");
    println!("  root:        {}", root.display());
    println!("  runtime:     {}", runtime_path.display());
    println!("  agent:       {}", agent_path.display());
    println!("  template:    {}", template_name(template));
    println!("  next:");
    println!(
        "    agentOS inspect-runtime --config {}",
        runtime_path.display()
    );
    println!(
        "    agentOS inspect-config --agent {}",
        agent_path.display()
    );

    Ok(())
}

pub async fn inspect_runtime_command(
    config_path: &str,
    format: OutputFormat,
) -> anyhow::Result<()> {
    let config = RuntimeConfig::from_file(config_path)
        .map_err(|e| anyhow::anyhow!("failed to parse runtime config: {e}"))?;

    if format == OutputFormat::Json {
        let agents = config
            .agents
            .iter()
            .map(|agent| {
                serde_json::json!({
                    "id": agent.id,
                    "name": agent.name,
                    "capabilities": agent.capabilities,
                    "max_restarts": agent.max_restarts,
                })
            })
            .collect::<Vec<_>>();

        println!(
            "{}",
            serde_json::json!({
                "path": config_path,
                "host": config.host,
                "http": config.listen_addr(),
                "grpc": config.grpc_addr(),
                "sse": config.sse_addr(),
                "max_agents": config.max_agents,
                "heartbeat_timeout_secs": config.heartbeat_timeout_secs,
                "log_level": config.log_level,
                "data_dir": config.data_dir,
                "agents": agents,
            })
        );
        return Ok(());
    }

    println!("AgentOS runtime config");
    println!("{}", "-".repeat(32));
    println!("path:          {config_path}");
    println!("host:          {}", config.host);
    println!("http:          {}", config.listen_addr());
    println!("grpc:          {}", config.grpc_addr());
    println!("sse:           {}", config.sse_addr());
    println!("max agents:    {}", config.max_agents);
    println!("heartbeat:     {}s", config.heartbeat_timeout_secs);
    println!("log level:     {}", config.log_level);
    println!("data dir:      {}", config.data_dir);
    println!("preload agents: {}", config.agents.len());

    for agent in &config.agents {
        println!("  - {} ({})", agent.id, agent.name);
        if !agent.capabilities.is_empty() {
            println!("    capabilities: {}", agent.capabilities.join(", "));
        }
    }

    Ok(())
}

pub async fn validate_runtime_command(
    config_path: &str,
    format: OutputFormat,
) -> anyhow::Result<()> {
    let config = RuntimeConfig::from_file(config_path)
        .map_err(|e| anyhow::anyhow!("failed to parse runtime config: {e}"))?;
    let findings = validate_runtime_config(&config);
    let errors = findings
        .iter()
        .filter(|finding| finding.level == FindingLevel::Error)
        .count();
    let warnings = findings
        .iter()
        .filter(|finding| finding.level == FindingLevel::Warning)
        .count();

    if format == OutputFormat::Json {
        let findings_json = findings
            .iter()
            .map(|finding| {
                serde_json::json!({
                    "level": finding.level.as_str(),
                    "field": finding.field,
                    "message": finding.message,
                })
            })
            .collect::<Vec<_>>();
        println!(
            "{}",
            serde_json::json!({
                "path": config_path,
                "valid": errors == 0,
                "errors": errors,
                "warnings": warnings,
                "findings": findings_json,
            })
        );

        if errors > 0 {
            anyhow::bail!("{errors} runtime validation error(s)");
        }

        return Ok(());
    }

    println!("AgentOS runtime validation");
    println!("{}", "-".repeat(32));
    println!("path:     {config_path}");
    println!(
        "status:   {}",
        if errors == 0 { "valid" } else { "invalid" }
    );
    println!("errors:   {errors}");
    println!("warnings: {warnings}");

    if findings.is_empty() {
        println!();
        println!("No validation findings.");
    } else {
        println!();
        for finding in &findings {
            println!(
                "[{}] {:<24} {}",
                finding.level.as_str(),
                finding.field,
                finding.message
            );
        }
    }

    if errors > 0 {
        anyhow::bail!("{errors} runtime validation error(s)");
    }

    Ok(())
}

pub async fn validate_workspace_command(path: &str, format: OutputFormat) -> anyhow::Result<()> {
    let root = Path::new(path);
    let runtime_path = root.join("agentos.toml");
    let agents_dir = root.join("agents");
    let mut findings = Vec::new();
    let mut agent_files = Vec::new();

    if !root.exists() {
        findings.push(ValidationFinding::error(
            "workspace",
            format!("workspace path '{}' does not exist", root.display()),
        ));
    }

    if !runtime_path.exists() {
        findings.push(ValidationFinding::error(
            "agentos.toml",
            "runtime config is missing",
        ));
    } else {
        match RuntimeConfig::from_file(&runtime_path) {
            Ok(config) => findings.extend(validate_runtime_config(&config)),
            Err(error) => findings.push(ValidationFinding::error(
                "agentos.toml",
                format!("failed to parse runtime config: {error}"),
            )),
        }
    }

    if !agents_dir.exists() {
        findings.push(ValidationFinding::warning(
            "agents",
            "agents directory is missing",
        ));
    } else {
        match std::fs::read_dir(&agents_dir) {
            Ok(entries) => {
                for entry in entries.flatten() {
                    let path = entry.path();
                    if path.extension().and_then(|ext| ext.to_str()) == Some("toml") {
                        agent_files.push(path);
                    }
                }
            }
            Err(error) => findings.push(ValidationFinding::error(
                "agents",
                format!("failed to read agents directory: {error}"),
            )),
        }
    }

    if agents_dir.exists() && agent_files.is_empty() {
        findings.push(ValidationFinding::warning(
            "agents",
            "no agent TOML files found",
        ));
    }

    for agent_path in &agent_files {
        match AgentConfig::from_toml(&agent_path.display().to_string()) {
            Ok(config) => {
                if config.name.trim().is_empty() {
                    findings.push(ValidationFinding::error(
                        agent_path.display().to_string(),
                        "agent name must not be empty",
                    ));
                }
                let unknown = unknown_capabilities(&config.capabilities);
                if !unknown.is_empty() {
                    findings.push(ValidationFinding::warning(
                        agent_path.display().to_string(),
                        format!("unknown capabilities: {}", unknown.join(", ")),
                    ));
                }
            }
            Err(error) => findings.push(ValidationFinding::error(
                agent_path.display().to_string(),
                format!("failed to parse agent config: {error}"),
            )),
        }
    }

    let errors = findings
        .iter()
        .filter(|finding| finding.level == FindingLevel::Error)
        .count();
    let warnings = findings
        .iter()
        .filter(|finding| finding.level == FindingLevel::Warning)
        .count();

    if format == OutputFormat::Json {
        let findings_json = findings
            .iter()
            .map(|finding| {
                serde_json::json!({
                    "level": finding.level.as_str(),
                    "field": finding.field,
                    "message": finding.message,
                })
            })
            .collect::<Vec<_>>();
        let agent_paths = agent_files
            .iter()
            .map(|path| path.display().to_string())
            .collect::<Vec<_>>();

        println!(
            "{}",
            serde_json::json!({
                "path": path,
                "valid": errors == 0,
                "errors": errors,
                "warnings": warnings,
                "runtime_config": runtime_path.display().to_string(),
                "agents_dir": agents_dir.display().to_string(),
                "agent_files": agent_paths,
                "findings": findings_json,
            })
        );

        if errors > 0 {
            anyhow::bail!("{errors} workspace validation error(s)");
        }

        return Ok(());
    }

    println!("AgentOS workspace validation");
    println!("{}", "-".repeat(32));
    println!("path:       {}", root.display());
    println!("runtime:    {}", runtime_path.display());
    println!("agents dir: {}", agents_dir.display());
    println!("agents:     {}", agent_files.len());
    println!(
        "status:     {}",
        if errors == 0 { "valid" } else { "invalid" }
    );
    println!("errors:     {errors}");
    println!("warnings:   {warnings}");

    if findings.is_empty() {
        println!();
        println!("No validation findings.");
    } else {
        println!();
        for finding in &findings {
            println!(
                "[{}] {:<24} {}",
                finding.level.as_str(),
                finding.field,
                finding.message
            );
        }
    }

    if errors > 0 {
        anyhow::bail!("{errors} workspace validation error(s)");
    }

    Ok(())
}

pub async fn summary_command(path: &str, format: OutputFormat) -> anyhow::Result<()> {
    let root = Path::new(path);
    let runtime_path = root.join("agentos.toml");
    let agents_dir = root.join("agents");

    let runtime = if runtime_path.exists() {
        Some(
            RuntimeConfig::from_file(&runtime_path)
                .map_err(|error| anyhow::anyhow!("failed to parse runtime config: {error}"))?,
        )
    } else {
        None
    };

    let mut agents = Vec::new();
    if agents_dir.exists() {
        for entry in std::fs::read_dir(&agents_dir)? {
            let entry = entry?;
            let path = entry.path();
            if path.extension().and_then(|ext| ext.to_str()) == Some("toml") {
                let config =
                    AgentConfig::from_toml(&path.display().to_string()).map_err(|error| {
                        anyhow::anyhow!(
                            "failed to parse agent config '{}': {error}",
                            path.display()
                        )
                    })?;
                agents.push(AgentSummary {
                    path: path.display().to_string(),
                    name: config.name,
                    capabilities: config.capabilities,
                });
            }
        }
    }

    let mut capability_counts = std::collections::BTreeMap::<String, usize>::new();
    for agent in &agents {
        for capability in &agent.capabilities {
            *capability_counts.entry(capability.clone()).or_insert(0) += 1;
        }
    }

    if format == OutputFormat::Json {
        let agents_json = agents
            .iter()
            .map(|agent| {
                serde_json::json!({
                    "path": agent.path,
                    "name": agent.name,
                    "capabilities": agent.capabilities,
                })
            })
            .collect::<Vec<_>>();

        println!(
            "{}",
            serde_json::json!({
                "path": path,
                "runtime": runtime.as_ref().map(|config| serde_json::json!({
                    "host": config.host,
                    "http": config.listen_addr(),
                    "grpc": config.grpc_addr(),
                    "max_agents": config.max_agents,
                    "data_dir": config.data_dir,
                    "preload_agents": config.agents.len(),
                })),
                "agents": agents_json,
                "agent_count": agents.len(),
                "capabilities": capability_counts,
            })
        );
        return Ok(());
    }

    println!("AgentOS workspace summary");
    println!("{}", "-".repeat(32));
    println!("path:       {}", root.display());
    println!("runtime:    {}", runtime_path.display());
    println!("agents dir: {}", agents_dir.display());

    match &runtime {
        Some(config) => {
            println!("http:       {}", config.listen_addr());
            println!("grpc:       {}", config.grpc_addr());
            println!("max agents: {}", config.max_agents);
            println!("data dir:   {}", config.data_dir);
            println!("preload:    {}", config.agents.len());
        }
        None => {
            println!("runtime:    missing");
        }
    }

    println!("agents:     {}", agents.len());
    if !agents.is_empty() {
        println!();
        for agent in &agents {
            println!("{}", agent.name);
            println!("  path: {}", agent.path);
            if agent.capabilities.is_empty() {
                println!("  capabilities: none");
            } else {
                println!("  capabilities: {}", agent.capabilities.join(", "));
            }
        }
    }

    if !capability_counts.is_empty() {
        println!();
        println!("capability usage:");
        for (capability, count) in capability_counts {
            println!("  {capability}: {count}");
        }
    }

    Ok(())
}

pub async fn graph_command(
    path: &str,
    format: GraphFormat,
    output: Option<&str>,
) -> anyhow::Result<()> {
    let graph = load_workspace_graph(path)?;

    let rendered = match format {
        GraphFormat::Json => {
            let agents = graph
                .agents
                .iter()
                .map(|agent| {
                    serde_json::json!({
                        "id": agent.id,
                        "name": agent.name,
                        "path": agent.path,
                        "capabilities": agent.capabilities,
                    })
                })
                .collect::<Vec<_>>();

            serde_json::json!({
                "path": graph.path,
                "runtime_config": graph.runtime_config,
                "agents_dir": graph.agents_dir,
                "agents": agents,
            })
            .to_string()
        }
        GraphFormat::Mermaid => render_mermaid_graph(&graph),
        GraphFormat::Markdown => {
            format!(
                "# AgentOS Workspace Graph\n\n```mermaid\n{}\n```\n",
                render_mermaid_graph(&graph)
            )
        }
        GraphFormat::Text => {
            let mut lines = vec![
                "AgentOS workspace graph".to_string(),
                "-".repeat(32),
                format!("workspace: {}", graph.path),
                format!("runtime:   {}", graph.runtime_config),
                format!("agents:    {}", graph.agents.len()),
                String::new(),
                "Runtime".to_string(),
                "  -> Kernel Supervisor".to_string(),
                "  -> Agent Bus".to_string(),
                "  -> Trace Recorder".to_string(),
                "  -> Vault".to_string(),
                "  -> Memory Store".to_string(),
                "  -> Registry".to_string(),
            ];

            if !graph.agents.is_empty() {
                lines.push(String::new());
                lines.push("Agents".to_string());
                for agent in &graph.agents {
                    lines.push(format!("  - {} ({})", agent.name, agent.id));
                    lines.push(format!("    path: {}", agent.path));
                    if agent.capabilities.is_empty() {
                        lines.push("    capabilities: none".to_string());
                    } else {
                        lines.push(format!(
                            "    capabilities: {}",
                            agent.capabilities.join(", ")
                        ));
                    }
                }
            }
            lines.join("\n")
        }
    };

    if let Some(output) = output {
        write_output_file(output, &rendered)?;
        println!("wrote graph output");
        println!("  path:   {output}");
        println!("  format: {:?}", format);
    } else {
        println!("{rendered}");
    }

    Ok(())
}

fn render_mermaid_graph(graph: &WorkspaceGraph) -> String {
    let mut lines = vec![
        "flowchart TD".to_string(),
        "  Runtime[\"AgentOS Runtime\"]".to_string(),
        "  Kernel[\"Kernel Supervisor\"]".to_string(),
        "  Bus[\"Agent Bus\"]".to_string(),
        "  Trace[\"Trace Recorder\"]".to_string(),
        "  Vault[\"Vault\"]".to_string(),
        "  Memory[\"Memory Store\"]".to_string(),
        "  Registry[\"Registry\"]".to_string(),
        "  Runtime --> Kernel".to_string(),
        "  Runtime --> Bus".to_string(),
        "  Runtime --> Trace".to_string(),
        "  Runtime --> Vault".to_string(),
        "  Runtime --> Memory".to_string(),
        "  Runtime --> Registry".to_string(),
    ];

    for agent in &graph.agents {
        let node_id = mermaid_node_id(&agent.id);
        lines.push(format!(
            "  {node_id}[\"{}\"]",
            escape_mermaid_label(&agent.name)
        ));
        lines.push(format!("  Kernel --> {node_id}"));
        lines.push(format!("  {node_id} --> Bus"));
        if agent
            .capabilities
            .iter()
            .any(|capability| capability.starts_with("trace_"))
        {
            lines.push(format!("  {node_id} --> Trace"));
        }
        if agent
            .capabilities
            .iter()
            .any(|capability| capability.starts_with("memory_"))
        {
            lines.push(format!("  {node_id} --> Memory"));
        }
        if agent
            .capabilities
            .iter()
            .any(|capability| capability.starts_with("vault_"))
        {
            lines.push(format!("  {node_id} --> Vault"));
        }
        if agent
            .capabilities
            .iter()
            .any(|capability| capability.starts_with("registry_"))
        {
            lines.push(format!("  {node_id} --> Registry"));
        }
    }

    lines.join("\n")
}

fn load_workspace_graph(path: &str) -> anyhow::Result<WorkspaceGraph> {
    let root = Path::new(path);
    let runtime_path = root.join("agentos.toml");
    let agents_dir = root.join("agents");
    let runtime = if runtime_path.exists() {
        Some(RuntimeConfig::from_file(&runtime_path)?)
    } else {
        None
    };
    let mut agents = Vec::new();

    if agents_dir.exists() {
        for entry in std::fs::read_dir(&agents_dir)? {
            let entry = entry?;
            let path = entry.path();
            if path.extension().and_then(|ext| ext.to_str()) == Some("toml") {
                let config = AgentConfig::from_toml(&path.display().to_string())?;
                agents.push(GraphAgent {
                    id: slugify(&config.name),
                    name: config.name,
                    path: path.display().to_string(),
                    capabilities: config.capabilities,
                });
            }
        }
    }

    if agents.is_empty() {
        if let Some(config) = runtime {
            agents = config
                .agents
                .into_iter()
                .map(|agent| GraphAgent {
                    id: agent.id,
                    name: agent.name,
                    path: runtime_path.display().to_string(),
                    capabilities: agent.capabilities,
                })
                .collect();
        }
    }

    Ok(WorkspaceGraph {
        path: root.display().to_string(),
        runtime_config: runtime_path.display().to_string(),
        agents_dir: agents_dir.display().to_string(),
        agents,
    })
}

fn validate_runtime_config(config: &RuntimeConfig) -> Vec<ValidationFinding> {
    let mut findings = Vec::new();

    if config.host.trim().is_empty() {
        findings.push(ValidationFinding::error("host", "host must not be empty"));
    }

    if config.http_port == config.grpc_port {
        findings.push(ValidationFinding::error(
            "ports",
            "http_port and grpc_port must be different",
        ));
    }

    if config.sse_port == config.http_port || config.sse_port == config.grpc_port {
        findings.push(ValidationFinding::error(
            "ports",
            "sse_port must differ from http_port and grpc_port",
        ));
    }

    if config.max_agents == 0 {
        findings.push(ValidationFinding::error(
            "max_agents",
            "max_agents must be greater than zero",
        ));
    }

    if config.agents.len() > config.max_agents {
        findings.push(ValidationFinding::error(
            "agents",
            "number of preload agents exceeds max_agents",
        ));
    }

    if config.heartbeat_timeout_secs < 5 {
        findings.push(ValidationFinding::warning(
            "heartbeat_timeout_secs",
            "heartbeat timeout below 5 seconds may cause noisy failures",
        ));
    }

    if config.data_dir.trim().is_empty() {
        findings.push(ValidationFinding::warning(
            "data_dir",
            "data_dir is empty; persistence may be hard to locate",
        ));
    }

    for agent in &config.agents {
        let prefix = format!("agents.{}", agent.id);
        if agent.id.trim().is_empty() {
            findings.push(ValidationFinding::error(
                &prefix,
                "agent id must not be empty",
            ));
        }
        if agent.name.trim().is_empty() {
            findings.push(ValidationFinding::error(
                &prefix,
                "agent name must not be empty",
            ));
        }
        let unknown = unknown_capabilities(&agent.capabilities);
        if !unknown.is_empty() {
            findings.push(ValidationFinding::warning(
                &prefix,
                format!("unknown capabilities: {}", unknown.join(", ")),
            ));
        }
    }

    findings
}

pub async fn ps_command(all: bool) -> anyhow::Result<()> {
    let state = load_state()?;
    let mut agents = list_agents(&state);
    agents.sort_by_key(|agent| agent.updated_at_ms);
    agents.reverse();

    if !all {
        agents.retain(|agent| agent.state == "running");
    }

    if agents.is_empty() {
        if all {
            print_empty_state("No agents found in .agentos/cli-state.json");
            print_hint("Run `agentOS run --agent <agent.toml>` to create local state.");
        } else {
            print_empty_state("No running agents found");
            print_hint("Use `agentOS ps --all` to show stopped and failed agents.");
        }
        return Ok(());
    }

    print_section("AgentOS agents");
    print_table_header(&[
        ("AGENT ID", 24),
        ("NAME", 18),
        ("STATUS", 12),
        ("STARTED AT", 20),
        ("UPDATED AT", 20),
        ("LOGS", 7),
        ("CHECKPOINTS", 11),
    ]);

    for agent in &agents {
        println!(
            "{} {} {} {} {} {} {}",
            table_cell(&agent.agent_id, 24),
            table_cell(&agent.name, 18),
            color_status_cell(&agent.state, 12),
            table_cell(&format_timestamp_ms(agent.started_at_ms), 20),
            table_cell(&format_timestamp_ms(agent.updated_at_ms), 20),
            table_cell(
                &logs_count_for_agent(&state, &agent.agent_id).to_string(),
                7
            ),
            table_cell(
                &checkpoints_count_for_agent(&state, &agent.agent_id).to_string(),
                11
            ),
        );
    }

    Ok(())
}

pub async fn state_doctor_command() -> anyhow::Result<()> {
    let report = doctor_state()?;

    print_section("AgentOS CLI state doctor");
    print_kv("backend", &report.backend);
    print_kv("path", report.path.display());
    match report.status {
        StateDoctorStatus::Missing => {
            print_kv("status", color_status("missing"));
            print_kv("action", "a new state file will be created when needed");
        }
        StateDoctorStatus::Healthy => {
            print_kv("status", color_status("healthy"));
        }
        StateDoctorStatus::CorruptBackedUp(path) => {
            print_kv("status", color_status("corrupt"));
            print_kv("backup", path.display());
            print_kv("action", "starting with a clean state");
        }
        StateDoctorStatus::Error(error) => {
            print_kv("status", color_status("error"));
            print_kv("error", truncate_cell(&error, 72));
        }
    }
    if let Some(version) = report.schema_version {
        print_kv("schema version", version);
    }
    print_kv("agents", report.agents);
    print_kv("logs", report.logs);
    print_kv("checkpoints", report.checkpoints);

    Ok(())
}

pub async fn state_clean_command(
    all: bool,
    older_than: Option<&str>,
    status: Option<&str>,
    dry_run: bool,
) -> anyhow::Result<()> {
    let older_than_ms = older_than.map(parse_duration_ms).transpose()?;
    let options = StateCleanOptions {
        all,
        older_than_ms,
        status: status.map(str::to_string),
        dry_run,
    };

    let report = crate::state::clean_state(&options)?;

    print_section("AgentOS CLI state clean");
    print_kv("mode", if report.dry_run { "dry-run" } else { "apply" });
    print_kv("all", all);
    print_kv("older than", older_than.unwrap_or("-"));
    print_kv("status", status.unwrap_or("-"));
    println!();
    print_kv("matched agents", report.matched_agents);
    print_kv("agents to remove", report.removed_agents);
    print_kv("logs to remove", report.removed_logs);
    print_kv("checkpoints remove", report.removed_checkpoints);
    print_kv("remaining agents", report.remaining_agents);

    if report.dry_run {
        print_success("No changes written.");
    } else if report.matched_agents == 0 {
        print_empty_state("Nothing to clean");
    } else {
        print_success("State cleaned.");
    }

    Ok(())
}

pub async fn state_export_command(output: &str, pretty: bool) -> anyhow::Result<()> {
    validate_cli_path(output)?;
    let report = export_state(Path::new(output), pretty)?;

    print_section("AgentOS CLI state export");
    print_kv("output", report.output.display());
    print_kv("format", if report.pretty { "pretty" } else { "compact" });
    print_kv("agents", report.agents);
    print_kv("logs", report.logs);
    print_kv("checkpoints", report.checkpoints);
    print_success("State exported.");

    Ok(())
}

pub async fn state_import_command(
    input: &str,
    dry_run: bool,
    merge: bool,
    replace: bool,
) -> anyhow::Result<()> {
    validate_cli_path(input)?;
    let mode = match (merge, replace) {
        (true, false) => StateImportMode::Merge,
        (false, true) => StateImportMode::Replace,
        (false, false) => anyhow::bail!("state import needs either --merge or --replace"),
        (true, true) => anyhow::bail!("state import accepts only one of --merge or --replace"),
    };

    let report = import_state(Path::new(input), mode, dry_run)?;

    print_section("AgentOS CLI state import");
    print_kv("input", report.input.display());
    print_kv(
        "mode",
        match report.mode {
            StateImportMode::Merge => "merge",
            StateImportMode::Replace => "replace",
        },
    );
    print_kv("write", if report.dry_run { "dry-run" } else { "apply" });
    print_kv("agents", report.imported_agents);
    print_kv("logs", report.imported_logs);
    print_kv("checkpoints", report.imported_checkpoints);
    print_kv("skipped", report.skipped_agents);
    if let Some(path) = &report.backup_path {
        print_kv("backup", path.display());
    }
    if report.dry_run {
        print_success("No changes written.");
    } else {
        print_success("State imported.");
    }

    Ok(())
}

pub async fn state_migrate_command(from: &str, to: &str, dry_run: bool) -> anyhow::Result<()> {
    let from = StateBackendKind::parse(from)?;
    let to = StateBackendKind::parse(to)?;
    let report = migrate_state(from, to, dry_run)?;

    print_section("AgentOS CLI state migrate");
    print_kv("from", &report.from);
    print_kv("to", &report.to);
    print_kv("source", report.source.display());
    print_kv("target", report.target.display());
    print_kv("mode", if report.dry_run { "dry-run" } else { "apply" });
    print_kv("agents", report.agents);
    print_kv("logs", report.logs);
    print_kv("checkpoints", report.checkpoints);
    if report.dry_run {
        print_success("No changes written.");
    } else {
        print_success("State migrated.");
    }

    Ok(())
}

pub async fn state_inspect_command(input: Option<&str>, json: bool) -> anyhow::Result<()> {
    let report = inspect_state(input.map(Path::new));

    if json {
        println!("{}", serde_json::to_string_pretty(&report)?);
        return Ok(());
    }

    print_section("AgentOS CLI state inspect");
    print_kv("path", report.path.display());
    print_kv("valid", if report.valid { "valid" } else { "invalid" });
    if let Some(error) = &report.error {
        print_kv("error", truncate_cell(error, 72));
    }
    print_kv("agents", report.agents);
    print_kv("logs", report.logs);
    print_kv("checkpoints", report.checkpoints);
    print_kv("file size", format!("{} bytes", report.file_size_bytes));
    print_kv("statuses", format_status_summary(&report.statuses));
    print_kv("oldest", format_agent_summary(&report.oldest_agent));
    print_kv("newest", format_agent_summary(&report.newest_agent));

    Ok(())
}

pub async fn logs_command(agent_id: &str) -> anyhow::Result<()> {
    let agent_id = agent_id.trim();
    if agent_id.is_empty() {
        anyhow::bail!("missing agent id; pass --id <agent_id>");
    }

    let state = load_state()?;
    if !has_agent(&state, agent_id) {
        anyhow::bail!(
            "agent '{agent_id}' not found in CLI state; run `agentOS ps --all` to list known agents"
        );
    }

    let logs = logs_for_agent(&state, agent_id, 100);
    if logs.is_empty() {
        anyhow::bail!("no logs found for agent '{agent_id}'; run the agent first");
    }

    let system = AgentOSSystem::new();
    for entry in &logs {
        system
            .log_event(agent_id, &entry.event_type, &entry.message)
            .await;
    }
    let system_logs = system.get_logs(agent_id, logs.len()).await;
    if system_logs.is_empty() {
        anyhow::bail!("AgentOSSystem returned no logs for agent '{agent_id}'");
    }

    print_logs(agent_id, &logs);
    Ok(())
}

pub async fn trace_command(agent_id: &str) -> anyhow::Result<()> {
    let agent_id = agent_id.trim();
    if agent_id.is_empty() {
        anyhow::bail!("missing agent id; pass --id <agent_id>");
    }

    let state = load_state()?;
    if !has_agent(&state, agent_id) {
        anyhow::bail!(
            "agent '{agent_id}' not found in CLI state; run `agentOS ps --all` to list known agents"
        );
    }

    let thoughts = trace_for_agent(&state, agent_id);
    if thoughts.is_empty() {
        anyhow::bail!("no trace checkpoints found for agent '{agent_id}'; run the agent first");
    }

    print_section(&format!("Trace timeline for {agent_id}"));
    print_table_header(&[
        ("STEP", 6),
        ("CHECKPOINT", 38),
        ("TIME", 20),
        ("CONTENT", 44),
    ]);
    for (index, thought) in thoughts.iter().enumerate() {
        println!(
            "{} {} {} {}",
            table_cell(&(index + 1).to_string(), 6),
            table_cell(&short_checkpoint(&thought.checkpoint_id), 38),
            table_cell(&format_timestamp_ms(thought.timestamp_ms), 20),
            table_cell(&thought.content, 44)
        );
    }

    Ok(())
}

pub async fn replay_command(checkpoint: &str) -> anyhow::Result<()> {
    let checkpoint = checkpoint.trim();
    if checkpoint.is_empty() {
        anyhow::bail!("missing checkpoint id; pass --checkpoint <checkpoint_id>");
    }

    let state = load_state()?;
    let thoughts = trace_containing_checkpoint(&state, checkpoint)
        .ok_or_else(|| {
            anyhow::anyhow!(
                "checkpoint '{checkpoint}' not found in CLI state; run `agentOS trace --id <agent_id>` to list checkpoints"
            )
        })?;

    if thoughts.is_empty() {
        anyhow::bail!("checkpoint '{checkpoint}' has no replayable trace");
    }

    let mut replayer = TraceReplayer::new(thoughts);
    let current = replayer
        .seek(checkpoint)
        .cloned()
        .ok_or_else(|| anyhow::anyhow!("checkpoint '{checkpoint}' not found in trace"))?;
    let cursor = replayer.cursor();

    print_section("Replay checkpoint");
    print_kv("agent", &current.agent_id);
    print_kv("checkpoint", &current.checkpoint_id);
    print_kv("cursor", format!("{}/{}", cursor.index + 1, replayer.len()));
    print_kv("time", format_timestamp_ms(current.timestamp_ms));
    println!();

    print_section("Visible state");
    print_table_header(&[("STEP", 6), ("CHECKPOINT", 38), ("CONTENT", 56)]);
    for (index, thought) in replayer.visible_state_at(None).iter().enumerate() {
        println!(
            "{} {} {}",
            table_cell(&(index + 1).to_string(), 6),
            table_cell(&short_checkpoint(&thought.checkpoint_id), 38),
            table_cell(&thought.content, 56)
        );
    }

    if cursor.index + 1 < replayer.len() {
        if let Some(next) = replayer.step_forward().cloned() {
            println!();
            print_section("Next checkpoint");
            println!(
                "{} {}",
                table_cell(&short_checkpoint(&next.checkpoint_id), 38),
                table_cell(&next.content, 56)
            );
        }
    } else {
        println!();
        print_section("Next checkpoint");
        print_empty_state("End of trace");
    }

    Ok(())
}

pub async fn fork_command(from: &str, prompt: Option<&str>) -> anyhow::Result<()> {
    let prompt_msg = prompt.unwrap_or("(same prompt)");
    anyhow::bail!(
        "trace fork is not implemented yet for checkpoint '{from}' with prompt: {prompt_msg}. Use `agentOS replay --checkpoint {from}` to inspect the checkpoint first"
    )
}

enum LifecycleExit {
    Stopped,
    Failed(String),
}

async fn monitor_lifecycle(system: Arc<AgentOSSystem>, agent_id: String) -> LifecycleExit {
    loop {
        tokio::select! {
            event = system.supervisor.recv_lifecycle() => {
                match event {
                    Some(LifecycleEvent::Started(id)) if id == agent_id => {
                        info!(agent_id = %id, "lifecycle: started");
                    }
                    Some(LifecycleEvent::Stopped(id)) if id == agent_id => {
                        info!(agent_id = %id, "lifecycle: stopped");
                        return LifecycleExit::Stopped;
                    }
                    Some(LifecycleEvent::Failed(id, reason)) if id == agent_id => {
                        info!(agent_id = %id, reason = %reason, "lifecycle: failed");
                        return LifecycleExit::Failed(reason);
                    }
                    Some(LifecycleEvent::Degraded(id, reason)) if id == agent_id => {
                        info!(agent_id = %id, reason = %reason, "lifecycle: degraded");
                    }
                    Some(_) => {}
                    None => {
                        return LifecycleExit::Failed("supervisor channel closed".into());
                    }
                }
            }
            _ = tokio::signal::ctrl_c() => {
                info!("received Ctrl+C, shutting down agent {agent_id}");
                let _ = system.supervisor.stop(&agent_id).await;
            }
        }
    }
}

struct DoctorCheck {
    name: &'static str,
    detail: &'static str,
    ok: bool,
}

struct QuickstartStep {
    title: String,
    command: String,
}

struct AgentSummary {
    path: String,
    name: String,
    capabilities: Vec<String>,
}

struct WorkspaceGraph {
    path: String,
    runtime_config: String,
    agents_dir: String,
    agents: Vec<GraphAgent>,
}

struct GraphAgent {
    id: String,
    name: String,
    path: String,
    capabilities: Vec<String>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum FindingLevel {
    Error,
    Warning,
}

impl FindingLevel {
    fn as_str(self) -> &'static str {
        match self {
            FindingLevel::Error => "error",
            FindingLevel::Warning => "warning",
        }
    }
}

struct ValidationFinding {
    level: FindingLevel,
    field: String,
    message: String,
}

impl ValidationFinding {
    fn error(field: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            level: FindingLevel::Error,
            field: field.into(),
            message: message.into(),
        }
    }

    fn warning(field: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            level: FindingLevel::Warning,
            field: field.into(),
            message: message.into(),
        }
    }
}

struct DocLink {
    path: &'static str,
    title: &'static str,
    description: &'static str,
}

struct BrokenLink {
    file: String,
    target: String,
}

fn collect_markdown_files(root: &Path) -> anyhow::Result<Vec<std::path::PathBuf>> {
    let mut files = Vec::new();
    collect_markdown_files_inner(root, &mut files)?;
    Ok(files)
}

fn collect_markdown_files_inner(
    path: &Path,
    files: &mut Vec<std::path::PathBuf>,
) -> anyhow::Result<()> {
    if path
        .file_name()
        .and_then(|name| name.to_str())
        .is_some_and(|name| name == "target" || name == ".git")
    {
        return Ok(());
    }

    if path.is_file() {
        if path.extension().and_then(|ext| ext.to_str()) == Some("md") {
            files.push(path.to_path_buf());
        }
        return Ok(());
    }

    if path.is_dir() {
        for entry in std::fs::read_dir(path)? {
            collect_markdown_files_inner(&entry?.path(), files)?;
        }
    }

    Ok(())
}

fn extract_markdown_links(content: &str) -> Vec<String> {
    let mut links = Vec::new();
    let mut remaining = content;

    while let Some(start) = remaining.find("](") {
        let after_start = &remaining[start + 2..];
        let Some(end) = after_start.find(')') else {
            break;
        };
        links.push(after_start[..end].trim().to_string());
        remaining = &after_start[end + 1..];
    }

    links
}

fn should_skip_link(link: &str) -> bool {
    link.is_empty()
        || link.starts_with('#')
        || link.starts_with("http://")
        || link.starts_with("https://")
        || link.starts_with("mailto:")
        || link.starts_with("file:")
}

fn normalize_markdown_link(link: &str) -> String {
    let without_anchor = link.split('#').next().unwrap_or_default().trim();
    without_anchor
        .trim_start_matches('<')
        .trim_end_matches('>')
        .to_string()
}

fn docs_catalog() -> &'static [DocLink] {
    &[
        DocLink {
            path: "README.md",
            title: "README",
            description: "Main project pitch, quick start, roadmap, and contribution entry points.",
        },
        DocLink {
            path: "PROJECT_OVERVIEW.md",
            title: "Project overview",
            description: "Short explanation of what AgentOS is and who it is for.",
        },
        DocLink {
            path: "FOUNDER.md",
            title: "Founder note",
            description: "Founder and project stewardship information for WAHIB EL KHADIRI.",
        },
        DocLink {
            path: "docs/architecture.md",
            title: "Architecture",
            description: "High-level runtime architecture and crate responsibilities.",
        },
        DocLink {
            path: "docs/cli-reference.md",
            title: "CLI reference",
            description: "Command reference for the agentOS CLI.",
        },
        DocLink {
            path: "docs/runtime-walkthrough.md",
            title: "Runtime walkthrough",
            description:
                "End-to-end terminal workflow for run, ps, logs, trace, replay, backup, and clean.",
        },
        DocLink {
            path: "ROADMAP.md",
            title: "Roadmap",
            description: "Milestones and contributor-friendly work areas.",
        },
        DocLink {
            path: "docs/contributing-guide.md",
            title: "Contributing guide",
            description: "How to run checks, choose issues, and submit focused pull requests.",
        },
        DocLink {
            path: "docs/security-model.md",
            title: "Security model",
            description: "Secrets, permissions, auditability, and current security limitations.",
        },
        DocLink {
            path: "docs/contributor-map.md",
            title: "Contributor map",
            description: "Where different kinds of contributors can help.",
        },
        DocLink {
            path: "docs/open-questions.md",
            title: "Open questions",
            description: "Design questions that are ready for community discussion.",
        },
        DocLink {
            path: "docs/comparison.md",
            title: "Comparison",
            description: "How AgentOS relates to existing agent frameworks.",
        },
        DocLink {
            path: "docs/time-travel-debugging.md",
            title: "Time-travel debugging",
            description: "Concepts behind trace replay, checkpoints, forks, and diffs.",
        },
    ]
}

impl QuickstartStep {
    fn new(title: impl Into<String>, command: impl Into<String>) -> Self {
        Self {
            title: title.into(),
            command: command.into(),
        }
    }
}

struct EnvVar<'a> {
    name: &'a str,
    value: &'a str,
    description: &'a str,
}

struct CapabilityInfo {
    name: &'static str,
    category: &'static str,
    description: &'static str,
}

fn capability_catalog() -> &'static [CapabilityInfo] {
    &[
        CapabilityInfo {
            name: "file_system_read",
            category: "filesystem",
            description: "Read files through approved tools",
        },
        CapabilityInfo {
            name: "file_system_write",
            category: "filesystem",
            description: "Write files through approved tools",
        },
        CapabilityInfo {
            name: "memory_read",
            category: "memory",
            description: "Read agent-scoped memory",
        },
        CapabilityInfo {
            name: "memory_write",
            category: "memory",
            description: "Store agent-scoped memory",
        },
        CapabilityInfo {
            name: "trace_record",
            category: "trace",
            description: "Record thoughts, events, and checkpoints",
        },
        CapabilityInfo {
            name: "trace_replay",
            category: "trace",
            description: "Replay checkpoints and trace branches",
        },
        CapabilityInfo {
            name: "bus_publish",
            category: "bus",
            description: "Publish messages to the AgentOS bus",
        },
        CapabilityInfo {
            name: "bus_subscribe",
            category: "bus",
            description: "Subscribe to bus topics",
        },
        CapabilityInfo {
            name: "vault_read",
            category: "vault",
            description: "Read agent-scoped secrets",
        },
        CapabilityInfo {
            name: "registry_discover",
            category: "registry",
            description: "Discover services by capability",
        },
        CapabilityInfo {
            name: "network",
            category: "runtime",
            description: "Access network resources through approved tools",
        },
    ]
}

fn is_known_capability(name: &str) -> bool {
    capability_catalog()
        .iter()
        .any(|capability| capability.name == name)
}

fn unknown_capabilities(capabilities: &[String]) -> Vec<String> {
    capabilities
        .iter()
        .filter(|capability| !is_known_capability(capability))
        .cloned()
        .collect()
}

impl<'a> EnvVar<'a> {
    fn new(name: &'a str, value: &'a str, description: &'a str) -> Self {
        Self {
            name,
            value,
            description,
        }
    }
}

fn check_path(path: &'static str, detail: &'static str) -> DoctorCheck {
    DoctorCheck {
        name: path,
        detail,
        ok: Path::new(path).exists(),
    }
}

fn render_agent_config(name: &str, template: AgentTemplate, capabilities: &[String]) -> String {
    let escaped_name = escape_toml_string(name);
    let escaped_prompt = escape_toml_string(template_prompt(template));
    let capability_lines = capabilities
        .iter()
        .map(|capability| format!("    \"{}\",", escape_toml_string(capability)))
        .collect::<Vec<_>>()
        .join("\n");

    let capabilities_block = if capability_lines.is_empty() {
        "capabilities = []".to_string()
    } else {
        format!("capabilities = [\n{capability_lines}\n]")
    };

    format!(
        "name = \"{escaped_name}\"\n\
prompt = \"{escaped_prompt}\"\n\
{capabilities_block}\n"
    )
}

fn resolve_template_capabilities(
    template: AgentTemplate,
    extra_capabilities: &[String],
) -> Vec<String> {
    let mut capabilities = template_capabilities(template)
        .iter()
        .map(|capability| (*capability).to_string())
        .collect::<Vec<_>>();

    for capability in extra_capabilities {
        if !capabilities.contains(capability) {
            capabilities.push(capability.clone());
        }
    }

    capabilities
}

fn template_name(template: AgentTemplate) -> &'static str {
    match template {
        AgentTemplate::Basic => "basic",
        AgentTemplate::Research => "research",
        AgentTemplate::Coding => "coding",
        AgentTemplate::Security => "security",
        AgentTemplate::Ops => "ops",
    }
}

fn template_prompt(template: AgentTemplate) -> &'static str {
    match template {
        AgentTemplate::Basic => {
            "You are a careful AgentOS agent. Explain important decisions clearly."
        }
        AgentTemplate::Research => {
            "You are a research agent. Gather evidence, track sources, and record useful checkpoints."
        }
        AgentTemplate::Coding => {
            "You are a coding agent. Inspect the codebase, make focused changes, and record implementation decisions."
        }
        AgentTemplate::Security => {
            "You are a security review agent. Identify risks, check permissions, and record audit findings."
        }
        AgentTemplate::Ops => {
            "You are an operations agent. Monitor runtime health, summarize incidents, and suggest safe recovery steps."
        }
    }
}

fn template_capabilities(template: AgentTemplate) -> &'static [&'static str] {
    match template {
        AgentTemplate::Basic => &["trace_record"],
        AgentTemplate::Research => &["memory_write", "trace_record", "registry_discover"],
        AgentTemplate::Coding => &[
            "file_system_read",
            "file_system_write",
            "memory_write",
            "trace_record",
        ],
        AgentTemplate::Security => &[
            "file_system_read",
            "vault_read",
            "trace_record",
            "registry_discover",
        ],
        AgentTemplate::Ops => &[
            "trace_record",
            "trace_replay",
            "bus_subscribe",
            "registry_discover",
        ],
    }
}

fn escape_toml_string(value: &str) -> String {
    value.replace('\\', "\\\\").replace('"', "\\\"")
}

fn render_runtime_config(
    host: &str,
    http_port: u16,
    grpc_port: u16,
    sse_port: u16,
    max_agents: usize,
) -> String {
    let escaped_host = escape_toml_string(host);
    format!(
        "host = \"{escaped_host}\"\n\
http_port = {http_port}\n\
grpc_port = {grpc_port}\n\
sse_port = {sse_port}\n\
max_agents = {max_agents}\n\
heartbeat_timeout_secs = 30\n\
log_level = \"info\"\n\
data_dir = \"data\"\n\
\n\
# Add agents here when you want the runtime to preload them.\n\
# [[agents]]\n\
# id = \"agent_1\"\n\
# name = \"research-agent\"\n\
# prompt = \"You are a careful AgentOS agent.\"\n\
# capabilities = [\"memory_write\", \"trace_record\"]\n\
# max_restarts = 5\n"
    )
}

fn render_workspace_runtime_config(
    agent_path: &Path,
    agent_name: &str,
    template: AgentTemplate,
) -> String {
    let escaped_agent_name = escape_toml_string(agent_name);
    let escaped_agent_id = escape_toml_string(&slugify(agent_name));
    let escaped_prompt = escape_toml_string(template_prompt(template));
    let capability_lines = template_capabilities(template)
        .iter()
        .map(|capability| format!("    \"{capability}\","))
        .collect::<Vec<_>>()
        .join("\n");
    let escaped_agent_path = escape_toml_string(&agent_path.display().to_string());

    format!(
        "host = \"127.0.0.1\"\n\
http_port = 8080\n\
grpc_port = 50051\n\
sse_port = 8081\n\
max_agents = 100\n\
heartbeat_timeout_secs = 30\n\
log_level = \"info\"\n\
data_dir = \"data\"\n\
\n\
[[agents]]\n\
id = \"{escaped_agent_id}\"\n\
name = \"{escaped_agent_name}\"\n\
prompt = \"{escaped_prompt}\"\n\
capabilities = [\n{capability_lines}\n]\n\
max_restarts = 5\n\
\n\
# Local file generated for this starter agent.\n\
# path = \"{escaped_agent_path}\"\n"
    )
}

fn slugify(value: &str) -> String {
    let mut slug = String::new();
    let mut last_was_separator = false;

    for character in value.chars() {
        if character.is_ascii_alphanumeric() {
            slug.push(character.to_ascii_lowercase());
            last_was_separator = false;
        } else if !last_was_separator {
            slug.push('-');
            last_was_separator = true;
        }
    }

    slug.trim_matches('-').to_string()
}

fn mermaid_node_id(value: &str) -> String {
    let slug = slugify(value).replace('-', "_");
    if slug.is_empty() {
        "agent".to_string()
    } else {
        format!("Agent_{slug}")
    }
}

fn validate_cli_path(path: &str) -> anyhow::Result<()> {
    if path.contains("..") {
        anyhow::bail!("path must not contain '..' traversal sequences");
    }
    if path.contains('\0') {
        anyhow::bail!("path must not contain null bytes");
    }
    Ok(())
}

fn escape_mermaid_label(value: &str) -> String {
    value.replace('"', "'")
}

fn write_output_file(path: &str, content: &str) -> anyhow::Result<()> {
    validate_cli_path(path)?;
    let output_path = Path::new(path);
    if let Some(parent) = output_path.parent() {
        if !parent.as_os_str().is_empty() {
            std::fs::create_dir_all(parent)?;
        }
    }
    std::fs::write(output_path, content)?;
    Ok(())
}

pub async fn init_plugin_command(
    name: &str,
    path: &str,
    force: bool,
    include_readme: bool,
) -> anyhow::Result<()> {
    let plugin_dir = Path::new(path).join(name);
    let src_dir = plugin_dir.join("src");
    let cargo_path = plugin_dir.join("Cargo.toml");
    let lib_path = src_dir.join("lib.rs");
    let readme_path = plugin_dir.join("README.md");

    if !force {
        let mut existing = Vec::new();
        if cargo_path.exists() {
            existing.push(cargo_path.display().to_string());
        }
        if lib_path.exists() {
            existing.push(lib_path.display().to_string());
        }
        if include_readme && readme_path.exists() {
            existing.push(readme_path.display().to_string());
        }
        if !existing.is_empty() {
            anyhow::bail!(
                "refusing to overwrite existing files: {}. Pass --force to replace them",
                existing.join(", ")
            );
        }
    }

    let kebab_name = name.replace('_', "-");
    let rs_name = name.replace('-', "_");

    std::fs::create_dir_all(&src_dir)?;

    let cargo_toml = format!(
        r#"[package]
name = "agentos-plugin-{kebab_name}"
version = "0.1.0"
edition = "2021"

[lib]
crate-type = ["cdylib"]

[dependencies]
serde = {{ version = "1", features = ["derive"] }}
serde_json = "1"
"#,
    );
    std::fs::write(&cargo_path, cargo_toml)?;

    let lib_rs = format!(
        r#"#![deny(unsafe_code)]

use serde::{{Deserialize, Serialize}};

mod host {{
    #![allow(unsafe_code)]

    extern "C" {{
        fn agentos_host_log(ptr: *const u8, len: i32);
        fn agentos_host_get_state(key_ptr: *const u8, key_len: i32) -> i32;
        fn agentos_host_set_state(
            key_ptr: *const u8,
            key_len: i32,
            val_ptr: *const u8,
            val_len: i32,
        );
    }}

    pub fn log(msg: &str) {{
        let bytes = msg.as_bytes();
        unsafe {{ agentos_host_log(bytes.as_ptr(), bytes.len() as i32); }}
    }}

    pub fn read_string(ptr: i32, len: i32) -> String {{
        if ptr == 0 || len <= 0 {{
            return String::new();
        }}
        let slice = unsafe {{ std::slice::from_raw_parts(ptr as *const u8, len as usize) }};
        String::from_utf8_lossy(slice).to_string()
    }}

    pub fn write_output(value: &super::PluginOutput) -> i64 {{
        let json = serde_json::to_string(value).unwrap_or_default();
        let bytes = json.into_bytes();
        let len = bytes.len() as i32;
        let ptr = Box::into_raw(bytes.into_boxed_slice()) as *mut u8 as i32;
        (ptr as i64) << 32 | (len as i64 & 0xFFFF_FFFF)
    }}
}}

#[derive(Debug, Deserialize)]
struct PluginInput {{
    method: String,
    #[serde(default)]
    params: serde_json::Value,
}}

#[derive(Debug, Serialize)]
struct PluginOutput {{
    ok: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    data: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<String>,
}}

impl PluginOutput {{
    fn success(data: serde_json::Value) -> Self {{
        Self {{ ok: true, data: Some(data), error: None }}
    }}
    fn error(msg: impl Into<String>) -> Self {{
        Self {{ ok: false, data: None, error: Some(msg.into()) }}
    }}
}}

#[no_mangle]
pub extern "C" fn agentos_plugin_init(_seed: i32) -> i32 {{
    host::log("plugin initialised");
    0
}}

#[no_mangle]
pub extern "C" fn agentos_plugin_process(input_ptr: i32, input_len: i32) -> i64 {{
    let input_str = host::read_string(input_ptr, input_len);
    let input: PluginInput = match serde_json::from_str(&input_str) {{
        Ok(v) => v,
        Err(e) => return host::write_output(&PluginOutput::error(format!("invalid input: {{e}}"))),
    }};

    match input.method.as_str() {{
        "ping" => {{
            host::write_output(&PluginOutput::success(serde_json::json!({{
                "pong": true,
                "plugin": "{kebab_name}",
            }})))
        }}
        "echo" => {{
            let message = input.params.get("message").and_then(|m| m.as_str()).unwrap_or("");
            host::log(&format!("[plugin] echo: {{message}}"));
            host::write_output(&PluginOutput::success(serde_json::json!({{
                "echoed": message,
                "length": message.len(),
            }})))
        }}
        other => host::write_output(&PluginOutput::error(format!("unknown method: {{other}}"))),
    }}
}}

#[cfg(test)]
mod tests {{
    use super::*;

    #[test]
    fn test_plugin_output_success() {{
        let output = PluginOutput::success(serde_json::json!({{"key": "value"}}));
        assert!(output.ok);
        assert!(output.error.is_none());
    }}

    #[test]
    fn test_plugin_output_error() {{
        let output = PluginOutput::error("fail");
        assert!(!output.ok);
        assert_eq!(output.error.unwrap(), "fail");
    }}

    #[test]
    fn test_read_string_null() {{
        assert_eq!(host::read_string(0, 0), "");
    }}

    #[test]
    fn test_write_output_roundtrip() {{
        let out = PluginOutput::success(serde_json::json!({{"x": 1}}));
        let packed = host::write_output(&out);
        // Verify it doesn't panic
        assert_ne!(packed, 0);
    }}
}}
"#,
    );
    std::fs::write(&lib_path, lib_rs)?;

    if include_readme {
        let readme = format!(
            r#"# WASM Plugin: {kebab_name}

A WASM plugin for AgentOS.

## Build

```bash
rustup target add wasm32-wasip1
cargo build --target wasm32-wasip1 --release
```

The compiled plugin will be at `target/wasm32-wasip1/release/agentos_plugin_{rs_name}.wasm`.

## Usage

Place the `.wasm` file in the AgentOS plugins directory and start the runtime.
"#,
        );
        std::fs::write(&readme_path, readme)?;
    }

    println!("created WASM plugin project");
    println!("  name:    agentos-plugin-{kebab_name}");
    println!("  path:    {}", plugin_dir.display());
    println!("  target:  wasm32-wasip1");
    println!();
    println!("  next:");
    println!("    cd {}/{}", path, name);
    println!("    rustup target add wasm32-wasip1");
    println!("    cargo build --target wasm32-wasip1 --release");

    Ok(())
}

pub async fn init_manifest_command(
    name: &str,
    runtime: &str,
    restart: &str,
    output: &str,
    force: bool,
    with_permissions: bool,
) -> anyhow::Result<()> {
    validate_cli_path(output)?;
    let output_path = Path::new(output);
    if output_path.exists() && !force {
        anyhow::bail!(
            "refusing to overwrite '{}'; pass --force to replace it",
            output
        );
    }

    if let Some(parent) = output_path.parent() {
        if !parent.as_os_str().is_empty() {
            std::fs::create_dir_all(parent)?;
        }
    }

    let permissions = if with_permissions {
        r#"permissions:
  - network
  - filesystem:read
"#
    } else {
        ""
    };

    let manifest = format!(
        r#"# Agent manifest for {name}
# Generated by agentOS init-manifest
apiVersion: agentos.io/v1
kind: AgentManifest
name: {name}
version: "1.0"
description: "An AI agent powered by AgentOS"
runtime: {runtime}
restart: {restart}
{permissions}prompt: "You are a helpful assistant."
capabilities:
  - web-search
"#,
    );

    std::fs::write(output_path, manifest)?;

    println!("created AgentOS manifest");
    println!("  name:    {name}");
    println!("  file:    {output}");
    println!("  runtime: {runtime}");
    println!("  restart: {restart}");
    println!();
    println!("  next:");
    println!("    agentOS inspect-config --agent {output}");
    println!("    agentOS run --agent {output}");

    Ok(())
}
