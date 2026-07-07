# AgentOS Runtime Walkthrough

This walkthrough shows the current local runtime workflow before SQLite
persistence. It is meant for contributors who want to understand what AgentOS
can do today and where polish still matters.

## 1. Run an Agent

Start with an agent TOML file:

```bash
agentOS run --agent agents/research.toml
```

Example output:

```text
started agent 'agent_research'
  id:      agent_research
  status:  Running
  trace:   8b7c9f62-4b83-4d60-8f7e-7ad43f9f8f5a
  (press Ctrl+C to stop)
```

When the agent stops, AgentOS records logs and checkpoints in
`.agentos/cli-state.json`.

## 2. Inspect Agents

Show currently running agents:

```bash
agentOS ps
```

Show every known agent, including stopped or completed agents:

```bash
agentOS ps --all
```

Example output:

```text
AgentOS agents
--------------------------------
AGENT ID                 NAME               STATUS       STARTED AT           UPDATED AT           LOGS    CHECKPOINTS
--------------------------------------------------------------------------------------------------------------------
agent_research           research-agent     stopped      2026-05-29 09:15:20 UTC 2026-05-29 09:18:02 UTC 2       2
```

## 3. Read Logs

```bash
agentOS logs --id agent_research
```

Example output:

```text
Logs for agent_research
--------------------------------
TIME                 EVENT          MESSAGE
--------------------------------------------------------------------------------------------------
2026-05-29 09:15:20 UTC spawned        Agent 'research-agent' started
2026-05-29 09:18:02 UTC stopped        Agent 'research-agent' stopped: normal shutdown
```

## 4. Inspect Trace

```bash
agentOS trace --id agent_research
```

Example output:

```text
Trace timeline for agent_research
---------------------------------
STEP   CHECKPOINT                             TIME                 CONTENT
---------------------------------------------------------------------------------------------------------------
1      8b7c9f62-4b83-4d60-8f7e-7ad43f9f8f5a  2026-05-29 09:15:20 UTC Agent 'research-agent' spawned
2      c2d9f1a0-6d11-4f68-9aa1-d0f0c34ef481  2026-05-29 09:18:02 UTC Agent 'research-agent' stopped: normal shutdown
```

## 5. Replay a Checkpoint

Use a checkpoint id from `trace`:

```bash
agentOS replay --checkpoint 8b7c9f62-4b83-4d60-8f7e-7ad43f9f8f5a
```

Example output:

```text
Replay checkpoint
--------------------------------
agent              agent_research
checkpoint         8b7c9f62-4b83-4d60-8f7e-7ad43f9f8f5a
cursor             1/2
time               2026-05-29 09:15:20 UTC
```

## 6. Export and Import State

Create a backup:

```bash
agentOS state export --output backup.json --pretty
```

Inspect before importing:

```bash
agentOS state inspect --input backup.json
agentOS state inspect --input backup.json --json
```

Dry-run a merge:

```bash
agentOS state import --input backup.json --dry-run --merge
```

Apply a merge:

```bash
agentOS state import --input backup.json --merge
```

Replace local state safely:

```bash
agentOS state import --input backup.json --replace
```

Replace creates an automatic backup of the current state before writing.

## 7. Clean Local State

Always dry-run destructive operations first:

```bash
agentOS state clean --status completed --dry-run
agentOS state clean --older-than 7d --dry-run
agentOS state clean --all --dry-run
```

Apply a focused clean:

```bash
agentOS state clean --status completed
```

Example output:

```text
AgentOS CLI state clean
--------------------------------
mode               dry-run
all                false
older than         -
status             completed

matched agents     1
agents to remove   1
logs to remove     3
checkpoints remove 2
remaining agents   4
[ok] No changes written.
```

## Contributor Notes

This workflow is intentionally JSON-backed for now. The next persistence stage
should preserve the same CLI experience while moving durable state into SQLite.
