# Dependabot Security Settings

## Context

TaskFence is being prepared for public open-source visibility. Repository
security settings are external GitHub state, but dependency vulnerability
alerts and automated security fixes are basic open-source maintenance
guardrails that can be enabled before switching repository visibility.

## Decision

Enable GitHub vulnerability alerts and Dependabot automated security fixes for
the repository before publication, then record their observed status in
`docs/publication-readiness.md`.

This does not claim that all GitHub security features are configured. Branch
protection, rulesets, private vulnerability reporting, secret scanning, and
push protection remain separate publication-readiness checks because the API
queries from this checkout were unavailable, disabled, or blocked by private
repository plan and visibility restrictions.

## Consequences

- Maintainers can see dependency security findings before the repository is
  public.
- Dependabot security fixes can propose update pull requests when GitHub
  identifies an actionable dependency advisory.
- Publication readiness still requires a final external GitHub settings review.

## Validation And Rollback

Validated through the GitHub API from this checkout:

- `PUT /repos/skz7s/TaskFence/vulnerability-alerts` returned `204 No Content`.
- `GET /repos/skz7s/TaskFence/vulnerability-alerts` returned `204 No Content`.
- `PUT /repos/skz7s/TaskFence/automated-security-fixes` returned
  `204 No Content`.
- `GET /repos/skz7s/TaskFence/automated-security-fixes` returned `200 OK`.
- `GET /repos/skz7s/TaskFence/dependabot/alerts?state=open` returned `200 OK`.

Rollback is to disable those GitHub repository settings and update
`docs/publication-readiness.md` with the observed state and reason.
