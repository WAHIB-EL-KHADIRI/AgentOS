use clap::{Parser, Subcommand, ValueEnum};

#[derive(Parser)]
#[command(
    name = "agentOS",
    about = "AI Agent Operating Layer",
    long_about = "AgentOS is a local runtime layer for AI agents. Use it to run agents, inspect state, read logs, replay trace checkpoints, and manage local state with a SQLite backend (JSON export/import available for backups).",
    version
)]
pub(crate) struct Cli {
    /// Select local state backend. SQLite is the default.
    #[arg(
        long,
        env = "AGENTOS_STATE_BACKEND",
        value_enum,
        default_value_t = StateBackendArg::Sqlite,
        global = true
    )]
    pub(crate) state_backend: StateBackendArg,
    #[command(subcommand)]
    pub(crate) command: Commands,
}

#[derive(Subcommand)]
pub(crate) enum Commands {
    /// Start an agent from a config file (.toml, .yaml, .yml)
    Run {
        /// Path to an agent config file
        #[arg(long)]
        agent: String,
        /// Path to a runtime config TOML file
        #[arg(long, default_value = "agentos.toml")]
        config: String,
    },
    /// Show local AgentOS runtime status
    Status {
        /// Output format
        #[arg(long, value_enum, default_value_t = OutputFormat::Text)]
        format: OutputFormat,
    },
    /// Print the recommended first-run workflow
    Quickstart {
        /// Output format
        #[arg(long, value_enum, default_value_t = OutputFormat::Text)]
        format: OutputFormat,
        /// Starter workspace path used in generated commands
        #[arg(long, default_value = "agentos-starter")]
        path: String,
        /// Starter agent template used in generated commands
        #[arg(long, value_enum, default_value_t = AgentTemplate::Research)]
        template: AgentTemplate,
    },
    /// List important AgentOS documentation files
    Docs {
        /// Output format
        #[arg(long, value_enum, default_value_t = OutputFormat::Text)]
        format: OutputFormat,
    },
    /// Check local Markdown links in project docs
    CheckLinks {
        /// Root directory to scan
        #[arg(long, default_value = ".")]
        path: String,
        /// Output format
        #[arg(long, value_enum, default_value_t = OutputFormat::Text)]
        format: OutputFormat,
    },
    /// Validate the local AgentOS development environment
    Doctor {
        /// Output format
        #[arg(long, value_enum, default_value_t = OutputFormat::Text)]
        format: OutputFormat,
    },
    /// Show AgentOS environment variables and resolved values
    Env {
        /// Output format
        #[arg(long, value_enum, default_value_t = OutputFormat::Text)]
        format: OutputFormat,
    },
    /// List known AgentOS capability names
    Capabilities {
        /// Output format
        #[arg(long, value_enum, default_value_t = OutputFormat::Text)]
        format: OutputFormat,
    },
    /// List available agent templates
    Templates {
        /// Output format
        #[arg(long, value_enum, default_value_t = OutputFormat::Text)]
        format: OutputFormat,
    },
    /// Inspect an agent config without starting it
    InspectConfig {
        /// Path to agent config file
        #[arg(long)]
        agent: String,
        /// Output format
        #[arg(long, value_enum, default_value_t = OutputFormat::Text)]
        format: OutputFormat,
        /// Fail if the config contains unknown capabilities
        #[arg(long)]
        strict_capabilities: bool,
    },
    /// Create a new agent config file (TOML)
    InitAgent {
        /// Agent name
        #[arg(long)]
        name: String,
        /// Agent template
        #[arg(long, value_enum, default_value_t = AgentTemplate::Basic)]
        template: AgentTemplate,
        /// Output TOML path
        #[arg(long, default_value = "agent.toml")]
        output: String,
        /// Agent capability. Can be passed multiple times.
        #[arg(long = "capability")]
        capabilities: Vec<String>,
        /// Overwrite the output file if it already exists
        #[arg(long)]
        force: bool,
        /// Fail if an unknown capability is provided
        #[arg(long)]
        strict_capabilities: bool,
    },
    /// Create a runtime configuration file
    InitRuntime {
        /// Output TOML path
        #[arg(long, default_value = "agentos.toml")]
        output: String,
        /// Runtime host
        #[arg(long, default_value = "127.0.0.1")]
        host: String,
        /// HTTP health/dashboard port
        #[arg(long, default_value_t = 8080)]
        http_port: u16,
        /// gRPC bus port
        #[arg(long, default_value_t = 50051)]
        grpc_port: u16,
        /// SSE event stream port (dashboard live events)
        #[arg(long, default_value_t = 8081)]
        sse_port: u16,
        /// Maximum number of supervised agents
        #[arg(long, default_value_t = 100)]
        max_agents: usize,
        /// Overwrite the output file if it already exists
        #[arg(long)]
        force: bool,
    },
    /// Create a Kubernetes-style agent manifest (YAML)
    InitManifest {
        /// Agent name
        #[arg(long)]
        name: String,
        /// Runtime type (native or wasm)
        #[arg(long, default_value = "native")]
        runtime: String,
        /// Restart policy (always, on-failure, never)
        #[arg(long, default_value = "on-failure")]
        restart: String,
        /// Output manifest path
        #[arg(long, default_value = "agent.yaml")]
        output: String,
        /// Overwrite if exists
        #[arg(long)]
        force: bool,
        /// Include example permissions
        #[arg(long)]
        with_permissions: bool,
    },
    /// Scaffold a new WASM plugin project
    InitPlugin {
        /// Plugin name (e.g. my-plugin)
        #[arg(long)]
        name: String,
        /// Output directory for the plugin project
        #[arg(long, default_value = ".")]
        path: String,
        /// Overwrite generated files if they already exist
        #[arg(long)]
        force: bool,
        /// Include a README for the plugin project
        #[arg(long)]
        readme: bool,
    },
    /// Create a starter AgentOS workspace
    InitWorkspace {
        /// Target directory
        #[arg(long, default_value = ".")]
        path: String,
        /// Starter agent template
        #[arg(long, value_enum, default_value_t = AgentTemplate::Research)]
        template: AgentTemplate,
        /// Starter agent name
        #[arg(long, default_value = "research-agent")]
        agent_name: String,
        /// Overwrite generated files if they already exist
        #[arg(long)]
        force: bool,
    },
    /// Inspect a runtime TOML config file
    InspectRuntime {
        /// Path to runtime config TOML file
        #[arg(long, default_value = "agentos.toml")]
        config: String,
        /// Output format
        #[arg(long, value_enum, default_value_t = OutputFormat::Text)]
        format: OutputFormat,
    },
    /// Validate a runtime TOML config file
    ValidateRuntime {
        /// Path to runtime config TOML file
        #[arg(long, default_value = "agentos.toml")]
        config: String,
        /// Output format
        #[arg(long, value_enum, default_value_t = OutputFormat::Text)]
        format: OutputFormat,
    },
    /// Validate a starter AgentOS workspace directory
    ValidateWorkspace {
        /// Workspace directory
        #[arg(long, default_value = ".")]
        path: String,
        /// Output format
        #[arg(long, value_enum, default_value_t = OutputFormat::Text)]
        format: OutputFormat,
    },
    /// Summarize an AgentOS workspace directory
    Summary {
        /// Workspace directory
        #[arg(long, default_value = ".")]
        path: String,
        /// Output format
        #[arg(long, value_enum, default_value_t = OutputFormat::Text)]
        format: OutputFormat,
    },
    /// Generate a workspace topology graph
    Graph {
        /// Workspace directory
        #[arg(long, default_value = ".")]
        path: String,
        /// Output format
        #[arg(long, value_enum, default_value_t = GraphFormat::Text)]
        format: GraphFormat,
        /// Optional file path to write the graph output
        #[arg(long)]
        output: Option<String>,
    },
    /// List agents known by the local CLI state
    Ps {
        /// Include stopped, completed, and failed agents instead of only running agents
        #[arg(long)]
        all: bool,
    },
    /// Inspect, clean, export, import, and repair the local CLI state file
    State {
        #[command(subcommand)]
        command: StateCommand,
    },
    /// Search, install, and manage AgentOS plugins
    Marketplace {
        #[command(subcommand)]
        command: MarketplaceCommand,
    },
    /// Show local logs for an agent recorded by AgentOS CLI state
    Logs {
        /// Agent ID to show logs for. Use `agentOS ps --all` to discover IDs.
        #[arg(long)]
        id: String,
    },
    /// Show the local trace checkpoint timeline for an agent
    Trace {
        /// Agent ID to show trace for. Use `agentOS ps --all` to discover IDs.
        #[arg(long)]
        id: String,
    },
    /// Replay visible trace state from a checkpoint
    Replay {
        /// Checkpoint ID to replay from. Use `agentOS trace --id <agent_id>` to discover IDs.
        #[arg(long)]
        checkpoint: String,
    },
    /// Report that trace forking is not available yet
    Fork {
        /// Checkpoint ID to fork from
        #[arg(long)]
        from: String,
        /// New prompt for the forked agent
        #[arg(long)]
        prompt: Option<String>,
    },
    /// Start the interactive AgentOS REPL
    Repl,

