# Replay a recorded session, no API key

This directory ships one recorded session so you can see deterministic replay
on a fresh clone, before configuring any provider. From the repository root:

```bash
cargo run -p agentos-cli -- replay --session demo_agent --config examples/replay/agentos.toml
```

The replay serves every LLM response from the journal, re-computes each request
fingerprint, and ends with:

```text
[ok] Replay is deterministic: no drift against the recording.
```

## What this recording is, and is not

`journals/demo_agent.json` was written by the real recording path — the same
code `agentOS run` uses — but the model behind it was a **scripted offline
provider**, which is why its `model` field says `scripted-offline`. No real LLM
was called. What the example proves is the replay side: an offline re-execution
that reproduces the recorded request fingerprints and final answer exactly.

It is not hand-written. The test
[`crates/kernel/tests/replay_example.rs`](../../crates/kernel/tests/replay_example.rs)
re-records it on every CI run and fails if the committed copy differs, and
replays it in a fresh system and fails on any drift. If the journal format or
the request fingerprint ever changes, that test fails before your existing
journals silently stop replaying.

## Record your own

Recording needs a provider. `AGENTOS_LLM_PROVIDER=ollama` records locally for
free; see [the CLI reference](../../docs/cli-reference.md#you-need-a-provider-to-record-one).

Replaying writes the replayed run's own journal next to the example
(`journals/demo_agent_replay_<timestamp>.json`). Those files are ignored by git.

If `AGENTOS_DATA_DIR` is set in your environment it overrides the `data_dir` in
this config, and the command reports no recorded session. Unset it to run the
example.
