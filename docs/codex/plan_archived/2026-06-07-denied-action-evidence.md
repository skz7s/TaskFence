# Denied Action Evidence Plan

## Goal

Advance Phase 2 by ensuring policy-denied and approval-denied task runs leave
structured local evidence and a Markdown report, instead of failing before the
artifact/report pipeline starts.

## Plan Source

User request on 2026-06-07: "继续推进后续的开发".

Actionable interpretation:

- Continue after local runner, local evidence lookup, and opt-in interactive
  approval slices.
- Pick the next coherent Phase 2 slice from the roadmap: denied action records
  and reports that show denied actions.
- Preserve secure defaults: deny still stops the current local run and no agent
  command is executed after a denied decision.
- Keep the scope inside orchestrator evidence/report behavior. Do not implement
  durable `approve` / `deny` commands, SQLite state, Web UI, gateway execution,
  replay, or skip-and-continue semantics for deny decisions.

## Intake / Snapshot

- Default branch: `main` detected from `origin/HEAD`.
- Working branch: `codex/governance-development-plan`.
- `git pull --ff-only` was attempted and failed because the current branch has
  no upstream tracking branch; work continues from the current checkout.
- Worktree status at intake: clean.
- Current implemented path: policy and approval decisions are structured audit
  events, but denial can occur before the orchestrator creates artifact refs and
  report output.
- Current roadmap gap: broader denied action records and reports that show
  approved and denied actions.

## Overall Status

done

## Phases

### Phase 1: Orchestrator Denial Evidence

Status: done

Scope:

- Ensure task artifact refs are available before command policy evaluation can
  deny or request approval.
- When policy denies or approval is denied/timed out, record the structured
  denial events, write resolved task evidence, attempt report generation, and
  return a `TaskResult` with `TaskStatus::Denied`.
- Preserve the invariant that denied runs do not start the runner.
- Add core tests for policy denial and approval denial evidence/report behavior.

Verification command:

```bash
cargo test -p taskfence-core
```

Verification evidence:

- `cargo test -p taskfence-core` passed with 6 tests, including policy denial
  and approval denial paths that return `TaskStatus::Denied`, generate report
  evidence, and do not prepare or start the runner.

### Phase 2: Report And Documentation

Status: done

Scope:

- Confirm Markdown reports surface denied actions and approval denials from
  structured events.
- Update README, architecture, roadmap, and runtime architecture docs to state
  denied local runs produce evidence/report artifacts when artifact creation is
  available.

Verification command:

```bash
cargo test -p taskfence-report
```

Verification evidence:

- `cargo test -p taskfence-report` passed with 3 tests, including an approval
  denial report built from structured policy and approval events.

### Phase 3: Quality Gate And Commit

Status: done

Scope:

- Run formatting, focused tests, clippy, and workspace tests.
- Archive this plan and create one focused commit.

Verification command:

```bash
cargo fmt --all --check
cargo test -p taskfence-core -p taskfence-report
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```

Verification evidence:

- `cargo fmt --all` ran and completed.
- `cargo fmt --all --check` passed.
- `cargo test -p taskfence-core -p taskfence-report` passed with 6
  `taskfence-core` tests and 3 `taskfence-report` tests.
- `cargo clippy --workspace --all-targets -- -D warnings` passed.
- `cargo test --workspace` passed. The Docker integration test remained
  ignored as designed unless explicitly run with Docker.

## Commit Plan

1. `feat: report denied local actions`

## Open Risks

- If artifact directory creation itself fails, there is still nowhere local to
  write report evidence. That remains an artifact failure path and should be
  reported as such.
- This slice records denied task runs; it does not implement partial
  skip-and-continue semantics for individual denied actions.

## Final Evidence

- All phases are terminal with verification evidence.
- `taskfence-core` now creates task artifact refs and writes resolved task
  evidence before command policy/approval evaluation, so pre-run denial paths
  can generate structured reports.
- Policy-denied and approval-denied command decisions now return
  `TaskStatus::Denied`, do not prepare or start the runner, and still attempt
  report generation.
- `taskfence-report` has coverage for approval-denied reports built from
  structured policy and approval events.
- README, architecture, roadmap, development design, and runtime architecture
  docs describe denied local-run evidence without overclaiming durable approval
  queues, skip-and-continue deny semantics, Web UI, gateway execution, or
  replay.
