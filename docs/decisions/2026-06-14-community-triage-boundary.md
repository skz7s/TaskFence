# Community Triage Boundary

## Context

TaskFence is preparing for public open-source visibility while remaining a
local-preview project. Public issues need a repeatable routing policy so
contributors and maintainers can separate security reports, reproducible
preview bugs, documentation gaps, support questions, and feature proposals
without implying production support.

## Decision

TaskFence will maintain `docs/community-triage.md` as the public issue routing
guide. It defines triage priority, label use, reproduction expectations,
feature proposal expectations, unsupported-surface routing, and closure
criteria for local-preview reports.

The guide is a community maintenance workflow. It does not create a hosted
support channel, service-level commitment, production readiness claim, or
permission to discuss secrets and vulnerability details in public issues.

## Consequences

- Maintainers have a documented first action for common public issue labels.
- Contributors can see what information is needed before filing or triaging an
  issue.
- Unsupported production-surface reports can still preserve useful
  requirements or security signals without widening the supported boundary.
- Security-sensitive reports continue to route through `SECURITY.md`.

## Validation And Rollback

Validate with Markdown link checks and the publication-readiness checklist. If
GitHub Discussions, a private vulnerability reporting path, or a different
support model is enabled later, update `docs/community-triage.md`,
`SUPPORT.md`, `docs/maintainers.md`, and this decision record together.
