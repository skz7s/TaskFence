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

## Pre-Release Checklist

Run from the repository root:

```bash
bash deploy/manage.sh readiness
bash -n deploy/manage.sh
cargo fmt --all --check
cargo check --workspace --locked
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo test --workspace --locked
python3 scripts/governance/build_agents.py --check
python3 scripts/governance/check_codex_governance.py
```

Confirm the GitHub Actions workflow for the release branch has passed the same
Rust, governance, shell syntax, and readiness checks.

When Docker, database, remote runner, or live connector integration tests are
unavailable, record that limitation in release notes instead of claiming
coverage.

## Release Notes

Each release note should include:

- release type and version
- implemented surfaces
- unsupported surfaces
- security-relevant changes
- migration or compatibility notes
- validation commands run
- skipped integration tests and why they were skipped

## Publication

Crates should not be published until crate descriptions, README links, licensing
metadata, API stability expectations, and semver policy have been reviewed.

Do not publish release artifacts, merge release branches, or push tags without
operator approval.
