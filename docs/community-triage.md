# Community Triage

This guide describes how public TaskFence issues should be routed while the
project is in local preview. It is a maintainer and contributor workflow, not a
production support commitment.

Use this guide with [SUPPORT.md](../SUPPORT.md),
[SECURITY.md](../SECURITY.md), and
[docs/config/readiness-checklist.md](config/readiness-checklist.md).

## Triage Priorities

Handle incoming reports in this order:

1. Security reports, approval bypasses, sandbox escapes, secret exposure,
   gateway credential leaks, and audit integrity issues.
2. Reproducible fail-closed or policy boundary bugs in implemented local
   preview surfaces.
3. Regression reports with a TaskFence commit, command, task file shape, and
   validation summary.
4. Documentation gaps that could cause unsafe setup, unsupported production
   claims, or incorrect validation expectations.
5. Feature proposals inside the task-level runtime and gateway boundary.
6. General local-preview support questions.

Do not ask reporters to post secrets, private repository URLs, customer data,
or credential-bearing logs in public issues. If a public issue contains
sensitive material, remove or redact the material through GitHub moderation
tools before continuing technical discussion.

## Routing

Use the issue type and labels to choose the first action:

| Label | First action |
| --- | --- |
| `bug` | Confirm the affected implemented surface, reproduction command, expected fail-closed behavior, and actual behavior. |
| `documentation` | Confirm the owning page and whether the problem is stale behavior, missing limitation, broken command, or unclear wording. |
| `enhancement` | Confirm the problem statement, security impact, non-goals, and whether the proposal fits the core feature boundary. |
| `question` | Point to the relevant local-preview documentation or ask for the missing command, task file shape, OS, Rust version, and validation summary. |
| `dependencies` | Follow [docs/supply-chain.md](supply-chain.md) and record MSRV, lockfile, advisory, and release-note impact. |
| `good first issue` | Use only for low-risk documentation, examples, tests, or narrow local-preview fixes with clear acceptance criteria. |
| `help wanted` | Use when the scope is well bounded and maintainers can review the security boundary without extra private context. |

Use `invalid`, `duplicate`, or `wontfix` only with a short explanation and a
link to the canonical issue or documented boundary. For unsupported production
surfaces, link to [docs/config/readiness-checklist.md](config/readiness-checklist.md)
and close or convert the issue only after preserving any useful requirement or
security signal.

## Reproduction Expectations

Bug reports should include:

- TaskFence commit or version.
- Operating system and Rust version.
- Exact command and task file shape, with secrets removed.
- Whether Docker, SSH, database, or live connector credentials were involved.
- A short validation summary, not a full credential-bearing log.

If the report touches an integration that maintainers cannot run locally,
classify it as environment-backed coverage. Ask for the smallest safe
reproduction and record unavailable Docker, database, remote host, or live
connector coverage in the issue or pull request.

## Feature Proposal Expectations

Feature proposals should state:

- user or operator problem
- proposed behavior
- security and policy impact
- non-goals
- intended readiness level

Reject or defer proposals that would turn TaskFence into a general-purpose
agent framework, expose gateway credentials to sandboxes, bypass policy or
approval decisions, or claim production support for unsupported surfaces.

## Closure

Close an issue when:

- a fix or documentation update has merged
- the report is a duplicate and the canonical issue is linked
- the requested behavior is outside the project boundary
- the issue lacks enough information after a reasonable maintainer request
- the report is a security issue that must move to the private reporting path

When closing without code, leave enough context for future maintainers to
understand the decision without relying on private chat or external state.
