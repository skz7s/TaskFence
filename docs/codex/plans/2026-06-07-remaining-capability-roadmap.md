# Remaining Capability Roadmap Plan

## Goal

Plan the remaining TaskFence implementation work after the current local runner
and local fixture gateway foundation, keeping future scope ordered by runtime
dependency and avoiding claims for unsupported gateway, Web UI, replay, team
server, or enterprise behavior before those surfaces are implemented and
verified.

## Plan Source

User request on 2026-06-07: after asking what the project can currently do and
what remains, the operator asked: "待完成生成plan".

Current capability baseline from README, roadmap, runtime architecture, CLI
help, and crate inspection:

- Rust workspace and crate boundaries exist for CLI, core, config, policy,
  approval, audit, artifacts, runner, agent, gateway, report, state, and
  testkit.
- The local CLI supports `init`, `validate`, `run`, `gateway call`, local task
  evidence queries, local log/diff/report reads, local approval listing/detail,
  and local approve/deny commands.
- The local Docker runner supports generic command execution, controlled
  mounts, environment allowlists, Docker network disabled/default deny/default
  allow modes, stdout/stderr capture, diff/report artifacts, timeout handling,
  and fail-closed domain allowlists.
- Built-in policy covers command, path, network, env, secret, tool, and typed
  budget decisions, with deny precedence and default-deny behavior.
- Local approval supports fail-closed non-interactive mode, opt-in terminal
  approval, and opt-in workspace-local external approval.
- Audit, artifacts, reports, and local state queries are file-backed under the
  task workspace.
- The executable gateway surface is limited to deterministic local fixture
  calls through `taskfence gateway call`, including GitHub-shaped `read_issue`
  and `create_pr` fixture behavior without live credentials or network calls.
- MCP/HTTP gateway adapters currently normalize request shapes and return
  explicit unsupported execution errors.

Scope for this plan:

- Complete the missing production path in a dependency-aware order.
- Preserve secure defaults: fail closed on unknown, unregistered, unsupported,
  denied, malformed, approval-denied, approval-timeout, path escape, and
  unavailable-secret cases.
- Keep raw gateway credentials gateway-side and out of sandbox processes,
  audit logs, reports, fixture artifacts, and task parameters.
- Keep implementation inside existing long-term crate ownership boundaries.
- Update README, roadmap, runtime architecture docs, examples, and decisions as
  behavior becomes real.

Non-goals for this plan:

- Do not replace TaskFence with a general-purpose agent framework.
- Do not implement model-provider switching or an LLM-only gateway.
- Do not claim perfect semantic inspection of arbitrary encrypted traffic.
- Do not treat current local fixture behavior as production GitHub, MCP, HTTP,
  or secret-broker execution.
- Do not introduce Web UI, API server, SQLite, replay, sidecar/listener, or
  team-server behavior in earlier phases without a phase-specific contract and
  verification.

Acceptance criteria:

- Each implementation phase has a bounded runtime contract, success behavior,
  failure behavior, focused tests, documentation updates, and no overclaiming.
- `cargo fmt --all`, `cargo clippy --workspace --all-targets -- -D warnings`,
  and `cargo test --workspace` pass before claiming a full implementation wave
  complete, with Docker integration tests run when Docker behavior changes or
  an explicit unavailable-Docker note recorded.
- The final system lets a sandboxed or integrated agent call mediated tools
  without receiving raw credentials, lets reviewers inspect and approve
  sensitive actions, and records structured evidence suitable for replay and
  team-server evolution.

Tradeoffs:

- Prefer a request/response spool prototype before a sidecar/listener because
  it keeps Docker networking disabled, makes every request file inspectable,
  and aligns with current artifact/state ownership.
- Keep the fixture gateway as an operator/test surface while adding production
  adapters separately, so deterministic tests remain available.
- Add SQLite/API/Web only after file-backed evidence contracts are stable enough
  to migrate without reinterpreting report text.

## Intake / Snapshot

Status: done

- Date: 2026-06-07
- Detected default branch: `origin/main`
- Current branch: `codex/governance-development-plan`
- Current branch is not the detected default branch, so no new branch was
  created for this planning document.
- Worktree before plan authoring: clean aside from branch status output.
- `git pull --ff-only` result: blocked because the current branch has no
  upstream tracking information. No remote update was applied.
