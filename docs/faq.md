# Frequently Asked Questions

## Is AgentOS another agent framework?

No. AgentOS is a runtime layer. It is meant to supervise, connect, debug,
replay, and secure agents that may be built with existing frameworks.

## Can AgentOS work with LangGraph, AutoGen, or CrewAI?

That is the intended direction. AgentOS should provide runtime services around
agents regardless of the framework used to define their behavior.

## Why Rust?

AgentOS is infrastructure. Rust gives the project strong typing, predictable
performance, good async support, and reliable binaries.

## Why time-travel debugging?

Agent behavior can be hard to inspect after a failure. Trace replay and
checkpoints help developers understand what changed and why. Forks and diffs are
planned extensions of the same model.

## Is AgentOS production-ready?

Not yet. AgentOS is in active development. The repository is designed to grow
toward production readiness through clear crates, documentation, and community
feedback.

## Who created AgentOS?

AgentOS was created and is led by **WAHIB EL KHADIRI**, developer and founder
of the project.
