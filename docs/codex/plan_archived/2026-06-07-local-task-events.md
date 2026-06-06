# Local Task Events Plan

## Goal

Advance local evidence review by adding a read-only workspace-local task event
summary command without introducing Web UI, API server behavior, SQLite state,
replay execution, service-side state, or cross-workspace indexing.

## Plan Source

User request on 2026-06-07: "继续推进后续的开发，完成后提交代码".

Actionable interpretation:

- Continue after the local runner, local task evidence lookup, approvals,
  denied-action evidence, tool policy evidence, gateway approval mediation,
  secret broker contract, MCP/HTTP adapter stubs, local task list, local task
  diff, local approval list, local task detail, and local approval detail
  slices.
- Pick the next bounded local review gap: operators can read generated reports,
  diffs, logs, task summaries, and approval records, but cannot ask for a
  structured local event timeline from `.taskfence/tasks/<task-id>/events.jsonl`
  without opening raw JSONL.
- Keep the implementation local, read-only, filesystem-backed, and scoped to a
  single workspace's `.taskfence/tasks/<task-id>/events.jsonl` artifact.

Non-goals:

- Do not introduce a Web UI, API server, SQLite state, replay execution,
  service-side task state, cross-workspace indexing, gateway execution, or
  team-server behavior.
- Do not render raw tool parameter values from tool-call policy or approval
  events.
- Do not infer events from reports or terminal output.

## Intake / Snapshot

- Default branch: `main` detected from `origin/HEAD`.
- Working branch: `codex/governance-development-plan`.
- `git pull --ff-only` was attempted and failed because the current branch has
  no upstream tracking branch; work continues from the current checkout.
- Worktree status at intake: clean.
- Current local evidence lookup supports reports, logs, diffs, task summaries,
  task lists, approval lists, and approval details, but not a structured task
  event timeline.

## Overall Status

done

## Phases

### Phase 1: State Reader And CLI Events Command

Status: done

Scope:

- Add a local state reader for `.taskfence/tasks/<task-id>/events.jsonl` that
  parses each non-empty JSONL line as an `AuditEvent`.
- Fail clearly for missing or malformed event files rather than scraping report
  text.
- Add `taskfence events <task-id> --workspace <workspace>` and render a compact
  timeline with event kind, timestamp, and safe summaries.
- Keep tool action rendering parameter-count based, not parameter-value based.

Verification command:

```bash
cargo test -p taskfence-cli -p taskfence-state
```

Verification evidence:

- `cargo test -p taskfence-cli -p taskfence-state` passed on 2026-06-07
  after adding the local `events.jsonl` state reader, `taskfence events`
  parser/renderer, malformed/mismatched event handling, and redaction coverage
  for tool-call event summaries.

### Phase 2: Docs

Status: done

Scope:

- Update README, architecture, roadmap, development design, and runtime facts to
  describe the local events command without claiming Web UI/API/SQLite/replay
  support.

Verification command:

```bash
cargo test -p taskfence-cli -p taskfence-state
```

Verification evidence:

- `cargo test -p taskfence-cli -p taskfence-state` passed on 2026-06-07
  after synchronizing README, architecture, roadmap, development design, and
  runtime facts for the local task events command.

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

- `cargo fmt --all --check` passed on 2026-06-07.
- `cargo test -p taskfence-cli -p taskfence-state` passed on 2026-06-07.
- `cargo clippy --workspace --all-targets -- -D warnings` passed on
  2026-06-07.
- `cargo test --workspace` passed on 2026-06-07; the existing Docker
  integration test remained ignored because it requires a Docker daemon and a
  locally available test image.

## Final Evidence

- Added `taskfence events <task-id> --workspace <workspace>` as a read-only
  workspace-local task event timeline query.
- `taskfence-state` now reads `.taskfence/tasks/<task-id>/events.jsonl` as
  structured `AuditEvent` records and fails clearly for missing, malformed, or
  mismatched event files.
- CLI event summaries render event kind, timestamp, and safe summaries without
  raw tool parameter values or raw log text.
- README, architecture, roadmap, development design, and runtime facts now
  describe the local events command without claiming Web UI, API, SQLite,
  replay, service-side state, or cross-workspace indexing support.

## Commit Plan

1. `feat: add local task events`

## Open Risks

- This remains a read-only workspace-local artifact query, not durable
  cross-workspace state.
- Event summaries must stay structured and redacted; raw tool parameter values
  must not appear in CLI output.
