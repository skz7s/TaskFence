# Local Approval List Plan

## Goal

Advance the local external approval workflow by adding a read-only
workspace-local approval queue listing command without introducing Web UI, API
server behavior, SQLite state, cross-workspace indexing, or broader gateway
execution.

## Plan Source

User request on 2026-06-07: "继续推进后续的开发，完成后提交代码".

Actionable interpretation:

- Continue after the local runner, local evidence query, approvals,
  denied-action evidence, tool policy evidence, gateway approval mediation,
  secret broker contract, MCP/HTTP adapter stubs, local task list, and local
  task diff slices.
- Pick the next bounded local review gap: operators can resolve approval IDs
  with `taskfence approve` / `taskfence deny`, but cannot list pending records
  from the CLI.
- Keep the implementation local, read-only, filesystem-backed, and scoped to a
  single workspace's `.taskfence/approvals` directory.

Non-goals:

- Do not introduce a Web UI, API server, SQLite state, replay execution,
  cross-workspace indexing, gateway execution, or team-server behavior.
- Do not infer approval state from task reports or terminal output.

## Intake / Snapshot

- Default branch: `main` detected from existing project plan history and
  `origin/HEAD`.
- Working branch: `codex/governance-development-plan`.
- `git pull --ff-only` was attempted and failed because the current branch has
  no upstream tracking branch; work continues from the current checkout.
- Worktree status at intake: clean.
- Current local external approval workflow writes
  `.taskfence/approvals/<approval-id>.json` records and supports resolving a
  known approval ID through `taskfence approve` / `taskfence deny`, but not
  listing local pending approval records.

## Overall Status

done

## Phases

### Phase 1: Approval Store Listing

Status: done

Scope:

- Add a typed local approval summary or listing API to `taskfence-approval`.
- Read approval JSON records from `.taskfence/approvals`.
- Preserve path safety for approval IDs and keep malformed approval evidence
  explicit instead of silently treating it as approved.
- Sort listed records deterministically.

Verification command:

```bash
cargo test -p taskfence-approval
```

Verification evidence:

- `cargo fmt --all` ran and completed.
- `cargo test -p taskfence-approval` passed on 2026-06-07 with 18 approval
  tests and the approval doc-test target.
- After adding approval record ID/path consistency validation, the focused
  `cargo test -p taskfence-cli -p taskfence-approval` run passed with 19
  approval tests.

### Phase 2: CLI And Docs

Status: done

Scope:

- Add `taskfence approvals --workspace <workspace>`.
- Render a compact local approval list with approval id, task id, status,
  requested time, and action summary.
- Update README, architecture, roadmap, development design, and runtime facts to
  describe the local approval list without claiming Web UI/API/SQLite support.

Verification command:

```bash
cargo test -p taskfence-cli -p taskfence-approval
```

Verification evidence:

- `cargo test -p taskfence-cli -p taskfence-approval` passed on 2026-06-07
  with 36 CLI tests, 19 approval tests, and the approval doc-test target.
- README, architecture, roadmap, development design, and runtime architecture
  facts were updated to describe the workspace-local approval list without
  claiming Web UI, API server, SQLite, or cross-workspace indexing.

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

- `cargo fmt --all --check` passed.
- `cargo test -p taskfence-cli -p taskfence-approval` passed with 36 CLI tests,
  19 approval tests, and the approval doc-test target.
- `cargo clippy --workspace --all-targets -- -D warnings` passed.
- `cargo test --workspace` passed. The Docker integration test remained ignored
  as designed unless explicitly run with Docker and a local test image.

## Commit Plan

1. `feat: add local approval list`

## Open Risks

- This remains a workspace-local file queue, not a durable multi-workspace
  approval index or service-side approval system.
- Malformed approval records should be visible to the operator and should not
  be silently considered resolved.

## Final Evidence

- All phases are terminal with verification evidence.
- `taskfence-approval` now exposes `LocalApprovalStore::list()` for deterministic
  workspace-local approval record listing.
- Approval listing validates file-safe approval IDs, rejects malformed JSON, and
  rejects records whose embedded approval ID does not match the queue file name.
- `taskfence approvals --workspace <workspace>` renders local approval records
  with approval ID, task ID, status, requested time, and action summary.
- README, architecture, roadmap, development design, and runtime architecture
  facts describe the local approval list without claiming Web UI, API server,
  SQLite state, replay, gateway execution, service-side approvals, or
  cross-workspace indexing.
