# Local Task Query Plan

## Goal

Advance the implemented local runner path by making generated task evidence
queryable from the CLI. The target slice is local filesystem-backed `logs` and
`report` lookup for artifacts already produced by `taskfence run`, without
expanding into gateway execution, Web UI, replay, team-server, or durable
approval storage.

## Plan Source

User request on 2026-06-07: "继续推进后续的开发".

Actionable interpretation:

- Continue development after the archived local runner implementation plan.
- Preserve current project governance and the Rust crate ownership boundaries.
- Choose the next coherent development slice from the current repository state.
- Keep secure defaults and unsupported future surfaces explicit.
- Validate Rust changes with the smallest relevant checks, broadening to
  workspace checks if shared contracts or docs are touched.

Non-goals for this slice:

- Do not implement interactive approval storage or `approve` / `deny`.
- Do not introduce SQLite, API server, Web UI, gateway execution, replay, or
  team-server behavior.
- Do not change Docker runner security behavior.

## Intake / Snapshot

- Default branch: `main` detected from `origin/HEAD`.
- Working branch: `codex/governance-development-plan`.
- `git pull --ff-only` was attempted and failed because the current branch has
  no upstream tracking branch; work continues from the current checkout.
- Worktree status at intake: clean.
- Previous local runner plan is archived at
  `docs/codex/plan_archived/2026-06-06-local-runner-development.md`.
- Current implemented path: `taskfence run <task-file>` writes
  `.taskfence/tasks/<task-id>/task.resolved.json`, `events.jsonl`,
  stdout/stderr logs when captured, `diff.patch`, and `report.md`.
- Current CLI gap: `logs` and `report` commands parse arguments but return
  explicit unsupported errors.

## Overall Status

done

## Phases

### Phase 1: Local Query Contracts

Status: done

Scope:

- Add a local filesystem query surface for task evidence under a workspace root.
- Validate task IDs as safe path components before reading artifact paths.
- Return clear state errors for missing task directories, missing reports, and
  missing logs.
- Keep artifact lookup read-only and scoped under
  `.taskfence/tasks/<task-id>/`.

Verification command:

```bash
cargo test -p taskfence-state
```

Verification evidence:

- `cargo test -p taskfence-state` passed with 8 tests covering in-memory status
  storage, stdout/stderr log lookup, report lookup, missing task directories,
  missing logs, and unsafe task ID rejection.

### Phase 2: CLI Logs And Report Commands

Status: done

Scope:

- Wire `taskfence logs <task-id>` to print captured stdout/stderr logs from a
  specified or default workspace.
- Wire `taskfence report <task-id>` to print the Markdown report from a
  specified or default workspace.
- Add CLI tests for parser shape, successful lookup, and missing artifact
  errors.
- Keep `init`, `approve`, and `deny` explicitly unsupported.

Verification command:

```bash
cargo test -p taskfence-cli
```

Verification evidence:

- `cargo test -p taskfence-cli` passed with 17 tests covering parser shape,
  `--workspace` lookup arguments, local stdout/stderr log lookup, local report
  lookup, missing task evidence errors, successful run artifact writes, config
  errors, orchestrator failures, and explicit unsupported approval commands.

### Phase 3: Documentation And Quality Gate

Status: done

Scope:

- Update README, architecture, roadmap, and runtime architecture docs to match
  the implemented local evidence query surface.
- Run formatting, focused tests, and workspace tests.
- Record final evidence and create one focused commit.

Verification command:

```bash
cargo fmt --all --check
cargo test -p taskfence-state -p taskfence-cli
cargo test --workspace
```

Verification evidence:

- `cargo fmt --all` ran and completed.
- `cargo fmt --all --check` passed.
- `cargo test -p taskfence-state -p taskfence-cli` passed with 8
  `taskfence-state` tests and 17 `taskfence-cli` tests.
- `cargo clippy --workspace --all-targets -- -D warnings` passed.
- `cargo test --workspace` passed. The Docker integration test remained
  ignored as designed unless explicitly run with Docker.

## Commit Plan

1. `feat: add local task evidence lookup commands`

## Open Risks

- Local filesystem lookup requires a workspace root. The first implementation
  will default to the current directory and provide an explicit `--workspace`
  option for looking up evidence under another repository.
- This is not durable cross-workspace state indexing; SQLite-backed task lists
  remain future Phase 4 work.

## Final Evidence

- All phases are terminal with verification evidence.
- `taskfence-state` now provides read-only local task evidence lookup scoped to
  `.taskfence/tasks/<task-id>/` under a workspace root.
- `taskfence-cli` now supports local `logs` and `report` lookup commands with
  `--workspace`, while `init`, `approve`, and `deny` remain explicitly
  unsupported.
- README, architecture, roadmap, and runtime architecture docs describe the
  implemented local query surface without overclaiming cross-workspace indexing,
  Web UI, gateway execution, or approval storage.
