# Installation Distribution Boundary

## Context

TaskFence has source-build quickstart and release documentation, but no
published crates.io packages, release binaries, package-manager formulas,
containers, or hosted production service. Public installation docs need to make
that boundary explicit so users do not assume unsupported artifacts exist.

## Decision

TaskFence will maintain `docs/installation.md` as the public installation
entrypoint. Until artifacts are actually published, installation guidance is
limited to source checkout plus local Cargo builds. Documentation must not
claim `cargo install`, package-manager installation, signed release binaries,
production containers, or hosted-service deployment until those artifacts exist
and their release, supply-chain, and provenance expectations are documented.

## Consequences

- New users have a clear source-build path for the local preview.
- Release and supply-chain docs have a single installation-status document to
  keep aligned with package publication.
- Future distribution work must update installation, release, readiness,
  versioning, and supply-chain docs together.

## Validation And Rollback

Validate with generated-governance live-doc checks and the source-build help
command in `docs/installation.md`. If published artifacts are added later,
update this decision or supersede it with a new distribution decision.
