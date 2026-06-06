# Gateway Approval Mediation Plan

## Goal

Advance the Phase 3 gateway contract by letting typed gateway mediation request
and resolve approvals for approval-required tool calls while still avoiding any
real MCP, HTTP, CLI wrapper, SDK, webhook, or secret-broker execution.

## Plan Source

User request on 2026-06-07: "继续推进后续的开发，完成后提交代码".

Actionable interpretation:

- Continue after the local runner, task evidence lookup, interactive approval,
  external approval, denied-action evidence, and tool policy evidence slices.
- Pick the next coherent roadmap gap without jumping to production gateway
  execution: gateway approval mediation for configured tool actions.
- Keep scope inside existing typed gateway, policy, approval, audit, report,
  examples, and docs contracts.
- Preserve secure defaults: explicit deny still stops immediately, approval
  denial/timeout fails closed, and allowed actions are only mediated, not
  executed.

Non-goals:

- Do not implement MCP/HTTP proxy execution, GitHub integration, secret broker
  credential use, Web UI, replay, SQLite state, API server, or real tool adapter
  invocation.
- Do not broaden the policy language beyond current normalized
  `tool.operation` patterns.

## Intake / Snapshot

- Default branch: `main` detected from `origin/HEAD`.
- Working branch: `codex/governance-development-plan`.
- `git pull --ff-only` was attempted and failed because the current branch has
  no upstream tracking branch; work continues from the current checkout.
- Worktree status at intake: clean.
- Current gateway path normalizes tool actions, evaluates policy, records a
  `PolicyDecision`, and returns the decision without executing the tool.
- Current approval path is available through `ApprovalEngine`, but gateway
  mediation does not yet request/wait on approvals or record
  `ApprovalRequested` / `ApprovalResolved` events.

## Overall Status

done

## Phases

### Phase 1: Gateway Approval Contract

Status: done

Scope:

- Extend gateway mediation with an optional `ApprovalEngine` dependency.
- Keep `GatewayMediator::new(policy, audit)` behavior compatible for policy-only
  mediation.
- Add an explicit `with_approval` path that requests/waits for approval-required
  tool calls and records structured approval audit events.
- Keep policy-only mediation compatible: an approval-required decision is
  returned without execution when no approval engine is configured.
- Fail closed for denied actions, denied approval, timed-out approval, and
  unsupported protocols.

Verification command:

```bash
cargo test -p taskfence-gateway
```

Verification evidence:

- `cargo test -p taskfence-gateway` passed on 2026-06-07 with 8 gateway tests
  and the gateway doc-test target. Coverage includes policy-only mediation,
  configured tool policy decisions, approved approval-required tool calls, and
  denied/timed-out approval fail-closed behavior.

### Phase 2: Report, Docs, And Examples

Status: done

Scope:

- Add/adjust report coverage if needed so gateway approval events for tool calls
  remain visible without rendering raw parameter values.
- Update README, architecture, roadmap, development design, runtime architecture
  facts, and example comments/config if the public contract changes.
- Keep all docs explicit that gateway mediation still does not execute external
  tools.

Verification command:

```bash
cargo test -p taskfence-report
```

Verification evidence:

- `cargo test -p taskfence-report` passed on 2026-06-07 with 4 report tests
  and the report doc-test target. Existing report coverage renders
  approval-required tool-call evidence without raw parameter values.

### Phase 3: Quality Gate, Archive, Commit

Status: done

Scope:

- Run formatting, focused tests, clippy, and workspace tests.
- Archive this plan and create one focused commit.

Verification command:

```bash
cargo fmt --all --check
cargo test -p taskfence-gateway -p taskfence-report
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```

Verification evidence:

- `cargo fmt --all` ran and completed.
- `cargo fmt --all --check` passed.
- `cargo test -p taskfence-gateway -p taskfence-report` passed with 8 gateway
  tests, 4 report tests, and both doc-test targets.
- `cargo clippy --workspace --all-targets -- -D warnings` passed.
- `cargo test --workspace` passed. The Docker integration test remained
  ignored as designed unless explicitly run with Docker and a local test image.

## Commit Plan

1. `feat: add gateway approval mediation contract`

## Open Risks

- Gateway approval mediation remains a typed contract and does not execute
  external tool actions after approval.
- Policy-only gateway mediation remains useful for evidence tests, but callers
  must opt into an approval engine to resolve approval-required decisions.

## Final Evidence

- All phases are terminal with verification evidence.
- `GatewayMediator` can now be configured with an optional `ApprovalEngine`
  through `with_approval`.
- Approval-required tool actions record `PolicyDecision`,
  `ApprovalRequested`, and `ApprovalResolved` events when approval mediation is
  explicitly attached.
- Approved tool calls return the resolved approval record without executing an
  external tool action.
- Denied and timed-out tool approvals fail closed after recording the resolved
  approval event.
- README, architecture, roadmap, development design, and runtime architecture
  facts document the optional gateway approval mediation contract without
  claiming real gateway execution.
