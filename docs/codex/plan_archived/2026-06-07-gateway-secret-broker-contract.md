# Gateway Secret Broker Contract Plan

## Goal

Advance the Phase 3 gateway contract by defining a typed secret broker boundary
and redacted secret references for gateway-mediated tool actions, without
reading raw secrets, injecting credentials into agents, or executing external
tools.

## Plan Source

User request on 2026-06-07: "继续推进后续的开发，完成后提交代码".

Actionable interpretation:

- Continue after the local runner, task evidence lookup, approvals,
  denied-action evidence, tool policy evidence, and gateway approval mediation
  slices.
- Pick the next coherent roadmap gap inside `taskfence-gateway`: secret broker
  contracts and redacted secret references.
- Keep scope inside existing typed gateway, task secret grants, audit/report
  redaction, examples, and docs.
- Preserve secure defaults: raw secret values must not enter tool parameters,
  audit evidence, reports, or the agent process.

Non-goals:

- Do not implement real secret loading, GitHub token usage, MCP/HTTP proxy
  execution, SDK/webhook adapters, Web UI, replay, SQLite state, API server, or
  production credential storage.
- Do not claim that gateway credentials are usable for real tool execution.

## Intake / Snapshot

- Default branch: `main` detected from `origin/HEAD`.
- Working branch: `codex/governance-development-plan`.
- `git pull --ff-only` was attempted and failed because the current branch has
  no upstream tracking branch; work continues from the current checkout.
- Worktree status at intake: clean.
- Current core types include `SecretConfig`, `SecretGrant`, `Action::SecretAccess`,
  and `RedactedValue`.
- Current gateway mediation handles policy and optional approval events, but it
  does not expose a secret broker trait or typed redacted secret references.

## Overall Status

done

## Phases

### Phase 1: Secret Broker Contract

Status: done

Scope:

- Add a gateway-owned `SecretBroker` trait and typed `SecretReference`.
- Add a bounded helper for authorizing gateway-side secret references against
  `ResolvedTask.secrets.available_to_gateway`.
- Return a redacted reference suitable for tool parameters without exposing raw
  values.
- Add tests for allowed grants, unavailable secret denial, wrong-scope denial,
  and no raw secret values in normalized tool parameters.

Verification command:

```bash
cargo test -p taskfence-gateway
```

Verification evidence:

- `cargo test -p taskfence-gateway` passed on 2026-06-07 with 12 gateway tests
  and the gateway doc-test target. Coverage includes allowed grants,
  unavailable secret denial, wrong-scope denial, `expose_to_agent` fail-closed
  behavior, and redacted tool parameters.

### Phase 2: Docs And Evidence

Status: done

Scope:

- Update README, architecture, roadmap, development design, runtime
  architecture facts, and example task secrets to describe the secret broker
  contract without claiming real credential use.
- Run any focused report/audit tests needed to prove raw values are not
  rendered.

Verification command:

```bash
cargo test -p taskfence-audit -p taskfence-report
```

Verification evidence:

- `cargo test -p taskfence-audit -p taskfence-report` passed on 2026-06-07
  with 3 audit tests, 4 report tests, and both doc-test targets.

### Phase 3: Quality Gate, Archive, Commit

Status: done

Scope:

- Run formatting, focused tests, clippy, and workspace tests.
- Archive this plan and create one focused commit.

Verification command:

```bash
cargo fmt --all --check
cargo test -p taskfence-gateway -p taskfence-audit -p taskfence-report
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```

Verification evidence:

- `cargo fmt --all` ran and completed.
- `cargo fmt --all --check` passed.
- `cargo test -p taskfence-gateway -p taskfence-audit -p taskfence-report`
  passed with 12 gateway tests, 3 audit tests, 4 report tests, and all three
  doc-test targets.
- `cargo clippy --workspace --all-targets -- -D warnings` passed.
- `cargo test --workspace` passed. The Docker integration test remained
  ignored as designed unless explicitly run with Docker and a local test image.

## Commit Plan

1. `feat: add gateway secret broker contract`

## Open Risks

- The broker returns redacted references only; it does not read, validate,
  rotate, or use real credentials.
- Secret grants are scoped by current `SecretGrant.use_for` strings; a richer
  credential policy language remains future work.

## Final Evidence

- All phases are terminal with verification evidence.
- `taskfence-gateway` now defines `SecretBroker` and `SecretReference`.
- `gateway_secret_reference` authorizes requested secret references against
  `ResolvedTask.secrets.available_to_gateway` and fails closed when secrets
  would be exposed to the agent.
- `attach_secret_reference` inserts a `RedactedValue::Redacted` tool parameter
  without adding raw credential values or broker handles.
- README, architecture, roadmap, development design, runtime architecture
  facts, and `examples/task.yaml` describe configured gateway secret grants
  without claiming real credential use or external tool execution.
