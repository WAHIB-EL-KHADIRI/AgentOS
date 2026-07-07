# Development Guide

This document describes how AgentOS is structured and how to work with it
effectively. It is written for developers — whether human or AI-assisted —
who need to understand the project conventions.

## Before Making Changes

1. State the goal in one sentence when the scope is non-trivial.
2. Identify the affected area: runtime, supervisor, CLI, dashboard, docs,
   tests, CI, SDKs, or demo assets.
3. Choose the smallest safe change that handles the request.
4. Name meaningful risks when runtime, lifecycle, security, or public APIs
   are involved.

## Conventions

- Prefer small patches over broad rewrites.
- Follow existing project patterns before introducing new abstractions.
- Keep documentation honest and specific.
- Do not add generated artifacts, build output, dependency folders, or
  fake outputs.
- Do not change public APIs unless the task requires it. If a public API
  must change, document it clearly.

## Runtime Guidelines

For runtime changes, prefer correctness over convenience:

- lifecycle events must be consistent
- spawn success must reflect the actual process state
- stop and timeout behavior must be explicit
- replay and demo claims must be reproducible

## Demo Honesty

AgentOS demos must show real project behavior. Do not fabricate outputs,
traces, logs, recovery behavior, or benchmark results. If something is
experimental, planned, partial, or not production-ready, say so plainly.

## Verification

Always run checks that match the change. For broad repository changes:

```bash
bash scripts/check.sh
```

If a required check cannot be run, report the reason honestly along with
the remaining risk. Do not imply validation that did not happen.

## Security

Do not add secrets, tokens, credentials, private keys, or personal state
to the repository. Treat filesystem access, process spawning, networking,
auth, logs, and replay data as security-sensitive areas.

Honesty over hype. Contributor trust is part of the product.
