# Time-Travel Debugging

Time-travel debugging is the main idea that makes AgentOS different.

Instead of only storing logs, AgentOS should store execution checkpoints that
allow a developer to inspect and replay an agent run. Forking and branch
comparison are planned parts of the same model.

## Concepts

| Concept | Meaning |
| --- | --- |
| Trace | A full timeline for one agent run |
| Thought | A recorded reasoning or planning step |
| Checkpoint | A replayable point in the timeline |
| Fork | Planned: a new branch created from an existing checkpoint |
| Diff | Planned: a comparison between two branches |

## Example Flow

```bash
agentOS run --agent my_agent.toml
agentOS trace --id agent_123
agentOS replay --checkpoint ckpt_456
```

Execution sessions are journaled automatically: `agentOS replay --session
<agent_id>` re-executes a recorded session deterministically (recorded LLM
responses, no API key) and reports drift, and `agentOS fork` replays a chosen
prefix before continuing with the live provider. The working local workflow is
`run -> trace -> replay -> fork`.

## Design Goal

The developer should be able to answer:

- what did the agent know?
- what did the agent choose?
- what tool did it call?
- what changed after this checkpoint?
- what would happen with a different prompt?
