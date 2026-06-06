# Gateway Adapter Stubs Plan

## Goal

Advance the Phase 3 gateway contract by adding typed MCP and HTTP adapter stubs
that normalize protocol-shaped requests into `ToolAction` values while
returning explicit unsupported errors for execution.

## Plan Source

User request on 2026-06-07: "继续推进后续的开发，完成后提交代码".

Actionable interpretation:

- Continue after the local runner, task evidence lookup, approvals,
  denied-action evidence, tool policy evidence, gateway approval mediation, and
  gateway secret broker contract slices.
- Pick the next bounded `taskfence-gateway` gap from `docs/development-design.md`:
  MCP/HTTP adapter stubs returning explicit unsupported errors.
- Keep scope inside typed gateway contracts, tests, examples, and docs.

Non-goals:

- Do not implement MCP/HTTP network servers, clients, proxying, GitHub
  integration, SDK/webhook adapters, real tool execution, or credential use.
- Do not overclaim gateway runtime support beyond normalization and explicit
  unsupported execution errors.

## Intake / Snapshot

- Default branch: `main` detected from `origin/HEAD`.
- Working branch: `codex/governance-development-plan`.
- `git pull --ff-only` was attempted and failed because the current branch has
  no upstream tracking branch; work continues from the current checkout.
- Worktree status at intake: clean.
- Current gateway mediation normalizes `ToolAction`, evaluates policy, can
  record optional approval evidence, and exposes redacted secret references.
- Current gateway crate does not yet expose protocol-specific MCP/HTTP request
  stubs.

## Overall Status

done

## Phases

### Phase 1: MCP/HTTP Adapter Stubs

Status: done

Scope:

- Add `McpToolRequest` and `HttpToolRequest` typed request structs.
- Add adapter structs that convert those requests into normalized `ToolAction`
  values.
- Add execution methods that return explicit `TaskFenceError::Unsupported`
  errors.
- Add tests for normalization and unsupported execution for both adapters.

Verification command:

```bash
cargo test -p taskfence-gateway
```

Verification evidence:

- `cargo test -p taskfence-gateway` passed on 2026-06-07 with 16 gateway tests
  and the gateway doc-test target. Coverage includes MCP/HTTP request
  normalization and explicit unsupported execution errors.

### Phase 2: Docs And Examples

Status: done

Scope:

- Update README, architecture, roadmap, development design, runtime
  architecture facts, and example comments/config if needed to document MCP/HTTP
  adapter stubs without claiming execution support.

Verification command:

```bash
cargo test -p taskfence-gateway
```

Verification evidence:

- README, architecture, roadmap, runtime architecture, and development design
  docs were updated to describe MCP/HTTP request normalization stubs without
  claiming real protocol execution.
- `cargo test -p taskfence-gateway` passed on 2026-06-07 with 16 gateway tests
  and the gateway doc-test target after the docs update.

### Phase 3: Quality Gate, Archive, Commit

Status: done

Scope:

- Run formatting, focused tests, clippy, and workspace tests.
- Archive this plan and create one focused commit.

Verification command:

```bash
cargo fmt --all --check
cargo test -p taskfence-gateway
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```

Verification evidence:

- `cargo fmt --all` ran and completed.
- `cargo fmt --all --check` passed.
- `cargo test -p taskfence-gateway` passed with 16 gateway tests and the
  gateway doc-test target.
- `cargo clippy --workspace --all-targets -- -D warnings` passed.
- `cargo test --workspace` passed. The Docker integration test remained
  ignored as designed unless explicitly run with Docker and a local test image.

## Commit Plan

1. `feat: add gateway adapter stubs`

## Open Risks

- Stubs intentionally do not listen on MCP/HTTP, make network calls, or execute
  external tools.
- Request-to-tool normalization is still a contract layer; production protocol
  parsing and transport behavior remain future work.

## Final Evidence

- All phases are terminal with verification evidence.
- `taskfence-gateway` now exposes typed `McpToolRequest` and `HttpToolRequest`
  values plus adapter stubs that normalize requests into `ToolAction`.
- MCP and HTTP adapter execution returns explicit `TaskFenceError::Unsupported`
  errors after normalization; no real protocol server, network client, tool
  execution, or credential use is implemented.
- README, architecture, roadmap, development design, and runtime architecture
  facts describe the current stub behavior without claiming production gateway
  execution.
