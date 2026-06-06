# Local Task Inputs Plan

## Goal

Implement a local `taskfence inputs <task-id>` command that reads the resolved task input saved for a completed or attempted local run.

## Overall Status

done

## Plan Source

The operator asked to continue follow-up development and commit the completed work. This slice advances the Phase 4 "task replay inputs" local evidence surface by exposing the existing `task.resolved.json` artifact as a read-only CLI query. It does not execute replay, add SQLite, add an API server, add a Web UI, or introduce cross-workspace indexing.

## Intake / Snapshot

- Status: done
- Date: 2026-06-07
- Default branch: `main` from `origin/HEAD`
- Working branch: `codex/governance-development-plan`
- Worktree at intake: clean
- Sync attempt: `git pull --ff-only` failed because the current branch has no upstream tracking branch.
- Existing behavior: users can read task lists, task summaries, latest statuses, event timelines, diffs, logs, reports, and approval records from local evidence, but cannot directly query the resolved task input artifact.
- Next executable phase: add structured input artifact reading and the CLI command.

## Scope

- Add state-layer reading for `.taskfence/tasks/<task-id>/task.resolved.json`.
- Parse the resolved task JSON and reject mismatched task IDs.
- Add `taskfence inputs <task-id> --workspace <workspace>` to the CLI.
- Print the saved resolved task JSON without scraping report text or terminal output.
- Update docs that list supported local task evidence commands.

## Non-Goals

- No replay execution.
- No SQLite, API server, Web UI, or cross-workspace index.
- No task execution behavior change.
- No task schema change.

## Acceptance Criteria

- `taskfence inputs <task-id>` parses with default workspace `.`.
- `taskfence inputs <task-id> --workspace <workspace>` parses an explicit workspace.
- The command reads and prints the saved resolved task JSON from local task evidence.
- Missing, malformed, mismatched, and unsafe task evidence paths fail explicitly.
- Documentation describes inputs lookup as workspace-local evidence reading only.

## Phases

1. State and CLI inputs implementation
   - Status: done
   - Scope: add state reader, CLI command, rendering helper, and targeted tests.
   - Verification: `cargo test -p taskfence-state`; `cargo test -p taskfence-cli`
   - Evidence: `taskfence-state` passed 26 tests covering resolved input reads plus missing, malformed, mismatched, and unsafe task ID failures. `taskfence-cli` passed 62 tests including inputs command parsing, successful resolved input rendering, and missing task evidence error propagation.
2. Documentation updates
   - Status: done
   - Scope: update README, architecture, roadmap, development design, and runtime facts.
   - Verification: `rg -n "taskfence inputs|resolved task input|task.resolved.json|replay input|resolved task inputs" README.md docs/architecture.md docs/development-design.md docs/roadmap.md docs/codex/runtime-architecture.md`
   - Evidence: Documentation now lists `taskfence inputs <task-id> --workspace <workspace>` as workspace-local resolved task input evidence reading and preserves the non-goals for replay, API server, Web UI, SQLite, and cross-workspace indexing.
3. Quality gate, archive, and commit
   - Status: done
   - Scope: run formatting, focused tests, workspace lint/tests, archive this plan, stage only task-owned files, and commit.
   - Verification: `cargo fmt --all --check`; `git diff --check`; `cargo test -p taskfence-state`; `cargo test -p taskfence-cli`; `cargo clippy --workspace --all-targets -- -D warnings`; `cargo test --workspace`
   - Evidence: Formatting and whitespace checks passed. `taskfence-state` passed 26 tests; `taskfence-cli` passed 62 tests. Workspace clippy passed with warnings denied. `cargo test --workspace` passed all non-ignored tests; the Docker integration test remained ignored because it requires a Docker daemon and locally available test image.

## Commit Plan

1. `feat: add local task inputs command`

## Final Evidence

- Documentation coverage was confirmed with `rg -n "taskfence inputs|resolved task input|task.resolved.json|replay input|resolved task inputs" README.md docs/architecture.md docs/development-design.md docs/roadmap.md docs/codex/runtime-architecture.md`.
- `cargo fmt --all --check` passed.
- `git diff --check` passed.
- `cargo test -p taskfence-state` passed 26 tests.
- `cargo test -p taskfence-cli` passed 62 tests.
- `cargo clippy --workspace --all-targets -- -D warnings` passed.
- `cargo test --workspace` passed all non-ignored tests; `tests/docker_integration.rs` kept its Docker-required test ignored.
- Commit planned: `feat: add local task inputs command`.
