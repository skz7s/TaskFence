# Local Task Detail Plan

## Goal

Advance the Phase 4 local review surface by adding a read-only
workspace-local single task summary command without introducing Web UI, API
server behavior, SQLite state, replay execution, live log streaming, or
cross-workspace indexing.

## Plan Source

User request on 2026-06-07: "继续推进后续的开发，完成后提交代码".

Actionable interpretation:

- Continue after the local runner, local evidence lookup, approvals,
  denied-action evidence, tool policy evidence, gateway approval mediation,
  secret broker contract, MCP/HTTP adapter stubs, local task list, local task
  diff, and local approval list slices.
- Pick the next bounded Phase 4 review gap: operators can list tasks and read
  individual report/diff/log artifacts, but cannot ask for a single task's
  structured summary and artifact availability.
- Keep the implementation local, read-only, filesystem-backed, and scoped to a
  single workspace's `.taskfence/tasks/<task-id>` directory.

Non-goals:

- Do not introduce a Web UI, API server, SQLite state, replay execution,
  live-log streaming, cross-workspace indexing, gateway execution, or
  team-server behavior.
- Do not infer task state from rendered report text or terminal output.

## Intake / Snapshot

- Default branch: `main` detected from `origin/HEAD`.
- Working branch: `codex/governance-development-plan`.
- `git pull --ff-only` was attempted and failed because the current branch has
  no upstream tracking branch; work continues from the current checkout.
- Worktree status at intake: clean.
- Current local evidence lookup supports workspace task lists and individual
  report, diff, and captured-log artifact reads, but not a single structured
  task summary command.

## Overall Status

done

## Phases

### Phase 1: State Task Summary

Status: done

Scope:

- Add a read-only single task summary API to `taskfence-state`.
- Reuse existing structured summary logic for goal, latest status, artifact
  flags, and malformed evidence warnings.
- Keep missing task directories and unsafe task IDs explicit state errors.

Verification command:

```bash
cargo test -p taskfence-state
```

Verification evidence:

- `cargo fmt --all` ran and completed.
- `cargo test -p taskfence-state` passed on 2026-06-07 with 17 state tests and
  the state doc-test target. Coverage includes structured single task summary
  reads, malformed evidence warnings, and missing task directory errors.

### Phase 2: CLI And Docs

Status: done

Scope:

- Add `taskfence task <task-id> --workspace <workspace>`.
- Render task ID, status, goal, artifact availability, task evidence path, and
  warnings without reading report text.
- Update README, architecture, roadmap, development design, and runtime facts
  to describe the local task detail command without claiming Web UI/API/SQLite
  support.

Verification command:

```bash
cargo test -p taskfence-cli -p taskfence-state
```

Verification evidence:

- `cargo test -p taskfence-cli -p taskfence-state` passed on 2026-06-07 with
  39 CLI tests, 17 state tests, and the state doc-test target.
- README, architecture, roadmap, development design, and runtime architecture
  facts were updated to describe the workspace-local task detail command
  without claiming Web UI, API server, SQLite, replay, or cross-workspace
  indexing.

### Phase 3: Quality Gate, Archive, Commit

Status: done

Scope:

- Run formatting, focused tests, clippy, and workspace tests.
- Archive this plan and create one focused commit.

Verification command:

```bash
cargo fmt --all --check
cargo test -p taskfence-cli -p taskfence-state
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```

Verification evidence:

- `cargo fmt --all --check` passed.
- `cargo test -p taskfence-cli -p taskfence-state` passed with 39 CLI tests,
  17 state tests, and the state doc-test target.
- `cargo clippy --workspace --all-targets -- -D warnings` passed.
- `cargo test --workspace` passed. The Docker integration test remained ignored
  as designed unless explicitly run with Docker and a local test image.

## Commit Plan

1. `feat: add local task detail`

## Open Risks

- This remains a read-only workspace-local artifact query, not durable
  cross-workspace state.
- Malformed task evidence should remain visible as warnings rather than being
  silently treated as healthy.

## Final Evidence

- All phases are terminal with verification evidence.
- `taskfence-state` now exposes `LocalTaskEvidenceStore::read_task_summary()`
  for safe, workspace-local single task summary reads.
- The single task summary reuses structured `task.resolved.json` and
  `events.jsonl` parsing, artifact availability flags, and malformed evidence
  warnings.
- `taskfence task <task-id> --workspace <workspace>` renders task ID, status,
  goal, artifact availability, evidence path, and warnings without reading
  rendered report text.
- README, architecture, roadmap, development design, and runtime architecture
  facts describe the local task detail query without claiming Web UI, API
  server, SQLite state, replay, gateway execution, service-side task state, or
  cross-workspace indexing.
