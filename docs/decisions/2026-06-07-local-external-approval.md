# Local External Approval Queue

## Context

TaskFence local approval was fail-closed by default, with an opt-in
in-process terminal prompt for `taskfence run --interactive-approval`. The CLI
already parsed `approve` and `deny`, but they were unsupported because there was
no local approval storage that another terminal could safely resolve.

Changing default `taskfence run` to wait for outside input would weaken the
current secure default and make non-interactive automation hang. Introducing
SQLite, an API server, Web UI, or cross-workspace approval indexing would exceed
the current local runner slice.

## Decision

Keep default `taskfence run <task-file>` fail-closed for approval-required
actions.

Add an explicit local external approval mode:

- `taskfence run --external-approval <task-file>` writes pending approval
  records under `.taskfence/approvals/<approval-id>.json` in the task workspace.
- `taskfence approve <approval-id> --workspace <workspace>` and
  `taskfence deny <approval-id> --workspace <workspace>` resolve those pending
  workspace-local records.
- The running task process polls the record until it is resolved or the task
  approval timeout expires.
- Timeout resolves as `ApprovalDecision::TimedOut`, preserving fail-closed task
  behavior.

This is a workspace-local file queue, not a cross-workspace approval index, API
server, Web UI, or team approval system.

## Consequences

- Non-interactive local runs keep the secure fail-closed default.
- Operators can approve or deny high-risk local actions from another terminal
  only when they opt into `--external-approval`.
- Approval records use the existing typed `ApprovalRecord` contract, so audit
  and report paths keep consuming structured approval evidence.
- Future SQLite, API, Web UI, or team-server approval work can replace or index
  the local file queue without changing default run semantics.

## Validation And Rollback

Validation:

- `cargo test -p taskfence-approval`
- `cargo test -p taskfence-cli`
- `cargo clippy --workspace --all-targets -- -D warnings`
- `cargo test --workspace`

Rollback is to remove `--external-approval`, the local approval store, and the
`approve` / `deny` command wiring while leaving default fail-closed and
`--interactive-approval` behavior intact.
