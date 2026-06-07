# Gateway Execution Foundation Plan

## Goal

Move TaskFence Phase 3 from typed gateway mediation contracts toward a local,
executable gateway foundation that can demonstrate policy, approval, registry,
secret-reference, audit, and report behavior for tool calls without exposing
raw credentials or claiming production MCP, HTTP, GitHub, Web UI, replay,
team-server, or enterprise behavior.

## Overall Status

done

## Plan Source

The operator requested, in Chinese, to identify the requirements and continue
generating a larger-scale plan.

Actionable interpretation:

- Continue from the archived local runner, approval, task evidence, budget,
  tool policy, gateway approval, gateway secret-broker, adapter-stub, and tool
  registry slices.
- Generate a new durable plan because `docs/codex/plans/` has no active plan.
- Increase scope beyond another narrow contract slice, but keep the work inside
  TaskFence's core feature boundary as a secure runtime and gateway for agent
  tasks.
- Target the next coherent roadmap gap: Phase 3 gateway execution foundation.
- Preserve current secure defaults and do not overclaim unsupported gateway,
  Web UI, replay, team-server, or enterprise behavior.

Recognized requirements:

- Provide an executable local gateway path, not only policy-only mediation.
- Keep tool actions normalized through typed protocol, tool, operation, and
  redacted parameter contracts.
- Enforce configured tool policy with fail-closed behavior for denied,
  unregistered, unsupported, unclassifiable, approval-denied, and
  approval-timeout actions.
- Reuse the existing approval engine path so approval-required tool actions
  write request and resolution evidence before any execution.
- Reuse the existing secret broker contract so gateway-side secret references
  are redacted and raw credential values do not enter tool parameters, audit
  logs, reports, or agent process environment.
- Add deterministic local examples and tests that prove gateway behavior
  without depending on real GitHub credentials, remote network access, or live
  MCP/HTTP servers.
- Keep generated evidence queryable through structured events and reports
  rather than scraped terminal output.
- Update README, architecture, roadmap, development design, runtime facts, task
  examples, and any ADRs required by new durable execution or schema policy.

## Intake / Snapshot

- Status: done
- Date: 2026-06-07
- Default branch: `main`, detected from `origin/HEAD`.
- Working branch: `codex/governance-development-plan`.
- Latest commit at intake: `9356768 feat: add gateway tool registry contract`.
- Worktree at intake: clean.
- Active durable plans at intake: none under `docs/codex/plans/`.
- Sync attempt: `git pull --ff-only` failed because the current branch has no
  upstream tracking branch; work continues from the current checkout.
- Current gateway state: `taskfence-gateway` defines typed MCP/HTTP request
  normalization stubs, `GatewayMediator`, optional approval mediation, optional
  known-tool registry validation, redacted secret references, and explicit
  unsupported execution errors.
- Current runner state: local Docker runner can execute generic commands and
  write structured task evidence, but black-box agents do not yet call an
  executable TaskFence gateway path.
- Current state/query state: local CLI reads structured `.taskfence/tasks`
  evidence for tasks, inputs, artifacts, compare, status, events, diff, report,
  logs, and workspace-local approvals; no SQLite, API server, or Web UI exists.
- Next executable phase: define gateway execution result contracts and the ADR
  for the first local executable gateway boundary.

## Scope

This plan covers a larger Phase 3 implementation wave with these surfaces:

- gateway-owned execution contracts for tool requests, results, failures,
  adapter selection, and redacted outputs
- task-file and/or local configuration needed to define known local gateway
  tools without dynamic discovery
- a local CLI-driven gateway call path that can create structured evidence for
  mediated tool calls
- approval, registry, secret-reference, and report hardening on the executable
  path
- deterministic local GitHub-shaped fixture behavior for `github.read_issue`
  and `github.create_pr` demonstration without real network calls
- documentation and examples that clearly separate fixture execution from
  production GitHub, MCP, HTTP, CLI wrapper, SDK, webhook, Web UI, replay, and
  team-server support
