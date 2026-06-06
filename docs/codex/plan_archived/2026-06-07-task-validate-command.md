# Task Validate Command Plan

## Goal

Implement a local `taskfence validate <task-file>` command that checks a task file before running it.

## Overall Status

done

## Plan Source

The operator asked to continue follow-up development and commit the completed work. This slice adds a bounded local CLI validation path that complements `taskfence init` and `taskfence run` without expanding TaskFence into gateway execution, Web UI, replay, SQLite state, or service behavior.

## Intake / Snapshot

- Status: done
- Date: 2026-06-07
- Default branch: `main` from `origin/HEAD`
- Working branch: `codex/governance-development-plan`
- Worktree at intake: clean
- Sync attempt: `git pull --ff-only` failed because the current branch has no upstream tracking branch.
- Existing behavior: users can scaffold and run task files, but there is no local command to validate a task file without starting orchestration.
- Next executable phase: implement the local validate command and targeted tests.

## Scope

- Add `taskfence validate <task-file>` to the CLI.
- Load and resolve the task file using the existing config parser.
- Build the generic agent invocation to catch adapter-level config errors.
- Evaluate the planned command against the built-in policy.
- Build the Docker runner plan to catch local runner preparation errors such as unsupported domain allowlists, sandbox kind mismatches, mount validation, and env allowlist issues.
- Print a concise success summary without creating artifacts or starting Docker.
- Update docs that list supported local CLI commands.

## Non-Goals

- No Docker daemon calls and no container execution.
- No artifact, audit, approval, report, or state writes.
- No interactive or external approvals.
- No gateway protocol execution, Web UI, replay, SQLite state, API server, team-server, or enterprise behavior.
- No broad task schema changes.

## Acceptance Criteria

- `taskfence validate <task-file>` parses and validates a runnable local task file.
- Validation fails closed for denied commands.
- Validation fails closed for unsupported local Docker domain allowlists.
- Validation catches generic adapter command-shape errors before run.
- Tests cover command parsing plus success and failure paths.
- Documentation accurately describes validation as local pre-run checking only.

## Phases

1. CLI validate implementation and tests
   - Status: done
   - Scope: add CLI command, validation helper, and targeted tests.
   - Verification: `cargo test -p taskfence-cli`
   - Evidence: `taskfence-cli` tests passed with 54 tests, including validate command parsing, successful local task validation without `.taskfence` artifact writes, denied command rejection, unsupported Docker domain allowlist rejection, and generic agent command-shape rejection.
2. Documentation updates
   - Status: done
   - Scope: update README, architecture, roadmap, development design, and runtime facts for the new local validation command.
   - Verification: `rg -n "taskfence validate|validate <task-file>|pre-run|预运行|starting Docker|启动 Docker" README.md docs/architecture.md docs/roadmap.md docs/development-design.md docs/codex/runtime-architecture.md crates/taskfence-cli/src/main.rs`
   - Evidence: README, architecture, roadmap, development design, and runtime facts describe validate as local pre-run checking that does not start Docker, write artifacts, request approvals, or expand gateway/Web/API/SQLite/replay behavior.
3. Quality gate, archive, and commit
   - Status: done
   - Scope: run formatting, focused tests, workspace lint/tests, archive this plan, stage only task-owned files, and commit.
   - Verification: `cargo fmt --all --check`; `git diff --check`; `cargo test -p taskfence-cli`; `cargo clippy --workspace --all-targets -- -D warnings`; `cargo test --workspace`
   - Evidence: all commands passed. `cargo test -p taskfence-cli` passed 54 tests, including validate command coverage. `cargo test --workspace` passed all unit/doc tests and kept `tests/docker_integration.rs` ignored because it requires a Docker daemon and locally available test image.

## Commit Plan

1. `feat: add task file validation command`

## Final Evidence

Implemented and verified `taskfence validate <task-file>` as a local pre-run validation path. The command resolves task files, builds the generic agent invocation through the adapter boundary, evaluates the planned command through the core validation path and policy engine, builds the Docker runner plan without starting Docker, and prints a concise summary without writing `.taskfence` artifacts or requesting approvals.

Validation completed:

1. `cargo fmt --all --check`
2. `git diff --check`
3. `cargo test -p taskfence-cli`
4. `cargo clippy --workspace --all-targets -- -D warnings`
5. `cargo test --workspace`

The Docker integration test remains ignored by the normal workspace test run because it requires Docker and a locally available test image.