- Active plan directory before authoring contained only `.gitkeep`.
- Next executable phase: Phase 1, agent-facing gateway spool prototype.

Verification:

- `git status --short --branch`
- `git symbolic-ref refs/remotes/origin/HEAD`
- `git pull --ff-only`
- README, roadmap, architecture, runtime architecture, and gateway decision
  records were inspected for implemented and explicitly deferred behavior.

## Overall Status

Status: planned

This file is the active durable plan for the remaining implementation roadmap.
Future execution should update phase statuses in this file as work starts,
finishes, or blocks. When all phases are terminal, archive this file under
`docs/codex/plan_archived/` with the same filename.

## Phases

### Phase 1: Agent-Facing Gateway Spool Prototype

Status: done

Scope:

- Define the sandbox-to-host request/response spool contract before adding a
  sidecar or listener.
- Add typed request, response, timeout, cancellation, malformed-request, and
  unsupported-action states.
- Mount only the minimal spool path into the sandbox and validate every spool
  path against the task artifact root.
- Reject `..`, symlink escapes, unknown request files, unknown tool keys,
  unregistered tools, unsupported protocols, denied actions, missing approval,
  denied approval, approval timeout, unavailable secret references, and adapter
  failures with structured evidence.
- Provide a thin generated wrapper command only as an agent-visible client over
  the spool, not as a direct credential or host-execution channel.
- Keep Docker network disabled/default-deny compatible for gateway calls.
- Update `docs/decisions/` if the spool contract changes the existing
  agent-facing gateway boundary.

Verification command:

```bash
cargo test -p taskfence-core -p taskfence-gateway -p taskfence-runner -p taskfence-cli
```

Completion evidence:

- Implemented typed gateway spool request/response contracts, safe spool path validation, generated task-local spool wrapper creation, dedicated Docker spool mounting with broad-mount rejection, and one-request `taskfence gateway spool process` handling for success, denied, timeout, cancellation, malformed request, unsupported action, secret, approval, policy, and adapter failure evidence.
- Updated README, architecture, roadmap, runtime architecture, development design, and `docs/decisions/2026-06-07-agent-facing-gateway-spool-boundary.md` to document only the bounded local spool prototype and keep production MCP/HTTP/GitHub, sidecar/listener, Web UI, replay, team-server, and enterprise behavior unclaimed.
- Passed: `cargo fmt --all`; `cargo clippy --workspace --all-targets -- -D warnings`; `cargo test -p taskfence-core -p taskfence-gateway -p taskfence-runner -p taskfence-cli`. The phase test command covered 83 CLI tests, 6 core tests, 32 gateway tests, 15 runner tests, and doc tests, with the Docker integration test remaining explicitly ignored because it requires a Docker daemon and local test image.

### Phase 2: Production Gateway Transports And First Real Connector

Status: pending

Scope:

- Promote MCP and HTTP from normalization stubs to bounded production transport
  prototypes after Phase 1 defines the agent-facing boundary.
- Add a real GitHub connector for issue read, branch/commit/PR proposal or PR
  creation, and comment operations, with operation-level policy and approval.
- Keep raw GitHub credentials gateway-side through the secret broker; do not
  expose tokens to the sandbox, task files, logs, reports, or artifacts.
- Preserve deterministic local fixture adapters for tests and offline demos.
- Add explicit unsupported errors for SDK/webhook/other connector families that
  are not implemented in this phase.
- Document exact supported operations and non-supported operations in README,
  roadmap, architecture, and examples.

Verification command:

```bash
cargo test -p taskfence-core -p taskfence-gateway -p taskfence-policy -p taskfence-audit -p taskfence-report
```

Completion evidence:

- Pending.

### Phase 3: Live Budget And Cost Metering

Status: pending

Scope:

- Extend the current typed budget policy into observed runtime accounting for
  mediated model/tool/provider actions.
- Define cost and token event contracts without coupling policy to one model
  provider.
- Record usage, limits, over-limit denials, and provider/accounting metadata as
  structured audit evidence.
- Keep current `permissions.budget.allow` default-deny behavior for mediated
  budget actions.
- Do not add billing, team quota, or enterprise chargeback until the team state
  model exists.

Verification command:

