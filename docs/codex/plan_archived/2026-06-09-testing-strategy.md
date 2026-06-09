# Testing Strategy Plan

## Goal

Continue improving TaskFence toward mature open-source readiness by adding a
public testing strategy that explains default CI coverage, focused local test
choices, integration prerequisites, and skipped coverage reporting.

## Plan Source

Continuation of the active 2026-06-09 goal:

> 继续优化项目，包括文档，将项目优化到一个成熟的开源项目的级别后，开源项目

Related archived plans:

- `docs/codex/plan_archived/2026-06-09-open-source-maturity.md`
- `docs/codex/plan_archived/2026-06-09-security-compatibility.md`
- `docs/codex/plan_archived/2026-06-09-cli-task-reference.md`
- `docs/codex/plan_archived/2026-06-09-rustdoc-release-gate.md`

Scope:

- inspect current test inventory and integration-test annotations
- add public testing strategy docs covering default CI, focused crate tests,
  examples, Docker integration, live connector/remote/database coverage, and
  release-note limitations
- link the testing strategy from README, CONTRIBUTING, release/readiness,
  maintainer, quickstart, and change-map docs

Non-goals:

- do not add new runtime tests in this slice
- do not unignore Docker tests or require Docker/live credentials in CI
- do not claim live connector, database, remote host, or Docker coverage when
  those environments are unavailable
- do not edit generated governance outputs directly

Acceptance criteria:

- contributors can choose narrow, workspace, and integration validation paths
- maintainers have a documented rule for skipped Docker/live coverage
- CI/release docs point to the testing matrix
- validation evidence is recorded before committing

## Snapshot

- Date: 2026-06-09
- Default branch: `origin/main`
- Working branch: `codex/governance-development-plan`
- Initial worktree status: clean after commits `f9a76a6` and `ba1983c`
- Test inventory: `cargo test --workspace --locked -- --list` lists the
  workspace unit/CLI/contract/doc tests and one ignored Docker integration test
  in `crates/taskfence-runner/tests/docker_integration.rs`
- Current observed gap: release and PR docs mention skipped integration
  limitations, but no public testing strategy explains which checks are default
  CI, which are local focused checks, and which require external services

## Phases

### 1. Intake And Plan

- Status: done
- Scope: record current branch, observed test inventory, scope, non-goals, and
  acceptance criteria
- Verification command: `git status --short --branch`
- Verification evidence: passed on 2026-06-09. Worktree contains only the new
  active testing strategy plan file.

### 2. Test Inventory Inspection

- Status: done
- Scope: inspect `cargo test --workspace --locked -- --list`, Docker
  integration test annotations, existing CI, and release docs
- Verification command: `cargo test --workspace --locked -- --list`
- Verification evidence: passed on 2026-06-09. The workspace list shows crate
  unit tests, CLI command/evidence tests, gateway/policy/state contract tests,
  doc-tests, and one ignored Docker integration test
  `docker_runner_captures_exit_logs_missing_image_and_timeout` annotated with
  `requires Docker daemon and a locally available test image`.

### 3. Testing Strategy Docs

- Status: done
- Scope: add testing strategy docs and link them from public maintainer and
  release surfaces
- Verification command: `git diff --check && python3 scripts/governance/check_codex_governance.py`
- Verification evidence: passed on 2026-06-09. Added `docs/testing.md` and
  `docs/decisions/2026-06-09-testing-strategy.md`; linked the strategy from
  README, CONTRIBUTING, quickstart, release, readiness, maintainer,
  project-structure, PR template, changelog, and change-map docs. `git diff
  --check`, `python3 scripts/governance/build_agents.py --check`, and
  `python3 scripts/governance/check_codex_governance.py` passed.

### 4. Final Review And Commit

- Status: done
- Scope: run final targeted validation, archive this plan if complete, and
  create focused local commits
- Verification command: `cargo test --workspace --locked -- --list`
- Verification evidence: passed on 2026-06-09. `cargo test --workspace
  --locked -- --list` wrote 382 lines to `/tmp/taskfence-test-list.txt` and
  listed the ignored Docker integration test
  `docker_runner_captures_exit_logs_missing_image_and_timeout`.

## Commit Plan

1. `docs: add testing strategy` - `7584d3e`

## Final Evidence

- Public testing strategy added with default local gate, CI additions, test
  inventory, focused crate ownership, example validation, Docker integration
  prerequisites, live environment limits, documentation/governance checks, and
  coverage reporting expectations.
- Durable decision record added for testing strategy and coverage reporting.
- Public entrypoints and maintainer docs now link the testing matrix.
- Validation passed on 2026-06-09:
  `cargo test --workspace --locked -- --list`, `git diff --check`,
  `python3 scripts/governance/build_agents.py --check`, and `python3
  scripts/governance/check_codex_governance.py`.
- Implementation commit: `7584d3e docs: add testing strategy`.
