# Rustdoc Release Gate Plan

## Goal

Continue improving TaskFence toward mature open-source readiness by making
rustdoc generation warning-clean and adding it to the documented release and CI
quality gates.

## Plan Source

Continuation of the active 2026-06-09 goal:

> 继续优化项目，包括文档，将项目优化到一个成熟的开源项目的级别后，开源项目

Related archived plans:

- `docs/codex/plan_archived/2026-06-09-open-source-maturity.md`
- `docs/codex/plan_archived/2026-06-09-security-compatibility.md`
- `docs/codex/plan_archived/2026-06-09-cli-task-reference.md`

Scope:

- fix the current rustdoc warning in the CLI doc comments
- add rustdoc generation with warnings denied to GitHub Actions and release
  documentation
- update maintainer, readiness, supply-chain, and change-map docs so future
  public API/doc changes keep rustdoc clean

Non-goals:

- do not publish docs or release artifacts
- do not rewrite public API documentation beyond the warning-clean fix
- do not change runtime behavior, task schema, dependency versions, or
  generated governance outputs

Acceptance criteria:

- `RUSTDOCFLAGS="-D warnings" cargo doc --workspace --no-deps --locked` passes
- CI and release docs include the rustdoc gate
- docs still avoid overclaiming stable production APIs
- validation evidence is recorded before committing

## Snapshot

- Date: 2026-06-09
- Default branch: `origin/main`
- Working branch: `codex/governance-development-plan`
- Initial worktree status: clean after commits `2523e8b` and `7d88235`
- Observed failure: `RUSTDOCFLAGS="-D warnings" cargo doc --workspace
  --no-deps --locked` failed because `crates/taskfence-cli/src/main.rs`
  contains the doc comment `Defaults to <task-id>-replay`, which rustdoc treats
  as an unclosed HTML tag

## Phases

### 1. Intake And Plan

- Status: done
- Scope: record the observed rustdoc failure, scope, non-goals, and acceptance
  criteria
- Verification command: `git status --short --branch`
- Verification evidence: passed on 2026-06-09. Worktree contains only the new
  active rustdoc plan file.

### 2. Rustdoc Gate Implementation

- Status: done
- Scope: fix rustdoc warning and update CI/release/maintenance docs with the
  rustdoc command
- Verification command: `RUSTDOCFLAGS="-D warnings" cargo doc --workspace --no-deps --locked`
- Verification evidence: passed on 2026-06-09 after replacing the invalid
  `<task-id>` rustdoc text with `TASK_ID`. Added the rustdoc gate to GitHub
  Actions, PR template, README, CONTRIBUTING, quickstart, release, readiness,
  maintainer, supply-chain, cross-platform ops, changelog, and change-map docs,
  plus a decision record.

### 3. Final Review And Commit

- Status: done
- Scope: run targeted validation, archive this plan if complete, and create
  focused local commits
- Verification command: `git diff --check && python3 scripts/governance/check_codex_governance.py`
- Verification evidence: passed on 2026-06-09. `cargo run -p taskfence-cli --
  replay run --help` rebuilt the CLI and showed `TASK_ID-replay`; `git diff
  --check`, `bash -n deploy/manage.sh`,
  `python3 scripts/governance/build_agents.py --check`, and `python3
  scripts/governance/check_codex_governance.py` passed.

## Commit Plan

1. `ci: add rustdoc release gate` - `f9a76a6`

## Final Evidence

- Fixed the CLI doc comment that caused rustdoc to treat `<task-id>` as an
  invalid HTML tag.
- Added warning-clean rustdoc generation to GitHub Actions and public release
  gates.
- Synchronized contributor, quickstart, maintainer, supply-chain,
  cross-platform ops, changelog, change-map, and decision docs.
- Validation passed on 2026-06-09:
  `RUSTDOCFLAGS="-D warnings" cargo doc --workspace --no-deps --locked`,
  `cargo run -p taskfence-cli -- replay run --help`, `git diff --check`,
  `bash -n deploy/manage.sh`, `python3 scripts/governance/build_agents.py
  --check`, and `python3 scripts/governance/check_codex_governance.py`.
- Implementation commit: `f9a76a6 ci: add rustdoc release gate`.