    /// Real-time agent health dashboard (supervisor mode)
    Supervisor {
        /// Runtime config path
        #[arg(long, default_value = "agentos.toml")]
        config: String,
    },

    /// Watch directory and auto-restart agents on file changes
    Dev {
        /// Directory to watch for agent TOML config files
        #[arg(long, default_value = ".")]
        path: String,
        /// Runtime config path
        #[arg(long, default_value = "agentos.toml")]
        config: String,
        /// File pattern(s) to watch. Can be repeated. Default: **/*.toml
        #[arg(long = "watch")]
        watch_patterns: Vec<String>,
    },

    /// Generate shell completions
    Completions {
        /// Shell to generate completions for (bash, zsh, fish, powershell)
        #[arg(value_enum)]
        shell: clap_complete::Shell,
    },
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, ValueEnum)]
pub enum OutputFormat {
    Text,
    Json,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, ValueEnum)]
pub enum GraphFormat {
    Text,
    Json,
    Mermaid,
    Markdown,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, ValueEnum)]
pub enum AgentTemplate {
    Basic,
    Research,
    Coding,
    Security,
    Ops,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, ValueEnum)]
pub(crate) enum StateBackendArg {
    Json,
    Sqlite,
}

impl StateBackendArg {
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::Json => "json",
            Self::Sqlite => "sqlite",
        }
    }
}

