# Publication Readiness Boundary

## Context

TaskFence is being prepared for public open-source visibility, but the
repository remains a local-preview project. Making the GitHub repository public
is a different action from publishing crates, tagging releases, shipping
containers, offering hosted services, or declaring production support.

## Decision

TaskFence will maintain `docs/publication-readiness.md` as the checklist for
repository visibility publication. It records local evidence, external GitHub
settings to verify, publication steps, and unsupported production surfaces. The
repository visibility switch must not imply crate publication, signed release
artifacts, package-manager installation, production deployment, or expanded
support.

## Consequences

- Maintainers have a concrete final checklist before switching GitHub
  visibility to public.
- External GitHub settings remain explicitly external state and are not claimed
  unless verified.
- Release and installation docs continue to control package, binary, container,
  and production support claims.

## Validation And Rollback

Validate with live-doc governance checks, the release/readiness commands, and a
GitHub visibility check. If repository publication is delayed or GitHub
settings change, update `docs/publication-readiness.md` rather than broadening
runtime readiness claims.
