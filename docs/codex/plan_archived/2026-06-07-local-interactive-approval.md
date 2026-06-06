# Local Interactive Approval Plan

## Goal

Advance Phase 2 by adding an opt-in local interactive approval path for
approval-required actions during `taskfence run`, while preserving the default
fail-closed CLI behavior and keeping durable approval storage commands for a
later slice.

## Plan Source

User request on 2026-06-07: "继续推进后续的开发".

Actionable interpretation:

- Continue after the local runner and local evidence lookup slices.
- Pick the next coherent Phase 2 development slice from the roadmap.
- Keep default secure behavior: approval-required actions still fail closed
  unless the operator explicitly opts into interactive local approval.
- Record approval requests and decisions through existing structured audit
  events and reports.
- Do not implement `taskfence approve` / `taskfence deny` durable lookup
  commands, SQLite state, API server, Web UI, gateway execution, or replay.

## Intake / Snapshot

- Default branch: `main` detected from `origin/HEAD`.
- Working branch: `codex/governance-development-plan`.
- `git pull --ff-only` was attempted and failed because the current branch has
  no upstream tracking branch; work continues from the current checkout.
- Worktree status at intake: clean.
- Current implemented path: `taskfence run <task-file>` uses
  `LocalApprovalEngine::fail_closed()` and denies approval-required commands in
  non-interactive CLI mode.
- Current approval engine supports preconfigured approved/denied/timed-out
  records in tests, but no terminal prompt for local operators.

## Overall Status

done

## Phases

### Phase 1: Approval Engine Prompting

Status: done

Scope:

- Add an interactive local approval mode to `taskfence-approval`.
- Render approval request details from typed action and policy decision data.
- Parse explicit approve/deny responses, reject empty or ambiguous responses,
  and support EOF as fail-closed denial.
- Keep existing fail-closed, timeout, and preconfigured modes intact.

Verification command:

```bash
cargo test -p taskfence-approval
```

Verification evidence:

- `cargo test -p taskfence-approval` passed with 8 tests covering existing
  preconfigured, timeout, fail-closed behavior plus interactive prompt
  decisions, explicit response parsing, unknown approval IDs, and prompt
  rendering without raw tool parameter values.

### Phase 2: CLI Run Integration

Status: done

Scope:

- Add an opt-in `taskfence run --interactive-approval <task-file>` flag.
- Keep default `taskfence run <task-file>` non-interactive and fail-closed.
- Add CLI tests proving the default mode remains fail-closed and the opt-in mode
  uses interactive approval infrastructure through test doubles.

Verification command:

```bash
cargo test -p taskfence-cli
```

Verification evidence:

- `cargo test -p taskfence-cli` passed with 20 tests covering parser shape,
  `--interactive-approval`, default fail-closed approval-required commands,
  approved approval-required commands continuing through the runner, local
  evidence lookup commands, config errors, and orchestrator failures.

### Phase 3: Documentation And Quality Gate

Status: done

Scope:

- Update README, architecture, roadmap, and runtime architecture facts to
  describe opt-in local interactive approval and unchanged unsupported durable
  approval commands.
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
- `cargo test -p taskfence-approval -p taskfence-cli` passed with 8
  `taskfence-approval` tests and 20 `taskfence-cli` tests.
- `cargo clippy --workspace --all-targets -- -D warnings` passed after moving
  the test-only `ApprovalDecision` import into the CLI test module.
- `cargo test --workspace` passed. The Docker integration test remained
  ignored as designed unless explicitly run with Docker.

## Commit Plan

1. `feat: add opt-in local interactive approvals`

## Open Risks

- Interactive approval only applies inside the running CLI process. It is not a
  durable approval queue, and `taskfence approve` / `taskfence deny` remain
  unsupported until persistent approval storage exists.
- Non-interactive environments may not have stdin available; the default run
  mode therefore remains fail-closed.

## Final Evidence

- All phases are terminal with verification evidence.
- `taskfence-approval` now supports opt-in interactive local approval via a
  prompt abstraction, explicit approve/deny parsing, EOF fail-closed behavior,
  and prompt rendering from typed action/policy data without raw tool parameter
  values.
- `taskfence-cli` now supports `taskfence run --interactive-approval
  <task-file>`, while default `taskfence run <task-file>` remains
  non-interactive and fail-closed for approval-required actions.
- README, architecture, roadmap, development design, and runtime architecture
  docs describe opt-in local interactive approval without overclaiming durable
  approval queues, `approve` / `deny` commands, Web UI, gateway execution, or
  replay.