- a follow-up spike for an agent-facing local wrapper or spool design that lets
  sandboxed agents call the local gateway without receiving host credentials

## Non-Goals

- Do not implement production GitHub API calls in this wave.
- Do not read, store, rotate, validate, or use raw credentials.
- Do not expose host secrets, host home paths, Docker socket, SSH agent socket,
  cloud credentials, package-manager tokens, or gateway credentials to the
  agent process.
- Do not implement a production MCP server, HTTP proxy, SDK, webhook receiver,
  browser tool, database connector, or real secret-broker execution.
- Do not implement Web UI, API server, SQLite-backed state, replay execution,
  team-server, RBAC, SSO, SIEM export, or enterprise connectors.
- Do not claim Docker domain allowlist enforcement; local Docker domain
  allowlists remain fail-closed unless an enforcing proxy is implemented.
- Do not split the Rust core runtime into another language.
- Do not introduce hidden helper dispatch, background auto-workers, provider
  switching, or external Codex execution state.

## Assumptions

- The first executable gateway path can be local and deterministic while still
  proving policy, approval, audit, report, and redaction behavior.
- A fixture connector is acceptable for the first GitHub-shaped demo as long as
  docs and reports do not describe it as live GitHub execution.
- Production connector behavior should come after the local executable contract
  is stable, because the credential and egress boundary needs tighter review.
- The CLI can own user-facing gateway demo commands, but long-term gateway
  execution contracts should stay in `taskfence-gateway` and shared domain
  contracts should stay in `taskfence-core`.

## Acceptance Criteria

- Gateway execution exposes typed request, result, failure, and adapter
  contracts instead of returning only unsupported errors.
- Unknown protocols, empty tool segments, unregistered actions, denied actions,
  missing approvals, approval denial, approval timeout, and unsupported
  execution all fail closed with structured evidence.
- Allowed deterministic fixture actions can execute after policy and registry
  checks and write structured audit/report evidence.
- Approval-required deterministic fixture actions execute only after an
  approval engine returns an approved decision.
- Secret references remain redacted in tool parameters, audit events, reports,
  local evidence queries, and fixture outputs.
- Local CLI examples can demonstrate reading a GitHub-shaped issue and
  producing a PR-shaped proposal through TaskFence without using a real token.
- Documentation names the exact implemented behavior and keeps all future
  surfaces explicitly out of scope.
- Focused tests pass for the changed crates, and final workspace validation
  passes with Docker integration tests still ignored unless Docker and the
  required local image are explicitly available.

## Phases

### Phase 1: Gateway Execution Contracts And ADR

Status: done

Scope:

- Add an ADR for the local executable gateway boundary before changing runtime
  semantics.
- Define typed gateway execution contracts, likely including `ToolRequest`,
  `ToolResult`, `ToolExecution`, `ToolExecutionError`, adapter identity, and
  redacted output fields.
- Keep shared action and audit types in `taskfence-core` when they cross crate
  boundaries; keep adapter orchestration in `taskfence-gateway`.
- Extend audit/report contracts only as far as needed to describe execution
  start, success, failure, and redacted result summaries.
- Preserve existing policy-only `GatewayMediator` compatibility for tests and
  future callers that only need decisions.

Verification command:

```bash
cargo test -p taskfence-core -p taskfence-gateway -p taskfence-report
```

Verification evidence:

- Passed on 2026-06-07: `cargo test -p taskfence-core -p taskfence-gateway -p taskfence-report`.
- Added `docs/decisions/2026-06-07-local-gateway-execution-boundary.md`
  to record the local fixture-only executable gateway boundary and non-goals.
- Added core typed execution contracts and audit events for `ToolRequest`,
  `ToolResult`, `ToolExecution`, `ToolExecutionError`, adapter identity,
  execution start, and execution finish.
- Added gateway-side `GatewayExecutor` and adapter contracts while preserving
  policy-only `GatewayMediator` compatibility.
- Updated report and local event summaries to render execution evidence without
  exposing tool parameter values.

