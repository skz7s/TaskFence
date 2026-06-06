# Local Task Artifacts Plan

## Goal

Add a local `taskfence artifacts <task-id>` command that lists the saved evidence
and artifact files for one workspace-local task run.

## Overall Status

done

## Plan Source

The operator asked to continue follow-up development and commit the completed
work. This slice advances the Phase 4 local review surface by exposing a
read-only artifact manifest for one task's `.taskfence/tasks/<task-id>/`
directory. It does not add artifact downloading, Web UI, SQLite, API server,
replay execution, cross-workspace indexing, or recursive artifact browsing.

## Intake / Snapshot

- Status: done
- Date: 2026-06-07
- Default branch: `main` from `origin/HEAD`
- Working branch: `codex/governance-development-plan`
- Worktree at intake: clean
- Latest commit at intake: `3eae9c1 feat: add local task inputs command`
- Sync attempt: `git pull --ff-only` failed because the current branch has no upstream tracking branch.
- Existing behavior: users can list tasks and read task summary, inputs, status, events, diff, report, logs, and approval records from local evidence, but cannot list the evidence/artifact files attached to a single task.
- Next executable phase: add state-layer artifact manifest reading and the CLI command.

## Scope

- Add state-layer reading for known evidence files under `.taskfence/tasks/<task-id>/`.
- Include immediate regular files under `.taskfence/tasks/<task-id>/artifacts/` when that directory exists.
- Avoid reading artifact file contents.
- Avoid recursive directory traversal and do not follow symlinked directories.
- Add `taskfence artifacts <task-id> --workspace <workspace>` to the CLI.
- Update docs that list supported local task evidence commands.

## Non-Goals

- No artifact download/extract command.
- No recursive browsing of artifact subdirectories.
- No replay execution.
- No SQLite, API server, Web UI, or cross-workspace index.
- No task execution behavior change.
- No artifact writer schema change.

## Acceptance Criteria

- `taskfence artifacts <task-id>` parses with default workspace `.`.
- `taskfence artifacts <task-id> --workspace <workspace>` parses an explicit workspace.
- The command lists known local evidence files and immediate custom artifact files with paths and sizes.
- Missing task directories and unsafe task IDs fail explicitly.
- Artifact subdirectories and symlinked entries are not traversed.
- Documentation describes artifact manifest lookup as workspace-local evidence reading only.

## Phases

1. State and CLI artifact manifest implementation
   - Status: done
   - Scope: add state reader, CLI command, rendering helper, and targeted tests.
   - Verification: `cargo test -p taskfence-state`; `cargo test -p taskfence-cli`
   - Evidence: `taskfence-state` passed 32 tests covering artifact manifest reads, missing task directories, unsafe task IDs, unsafe custom artifact names, non-directory `artifacts` paths, and symlinked custom artifact entries. `taskfence-cli` passed 66 tests including artifacts command parsing, manifest rendering, missing task error propagation, and non-recursive custom artifact listing.
2. Documentation updates
   - Status: done
   - Scope: update README, architecture, roadmap, development design, and runtime facts.
   - Verification: `rg -n "taskfence artifacts|artifact manifest|artifact manifests|custom artifact|recursive|symlink|artifact files" README.md docs/architecture.md docs/development-design.md docs/roadmap.md docs/codex/runtime-architecture.md docs/codex/plans/2026-06-07-local-task-artifacts.md crates/taskfence-cli/src/main.rs crates/taskfence-state/src/lib.rs`
   - Evidence: Documentation now lists `taskfence artifacts <task-id> --workspace <workspace>` as a workspace-local artifact manifest query and preserves the non-goals for downloads, recursive browsing, Web UI, API server, SQLite, cross-workspace indexing, and replay execution.
3. Quality gate, archive, and commit
   - Status: done
   - Scope: run formatting, focused tests, workspace lint/tests, archive this plan, stage only task-owned files, and commit.
   - Verification: `cargo fmt --all --check`; `git diff --check`; `cargo test -p taskfence-state`; `cargo test -p taskfence-cli`; `cargo clippy --workspace --all-targets -- -D warnings`; `cargo test --workspace`
   - Evidence: Formatting and whitespace checks passed. Focused state and CLI tests passed. Workspace clippy passed with warnings denied. `cargo test --workspace` passed all non-ignored tests; the Docker integration test remained ignored because it requires a Docker daemon and locally available test image.

## Commit Plan

1. `feat: add local task artifacts command`

## Final Evidence

- Documentation coverage was confirmed with `rg -n "taskfence artifacts|artifact manifest|artifact manifests|custom artifact|recursive|symlink|artifact files" README.md docs/architecture.md docs/development-design.md docs/roadmap.md docs/codex/runtime-architecture.md docs/codex/plans/2026-06-07-local-task-artifacts.md crates/taskfence-cli/src/main.rs crates/taskfence-state/src/lib.rs`.
- `cargo fmt --all --check` passed.
- `git diff --check` passed.
- `cargo test -p taskfence-state` passed 32 tests.
- `cargo test -p taskfence-cli` passed 66 tests.
- `cargo clippy --workspace --all-targets -- -D warnings` passed.
- `cargo test --workspace` passed all non-ignored tests; `tests/docker_integration.rs` kept its Docker-required test ignored.
- Commit planned: `feat: add local task artifacts command`.
