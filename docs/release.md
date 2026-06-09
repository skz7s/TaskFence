# Release Process

TaskFence has not shipped a stable release yet. This process defines the gate
for future preview, beta, and stable releases without claiming production
support before it exists.

## Release Types

- Preview: local development and demonstration surfaces that pass the workspace
  validation gate.
- Beta: surfaces with documented operational boundaries and enough integration
  coverage for real pilot use.
- Stable: production-supported surfaces with security, compatibility,
  migration, and incident-response expectations.

The current project status is local preview.

Versioning and compatibility expectations are documented in
[docs/versioning.md](versioning.md). A release must not claim stable CLI,
task-file, audit-event, gateway, runner, or state compatibility beyond that
policy.

## Pre-Release Checklist

Run from the repository root:

```bash
bash deploy/manage.sh readiness
bash -n deploy/manage.sh
cargo fmt --all --check
cargo check --workspace --locked
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo test --workspace --locked
RUSTDOCFLAGS="-D warnings" cargo doc --workspace --no-deps --locked
cargo package --workspace --no-verify --locked
python3 scripts/governance/build_agents.py --check
python3 scripts/governance/check_codex_governance.py
```

Confirm the GitHub Actions workflow for the release branch has passed the same
Rust, governance, shell syntax, and readiness checks. Superseded workflow runs
may be canceled by CI concurrency; use the latest run for release evidence.

When Docker, database, remote runner, or live connector integration tests are
unavailable, record that limitation in release notes instead of claiming
coverage.

Use [docs/testing.md](testing.md) to decide which focused, workspace, example,
Docker, and live-environment checks apply to the release.

Follow [docs/supply-chain.md](supply-chain.md) for dependency-update review,
package publication checks, and optional external advisory/license/source
checks. `cargo-audit` and `cargo-deny` are recommended when available, but they
are not mandatory release gates until configured in this repository and CI.

For the public repository visibility switch, use
[docs/publication-readiness.md](publication-readiness.md). Repository
publication is separate from crate publication, tags, release binaries,
containers, or package-manager artifacts.

## Release Notes

Start from [docs/release-notes-template.md](release-notes-template.md).

Each release note should include:

- release type and version
- implemented surfaces
- unsupported surfaces
- security-relevant changes
- migration or compatibility notes
- dependency and supply-chain notes
- validation commands run
- integration coverage and skipped coverage by surface
- skipped integration tests and why they were skipped

## Publication

Crates should not be published until crate descriptions, README links, licensing
metadata, API stability expectations, compatibility policy, and semver impact
have been reviewed.
Run `cargo package -p taskfence-core` as the first full packaging verification.
Other TaskFence crates depend on internal crates by version plus path, so their
full `cargo package` verification will resolve those internal dependencies from
crates.io and requires publishing in dependency order. Before that first publish
wave, use `cargo package --workspace --no-verify --locked` to verify package
manifests and included files against the committed dependency graph without
claiming that unpublished internal dependencies are already available from
crates.io.

Keep [docs/installation.md](installation.md) aligned with the actual
publication state. Do not document `cargo install`, package-manager installs,
release binaries, or container images until those artifacts exist and their
provenance expectations are recorded.

Do not publish release artifacts, merge release branches, or push tags without
operator approval.