### Phase 2: Local Tool Registry And Task Configuration

Status: done

Scope:

- Decide whether local known-tool entries live in task-file configuration,
  gateway fixture configuration, or both.
- Add typed parsing and validation for known local gateway tools if task-file
  schema changes are needed.
- Reject empty protocol, tool, operation, connector, or fixture paths.
- Reject path escapes and symlink escapes for any local fixture input or output
  paths.
- Keep old task files compatible unless a schema conflict requires an explicit
  validation error.
- Update examples to show the minimal local gateway fixture configuration.

Verification command:

```bash
cargo test -p taskfence-config -p taskfence-gateway
```

Verification evidence:

- Passed on 2026-06-07: `cargo test -p taskfence-config -p taskfence-gateway`.
- Passed example compatibility check on 2026-06-07: `cargo run -p taskfence-cli -- validate examples/task.yaml`.
- Chose task-file-owned local known tools under `gateway.tools` for the first
  local fixture boundary.
- Added typed parsing and validation for normalized protocol/tool/operation,
  local fixture connector kind/path, and optional secret references.
- Local fixture paths are canonicalized, must exist, and must stay inside the
  resolved task workspace; parent path escapes are rejected.
- Existing task files remain compatible because `gateway.tools` defaults to an
  empty list.
- Updated `examples/task.yaml` and added
  `examples/repo/fixtures/github.json` for the local GitHub-shaped fixture.

### Phase 3: Executable Local Gateway Call Path

Status: done

Scope:

- Add a CLI-owned local gateway command, for example a `gateway call` shape,
  that loads a task file, resolves the task, mediates one tool action, executes
  an allowed local adapter, and writes structured evidence.
- Keep command naming and argument shape stable enough to document, but do not
  present it as the final production MCP or HTTP server interface.
- Reuse existing local artifact, audit, state, approval, policy, and report
  ports instead of writing gateway evidence through ad hoc files.
- Ensure CLI output is human-readable while structured evidence remains the
  source of truth.
- Add tests for allowed execution, denied execution, unsupported protocol,
  unregistered tool, invalid parameters, and evidence creation.

Verification command:

```bash
cargo test -p taskfence-cli -p taskfence-gateway -p taskfence-state
```

Verification evidence:

- Passed on 2026-06-07: `cargo test -p taskfence-cli -p taskfence-gateway -p taskfence-state`.
- Added CLI-owned `taskfence gateway call <task-file> <tool> <operation>`
  with `--protocol` and repeated `--param KEY=VALUE` arguments for local
  fixture gateway execution.
- The command loads a resolved task, builds a task-file backed registry,
  mediates policy, selects a configured local fixture adapter, writes
  `.taskfence/tasks/<task-id>/` evidence, generates a Markdown report, and
  keeps CLI output human-readable while structured evidence remains the source
  of truth.
- Added CLI coverage for command parsing, allowed execution evidence,
  policy-denied execution without adapter artifacts, unsupported protocol,
  unregistered tool, adapter-level invalid parameters, report generation, and
  local event summaries containing `tool-started` / `tool-finished` without
  rendering raw parameter values.

### Phase 4: Approval, Secret, And Redaction Hardening

Status: done

Scope:

- Wire approval-required executable tool actions through the existing approval
  engine before adapter execution.
- Keep non-interactive behavior fail-closed unless the caller explicitly opts
  into an approved or externally resolved path.
- Require denied and timed-out approvals to write structured request and
  resolution evidence without executing the adapter.
- Attach gateway-side secret references only after policy and registry checks
  and before adapter execution when the adapter contract requires them.
- Add regression tests proving raw secret-looking values do not appear in
  `events.jsonl`, reports, CLI event summaries, or fixture outputs.

Verification command:

```bash
cargo test -p taskfence-gateway -p taskfence-approval -p taskfence-audit -p taskfence-report -p taskfence-state
```

Verification evidence:

