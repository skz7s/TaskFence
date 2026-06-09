# Markdown Link Release Gate

## Context

TaskFence is being prepared for public open-source visibility. The repository
now has a larger public documentation surface across README, contributor
guides, support/security docs, release docs, examples, GitHub templates, and
governance entry documents. Broken local links would make the project harder to
evaluate and maintain after publication.

## Decision

Add a dependency-free Markdown relative-link checker at
`scripts/docs/check_markdown_links.py` and include it in CI, release,
readiness, contributor, maintainer, testing, supply-chain, troubleshooting, and
publication-readiness documentation.

The checker validates local Markdown link targets for public-facing Markdown
files and selected governance entry documents. It intentionally skips external
URLs, fragment-only anchors, generated governance output, archived Codex plans,
and active plan files.

## Consequences

- Public documentation changes now have a lightweight local broken-link gate.
- CI can catch moved or deleted docs before publication.
- Historical archived plans are not forced to keep old links alive.

## Validation And Rollback

Validate with:

```bash
python3 scripts/docs/check_markdown_links.py
python3 scripts/governance/check_codex_governance.py
```

Rollback is to remove the CI step and release-doc references, then delete the
script if a fuller documentation tooling stack replaces it.