#[derive(Subcommand)]
pub(crate) enum StateCommand {
    /// Check and repair the local CLI state file if corruption is detected
    Doctor,
    /// Remove agents and their logs/checkpoints from local CLI state
    Clean {
        /// Remove every agent from local CLI state. Prefer --dry-run first.
        #[arg(long)]
        all: bool,
        /// Remove agents updated before this age. Supports m, h, d.
        #[arg(long)]
        older_than: Option<String>,
        /// Remove agents matching this status
        #[arg(long)]
        status: Option<String>,
        /// Show what would be removed without writing changes
        #[arg(long)]
        dry_run: bool,
    },
    /// Export the local CLI state to a JSON backup file
    Export {
        /// Output backup JSON path, for example backup.json
        #[arg(long)]
        output: String,
        /// Write formatted JSON
        #[arg(long)]
        pretty: bool,
    },
    /// Import a JSON backup into the local CLI state
    Import {
        /// Input backup JSON path created by `agentOS state export`
        #[arg(long)]
        input: String,
        /// Show summary without writing changes
        #[arg(long)]
        dry_run: bool,
        /// Merge only new agents into current state
        #[arg(long)]
        merge: bool,
        /// Replace current state with the backup after creating an automatic backup
        #[arg(long)]
        replace: bool,
    },
    /// Inspect the current CLI state or an external backup file
    Inspect {
        /// Optional state/backup JSON file to inspect
        #[arg(long)]
        input: Option<String>,
        /// Print machine-readable JSON
        #[arg(long)]
        json: bool,
    },
    /// Copy state between supported backends
    Migrate {
        /// Source backend
        #[arg(long, value_enum)]
        from: StateBackendArg,
        /// Target backend
        #[arg(long, value_enum)]
        to: StateBackendArg,
        /// Show summary without writing changes
        #[arg(long)]
        dry_run: bool,
    },
}

#[derive(Subcommand)]
pub(crate) enum MarketplaceCommand {
    /// Install a plugin from the marketplace
    Install {
        /// Plugin name
        name: String,
        /// Output directory for the plugin
        #[arg(long, default_value = ".")]
        path: String,
    },
    /// Uninstall a plugin
    Uninstall {
        /// Plugin name
        name: String,
    },
    /// Search available plugins
    Search {
        /// Search query
        query: String,
    },
    /// List installed plugins
    List,
}
