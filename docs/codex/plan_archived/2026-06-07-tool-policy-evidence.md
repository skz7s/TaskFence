# Tool Policy Evidence Plan

## Goal

Advance the Phase 2 to Phase 3 contract boundary by making tool-call policy
configuration parseable from task files and proving gateway/tool-call
allow/approval/deny decisions appear in structured audit/report evidence.

## Plan Source

User request on 2026-06-07: "继续推进后续的开发，完成后提交代码".

Actionable interpretation:

- Continue after the local runner, task evidence lookup, interactive approval,
  denied-action evidence, and local external approval slices.
- Pick the next coherent roadmap gap without jumping to full gateway execution:
  tool-call policy evidence for future gateway-mediated actions.
- Keep scope inside existing typed contracts, built-in policy, gateway
  mediation stubs, report rendering, examples, and docs.
- Do not implement MCP/HTTP execution, secret broker runtime behavior, Web UI,
  replay, SQLite state, API server, or real tool adapter invocation.

## Intake / Snapshot

- Default branch: `main` detected from `origin/HEAD`.
- Working branch: `codex/governance-development-plan`.
- `git pull --ff-only` was attempted and failed because the current branch has
  no upstream tracking branch; work continues from the current checkout.
- Worktree status at intake: clean.
- Current implemented path: `PermissionConfig` already has typed
  `ToolPermissions`, and `BuiltInPolicyEngine` can evaluate `Action::ToolCall`,
  but task-file parsing always leaves tool permissions at defaults.
- Current gateway path normalizes tool actions, evaluates policy, and writes a
  `PolicyDecision` event, but tests do not prove configured tool policy through
  the task-file schema or report denied/approval-required tool-call evidence.

## Overall Status

done

## Phases

### Phase 1: Task-File Tool Permissions

Status: done

Scope:

- Parse `permissions.tools.allow`, `permissions.tools.approval_required`, and
  `permissions.tools.deny` from task files into `PermissionConfig`.
- Add config tests proving tool permissions are accepted and unknown fields
  still fail closed.
- Add built-in policy tests for tool allow, approval-required, deny precedence,
  and default deny behavior.

Verification command:

```bash
cargo test -p taskfence-config -p taskfence-policy
```

Verification evidence:

- `cargo test -p taskfence-config -p taskfence-policy` passed on 2026-06-07:
  7 config tests, 10 policy tests, and both doc-test targets passed.

### Phase 2: Gateway And Report Evidence

Status: done

Scope:

- Add gateway tests proving a configured tool policy decision is emitted as a
  structured `PolicyDecision` audit event without executing the tool action.
- Add report tests proving denied and approval-required tool calls appear in
  the Tool Calls, Approvals, and Denied Actions sections without leaking raw
  secret-like parameter values.

Verification command:

```bash
cargo test -p taskfence-gateway -p taskfence-report
```

Verification evidence:

- `cargo test -p taskfence-gateway -p taskfence-report` passed on
  2026-06-07: 5 gateway tests, 4 report tests, and both doc-test targets
  passed.

### Phase 3: Examples, Docs, Quality Gate, Commit

Status: done

Scope:

- Update `examples/task.yaml`, README, architecture, roadmap, development
  design, and runtime architecture facts to describe configured tool policy
  evidence without claiming real gateway execution.
- Run formatting, focused tests, clippy, and workspace tests.
- Archive this plan and create one focused commit.

Verification command:

```bash
cargo fmt --all --check
cargo test -p taskfence-config -p taskfence-policy -p taskfence-gateway -p taskfence-report
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```

Verification evidence:

- `cargo fmt --all` ran and completed.
- `cargo fmt --all --check` passed.
- `cargo test -p taskfence-config -p taskfence-policy -p taskfence-gateway -p taskfence-report`
  passed with 7 config tests, 10 policy tests, 5 gateway tests, 4 report tests,
  and all four doc-test targets.
- `cargo clippy --workspace --all-targets -- -D warnings` passed.
- `cargo test --workspace` passed. The Docker integration test remained
  ignored as designed unless explicitly run with Docker and a local test image.

## Commit Plan

1. `feat: add tool policy evidence contracts`

## Open Risks

- Gateway mediation remains a typed contract/stub; it does not execute MCP,
  HTTP, CLI wrapper, SDK, webhook, or secret-broker actions.
- Tool policy patterns currently match normalized `tool.operation` strings and
  are intentionally narrower than a full policy language.

## Final Evidence

- All phases are terminal with verification evidence.
- Task files now parse `permissions.tools.allow`,
  `permissions.tools.approval_required`, and `permissions.tools.deny` into
  `PermissionConfig`.
- The built-in policy engine is tested for tool allow, approval-required, deny
  precedence, and default deny behavior.
- Gateway mediation is tested with configured tool policy decisions and writes
  structured `PolicyDecision` audit events without executing the tool action.
- Markdown reports are tested for denied and approval-required tool-call
  evidence without rendering raw parameter values.
- README, architecture, roadmap, development design, runtime architecture
  facts, and the example task describe configured tool policy evidence without
  claiming real gateway execution.
