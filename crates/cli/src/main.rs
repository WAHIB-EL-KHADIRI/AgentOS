#![forbid(unsafe_code)]

mod commands;
mod dev;
mod marketplace;
mod repl;
mod run;
mod run_format;
mod state;
mod supervisor;

use clap::{CommandFactory, Parser};

pub use commands::{AgentTemplate, GraphFormat, OutputFormat};
use commands::{Cli, Commands, MarketplaceCommand, StateCommand};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .init();

    let cli = Cli::parse();
    state::set_state_backend(cli.state_backend.as_str())?;

    match cli.command {
        Commands::Run { agent, config } => run::run_command(&agent, &config).await,
        Commands::Status { format } => run::status_command(format).await,
        Commands::Quickstart {
            format,
            path,
            template,
        } => run::quickstart_command(format, &path, template).await,
        Commands::Docs { format } => run::docs_command(format).await,
        Commands::CheckLinks { path, format } => run::check_links_command(&path, format).await,
        Commands::Doctor { format } => run::doctor_command(format).await,
        Commands::Env { format } => run::env_command(format).await,
        Commands::Capabilities { format } => run::capabilities_command(format).await,
        Commands::Templates { format } => run::templates_command(format).await,
        Commands::InspectConfig {
            agent,
            format,
            strict_capabilities,
        } => run::inspect_config_command(&agent, format, strict_capabilities).await,
        Commands::InitAgent {
            name,
            template,
            output,
            capabilities,
            force,
            strict_capabilities,
        } => {
            run::init_agent_command(
                &name,
                template,
                &output,
                &capabilities,
                force,
                strict_capabilities,
            )
            .await
        }
        Commands::InitRuntime {
            output,
            host,
            http_port,
            grpc_port,
            max_agents,
            force,
        } => {
            run::init_runtime_command(&output, &host, http_port, grpc_port, max_agents, force).await
        }
        Commands::InitManifest {
            name,
            runtime,
            restart,
            output,
            force,
            with_permissions,
        } => {
            run::init_manifest_command(&name, &runtime, &restart, &output, force, with_permissions)
                .await
        }
        Commands::InitPlugin {
            name,
            path,
            force,
            readme,
        } => run::init_plugin_command(&name, &path, force, readme).await,
        Commands::InitWorkspace {
            path,
            template,
            agent_name,
            force,
        } => run::init_workspace_command(&path, template, &agent_name, force).await,
        Commands::InspectRuntime { config, format } => {
            run::inspect_runtime_command(&config, format).await
        }
        Commands::ValidateRuntime { config, format } => {
            run::validate_runtime_command(&config, format).await
        }
        Commands::ValidateWorkspace { path, format } => {
            run::validate_workspace_command(&path, format).await
        }
        Commands::Summary { path, format } => run::summary_command(&path, format).await,
        Commands::Graph {
            path,
            format,
            output,
        } => run::graph_command(&path, format, output.as_deref()).await,
        Commands::Ps { all } => run::ps_command(all).await,
        Commands::State { command } => match command {
            StateCommand::Doctor => run::state_doctor_command().await,
            StateCommand::Clean {
                all,
                older_than,
                status,
                dry_run,
            } => {
                run::state_clean_command(all, older_than.as_deref(), status.as_deref(), dry_run)
                    .await
            }
            StateCommand::Export { output, pretty } => {
                run::state_export_command(&output, pretty).await
            }
            StateCommand::Import {
                input,
                dry_run,
                merge,
                replace,
            } => run::state_import_command(&input, dry_run, merge, replace).await,
            StateCommand::Inspect { input, json } => {
                run::state_inspect_command(input.as_deref(), json).await
            }
            StateCommand::Migrate { from, to, dry_run } => {
                run::state_migrate_command(from.as_str(), to.as_str(), dry_run).await
            }
        },
        Commands::Logs { id } => run::logs_command(&id).await,
        Commands::Trace { id } => run::trace_command(&id).await,
        Commands::Replay { checkpoint } => run::replay_command(&checkpoint).await,
        Commands::Fork { from, prompt } => run::fork_command(&from, prompt.as_deref()).await,
        Commands::Marketplace { command } => match command {
            MarketplaceCommand::Install { name, path } => {
                marketplace::marketplace_install_command(&name, &path).await
            }
            MarketplaceCommand::Uninstall { name } => {
                marketplace::marketplace_uninstall_command(&name).await
            }
            MarketplaceCommand::Search { query } => {
                marketplace::marketplace_search_command(&query).await
            }
            MarketplaceCommand::List => marketplace::marketplace_list_command().await,
        },
        Commands::Supervisor { config } => supervisor::supervisor_command(&config).await,
        Commands::Dev {
            path,
            config,
            watch_patterns,
        } => dev::dev_command(&path, &config, &watch_patterns).await,
        Commands::Repl => repl::run_repl().await,
        Commands::Completions { shell } => {
            let mut cmd = Cli::command();
            let name = cmd.get_name().to_string();
            clap_complete::generate(shell, &mut cmd, name, &mut std::io::stdout());
            Ok(())
        }
    }
}
