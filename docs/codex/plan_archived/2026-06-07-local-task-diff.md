# Local Task Diff Plan

## Goal

Advance the Phase 4 local review surface by adding a workspace-local diff query
for task `diff.patch` artifacts without introducing Web UI, SQLite, replay
execution, live log streaming, or cross-workspace indexing.

## Plan Source

User request on 2026-06-07: "继续推进后续的开发，完成后提交代码".

Actionable interpretation:

- Continue after the local runner, task evidence lookup, approvals,
  denied-action evidence, tool policy evidence, gateway approval mediation,
  secret broker contract, MCP/HTTP adapter stubs, and local task list slices.
- Pick the next bounded Phase 4 review gap: a local diff-viewer query surface.
- Keep the implementation local, read-only, filesystem-backed, and scoped to a
  single workspace's `.taskfence/tasks/<task-id>/diff.patch` artifact.

Non-goals:

- Do not introduce a Web UI, SQLite, API server, replay execution, live logs,
  cross-workspace indexing, gateway execution, or team-server behavior.
- Do not infer diff state from rendered reports or terminal output.

## Intake / Snapshot

- Default branch: `main` detected from `origin/HEAD`.
- Working branch: `codex/governance-development-plan`.
- `git pull --ff-only` was attempted and failed because the current branch has
  no upstream tracking branch; work continues from the current checkout.
- Worktree status at intake: clean.
- Current local evidence lookup supports `taskfence tasks`, `taskfence report`,
  and `taskfence logs` against one workspace-local artifact directory, but not
  direct diff artifact viewing.

## Overall Status

done

## Phases

### Phase 1: State Diff Query

Status: done

Scope:

- Add a typed local task diff value to `taskfence-state`.
- Add read-only `diff.patch` lookup under safe task IDs.
- Keep missing task directory and missing diff artifact errors explicit.
- Include diff artifact availability in local task summaries.

Verification command:

```bash
cargo test -p taskfence-state
```

Verification evidence:

- `cargo fmt --all` ran and completed.
- `cargo test -p taskfence-state` passed on 2026-06-07 with 14 state tests and
  the state doc-test target. Coverage includes diff reads, missing diff
  artifacts, unsafe task IDs, and local task summary diff availability.

### Phase 2: CLI And Docs

Status: done

Scope:

- Add `taskfence diff <task-id> --workspace <workspace>`.
- Render the diff artifact contents directly from `diff.patch`.
- Update README, architecture, roadmap, development design, and runtime facts to
  describe the local diff query without claiming Web UI or SQLite support.

Verification command:

```bash
cargo test -p taskfence-cli -p taskfence-state
```

Verification evidence:

- `cargo test -p taskfence-cli -p taskfence-state` passed on 2026-06-07 with
  33 CLI tests, 14 state tests, and the state doc-test target.
- README, architecture, roadmap, development design, and runtime architecture
  facts were updated to describe the workspace-local diff query without
  claiming Web UI, SQLite, replay, or cross-workspace indexing.

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
- `cargo test -p taskfence-cli -p taskfence-state` passed with 33 CLI tests,
  14 state tests, and the state doc-test target.
- `cargo clippy --workspace --all-targets -- -D warnings` passed.
- `cargo test --workspace` passed. The Docker integration test remained ignored
  as designed unless explicitly run with Docker and a local test image.

## Commit Plan

1. `feat: add local task diff query`

## Open Risks

- This remains a read-only workspace-local artifact query, not a browser diff
  viewer or durable cross-workspace index.
- Diff content comes from the existing artifact writer and may include metadata
  explaining unavailable git or dirty-baseline conditions.

## Final Evidence

- All phases are terminal with verification evidence.
- `taskfence-state` now exposes `TaskDiff` and
  `LocalTaskEvidenceStore::read_diff()` for safe, workspace-local `diff.patch`
  reads.
- Local task summaries now include diff artifact availability.
- `taskfence diff <task-id> --workspace <workspace>` renders the local
  `diff.patch` artifact directly.
- README, architecture, roadmap, development design, and runtime architecture
  facts describe the local diff query without claiming Web UI, SQLite, replay,
  API server behavior, or cross-workspace indexing.
