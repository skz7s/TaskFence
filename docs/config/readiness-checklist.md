# TaskFence Readiness Checklist

This checklist separates implemented local preview surfaces from beta and
production-supported surfaces. It is a release and operator-readiness aid, not a
deployment claim.

## Local Preview Supported

- Rust 1.88 or newer workspace build and validation through `cargo fmt --all`,
  `cargo check --workspace --locked`, `cargo clippy --workspace --all-targets
  --locked -- -D warnings`, and `cargo test --workspace --locked`.
- Local task-file scaffolding, validation, Docker execution when Docker and the
  task image are already available, local review serving, local replay for
  supported inputs, bounded gateway fixture/GitHub/enterprise connector
  surfaces, remote SSH runner under its explicit capability contract, and local
  team-state CLI/state-layer operations.
- `deploy/manage.sh detect-env`, `doctor`, `setup`, `dev`, and `build` for
  repo-local Rust development and build checks.

## Beta Candidates

- Production API daemon contract with local/team modes, health/readiness,
  structured diagnostics, and authenticated routes.
- MCP gateway transport and bounded HTTP adapter routes with request auth,
  limits, redaction, timeout, rate-limit, and structured error behavior.
- Production review UI after the daemon/API contract is implemented and browser
  validation gates are selected.
- Kubernetes, microVM, and managed cloud runners after each backend proves
  isolation, network, secret, limit, cancellation, teardown, and artifact
  guarantees with integration tests.
- Team API service, worker service, SSO, object storage, and quota reporting
  after their state, access-control, credential, and integrity prerequisites
  are implemented.

## Not Production Supported Yet

- Long-lived production API daemon, deployed team server, production Web UI,
  production MCP server, arbitrary HTTP proxy, SDK/webhook connectors, Slack,
  SSO provider integration, object storage adapter, background audit export
  service, Kubernetes/microVM/managed-cloud live execution, and live replay of
  destructive or externally visible connector effects.

## Release Gate

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

Docker, database, remote runner, and live connector integration tests require
matching local services or credentials. When unavailable, record the exact
limitation in release notes instead of claiming coverage.

## Repository Automation

GitHub Actions checks the Rust workspace gate with locked dependencies,
generated-governance drift, governance asset health, shell syntax, and
readiness output for pull requests and pushes to `main`. Dependabot proposes
Cargo and GitHub Actions dependency updates weekly. CI does not imply Docker,
database, remote runner, or live connector integration coverage unless a
workflow explicitly provisions those services.
