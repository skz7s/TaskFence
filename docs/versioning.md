# Versioning And Compatibility

TaskFence has not shipped a stable release. The current project status is
local preview with `0.x` crate versions.

This policy describes how maintainers should treat compatibility before the
first stable release.

## Release Levels

TaskFence uses the release levels in [docs/release.md](release.md):

- preview: local development and demonstration surfaces that pass the workspace
  validation gate
- beta: surfaces with documented operational boundaries and enough integration
  coverage for real pilot use
- stable: production-supported surfaces with security, compatibility,
  migration, and incident-response expectations

Unsupported and contract-only surfaces are not compatibility commitments.

## Rust And Package Compatibility

The workspace currently targets Rust 1.88 or newer through
`workspace.package.rust-version`.

Raising the minimum supported Rust version is a compatibility-impacting change.
Before doing it, maintainers should:

- confirm the effective dependency tree requires the newer Rust version
- update `Cargo.toml`, `README.md`, `CONTRIBUTING.md`,
  `docs/config/readiness-checklist.md`, and release notes together
- keep the GitHub Actions MSRV job aligned with the declared MSRV

Workspace crates are versioned together during preview. Internal TaskFence
dependencies use both `path` and `version` so local development works before
publication and package metadata remains reviewable. A first crates.io publish
wave must publish internal dependencies in dependency order. Until independent
crate stability is documented, do not promise that individual TaskFence crates
can be upgraded independently. `docs/installation.md` must keep the public
installation guidance aligned with the actual package publication state.

## Semver Policy During 0.x

TaskFence follows the spirit of semantic versioning, but `0.x` releases are
preview releases. Breaking changes may occur in minor releases when needed to
preserve the security boundary, simplify unstable contracts, or correct
overclaimed behavior.

Patch releases should avoid intentional breaking changes except for security
fixes, fail-closed corrections, or documentation fixes that remove unsafe or
unsupported claims.

Before a stable `1.0` release, maintainers must identify which CLI commands,
task-file fields, audit event shapes, gateway actions, runner contracts, and
state APIs are stable public contracts.

## Task File Compatibility

The current task-file contract is preview-level. The Rust policy contract
records `task_file` schema version `1`, but task YAML files do not yet expose a
top-level public schema-version field.

Compatible task-file changes usually include:

- adding optional fields with secure defaults
- adding new enum variants that fail closed when unsupported
- adding stricter validation for unsafe configurations
- improving error messages without changing accepted safe input

Breaking task-file changes include:

- removing or renaming fields
- changing default policy behavior from deny to allow
- widening path, command, network, secret, gateway, or artifact access by
  default
- treating unsupported runner or gateway behavior as supported without the
  required tests and docs

When a task-file change is intentionally breaking, release notes must include
the old shape, the new shape, the migration path, and any fail-closed behavior
users should expect.

Task-file changes must keep [docs/task-file-reference.md](task-file-reference.md)
aligned with the parser and examples.

## CLI Compatibility

The `taskfence` CLI is preview-level. Maintainers should keep documented
commands stable when practical, but security fixes may tighten behavior.

CLI changes must update examples, quickstart docs, release docs, and issue/PR
templates when they alter user-facing commands or validation expectations.
Removing a documented command should include at least one preview release note
of warning unless the old command is unsafe.

CLI changes must keep [docs/cli-reference.md](cli-reference.md) aligned with
the Clap command tree.

## Evidence And API Compatibility

Structured audit events, reports, replay inputs, local state indexes, team
state records, connector policy templates, and runner capability contracts are
preview contracts. Consumers should tolerate added fields and new explicit
unsupported states.

Compatibility-sensitive evidence changes must preserve:

- redacted secret references instead of raw secret values
- structured policy and approval decisions
- enough task, runner, gateway, artifact, and state context to reconstruct a
  task without scraping terminal output
- explicit unsupported-operation evidence for contract-only surfaces

Rendered Markdown reports are review artifacts, not source-of-truth migration
state.

## Deprecation Rules

Preview deprecations should be documented in `CHANGELOG.md` and release notes.
When security allows, keep a deprecated behavior for one minor preview release
before removal. Security boundary fixes may remove or fail-closed unsafe
behavior immediately, but the release note must call that out.

## Production Readiness

No surface should be described as stable or production-supported until it meets
the readiness bar in [docs/config/readiness-checklist.md](config/readiness-checklist.md)
and the security review bar in [docs/security-model.md](security-model.md).
