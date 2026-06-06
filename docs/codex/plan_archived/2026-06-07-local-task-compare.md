# Local Task Compare Plan

## Goal

Add a local `taskfence compare <left-task-id> <right-task-id>` command that
compares two workspace-local task summaries from structured evidence.

## Overall Status

done

## Plan Source

The operator asked to continue follow-up development and commit the completed
work. This slice advances the Phase 4 local review surface by exposing a
read-only comparison between two task runs in one workspace. It does not add a
Web UI comparison view, replay execution, SQLite, API server, cross-workspace
indexing, report scraping, or artifact content diffing.

## Intake / Snapshot

- Status: done
- Date: 2026-06-07
- Default branch: `main` from `origin/HEAD`
- Working branch: `codex/governance-development-plan`
- Worktree at intake: clean
- Latest commit at intake: `a4b4ae7 feat: add local task artifacts command`
- Sync attempt: `git pull --ff-only` failed because the current branch has no upstream tracking branch.
- Existing behavior: users can list tasks and read task summary, inputs, artifacts, status, events, diff, report, logs, and approval records from local evidence, but cannot compare two task summaries directly.
- Next executable phase: add the CLI comparison command and targeted tests.

## Scope

- Add `taskfence compare <left-task-id> <right-task-id> --workspace <workspace>` to the CLI.
- Read both task summaries from `.taskfence/tasks` using existing structured evidence logic.
- Render a compact tabular comparison of status, goal, artifact flags, warning count, and evidence path.
- Preserve existing warning behavior for malformed local evidence.
- Update docs that list supported local task evidence commands.

## Non-Goals

- No Web UI comparison view.
- No replay execution.
- No SQLite, API server, or cross-workspace index.
- No report text scraping.
- No diffing of artifact contents.
- No task execution behavior change.

## Acceptance Criteria

- `taskfence compare <left-task-id> <right-task-id>` parses with default workspace `.`.
- `taskfence compare <left-task-id> <right-task-id> --workspace <workspace>` parses an explicit workspace.
- The command reads both workspace-local task summaries from structured evidence.
- Missing or unsafe task IDs fail explicitly through existing state validation.
- Documentation describes comparison as workspace-local structured summary reading only.

## Phases

1. CLI comparison implementation
   - Status: done
   - Scope: add CLI command, render helper, and targeted tests.
   - Verification: `cargo test -p taskfence-cli`
   - Evidence: `taskfence-cli` passed 70 tests including compare command parsing, successful structured task summary comparison, and missing task error propagation.
2. Documentation updates
   - Status: done
   - Scope: update README, architecture, roadmap, development design, and runtime facts.
   - Verification: `rg -n "taskfence compare|structured task comparison|structured task comparisons|task summary comparison|comparison view|artifact content diffing|report scraping" README.md docs/architecture.md docs/development-design.md docs/roadmap.md docs/codex/runtime-architecture.md docs/codex/plan_archived/2026-06-07-local-task-compare.md crates/taskfence-cli/src/main.rs`
   - Evidence: command coverage confirmed the CLI command and non-goal documentation are present in the supported local evidence docs and implementation.
3. Quality gate, archive, and commit
   - Status: done
   - Scope: run formatting, focused tests, workspace lint/tests, archive this plan, stage only task-owned files, and commit.
   - Verification: `cargo fmt --all --check`; `git diff --check`; `cargo test -p taskfence-cli`; `cargo clippy --workspace --all-targets -- -D warnings`; `cargo test --workspace`
   - Evidence: formatting and whitespace checks passed; `taskfence-cli` passed 70 tests; workspace clippy passed with warnings denied; workspace tests passed, with the Docker integration test remaining ignored because it requires a Docker daemon and locally available test image.

## Commit Plan

1. `feat: add local task compare command`

## Final Evidence

- `git pull --ff-only` was attempted before implementation and failed because the current branch has no upstream tracking branch.
- `rg -n "taskfence compare|structured task comparison|structured task comparisons|task summary comparison|comparison view|artifact content diffing|report scraping" README.md docs/architecture.md docs/development-design.md docs/roadmap.md docs/codex/runtime-architecture.md docs/codex/plan_archived/2026-06-07-local-task-compare.md crates/taskfence-cli/src/main.rs` confirmed documentation and implementation coverage.
- `cargo fmt --all --check` passed.
- `git diff --check` passed.
- `cargo test -p taskfence-cli` passed 70 tests.
- `cargo clippy --workspace --all-targets -- -D warnings` passed.
- `cargo test --workspace` passed; the Docker integration test stayed ignored as expected because it requires a Docker daemon and a locally available test image.
