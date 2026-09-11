# Maintainer

AgentOS is written and maintained by
[WAHIB EL KHADIRI](https://github.com/WAHIB-EL-KHADIRI).

## Scope of the project

AgentOS is runtime infrastructure for AI agents: supervised lifecycle, a gRPC
message bus, a secrets vault, SQLite-backed state, and deterministic replay of
journaled agent runs. It sits below the agent frameworks rather than competing
with them — it is the layer that keeps a long-lived agent running, records what
it did, and lets you re-run that offline.

## How decisions get made

One maintainer, which has consequences worth stating plainly rather than hiding:

- Technical direction is set by the maintainer today. That is a property of the
  project's size, not a policy, and it changes as contributors take ownership of
  areas.
- Design questions are settled in the open, in issues, before code. Recent
  examples are [#200](https://github.com/WAHIB-EL-KHADIRI/AgentOS/issues/200)
  and [#34](https://github.com/WAHIB-EL-KHADIRI/AgentOS/issues/34) — the latter
  closed because a contributor showed the behaviour it asked to test could not
  occur.
- Contributions are reviewed against the code, not waved through, and are
  credited in [`AUTHORS`](AUTHORS).

See [`MAINTAINERS.md`](MAINTAINERS.md) for the review process and
[`CONTRIBUTING.md`](CONTRIBUTING.md) for how to start.

## Licensing and copyright

The core is Apache-2.0. Copyright in the original work stays with the author;
see [`AUTHORS`](AUTHORS), [`NOTICE`](NOTICE) and [`LICENSING.md`](LICENSING.md).

## Contact

Project matters: [wahibelkhadiri06@gmail.com](mailto:wahibelkhadiri06@gmail.com)
or an issue on this repository, which is usually faster and leaves a record
others can read.
