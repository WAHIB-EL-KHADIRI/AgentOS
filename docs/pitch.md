# AgentOS Pitch

Use this page when explaining AgentOS in issues, discussions, submissions, or
short project descriptions. Keep the language precise: AgentOS is runtime
infrastructure, not a prompt framework and not a production-hardened control
plane yet.

## One Sentence

AgentOS is a Rust-first runtime layer for running, supervising, observing, and
replaying AI agents.

## Short Pitch

Most AI agent frameworks help developers build workflows. AgentOS focuses on
what happens after those workflows need runtime infrastructure: lifecycle
controls, supervision, agent-to-agent messaging, state, secrets, trace recording,
and replay.

## Longer Pitch

AI agents are becoming long-running software processes. They call tools, hold
state, talk to other services, fail in non-obvious ways, and need inspection
after a run completes.

AgentOS provides the infrastructure layer around those agents:

- run and supervise agent processes
- inspect lifecycle state from the CLI
- connect agents through a bus
- isolate secrets through a vault
- record traces and checkpoints
- replay recorded execution paths
- expose a dashboard for debugging

The project is designed to work with existing agent frameworks rather than
replace them.

## What To Avoid Saying

Avoid claims that are not backed by tests or real demos:

- "production-ready distributed control plane"
- "guaranteed autonomous recovery"
- "complete time-travel branching"
- "secure plugin marketplace"

Prefer honest wording:

- "working local runtime"
- "trace and replay workflow"
- "fork/branch workflows planned"
- "plugin runtime is experimental"
- "restart and recovery guarantees are still being hardened"

## Founder Line

Created and led by **WAHIB EL KHADIRI**, developer and founder of AgentOS.
