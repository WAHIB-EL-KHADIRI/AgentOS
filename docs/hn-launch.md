# Hacker News Launch Kit

Everything needed to post AgentOS to Hacker News, and to survive the thread.

The single rule that matters: **HN rewards a specific, honest claim and
punishes a feature list.** A post that says what the project does *not* do gets
better treatment than one that implies it does everything.

---

## Before you post — non-negotiable

These are not polish items. Each one is a thread-killer if it is missing.

- [ ] **A tagged release with prebuilt binaries.** A cold `cargo build
      --workspace` takes about 24 minutes on a normal machine. If the first
      thing a visitor must do is wait 24 minutes, most never see the project.
      The release workflow already produces signed, provenance-attested
      binaries — tag a version and let it run.
- [ ] **A demo recording**, 15–30 seconds, of the Recordings scrubber: run →
      something goes wrong → replay → fork. Terminal asciinema is fine and
      loads faster than video. This is the highest-converting asset on the page
      and nobody else can record it.
- [ ] **The Quick Start works from a clean clone**, verified today, on the
      machine you will be answering questions from.
- [ ] Repository topics set: `ai-agents`, `agent-runtime`, `rust`,
      `observability`, `time-travel-debugging`, `developer-tools`.
- [ ] Discussions enabled — HN readers who do not open issues will still post
      there.
- [ ] You are free for **three hours** after posting.

## Timing

Tuesday, Wednesday, or Thursday, **08:00–10:00 US Eastern**
(**13:00–15:00 UTC**, **14:00–16:00 Casablanca**).

Avoid Friday and the weekend: fewer readers, and a post that stalls cannot be
reposted for weeks.

## Title

HN allows 80 characters. No hype words, no exclamation marks, no "revolutionary".

**Preferred:**

```text
Show HN: AgentOS – Replay any AI agent run offline and deterministically
```

Alternatives, in order of preference:

```text
Show HN: AgentOS – Time-travel debugging for AI agents, written in Rust
Show HN: I journal every LLM call so agent runs can be replayed for free
```

Use `Show HN:` — it is a project you built and readers can run. That prefix
gets a dedicated audience and a more forgiving one.

## First comment

Post this as the first comment immediately after submitting. On Show HN, the
comment carries the post.

```text
Author here.

I kept hitting the same wall building agents: something goes wrong on step 7 of
a run, and to see it again I have to pay for the whole run — and it never
behaves the same way twice. Print-debugging an agent means paying per attempt.

So AgentOS journals every LLM exchange and tool result at the provider
boundary, and can re-execute a session against those recorded responses. The
replay is a real run of your agent code with the network removed, not a log
viewer: it reports drift when your code no longer produces the same calls, and
you can fork from any checkpoint to try a different path.

    agentOS run --agent my_agent.toml
    agentOS replay --session agent_123          # no API key, no cost
    agentOS fork --from ckpt_4 --prompt "try the other path"

It is a runtime layer, not a framework. It sits underneath LangGraph, AutoGen,
CrewAI, or a hand-written loop — it does not replace them and has no opinion
about how you build your agent.

What is real today: the CLI flows (run/ps/logs/trace/replay/fork), the
supervisor, the bus, journaling, and a dashboard with a scrubbable timeline.
What is not: distributed control, production hardening, published SDK packages.
It is pre-1.0 and I am not going to pretend otherwise.

Written in Rust because this is a long-lived supervising process, and I wanted
predictable memory and a small footprint next to the agent, not another
service.

What I would most like feedback on: whether the drift detection model is right.
Replay only helps if it tells you honestly when the recording no longer matches
your code, and that boundary is the hardest part.
```

## Prepared answers

HN asks these. Answer in one or two paragraphs, never defensively, and concede
what is true.

**"This is just logging."**
> Logging tells you what happened. Replay re-executes your agent code with
> recorded provider responses substituted at the boundary, so control flow runs
> again for real — including tool calls — and reports drift when your code stops
> matching the recording. Forking then continues live from any checkpoint. A log
> viewer cannot do the second or third of those.

**"LLMs are non-deterministic; determinism is impossible."**
> Agreed, and that is the point. Replay does not make the model deterministic —
> it takes the model out of the loop. The recorded response is replayed, so what
> is deterministic is *your code's* behaviour given those responses. That is the
> part you are usually debugging.

**"Why not LangSmith / Langfuse / W&B Weave?"**
> Those are observability products, mostly hosted, and they are good at it. This
> is a local runtime: it supervises the process, and re-executes and forks runs
> on your machine with no account and no data leaving it. Different layer. You
> can run both.

**"Why Rust? This could be a Python library."**
> The supervisor is a long-lived process sitting next to your agent; I did not
> want it to be the thing that leaks memory or adds latency. The SDKs are Python
> and TypeScript — Rust is an implementation choice for the runtime, not a
> requirement for users.

**"Pre-1.0 — is this usable?"**
> Locally, yes: I use it. In production, no, and the README says so. If that is
> your bar, star it and come back at 1.0; if you build agents and debugging them
> hurts, the replay loop is useful today.

**"What is the security story?"**
> Actions pinned by commit SHA, least-privilege CI tokens, signed build
> provenance on release binaries and container images, SBOMs, secret scanning,
> fuzzing of the network-facing parser, and OpenSSF Scorecard on a schedule.
> Private disclosure is in SECURITY.md with response windows.

**"How do I know your binaries are what the source says?"**
> `gh attestation verify <file> --repo WAHIB-EL-KHADIRI/AgentOS`

**Any bug report or correction.**
> Thank them, confirm quickly, open an issue, and link it in the reply. A
> maintainer who fixes something during the thread earns more trust than one who
> argues.

## During the thread

- Reply to every top-level comment in the first two hours. Speed matters more
  than length.
- Never ask for upvotes, anywhere. It is the fastest way to be penalised.
- Do not argue with dismissive comments. Answer the technical part, ignore the
  tone, move on.
- If someone finds a real flaw, say so plainly. HN respects that far more than
  a defence.
- Do not post the same link to multiple aggregators in the same hour.

## After

- Convert every substantive thread into an issue, and link the issue back.
- Post a short "what the launch taught me" note within a week, while attention
  lasts.
- Whatever people asked for most is your roadmap. Not what you planned.

## If it does not take off

Most Show HN posts do not, and that says little about the project. Do not
repost the same link — write about a specific problem you solved (the drift
detection model, or journaling at the provider boundary) and post that instead.
A concrete technical article outlives a launch.
