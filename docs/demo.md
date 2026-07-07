# AgentOS Killer Demo

This is the official short demo for AgentOS. It is designed to show the current
local runtime workflow in under two minutes:

```text
run -> ps -> logs -> trace -> replay
```

The demo uses real CLI output only. It does not print simulated logs, fake
traces, or fabricated dashboard data.

## What It Shows

- a real agent config is inspected
- `agentOS run` starts the demo agent
- `agentOS ps --all` reads real local CLI state
- `agentOS logs --id ...` shows recorded lifecycle logs
- `agentOS trace --id ...` shows recorded checkpoints
- `agentOS replay --checkpoint ...` replays a real checkpoint

Restart/recovery is not part of this demo yet. Treat restart behavior as future
or experimental demo work until the supervisor flow has a stable public demo.

## Requirements

Run from the repository root.

Install the CLI first:

```bash
cargo install --path crates/cli
```

Optional dashboard setup:

```bash
cd dashboard
npm install
npm run dev
```

The dashboard connection indicator is real. It will show `Live` only if the
dashboard can reach an SSE endpoint. Otherwise it should show reconnecting or
disconnected.

## Run The Demo

```bash
bash scripts/demo.sh
```

Run the fast smoke check without starting an agent:

```bash
bash scripts/demo.sh --check
```

The smoke check verifies that the demo files exist, the required CLI commands
are present, the sample agent config parses, and the script does not contain
known fake-output fallback patterns.

The script uses:

```bash
agentOS init-runtime --output .agentos/demo/agentos.demo.toml --http-port 18080 --grpc-port 15051 --sse-port 18081 --force
agentOS inspect-config --agent examples/simple_agent.toml
agentOS run --agent examples/simple_agent.toml --config .agentos/demo/agentos.demo.toml
agentOS ps --all
agentOS logs --id agent_simple_agent
agentOS trace --id agent_simple_agent
agentOS replay --checkpoint <checkpoint_id>
```

`agentOS run` is interrupted by the script after a few seconds so the demo can
continue. This is an intentional demo interrupt, not a production recovery
scenario.

## Recording

Record a real terminal session if you want a GIF or cast:

```bash
asciinema rec assets/demo/agentos-demo.cast -c 'bash scripts/demo.sh'
```

Generated recordings should be small and should reflect real command output.
Do not add fake screenshots or edited terminal output.
