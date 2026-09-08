# Maintainers

AgentOS is created and led by **WAHIB EL KHADIRI**.

## Lead Maintainer

| Name | Role |
| --- | --- |
| WAHIB EL KHADIRI | Founder, lead developer, project steward |

## Maintainer Responsibilities

Maintainers are responsible for:

- protecting the project vision
- reviewing large architecture changes
- keeping issues and discussions organized
- guiding contributors toward useful work
- preserving attribution and project identity
- making sure the runtime stays focused

## Contribution Stewardship

AgentOS welcomes contributors, but the project direction is stewarded by WAHIB
EL KHADIRI.

Large changes should be discussed before implementation, especially changes to:

- crate boundaries
- protocol design
- security model
- runtime lifecycle
- project name or identity
- licensing and ownership

## Branch Protection And Review

`main` requires one approving review, twelve passing status checks, an
up-to-date branch and linear history. Administrator enforcement is deliberately
off.

There is one maintainer, and GitHub does not let an author approve their own
pull request. With administrator enforcement on, every pull request opened by
the lead maintainer would be permanently unmergeable. The exception is the
escape hatch that keeps the project moving, not an oversight.

The cost is real and worth stating plainly:

- OpenSSF Scorecard reports `Branch-Protection` and `Code-Review` as failing.
  Both are accurate. They measure review that a single maintainer cannot
  perform on their own work.
- An administrator merge bypasses the required checks as well as the review,
  so the checks are a discipline here, not a guarantee. The rule the project
  holds itself to is that checks are green before an administrator merge, and
  the run is linked from the pull request.

The two alternatives are worse. Turning administrator enforcement on freezes
the repository. Dropping the review requirement removes the gate for outside
contributions, which is the case where it actually works.

This is revisited the moment a second maintainer has write access: at that
point administrator enforcement goes on, administrator merges stop, and the two
Scorecard findings close on their own.
