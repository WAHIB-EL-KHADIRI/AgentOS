# Licensing

AgentOS is **Open Core**. This page states exactly what you get, what is
reserved, and who needs to talk to me.

| | Licence | Cost |
|---|---|---|
| **Community Edition** (this repository) | [Apache-2.0](LICENSE) | Free, forever |
| **Enterprise Edition** (separate, in development) | Proprietary | Contact |
| **The name "AgentOS"** | Reserved — not licensed | Written permission |

Copyright (c) 2026 WAHIB EL KHADIRI.
See [`docs/editions.md`](docs/editions.md) for the feature boundary.

---

## The core is Apache-2.0, and that is permanent

Everything in this repository is licensed under the
[Apache License, Version 2.0](LICENSE). That is an OSI-approved open-source
licence, and the grant for every released version is **irrevocable**.

You may, at no cost and without asking:

- Run AgentOS in production, commercially, at any scale.
- Read, modify, fork, and self-host it.
- Build products and services on top of it and sell them.
- Redistribute it, including inside a closed-source product.
- Implement any of the public extension traits yourself, including ones the
  Enterprise Edition also implements.

There is no licence key, no seat count, no expiry, no phone-home, and no
telemetry.

**AgentOS cannot be taken away from you.** Apache-2.0 on a published release
cannot be revoked. Even if this project is relicensed tomorrow, every version
released up to that point stays Apache-2.0 for everyone who has it.

### What Apache-2.0 asks of you

Two things, both light:

- **Attribution** (§4) — keep the licence, copyright notices, and the
  [`NOTICE`](NOTICE) file with any redistribution, and state what you changed.
- **Patent peace** (§3) — the licence includes a patent grant, and it ends for
  you if you sue the project or its users for patent infringement over it.

### Why Apache-2.0 and not MIT

Apache-2.0 gives *you* an express patent licence that MIT does not, and gives
the project an explicit trademark carve-out (§6) and attribution requirement
(§4). It is strictly more protective for both sides.

## Versions released before 2026

Releases up to and including **v0.1.0-alpha.3** were published under
`MIT OR Apache-2.0`. That dual grant is permanent for those versions and is not
revoked — anyone holding them keeps the choice of either licence.

[`LICENSE-MIT`](LICENSE-MIT) is retained in this repository **solely as the
record of that historical grant.** It does not apply to current releases.
The licence for everything from here on is [`LICENSE`](LICENSE), Apache-2.0.

## What is not covered by the licence

### The trademark

The marks *AgentOS* and *agentOS*, the project identity, and the associated
branding are reserved by the author. No copyright licence grants trademark
rights — Apache-2.0 §6 says so explicitly. Details in [`NOTICE`](NOTICE).

**You may:** say "built on AgentOS", "compatible with AgentOS", "a fork of
AgentOS".
**You may not:** name a fork, product, or hosted service *AgentOS*, or imply
official endorsement.

Fork the code freely. Just give your fork its own name.

### The Enterprise Edition

Multi-tenancy, RBAC/SSO, hosted trace storage, audit logging, fleet analytics,
managed integrations, the cloud control plane, and commercial support are a
separate, proprietary product — currently **in development, not yet
purchasable**. It is not in this repository and is not under Apache-2.0.

See [`docs/editions.md`](docs/editions.md) for the exact boundary and its
rationale.

## Contributing

Contributions are accepted under the
[Contributor Licence Agreement](.github/CLA.md), signed with a
`Signed-off-by` trailer (`git commit -s`).

The CLA grants the right to ship your contribution in **both** the open-source
core and the commercial edition. **You keep the copyright to your work** — it is
a licence, not an assignment, and you remain free to use your own contribution
anywhere else on any terms.

This is the standard Open Core arrangement (GitLab, Elastic, and others use the
same structure). It exists so the project can sustain itself commercially
without the licensing history becoming unusable.

## Who needs to contact me

| Situation | Action |
|---|---|
| Running AgentOS in production, any scale | Nothing. Go ahead. |
| Building and selling a product on top of it | Nothing. Go ahead. |
| Forking it under a different name | Nothing. Go ahead. |
| Want Enterprise features or support | Email me |
| Want to use the *name* AgentOS | Email me |
| Not sure | Email me — a written answer costs nothing |

**WAHIB EL KHADIRI** — <wahibelkhadiri06@gmail.com>

Genuine open-source projects, students, researchers, and non-profits: the
answer to almost anything is yes, in writing, free.
