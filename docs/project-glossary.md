# Project Glossary

This glossary keeps AgentOS language consistent across code, docs, issues, and
discussions.

## Agent

A runtime unit managed by AgentOS. An agent may wrap a framework workflow, a
custom tool-calling process, or a long-running service.

## Runtime

The layer that turns an agent definition or command into managed execution,
lifecycle events, logs, state, traces, and replayable records.

## Kernel

The Rust crate that owns lifecycle primitives, agent handles, supervision
integration, and system coordination.

## Supervisor

The component that starts, stops, tracks, and coordinates managed agents. It is
responsible for lifecycle correctness and should be changed carefully.

## Lifecycle Event

A recorded runtime transition such as spawn, start, stop, failure, or timeout.
Lifecycle events should be consistent and observable by the right watchers.

## Bus

The messaging layer used by agents and runtime components. It may be local or
networked depending on the transport.

## State

Operational data used by AgentOS to inspect and recover local workflows. State
is different from memory: state is about runtime operation, while memory is
agent-owned knowledge or records.

## Memory

Agent-owned records and optional embedding-backed search.

## Trace

A recorded timeline of agent behavior used for debugging and replay.

## Checkpoint

A replayable point in a trace.

## Replay

The act of inspecting or stepping through recorded execution from a checkpoint
or timeline. Replay should be deterministic when possible and explicit when
external state affects the result.

## Fork

A planned execution branch created from an existing checkpoint. The CLI command
exists as a placeholder, but trace forking is not implemented yet.

## Vault

The subsystem responsible for secrets, scopes, audit records, and
permission-sensitive access.

## Registry

The subsystem responsible for service discovery and runtime health metadata.

## Dashboard

The user interface for inspecting agents, lifecycle state, traces, checkpoints,
and runtime behavior.

## SDK

A language-specific client used to connect external agents and applications to
AgentOS.

## Demo

A reproducible workflow that shows real behavior. AgentOS demos should not use
fake logs, fake traces, fake screenshots, or invented command output.