```bash
cargo test -p taskfence-core -p taskfence-policy -p taskfence-audit -p taskfence-report
```

Completion evidence:

- Pending.

### Phase 4: Local Review UI And Replay Foundation

Status: pending

Scope:

- Add local SQLite or another explicitly selected queryable state store only
  after file-backed evidence schemas are stable.
- Add local API boundaries for task list, task detail, events, logs, diffs,
  reports, approvals, artifacts, and replay inputs.
- Build a Web UI for reviewing local tasks, reading reports, inspecting diffs
  and logs, and approving or denying pending actions.
- Add replay input capture and replay execution contracts, including what can
  and cannot be deterministic.
- Add structured comparison views for multiple runs without scraping Markdown
  reports.
- Keep current CLI evidence queries working during any state migration.

Verification command:

```bash
cargo test --workspace
```

Additional verification:

- Rendered UI verification when Web UI exists.
- Replay tests for success, denied action, approval path, timeout, and missing
  artifact cases.

Completion evidence:

- Pending.

### Phase 5: Runner Expansion

Status: pending

Scope:

- Add runner contracts for remote SSH, Kubernetes job, microVM, or managed cloud
  runners only after local Docker and gateway contracts are stable.
- Preserve identical policy, approval, audit, artifact, report, and state
  semantics across runner implementations.
- Add capability detection for each runner and fail closed when a configured
  runner cannot provide required isolation or network controls.
- Keep Docker integration tests separate and explicit.

Verification command:

```bash
cargo test -p taskfence-core -p taskfence-runner -p taskfence-testkit
```

Additional verification:

- Runner-specific integration tests on hosts where the runner is available, or
  explicit unavailable-runner notes.

Completion evidence:

- Pending.

### Phase 6: Team Server Foundation

Status: pending

Scope:

- Add API server and worker model for team execution.
- Introduce Postgres-backed team state while preserving local development
  behavior.
- Add RBAC, organization policies, approval ownership, audit export, and
  artifact storage boundaries.
- Define migration paths from local `.taskfence` state to server-owned state
  without treating rendered reports as source of truth.
- Keep deployment docs clear about supported operating systems and service
  entrypoints.

Verification command:

```bash
cargo test --workspace
```

Additional verification:

- API and state migration tests.
- Deployment or ops-script validation for any service entrypoint changes.

Completion evidence:

- Pending.

### Phase 7: Enterprise Connectors And Audit Export

Status: pending

Scope:

- Add GitHub Enterprise, GitLab, Jira, Feishu, WeCom, DingTalk, Gitee, CODING,
  database, internal HTTP API, and SIEM/export integrations as separately
  bounded connector slices.
- Require connector-specific policy templates, approval rules, redaction tests,
  credential handling tests, and unsupported-action behavior.
- Keep each connector opt-in and explicitly documented.
- Do not generalize one connector's semantics into a broad policy claim without
  tests.

Verification command:

```bash
cargo test --workspace
```

Additional verification:

- Connector-specific mocked integration tests.
- Live integration tests only when credentials and environment are explicitly
  supplied by the operator.

Completion evidence:

- Pending.

## Cross-Phase Requirements

- Maintain README, `docs/roadmap.md`, `docs/architecture.md`,
  `docs/codex/runtime-architecture.md`, examples, and decision records when
  public behavior or durable policy changes.
- Add or update project-private governance only when a repeatable workflow or
  runtime rule is confirmed, then regenerate and check governance outputs.
- Keep `.codex-helper/local-env.toml` as machine-local state and do not promote
  host-specific paths into stable docs.
- Preserve generated-but-committed governance ownership: edit source under
  `governance/private/*` or reusable template source first, then build generated
  outputs.
- Do not run full Docker integration tests implicitly on machines without
  Docker or without required local images; record the skip reason.

## Commit Plan

1. `feat: add agent-facing gateway spool prototype`
2. `feat: add production gateway transports`
3. `feat: add live budget metering`
4. `feat: add local review UI and replay state`
5. `feat: add expanded runner contracts`
6. `feat: add team server foundation`
7. `feat: add enterprise connector and audit export foundations`

## Final Evidence

Status: pending

This roadmap plan has been authored, but the implementation phases remain
pending. Record phase-specific validation output here as future work completes.
