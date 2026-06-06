# Local Task Status Plan

## Goal

Implement a local `taskfence status <task-id>` command that reads a task's latest structured status from workspace-local evidence.

## Overall Status

done

## Plan Source

The operator asked to continue follow-up development and commit the completed work. This slice keeps development inside the existing local task evidence query boundary by adding a concise status lookup command. It does not add SQLite, API server, Web UI, replay, cross-workspace indexing, or report scraping.

## Intake / Snapshot

- Status: done
- Date: 2026-06-07
- Default branch: `main` from `origin/HEAD`
- Working branch: `codex/governance-development-plan`
- Worktree at intake: clean
- Sync attempt: `git pull --ff-only` failed because the current branch has no upstream tracking branch.
- Existing behavior: users can list task summaries and read a single task summary, events, logs, diff, and report, but there is no concise task status command.
- Next executable phase: add the CLI status command and targeted tests.

## Scope

- Add `taskfence status <task-id> --workspace <workspace>` to the CLI.
- Reuse `LocalTaskEvidenceStore::read_task_summary` so status comes from structured `events.jsonl` `TaskStatusChanged` events.
- Render a concise status view with task ID, status, evidence directory, and any evidence warnings.
- Update docs that list supported local task evidence commands.

## Non-Goals

- No SQLite or long-lived state backend.
- No Web UI, API server, replay, or cross-workspace index.
- No report text scraping or terminal-output parsing.
- No task execution behavior change.

## Acceptance Criteria

- `taskfence status <task-id>` parses with default workspace `.`.
- `taskfence status <task-id> --workspace <workspace>` parses an explicit workspace.
- The command reads the latest structured task status from local task evidence.
- Missing task evidence still surfaces the existing state error.
- Documentation describes status lookup as workspace-local evidence reading only.

## Phases

1. CLI status implementation and tests
   - Status: done
   - Scope: add command parsing, renderer, helper, and targeted tests.
   - Verification: `cargo test -p taskfence-cli`
   - Evidence: passed with 58 tests, including status command parsing, successful latest-status rendering from workspace-local task evidence, and missing task evidence error propagation.
2. Documentation updates
   - Status: done
   - Scope: update README, architecture, roadmap, development design, and runtime facts.
   - Verification: `rg -n "taskfence status|latest task status|latest-status|latest local task status|最新本地任务状态|status <task-id>" README.md docs/architecture.md docs/development-design.md docs/roadmap.md docs/codex/runtime-architecture.md crates/taskfence-cli/src/main.rs docs/codex/plans/2026-06-07-local-task-status.md`
   - Evidence: README, architecture, roadmap, development design, runtime facts, CLI source, and this plan describe the local status command and its workspace-local evidence boundary.
3. Quality gate, archive, and commit
   - Status: done
   - Scope: run formatting, focused tests, workspace lint/tests, archive this plan, stage only task-owned files, and commit.
   - Verification: `cargo fmt --all --check`; `git diff --check`; `cargo test -p taskfence-cli`; `cargo clippy --workspace --all-targets -- -D warnings`; `cargo test --workspace`
   - Evidence: all commands passed. `cargo test -p taskfence-cli` passed 58 tests, including status command coverage. `cargo test --workspace` passed all unit/doc tests and kept `tests/docker_integration.rs` ignored because it requires a Docker daemon and locally available test image.

## Commit Plan

1. `feat: add local task status command`

## Final Evidence

Implemented and verified `taskfence status <task-id> --workspace <workspace>` as a local task evidence lookup. The command reuses `LocalTaskEvidenceStore::read_task_summary`, renders the latest structured task status from `events.jsonl` `TaskStatusChanged` records, includes the task evidence directory, preserves warning output, and surfaces missing task evidence through the existing state error path.

Validation completed:

1. `cargo fmt --all --check`
2. `git diff --check`
3. `cargo test -p taskfence-cli`
4. `cargo clippy --workspace --all-targets -- -D warnings`
5. `cargo test --workspace`

The Docker integration test remains ignored by the normal workspace test run because it requires Docker and a locally available test image.
