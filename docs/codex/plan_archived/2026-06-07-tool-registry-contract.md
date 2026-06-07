# Gateway Tool Registry Contract Plan

## Goal

Add a narrow gateway-owned tool registry contract so gateway mediation can
optionally reject unregistered tool actions before policy evaluation, without
executing real MCP, HTTP, CLI wrapper, SDK, webhook, or secret-broker actions.

## Overall Status

done

## Plan Source

The operator asked to continue follow-up development and commit the completed
work. This slice advances Phase 3 by adding a typed registry boundary for
known tool operations. It does not add production MCP/HTTP execution, GitHub
API calls, credential use, a Web UI registry, cross-workspace state, or dynamic
tool discovery.

## Intake / Snapshot

- Status: done
- Date: 2026-06-07
- Default branch: `main` from `origin/HEAD`
- Working branch: `codex/governance-development-plan`
- Worktree at intake: clean
- Latest commit at intake: `b78dd1c feat: add explicit budget policy limits`
- Sync attempt: `git pull --ff-only` failed because the current branch has no upstream tracking branch.
- Existing behavior: gateway mediation normalizes protocol-shaped actions,
  checks supported protocols, evaluates policy, and records audit evidence; it
  has no registry boundary for known tool operations.
- Next executable phase: add registry types and optional mediator validation.

## Scope

- Add typed gateway tool registration structs.
- Normalize registry keys through the existing tool action normalization path.
- Add an in-memory registry implementation for tests and local composition.
- Let `GatewayMediator` optionally validate normalized tool actions against a
  configured registry before policy evaluation.
- Record an audit error and return an explicit gateway error for unregistered
  tool actions when a registry is configured.
- Update docs that describe Phase 3 gateway contract coverage.

## Non-Goals

- No real MCP, HTTP, CLI wrapper, SDK, webhook, or secret-broker execution.
- No GitHub API integration.
- No dynamic registry discovery.
- No Web UI, API server, SQLite, or team-server registry state.
- No task-file schema changes.

## Acceptance Criteria

- Gateway code exposes typed registered tool entries and a registry lookup
  contract.
- Registered tool entries normalize protocol, tool, and operation values.
- Empty registry segments are rejected.
- Existing gateway mediation behavior remains compatible when no registry is
  configured.
- When a registry is configured, registered actions continue to policy
  evaluation.
- When a registry is configured, unregistered actions fail before policy
  evaluation and write an audit error.

## Phases

1. Registry contract and implementation
   - Status: done
   - Scope: add gateway registry types, normalization, and tests.
   - Verification: `cargo test -p taskfence-gateway`
   - Evidence: passed before docs updates with 21 gateway tests.
2. Mediator validation and docs
   - Status: done
   - Scope: add optional registry validation in `GatewayMediator` and update
     README/architecture/roadmap/runtime docs.
   - Verification: `cargo test -p taskfence-gateway`
   - Evidence: passed with 21 gateway tests after registry and docs updates;
     docs updated and ADR added at
     `docs/decisions/2026-06-07-gateway-tool-registry-contract.md`.
3. Validation, archive, and commit
   - Status: done
   - Scope: run formatting, focused tests, workspace gates, archive this plan,
     stage task-owned files, and commit.
   - Verification: `cargo fmt --all --check`; `git diff --check`;
     `cargo test -p taskfence-gateway`;
     `cargo clippy --workspace --all-targets -- -D warnings`;
     `cargo test --workspace`
   - Evidence: all listed validation commands passed. Workspace tests kept the
     Docker integration test ignored as designed because it requires a Docker
     daemon and locally available test image.

## Commit Plan

1. `feat: add gateway tool registry contract`

## Final Evidence

- Added typed gateway tool registry keys, registered tool entries, an in-memory
  registry, and optional mediator registry validation before policy evaluation.
- Added tests for key normalization, empty segment rejection, normalized
  registry matching, registered action policy continuation, and unregistered
  action fail-closed behavior with audit evidence.
- Updated README, architecture, roadmap, runtime architecture, development
  design, and the ADR at
  `docs/decisions/2026-06-07-gateway-tool-registry-contract.md`.
- Validation passed: `cargo fmt --all --check`, `git diff --check`,
  `cargo test -p taskfence-gateway`, `cargo clippy --workspace --all-targets -- -D warnings`,
  and `cargo test --workspace`.
