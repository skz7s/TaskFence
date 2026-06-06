# Task Init Scaffold Plan

## Goal

Implement a safe `taskfence init [path]` command that writes one starter task file for local TaskFence use.

## Plan Source

The operator asked to continue follow-up development and commit the completed work. This slice advances the already parsed but unsupported `taskfence init` command without expanding TaskFence beyond the current local CLI/runtime boundary.

## Intake / Snapshot

- Status: done
- Date: 2026-06-07
- Default branch: `main` from `origin/HEAD`
- Working branch: `codex/governance-development-plan`
- Worktree at intake: clean
- Sync attempt: `git pull --ff-only` failed because the current branch has no upstream tracking branch.
- Existing behavior: `taskfence init` parses but returns an explicit unsupported-operation error.
- Next executable phase: implement local starter task-file scaffolding and targeted tests.

## Scope

- Replace the unsupported `init` execution branch with starter task-file creation.
- Refuse to overwrite an existing file.
- Create parent directories for nested starter paths.
- Keep the starter YAML valid for the current parser and local runner shape.
- Update docs that currently describe `taskfence init` as unsupported.

## Non-Goals

- No agent execution during `init`.
- No project generator beyond writing one task file.
- No gateway execution, Web UI, replay, SQLite state, API server, team-server, or enterprise behavior.
- No hidden workspace creation beyond the requested task-file parent directories.
- No Docker integration behavior change.

## Acceptance Criteria

- `taskfence init` writes `taskfence.yaml` by default.
- `taskfence init tasks/fix.yaml` creates parent directories and writes the starter task file.
- Existing target files are left unchanged and return a configuration error.
- The generated YAML parses through `taskfence_config::parse_task_file`.
- Documentation accurately describes the implemented behavior and still avoids overclaiming unsupported surfaces.

## Phases

1. CLI init implementation and tests
   - Status: done
   - Scope: add the init writer, starter task YAML, and targeted CLI tests.
   - Verification: `cargo fmt --all --check`; `cargo test -p taskfence-cli`
   - Evidence: formatting check passed; `taskfence-cli` tests passed with 49 tests, including starter write, parent directory creation, parse validation, and overwrite refusal.
2. Documentation updates
   - Status: done
   - Scope: align README, architecture, roadmap, development design, and runtime facts with the implemented `init` behavior.
   - Verification: `rg -n "taskfence init|init command|scaffolding is not implemented|parsed but remains explicitly unsupported|unsupported until task-file" README.md docs crates/taskfence-cli/src/main.rs`
   - Evidence: README, architecture, roadmap, development design, and runtime facts describe starter task-file creation and overwrite refusal. No stale user-facing "init unsupported" documentation remains outside historical plan snapshot text.
3. Quality gate, archive, and commit
   - Status: done
   - Scope: run formatting, focused tests, workspace lint/tests, archive this plan, stage only task-owned files, and commit.
   - Verification: `cargo fmt --all --check`; `cargo test -p taskfence-cli`; `cargo clippy --workspace --all-targets -- -D warnings`; `cargo test --workspace`
   - Evidence: formatting passed; `taskfence-cli` tests passed with 49 tests; workspace clippy passed with warnings denied; workspace tests passed, with the Docker integration test remaining ignored because it requires a Docker daemon and locally available test image.

## Commit Plan

1. `feat: scaffold starter task file`

## Final Evidence

Status: complete before commit.

- Implemented `taskfence init [path]` starter task-file scaffolding.
- Starter YAML parses through `taskfence_config::parse_task_file`.
- Existing target files are refused without overwrite.
- Documentation updated to describe the implemented local behavior and preserve non-goals.
- Validation passed: `cargo fmt --all --check`, `cargo test -p taskfence-cli`, `cargo clippy --workspace --all-targets -- -D warnings`, and `cargo test --workspace`.
- Docker integration test remained ignored by the workspace test suite because it requires a Docker daemon and locally available test image.
