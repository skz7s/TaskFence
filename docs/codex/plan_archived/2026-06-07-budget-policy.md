# Local Budget Policy Plan

## Goal

Add a narrow built-in budget policy boundary so budget consumption actions are denied by default and only allowed when explicitly configured under task permissions.

## Overall Status

done

## Plan Source

The operator asked to continue follow-up development and commit the completed work. This slice advances Phase 2 policy and approval work by replacing the current permissive `Action::Budget` default with an explicit task-file budget allowlist and limit check. It does not add live cost metering, model/provider accounting, Web UI budget display, team quotas, billing integrations, or runtime observation of agent token usage.

## Intake / Snapshot

- Status: done
- Date: 2026-06-07
- Default branch: `main` from `origin/HEAD`
- Working branch: `codex/governance-development-plan`
- Worktree at intake: clean
- Latest commit at intake: `07f5a03 feat: add local task compare command`
- Sync attempt: `git pull --ff-only` failed because the current branch has no upstream tracking branch.
- Existing behavior: `Action::Budget` is allowed by default in the built-in policy engine, and task files have no explicit `permissions.budget` schema.
- Next executable phase: add typed budget permissions, parser support, policy evaluation, tests, and docs.

## Scope

- Add typed budget permission configuration to the shared task contract.
- Parse `permissions.budget.allow` entries from task YAML.
- Validate budget kinds and positive limits.
- Evaluate `Action::Budget { kind, amount }` by explicit kind match and maximum amount.
- Deny missing kinds and over-limit amounts.
- Update docs that describe implemented Phase 2 policy boundaries.

## Non-Goals

- No live token, cost, or provider metering.
- No model gateway budget accounting.
- No team, organization, or billing quotas.
- No Web UI budget view.
- No task runner budget observation beyond policy evaluation of typed actions.

## Acceptance Criteria

- Task files may configure `permissions.budget.allow` with kind and max amount.
- Empty budget kinds and zero max amounts fail config validation.
- Budget actions with no matching configured kind are denied.
- Budget actions over the configured max amount are denied.
- Budget actions within the configured max amount are allowed.
- Existing task files without budget configuration keep parsing but budget actions deny by default.

## Phases

1. Budget contract and parser
   - Status: done
   - Scope: add core budget permission types and config parsing/validation.
   - Verification: `cargo test -p taskfence-config`; `cargo check --workspace`; `cargo test -p taskfence-runner --tests --no-run`
   - Evidence: `taskfence-config` passed 9 tests, including budget parsing and invalid budget config rejection; workspace check passed; runner integration tests compiled after adding the new default budget field to test fixtures.
2. Policy evaluation and tests
   - Status: done
   - Scope: replace default budget allow with explicit limit checks and targeted tests.
   - Verification: `cargo test -p taskfence-policy`
   - Evidence: `taskfence-policy` passed 13 tests, including no-limit denial, within-limit allow, and over-limit denial for typed budget actions.
3. Documentation, validation, archive, and commit
   - Status: done
   - Scope: update relevant docs, run formatting/tests/lint, archive the plan, stage task-owned files, and commit.
   - Verification: `rg -n 'permissions\.budget|Budget Policy Default Deny|budget actions deny|budget action|budget policy|max_amount' README.md docs examples crates/taskfence-core/src/lib.rs crates/taskfence-config/src/lib.rs crates/taskfence-policy/src/lib.rs`; `cargo fmt --all --check`; `git diff --check`; full clippy/tests pending final closeout.
   - Evidence: docs and code references for the explicit budget policy were found; formatting and whitespace checks passed; workspace clippy passed with warnings denied; workspace tests passed with the Docker integration test ignored because it requires a Docker daemon and locally available test image.

## Commit Plan

1. `feat: add explicit budget policy limits`

## Final Evidence

- `git pull --ff-only` was attempted before implementation and failed because the current branch has no upstream tracking branch.
- `cargo check --workspace` passed after adding the budget permission field and parser.
- `cargo test -p taskfence-config` passed 9 tests.
- `cargo test -p taskfence-policy` passed 13 tests.
- `cargo test -p taskfence-runner --tests --no-run` compiled runner unit tests and Docker integration tests.
- `rg -n 'permissions\.budget|Budget Policy Default Deny|budget actions deny|budget action|budget policy|max_amount' README.md docs examples crates/taskfence-core/src/lib.rs crates/taskfence-config/src/lib.rs crates/taskfence-policy/src/lib.rs` confirmed documentation, ADR, example, and implementation coverage.
- `cargo fmt --all --check` passed.
- `git diff --check` passed.
- `cargo clippy --workspace --all-targets -- -D warnings` passed.
- `cargo test --workspace` passed; the Docker integration test stayed ignored as expected because it requires a Docker daemon and a locally available test image.