- Passed on 2026-06-07: `cargo test -p taskfence-gateway -p taskfence-approval -p taskfence-audit -p taskfence-report -p taskfence-state`.
- Wired executable gateway calls through an explicit approval mode:
  default fail-closed, local pre-approved demo/test mode, and the existing
  external file-backed approval workflow.
- Approval-required executable fixture calls now write `ApprovalRequested` and
  `ApprovalResolved` evidence before execution; denied or timed-out approvals
  finish with `ApprovalDeniedOrTimedOut` and do not attach secret references or
  execute the adapter.
- Configured gateway `secret_refs` are attached only after policy, registry,
  and approval checks, and they attach as `RedactedValue::Redacted` parameters
  through a local redacted reference broker without reading raw credentials.
- Hardened audit JSON sanitization so secret-like tool parameter keys remain
  deserializable typed `RedactedValue` evidence after redaction.
- Added regression coverage proving raw secret-looking values are absent from
  `events.jsonl`, reports, CLI event summaries, and deterministic fixture PR
  proposal artifacts.

### Phase 5: Deterministic GitHub-Shaped Fixture Demo

Status: done

Scope:

- Add a deterministic local fixture connector for GitHub-shaped operations that
  can model `github.read_issue` and `github.create_pr`.
- Make `github.read_issue` return redacted structured issue data from local
  fixture input.
- Make `github.create_pr` produce a PR-shaped proposal artifact after approval
  instead of creating a real pull request.
- Ensure the fixture connector can request a redacted `github_token` reference
  when configured, without reading or using a raw token.
- Add example task files and README/demo docs for the local gateway fixture.
- Keep all wording explicit that this is a fixture connector and not live
  GitHub API integration.

Verification command:

```bash
cargo test -p taskfence-gateway -p taskfence-cli -p taskfence-report
cargo run -p taskfence-cli -- validate examples/task.yaml
```

Verification evidence:

- Passed on 2026-06-07: `cargo test -p taskfence-gateway -p taskfence-cli -p taskfence-report`.
- Passed on 2026-06-07: `cargo run -p taskfence-cli -- validate examples/task.yaml`.
- Added deterministic local GitHub-shaped fixture behavior for
  `github.read_issue` and `github.create_pr`; `read_issue` returns structured
  local issue data and `create_pr` writes a PR-shaped proposal artifact after
  explicit approval rather than creating a real pull request.
- `examples/task.yaml` and `examples/repo/fixtures/github.json` configure and
  exercise the local fixture without requiring network access or a real token.
- README demo docs now show `taskfence gateway call` fixture commands and
  explicitly state that this is not live GitHub, MCP, HTTP, or raw credential
  execution.

### Phase 6: Agent-Facing Wrapper Or Spool Spike

Status: done

Scope:

- Design and prototype, only if the previous phases are stable, how a sandboxed
  black-box agent could call the local gateway without receiving host
  credentials.
- Compare a generated CLI wrapper, a mounted request/response spool, and a
  local sidecar model against Docker networking, host isolation, approval wait,
  and audit requirements.
- Keep this phase bounded to a documented prototype or ADR if implementation
  would require a daemon, network listener, or runner rewrite.
- Do not merge a long-lived wrapper interface until path validation, approval
  wait semantics, and failure behavior are testable.

Verification command:

```bash
cargo test -p taskfence-runner -p taskfence-gateway -p taskfence-core
```

Verification evidence:

- Passed on 2026-06-07: `cargo test -p taskfence-runner -p taskfence-gateway -p taskfence-core`.
- Added `docs/decisions/2026-06-07-agent-facing-gateway-spool-boundary.md`
  as a bounded ADR for the agent-facing integration spike.
- Compared generated CLI wrapper, mounted request/response spool, and local
  sidecar/listener models against Docker networking, host isolation, approval
  wait, credential, audit, timeout, cancellation, and path-validation
  requirements.
- Chose not to promote a long-lived agent-facing gateway interface in this
  wave; a future spool prototype is preferred before any sidecar/listener
  because it can keep credentials gateway-side and evidence file-backed without
  changing Docker networking or runner process boundaries.

