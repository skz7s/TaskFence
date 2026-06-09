# Open Source Release Boundary

## Context

TaskFence is moving from local implementation work toward a public open-source
project. The repository already contains substantial runtime, gateway,
governance, and readiness contracts, but public collaboration surfaces and
release automation were missing. Adding those surfaces changes project
lifecycle policy and should be reviewable later.

## Decision

Add an open-source release shell before claiming broader production readiness:

- public contribution, support, security, maintainer, changelog, and release
  process documents
- pull request and issue templates that ask contributors to state validation
  evidence and security impact
- GitHub Actions checks for Rust formatting, linting, tests, governance drift,
  shell syntax, and readiness output
- Dependabot dependency update proposals for Cargo and GitHub Actions
- package metadata improvements that make crate intent, licensing, repository,
  and documentation links explicit

This decision does not make unsupported production surfaces live. The current
status remains local preview, with beta and production boundaries documented in
the readiness checklist.

## Consequences

External contributors have a clearer path for safe changes, and maintainers get
standard review inputs before merging. CI will exercise the same core gates
documented for local development, while integration tests that require Docker,
databases, remote hosts, or live connector credentials remain explicitly
separate.

The repository must keep `README.md`, `CONTRIBUTING.md`, `SECURITY.md`,
`CHANGELOG.md`, `docs/release.md`, and `docs/config/readiness-checklist.md`
aligned when supported surfaces change.

## Validation And Rollback

Validation is the documented release shell gate:

```bash
bash -n deploy/manage.sh
cargo fmt --all --check
cargo check --workspace --locked
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo test --workspace --locked
python3 scripts/governance/build_agents.py --check
python3 scripts/governance/check_codex_governance.py
```

Rollback is to remove the collaboration/CI/release shell files and package
metadata additions. No runtime behavior or deployed service is introduced by
this decision.
