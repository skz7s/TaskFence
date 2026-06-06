# Local Approval Detail Plan

## Goal

Advance the local external approval review workflow by adding a read-only
workspace-local single approval detail command without introducing Web UI, API
server behavior, SQLite state, service-side approvals, or cross-workspace
indexing.

## Plan Source

User request on 2026-06-07: "继续推进后续的开发，完成后提交代码".

Actionable interpretation:

- Continue after the local runner, local evidence lookup, approvals,
  denied-action evidence, tool policy evidence, gateway approval mediation,
  secret broker contract, MCP/HTTP adapter stubs, local task list, local task
  diff, local approval list, and local task detail slices.
- Pick the next bounded local review gap: operators can list approval records
  and resolve known approval IDs, but cannot ask for one approval record's
  structured details from the CLI.
- Keep the implementation local, read-only, filesystem-backed, and scoped to a
  single workspace's `.taskfence/approvals/<approval-id>.json` record.

Non-goals:

- Do not introduce a Web UI, API server, SQLite state, service-side approval
  system, replay execution, cross-workspace indexing, gateway execution, or
  team-server behavior.
- Do not expose raw tool parameter values or infer approval state from reports
  or terminal output.

## Intake / Snapshot

- Default branch: `main` detected from `origin/HEAD`.
- Working branch: `codex/governance-development-plan`.
- `git pull --ff-only` was attempted and failed because the current branch has
  no upstream tracking branch; work continues from the current checkout.
- Worktree status at intake: clean.
- Current local approval lookup supports listing local approval records and
  resolving a known approval ID, but not rendering one approval record's
  structured details.

## Overall Status

done

## Phases

### Phase 1: CLI Approval Detail

Status: done

Scope:

- Add `taskfence approval <approval-id> --workspace <workspace>`.
- Read the approval JSON record through `LocalApprovalStore::read()`.
- Render approval ID, task ID, status, actor/source, requested/resolved times,
  action summary, and policy decision summary without raw tool parameter values.

Verification command:

```bash
cargo test -p taskfence-cli -p taskfence-approval
```

Verification evidence:

- `cargo test -p taskfence-cli -p taskfence-approval` passed on 2026-06-07
  after adding the CLI parser, local store read, structured rendering, and
  redaction coverage for approval detail output.

### Phase 2: Docs

Status: done

Scope:

- Update README, architecture, roadmap, development design, and runtime facts to
  describe the local approval detail command without claiming Web UI/API/SQLite
  support.

Verification command:

```bash
cargo test -p taskfence-cli -p taskfence-approval
```

Verification evidence:

- `cargo test -p taskfence-cli -p taskfence-approval` passed on 2026-06-07
  after synchronizing README, architecture, roadmap, development design, and
  runtime facts for the local approval detail command.

### Phase 3: Quality Gate, Archive, Commit

Status: done

Scope:

- Run formatting, focused tests, clippy, and workspace tests.
- Archive this plan and create one focused commit.

Verification command:

```bash
cargo fmt --all --check
cargo test -p taskfence-cli -p taskfence-approval
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```

Verification evidence:

- `cargo fmt --all --check` passed on 2026-06-07.
- `cargo test -p taskfence-cli -p taskfence-approval` passed on 2026-06-07.
- `cargo clippy --workspace --all-targets -- -D warnings` passed on
  2026-06-07.
- `cargo test --workspace` passed on 2026-06-07; the existing Docker
  integration test remained ignored because it requires a Docker daemon and a
  locally available test image.

## Final Evidence

- Added `taskfence approval <approval-id> --workspace <workspace>` as a
  read-only workspace-local approval detail query.
- Approval detail rendering uses `LocalApprovalStore::read()` and prints
  structured ID, task, status, actor/source, timestamps, action summary, and
  policy summary without raw tool parameter values.
- README, architecture, roadmap, development design, and runtime facts now
  describe the local approval detail command without claiming Web UI, API,
  SQLite, service-side approval, or cross-workspace indexing support.

## Commit Plan

1. `feat: add local approval detail`

## Open Risks

- This remains a workspace-local file query, not a durable multi-workspace
  approval index or service-side approval system.
- Approval action and policy summaries must stay redacted and must not display
  raw tool parameter values.
