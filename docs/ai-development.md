# AI-Assisted Development Guide

AgentOS welcomes AI-assisted contributions when they improve reliability,
developer experience, documentation quality, or reproducibility. AI tools should
support careful engineering work, not replace it.

This guide is inspired by ECC-style agent operating context and adapted for
AgentOS. It does not vendor ECC and does not add runtime functionality.

## Before You Start

- Read `AGENTS.md`.
- For Claude-specific sessions, also read `.claude/CLAUDE.md`.
- Work on one phase at a time.
- Identify whether the change touches runtime, supervisor, CLI, dashboard, docs,
  tests, CI, SDKs, or demo assets.
- Prefer a small patch that is easy to review.
- Use research-first development for unfamiliar APIs, security-sensitive areas,
  dependencies, or runtime guarantees.

## While Working

- Do not add random features.
- Do not change runtime, supervisor, CLI behavior, dashboard behavior, or public
  APIs unless the task explicitly requires it.
- Do not create fake demos, fake logs, fake traces, fake benchmark results, or
  fake command output.
- Do not add secrets or local machine state.
- Keep generated artifacts out of the repository.
- If behavior is experimental or planned, label it that way.

## Verification

Run the checks that match the change. For repository-wide validation, run:

```bash
bash scripts/check.sh
```

When submitting work, state:

- what changed
- which files changed
- which checks ran
- which checks did not run, if any
- whether runtime, CLI, supervisor, dashboard, or public APIs changed
- any remaining risk or uncertainty

Honesty over hype. AgentOS should be easy to trust because its claims are
specific, reproducible, and tested.
