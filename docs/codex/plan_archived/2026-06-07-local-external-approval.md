# Local External Approval Plan

## Goal

Advance Phase 2 by adding a local filesystem-backed approval queue that lets
`taskfence run` explicitly wait for `taskfence approve` or `taskfence deny`
from another terminal, while preserving the default fail-closed behavior for
non-interactive runs.

## Plan Source

User request on 2026-06-07: "继续推进后续的开发，完成后提交代码".

Actionable interpretation:

- Continue after local task evidence lookup, opt-in interactive approval, and
  denied-action evidence/reporting slices.
- Pick the next coherent Phase 2 roadmap gap: durable local approval lookup
  commands.
- Preserve secure defaults: default `taskfence run <task-file>` remains
  fail-closed for approval-required actions.
- Add an explicit local external approval mode rather than silently changing
  default run behavior.
- Keep scope local and filesystem-backed. Do not introduce SQLite, API server,
  Web UI, gateway execution, replay, team-server behavior, or cross-workspace
  approval indexing.

## Intake / Snapshot

- Default branch: `main` detected from `origin/HEAD`.
- Working branch: `codex/governance-development-plan`.
- `git pull --ff-only` was attempted and failed because the current branch has
  no upstream tracking branch; work continues from the current checkout.
- Worktree status at intake after committing the previous slice: clean.
- Current implemented path: approval-required actions either fail closed by
  default, prompt in-process with `--interactive-approval`, or use
  preconfigured decisions in tests.
- Current CLI gap: `approve` and `deny` parse but return explicit unsupported
  errors.

## Overall Status

done

## Phases

### Phase 1: Local Approval Store

Status: done

Scope:

- Add a local approval engine mode that writes pending approval records under
  `.taskfence/approvals/<approval-id>.json` in the task workspace.
- Poll that file until another command resolves it or the task approval timeout
  expires.
- Resolve timeout as `ApprovalDecision::TimedOut` and keep fail-closed task
  behavior.
- Add approval crate tests for pending writes, approve/deny updates, unknown
  approval IDs, unsafe approval IDs, and timeout behavior.

Verification command:

```bash
cargo test -p taskfence-approval
```

Verification evidence:

- `cargo test -p taskfence-approval` passed with 13 tests covering the existing
  fail-closed, preconfigured, timeout, and interactive behavior plus
  filesystem-backed pending writes, approve/deny resolution, unknown approval
  IDs, unsafe approval IDs, double resolution rejection, external wait
  resolution, and timeout-to-`TimedOut` behavior.

### Phase 2: CLI Integration

Status: done

Scope:

- Add `taskfence run --external-approval <task-file>` as the explicit mode that
  waits for local approval files.
- Wire `taskfence approve <approval-id> --workspace <workspace>` and
  `taskfence deny <approval-id> --workspace <workspace>` to resolve pending
  local approval records.
- Reject using `--interactive-approval` and `--external-approval` together.
- Add CLI tests for parser shape, mutual exclusion, successful external
  approval continuation, denied external approval, and approve/deny updates.

Verification command:

```bash
cargo test -p taskfence-cli
```

Verification evidence:

- `cargo test -p taskfence-cli` passed with 27 tests covering parser shape,
  `--external-approval`, approval mode mutual exclusion, workspace-scoped
  `approve` / `deny`, successful external approval continuation, external
  approval denial, default fail-closed behavior, and local task evidence lookup.

### Phase 3: Documentation And Quality Gate

Status: done

Scope:

- Update README, architecture, roadmap, development design, and runtime
  architecture facts to describe the local external approval mode and current
  local-only limits.
- Run formatting, focused tests, clippy, and workspace tests.
- Archive this plan and create one focused commit.

Verification command:

```bash
cargo fmt --all --check
cargo test -p taskfence-approval -p taskfence-cli
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```

Verification evidence:

- `cargo fmt --all` ran and completed.
- `cargo fmt --all --check` passed.
- `cargo test -p taskfence-approval -p taskfence-cli` passed with 13
  `taskfence-approval` tests and 27 `taskfence-cli` tests.
- `cargo clippy --workspace --all-targets -- -D warnings` passed after limiting
  a test-only CLI helper to `#[cfg(test)]`.
- `cargo test --workspace` passed. The Docker integration test remained
  ignored as designed unless explicitly run with Docker.

## Commit Plan

1. `feat: add local external approval commands`

## Open Risks

- External approval requires the `run` process to remain active while another
  terminal resolves the approval record.
- Approval lookup is workspace-local and file-backed; there is no
  cross-workspace queue, SQLite index, API server, or Web UI.

## Final Evidence

- All phases are terminal with verification evidence.
- `taskfence-approval` now provides `LocalApprovalStore` for
  `.taskfence/approvals/<approval-id>.json` records and
  `LocalExternalApprovalEngine` for explicit local external approval waits.
- `taskfence-cli` now supports `taskfence run --external-approval <task-file>`,
  rejects combining external and interactive approval modes, and wires
  `taskfence approve` / `taskfence deny` with `--workspace` to resolve pending
  local approval records.
- README, architecture, roadmap, development design, runtime architecture facts,
  and a decision record document the secure default and local-only external
  approval boundary.
