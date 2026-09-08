# AgentOS Contributor Licence Agreement

**Version 1.0**

Thank you for contributing to AgentOS. This agreement clarifies the
intellectual-property licence granted with contributions from any person or
entity. It protects you, it protects every user of AgentOS, and it keeps the
project's licensing history clean.

Please read it carefully before your first contribution.

---

## Why this exists

AgentOS is **Open Core**: the core is Apache-2.0 open source, and a separate
commercial edition is offered under a proprietary licence
(see [`docs/editions.md`](../docs/editions.md)).

For that model to work, the project maintainer must be able to ship
contributed code in **both** editions. A plain inbound=outbound licence would
permit only the open-source half. This agreement grants the extra permission —
and nothing more than that.

**You keep the copyright to your work.** This is a licence, not an assignment.
You may use, sell, and relicense your own contribution anywhere else, without
restriction and without asking.

---

## 1. Definitions

**"You"** means the individual or legal entity agreeing to these terms. For a
legal entity, "You" includes any entity that controls, is controlled by, or is
under common control with that entity.

**"Contribution"** means any original work of authorship, including any
modification to or addition to an existing work, that You intentionally submit
to AgentOS for inclusion in the project. "Submit" means any form of
communication sent to the maintainer or the project — pull requests, patches,
issues, and discussion — excluding anything you clearly mark in writing as
"Not a Contribution".

**"Project"** means the AgentOS project, owned and maintained by
WAHIB EL KHADIRI.

## 2. Copyright licence

You grant to WAHIB EL KHADIRI and to recipients of software distributed by the
Project a perpetual, worldwide, non-exclusive, no-charge, royalty-free,
irrevocable copyright licence to reproduce, prepare derivative works of,
publicly display, publicly perform, sublicense, and distribute Your
Contribution and such derivative works.

This licence expressly includes the right to **sublicense the Contribution
under any licence terms, including proprietary and commercial terms**, so that
Your Contribution may be shipped in both the open-source core and the
commercial edition of AgentOS.

## 3. Patent licence

You grant to WAHIB EL KHADIRI and to recipients of software distributed by the
Project a perpetual, worldwide, non-exclusive, no-charge, royalty-free,
irrevocable (except as stated in this section) patent licence to make, have
made, use, offer to sell, sell, import, and otherwise transfer Your
Contribution, where such licence applies only to those patent claims licensable
by You that are necessarily infringed by Your Contribution alone or by
combination of Your Contribution with the Project.

If any entity institutes patent litigation against You or any other entity
alleging that Your Contribution, or the Project to which You contributed,
constitutes direct or contributory patent infringement, then any patent
licences granted to that entity under this agreement for that Contribution
terminate as of the date such litigation is filed.

## 4. You retain your rights

You retain all right, title, and interest in and to Your Contribution. Nothing
in this agreement transfers ownership. You are free to use Your Contribution
for any other purpose, under any terms you choose.

## 5. Your representations

You represent that:

1. Each Contribution is Your original creation, or You have the right to submit
   it under the terms of this agreement.
2. You are legally entitled to grant the licences above. If your employer holds
   rights to work you create, you have received permission to make the
   Contribution on their behalf, or your employer has waived those rights.
3. Your Contribution does not knowingly include third-party code, trade
   secrets, or material subject to a licence incompatible with this agreement.

If You submit a Contribution that includes work You did not author, You will
identify the source and the licence it carries in the pull request.

## 6. Third-party material

If You wish to submit work that is not Your original creation, submit it
separately from any Contribution, clearly marked as
"Submitted on behalf of a third party: [name] — licensed under [licence]".

## 7. No obligation, no warranty

You are not expected to provide support for Your Contribution. Contributions
are provided "AS IS", without warranty of any kind, express or implied, to the
fullest extent permitted by law.

Nothing in this agreement obliges the Project to accept, merge, or ship any
Contribution.

## 8. Governing terms

This agreement is the complete agreement concerning Contributions. It does not
create an employment, partnership, agency, or joint-venture relationship, and
does not entitle You to compensation.

---

## How to accept

There are two separate requirements. They mean different things, and one does
not substitute for the other.

### 1. Agree to this CLA — once, in your first pull request

Post this sentence as a comment on your first pull request:

```
I have read the AgentOS Contributor Licence Agreement (CLA v1.0)
and I agree to it.
```

The maintainer records your GitHub username, the pull request, and the date in
[`cla-signatures.md`](cla-signatures.md). That file is the project's record of
who has accepted, and which version they accepted. You do this once, not per
pull request.

If you are contributing on behalf of a company, say so in the same comment and
name the company.

### 2. Sign off every commit — the Developer Certificate of Origin

Add a `Signed-off-by` trailer to every commit:

```
git commit -s -m "feat(kernel): add supervision backoff"
```

which appends:

```
Signed-off-by: Your Name <your.email@example.com>
```

Use your real name and an email address you control. Configure git once so it
is automatic:

```
git config user.name  "Your Name"
git config user.email "your.email@example.com"
```

This is checked automatically by CI on every pull request, and only against
the commits that pull request introduces.

### Why both, and why they are not the same thing

The `Signed-off-by` trailer is the **Developer Certificate of Origin**. It
certifies one thing: that you wrote the work, or otherwise have the right to
submit it. That is its settled, widely understood meaning across the free
software world, and this project does not redefine it.

The DCO contains **no licence grant beyond the project's own licence.** It
does not, and cannot, express the sublicensing permission in section 2 above —
the permission that lets a contribution ship in the commercial edition.

So a signed-off commit is not, by itself, agreement to this CLA. That is why
step 1 exists and is recorded separately and explicitly. Being asked for a
plain sentence rather than having it inferred from a git trailer is the
honest way round: you should know what you agreed to, and the project should
be able to show that you did.

## Questions

If any part of this agreement blocks you from contributing — in particular if
your employer's policy conflicts with it — open a discussion or email
<wahibelkhadiri06@gmail.com>. Bring the concern; there is usually a way through.
