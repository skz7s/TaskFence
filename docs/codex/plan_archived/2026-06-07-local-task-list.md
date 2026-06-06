# Local Task List Plan

## Goal

Advance the Phase 4 local state surface by adding a workspace-local task list
that reads structured task evidence under `.taskfence/tasks` without
introducing SQLite, an API server, Web UI, replay, or cross-workspace indexing.

## Plan Source

User request on 2026-06-07: "继续推进后续的开发，完成后提交代码".

Actionable interpretation:

- Continue after the local runner, task evidence lookup, approvals, denied-action
  evidence, tool policy evidence, gateway approval mediation, secret broker
  contract, and MCP/HTTP adapter stub slices.
- Pick the next bounded Phase 4 gap: a task-list query surface.
- Keep the implementation local, filesystem-backed, and scoped to a single
  workspace's `.taskfence/tasks` directory.

Non-goals:

- Do not introduce SQLite, an API server, Web UI, replay execution, live logs,
  diff viewer, cross-workspace indexing, gateway execution, or team-server
  behavior.
- Do not infer task status from rendered report text or terminal output.

## Intake / Snapshot

- Default branch: `main` detected from `origin/HEAD`.
- Working branch: `codex/governance-development-plan`.
- `git pull --ff-only` was attempted and failed because the current branch has
  no upstream tracking branch; work continues from the current checkout.
- Worktree status at intake: clean.
- Current local evidence lookup supports `taskfence report` and `taskfence logs`
  against one workspace-local task directory, but not listing available tasks.

## Overall Status

done

## Phases

### Phase 1: State Task List

Status: done

Scope:

- Add a typed local task summary to `taskfence-state`.
- List safe task directories under `.taskfence/tasks`.
- Read `task.resolved.json` for goal when present.
- Read structured `events.jsonl` for the latest `TaskStatusChanged` status when
  present.
- Record artifact availability for report/stdout/stderr without reading rendered
  report text.

Verification command:

```bash
cargo test -p taskfence-state
```

Verification evidence:

- `cargo fmt --all` ran and completed.
- `cargo test -p taskfence-state` passed on 2026-06-07 with 12 state tests and
  the state doc-test target. Coverage includes empty workspaces, structured
  summaries from `task.resolved.json` and `events.jsonl`, malformed evidence
  warnings, artifact flags, stable sorting, and unsafe task directory rejection.

### Phase 2: CLI And Docs

Status: done

Scope:

- Add a `taskfence tasks --workspace <workspace>` command.
- Render a compact local task list with task id, status, artifact flags, and
  goal.
- Update README, architecture, roadmap, development design, and runtime facts to
  describe the local task list without claiming SQLite/Web UI support.

Verification command:

```bash
cargo test -p taskfence-cli -p taskfence-state
```

Verification evidence:

- `cargo test -p taskfence-cli -p taskfence-state` passed on 2026-06-07 with
  30 CLI tests, 12 state tests, and the state doc-test target.
- README, architecture, roadmap, development design, and runtime architecture
  facts were updated to describe the workspace-local task list without claiming
  SQLite, Web UI, or cross-workspace indexing.

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
- `cargo test -p taskfence-cli -p taskfence-state` passed with 30 CLI tests,
  12 state tests, and the state doc-test target.
- `cargo clippy --workspace --all-targets -- -D warnings` passed.
- `cargo test --workspace` passed. The Docker integration test remained ignored
  as designed unless explicitly run with Docker and a local test image.

## Commit Plan

1. `feat: add local task list`

## Open Risks

- This remains a read-only workspace-local artifact query, not durable
  cross-workspace state.
- Missing or malformed per-task evidence should degrade per task where possible
  while path safety failures remain fail-closed.

## Final Evidence

- All phases are terminal with verification evidence.
- `taskfence-state` now exposes `TaskSummary` and
  `LocalTaskEvidenceStore::list_tasks()` for workspace-local task summaries.
- Task summaries read goal from `task.resolved.json`, latest status from
  structured `events.jsonl` `TaskStatusChanged` events, and report/stdout/stderr
  artifact flags from file existence.
- Malformed per-task evidence becomes a task warning, while unsafe task
  directory names fail closed.
- `taskfence tasks --workspace <workspace>` renders the local task list without
  introducing SQLite, Web UI, replay, API server behavior, or cross-workspace
  indexing.
