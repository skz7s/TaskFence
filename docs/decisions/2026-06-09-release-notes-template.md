# Release Notes Template

## Context

TaskFence release documentation requires release notes to separate implemented
surfaces from unsupported or skipped coverage. Without a reusable template,
maintainers could omit security, compatibility, integration, or supply-chain
limitations during preview releases.

## Decision

TaskFence will maintain `docs/release-notes-template.md` as the default release
notes structure for preview, beta, and stable releases. Release notes should
record implemented surfaces, unsupported surfaces, security-relevant changes,
compatibility impact, validation commands, skipped integration coverage,
dependency and supply-chain notes, known issues, and operator approval for
merge, tag, artifact, or crate publication.

## Consequences

- Maintainers have a consistent release-note checklist before publishing.
- Preview releases can avoid overclaiming unsupported production behavior.
- Skipped Docker, database, remote runner, live connector, browser/UI, and
  external advisory-tool coverage remains visible to users.
- The template must stay aligned with `docs/release.md`,
  `docs/config/readiness-checklist.md`, `docs/testing.md`, and
  `docs/supply-chain.md`.

## Validation And Rollback

Validate changes with generated-governance and live-doc checks. If the template
becomes stale, update or remove its links in the same change rather than
letting release docs point at an obsolete checklist.
