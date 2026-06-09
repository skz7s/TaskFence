# CODEOWNERS Review Routing

## Context

TaskFence now has public contribution, support, security, issue-template, and
release documentation, but the repository did not expose a default review owner
for pull requests. The Git remote and package metadata identify the public
repository owner as `skz7s`.

## Decision

TaskFence will maintain `.github/CODEOWNERS` with a repository-wide default
review route to `@skz7s`. This is a lightweight open-source triage signal, not
a production support commitment or a module-level ownership model.

## Consequences

- Pull requests have a clear default reviewer path.
- Future module-specific ownership can be added only when maintainers are
  explicitly confirmed.
- Security-sensitive or production-readiness changes still follow
  `SECURITY.md`, `docs/security-model.md`, `docs/maintainers.md`, and
  `docs/config/readiness-checklist.md`.

## Validation And Rollback

Validate with repository metadata checks and generated-governance live-doc
checks. If ownership changes, update `.github/CODEOWNERS`,
`docs/maintainers.md`, and this decision record together.