### Phase 7: Documentation, Governance Check, Archive, And Commits

Status: done

Scope:

- Update README, `docs/architecture.md`, `docs/roadmap.md`,
  `docs/development-design.md`, `docs/codex/runtime-architecture.md`, examples,
  and any relevant decision records.
- If durable governance or repeatable workflow constraints are discovered,
  update source-owned governance under `governance/private/*`, rebuild
  generated outputs, and run governance checks.
- Run formatting, focused tests, clippy, and workspace tests.
- Commit coherent verified scopes; do not stage unrelated files.
- When every phase is terminal and the requested work is complete, update final
  evidence and move this plan to `docs/codex/plan_archived/` with the same
  filename.

Verification command:

```bash
cargo fmt --all --check
git diff --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
python3 scripts/governance/check_codex_governance.py
```

Verification evidence:

- Passed on 2026-06-07: `cargo fmt --all --check`.
- Passed on 2026-06-07: `git diff --check`.
- Passed on 2026-06-07: `cargo clippy --workspace --all-targets -- -D warnings`.
- Passed on 2026-06-07: `cargo test --workspace`.
- Passed on 2026-06-07: `python3 scripts/governance/check_codex_governance.py`.
- Docker integration test remained ignored by design with the existing reason:
  `requires Docker daemon and a locally available test image`.
- Updated README, architecture, roadmap, development design, runtime
  architecture facts, examples, and ADRs to describe the implemented local
  fixture gateway foundation without claiming production MCP/HTTP/GitHub,
  wrapper/spool/sidecar, Web UI, replay, team-server, or enterprise behavior.
- No durable governance source changes or repeatable workflow changes were
  discovered, so generated governance was not rebuilt.

## Commit Plan

1. `feat: define executable gateway contracts`
2. `feat: add local gateway call evidence path`
3. `feat: harden gateway approval and secret execution`
4. `feat: add deterministic github gateway fixture`
5. `docs: document gateway execution foundation`

## Validation Strategy

- Prefer focused crate tests after each phase.
- Run `cargo fmt --all --check` and `git diff --check` before committing each
  coherent scope.
- Run `cargo clippy --workspace --all-targets -- -D warnings` and
  `cargo test --workspace` before archiving the plan.
- Keep Docker integration tests ignored unless Docker is available and the
  required image is already local; if unavailable, record that explicitly.
- Run governance validation only if governance source, generated runtime rules,
  or repeatable workflow skills change.

## Risk Controls

- Explicit deny continues to win over approval and allow.
- Approval-required continues to win over allow.
- Default deny applies when no rule matches.
- Unknown, malformed, unregistered, unsupported, or unclassifiable tool actions
  fail closed.
- Tool parameter and result rendering must use `RedactedValue` or equivalent
  typed redaction, never raw string dumps.
- Fixture connector outputs must be deterministic and local, not scraped from
  terminal output or remote services.
- Any future live GitHub, MCP, HTTP, SDK, webhook, or secret-broker execution
  requires a separate plan and likely an ADR before implementation.

## Open Questions

- Whether the first public CLI should be named `gateway call`, `tool call`, or
  another shape that better fits the eventual gateway surface.
- Whether task-file schema should own local gateway fixtures directly or point
  to a separate gateway fixture file.
- Whether the agent-facing integration should prioritize a CLI wrapper or a
  request/response spool after the deterministic local gateway path is proven.

## Current Evidence

- Requirements were identified from README, requirements, architecture, roadmap,
  development design, runtime architecture facts, current crate surfaces, and
  archived Phase 2/Phase 3 plans.
- Active plan directory was empty before this file was created.
- The completed implementation now adds the local executable gateway foundation
  with typed contracts, task-file fixture configuration, `taskfence gateway
  call`, approval/secret/redaction hardening, deterministic GitHub-shaped
  fixtures, a bounded wrapper/spool ADR, updated docs, and passing final
  validation.
- Completed plan archived to `docs/codex/plan_archived/2026-06-07-gateway-execution-foundation.md`.
