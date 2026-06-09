# Locked Package Gate

## Context

TaskFence CI and release gates already use `--locked` for check, clippy, tests,
and rustdoc, but the package manifest inspection command used
`cargo package --workspace --no-verify` without `--locked`. That allowed
package inspection to resolve dependencies outside the committed lockfile even
though the rest of the gate was lockfile-bound.

## Decision

TaskFence package manifest inspection will use
`cargo package --workspace --no-verify --locked` in CI and release
documentation.

## Consequences

- CI and release candidates inspect package manifests against the committed
  dependency graph.
- Package inspection remains a manifest/inclusion check before the first
  publish wave, not proof that unpublished internal crates are already
  available from crates.io.
- If the lockfile is stale, package inspection fails rather than silently
  resolving newer dependency versions.

## Validation And Rollback

Validate with `cargo package --workspace --no-verify --locked` plus the normal
governance checks. Rollback is to remove `--locked`, but that should happen
only if Cargo behavior changes and the release gate documents the replacement
lockfile guarantee.
