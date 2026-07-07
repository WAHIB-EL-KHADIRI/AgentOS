# CLI Reference

The `agentOS` CLI is the developer entry point for running and inspecting the
local runtime.

## Runtime Commands

```bash
agentOS status
agentOS status --format json
```

Shows the local runtime configuration and component readiness.

```bash
agentOS quickstart
agentOS quickstart --template coding --path agentos-starter
agentOS quickstart --format json
```

Prints the recommended first-run workflow for a new developer.

```bash
agentOS docs
agentOS docs --format json
```

Lists important project documentation files and what each one is for.

```bash
agentOS check-links
agentOS check-links --format json
```

Checks local Markdown links in project documentation and fails if a referenced
local file is missing.

```bash
agentOS doctor
agentOS doctor --format json
```

Checks that important project files and runtime defaults are available.

```bash
agentOS env
agentOS env --format json
```

Shows supported AgentOS environment variables, resolved values, and whether each
value came from the environment or the default runtime config.

```bash
agentOS capabilities
agentOS capabilities --format json
```

Lists known capability names and categories that can be used with
`init-agent`.

```bash
agentOS templates
agentOS templates --format json
```

Lists available `init-agent` templates, their prompts, and their default
capabilities.

```bash
agentOS run --agent examples/simple_agent.toml
```

Starts an agent from a TOML config file.

```bash
agentOS inspect-config --agent examples/simple_agent.toml
agentOS inspect-config --agent examples/simple_agent.toml --format json
agentOS inspect-config --agent examples/simple_agent.toml --strict-capabilities
```

Reads an agent config and prints its name, prompt length, and capabilities
without starting the agent. It also reports unknown capabilities. Use
`--strict-capabilities` to fail when unknown capabilities are present.

```bash
agentOS init-agent --name research-agent --output agents/research.toml \
  --capability memory_write --capability trace_record
```

Creates a new agent TOML config file. Use `--force` to replace an existing file.
Use `--strict-capabilities` to fail when a capability is not in the known
AgentOS capability catalog.

Available templates:

```bash
agentOS init-agent --name research-agent --template research --output agents/research.toml
agentOS init-agent --name coding-agent --template coding --output agents/coding.toml
agentOS init-agent --name security-agent --template security --output agents/security.toml
agentOS init-agent --name ops-agent --template ops --output agents/ops.toml
```

```bash
agentOS init-runtime --output agentos.toml
```

Creates a runtime TOML config file with host, HTTP port, gRPC port, SSE event
stream port, max agent count, data directory, and commented agent preload
examples.

```bash
agentOS init-workspace --path agentos-starter --template research
```

Creates a starter workspace containing `agentos.toml` and an `agents/`
directory with a template-based agent config.

```bash
agentOS inspect-runtime --config agentos.toml
agentOS inspect-runtime --config agentos.toml --format json
```

Validates and prints a runtime TOML config file, including host, ports,
heartbeat timeout, data directory, and preloaded agents.

```bash
agentOS validate-runtime --config agentos.toml
agentOS validate-runtime --config agentos.toml --format json
```

Validates runtime configuration rules and reports errors or warnings for ports,
agent limits, heartbeat settings, and unknown capabilities.

```bash
agentOS validate-workspace --path agentos-starter
agentOS validate-workspace --path agentos-starter --format json
```

Validates a starter workspace directory, including `agentos.toml`, `agents/`,
agent TOML files, and capabilities.

```bash
agentOS summary --path agentos-starter
agentOS summary --path agentos-starter --format json
```

Summarizes a workspace: runtime settings, agent files, agent count, and
capability usage.

```bash
agentOS graph --path agentos-starter
agentOS graph --path agentos-starter --format json
agentOS graph --path agentos-starter --format mermaid
agentOS graph --path agentos-starter --format markdown
agentOS graph --path agentos-starter --format mermaid --output docs/workspace-graph.mmd
agentOS graph --path agentos-starter --format markdown --output docs/workspace-graph.md
```

Generates a topology graph of the runtime, core components, and workspace
agents. Mermaid output can be pasted directly into GitHub Markdown.

## Agent Inspection Commands

```bash
agentOS ps
agentOS ps --all
agentOS logs --id agent_123
agentOS trace --id agent_123
agentOS replay --checkpoint ckpt_456
```

These commands define the intended developer workflow for inspecting agents,
logs, traces, and replay checkpoints.

The `fork` subcommand is reserved for the planned trace-fork workflow and
currently returns a clear "not implemented yet" message.

## State Lifecycle Commands

```bash
agentOS state inspect
agentOS state inspect --input backup.json
agentOS state inspect --input backup.json --json
```

Reads the current state file or an external backup without modifying anything.

```bash
agentOS state doctor
```

Checks the local state file and recovers from corruption by backing up the
damaged file.

```bash
agentOS state export --output backup.json --pretty
agentOS state import --input backup.json --dry-run --merge
agentOS state import --input backup.json --merge
agentOS state import --input backup.json --replace
```

Exports and imports state before SQLite persistence. `--replace` creates an
automatic backup of the current state before writing.

```bash
agentOS state clean --status completed --dry-run
agentOS state clean --older-than 7d --dry-run
agentOS state clean --all --dry-run
```

Cleans local state by status, age, or all agents. Use `--dry-run` first to see
what would be removed.

## Shell Completions

```bash
agentOS completions bash
agentOS completions zsh
agentOS completions fish
agentOS completions powershell
```

Generate shell completions for the selected shell.
