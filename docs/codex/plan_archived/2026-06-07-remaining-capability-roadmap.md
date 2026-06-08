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

Status: done

All implementation phases are terminal. The completed roadmap implementation is
archived under `docs/codex/plan_archived/` with this same filename.

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

Status: done

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

- Implemented bounded MCP/HTTP adapter execution through the existing gateway executor for explicitly configured tool actions, replacing normalization-only unsupported execution while keeping listener/proxy behavior unimplemented.
- Added `github_rest` task-file connector parsing, typed core connector config, a GitHub REST adapter/client for `github.read_issue`, `github.create_pr`, and `github.comment_issue`, and CLI adapter selection for live connectors.
- Kept raw GitHub credentials gateway-side through `EnvironmentSecretBroker`, which reads `TASKFENCE_GATEWAY_SECRET_<NORMALIZED_SECRET_NAME>` only after registry, policy, and approval checks; audit events, reports, task files, fixture artifacts, and sandbox parameters carry only redacted secret references.
- Preserved deterministic local fixtures for offline demos/tests and documented unsupported SDK/webhook connectors, arbitrary HTTP proxying, MCP/HTTP listener servers, branch/commit creation, Web UI, replay, team-server, and enterprise connector behavior as future work.
- Updated README, architecture, roadmap, runtime architecture, development design, `docs/decisions/2026-06-07-bounded-github-rest-connector.md`, `examples/github-rest-task.yaml`, and the report golden fixture to reflect the bounded implemented behavior.
- Passed: `cargo fmt --all`; `cargo test -p taskfence-config -p taskfence-gateway`; `cargo test -p taskfence-cli gateway_call_github_rest_missing_env_secret_fails_closed_with_evidence`; `cargo test -p taskfence-core -p taskfence-gateway -p taskfence-policy -p taskfence-audit -p taskfence-report`; `cargo clippy --workspace --all-targets -- -D warnings`.

### Phase 3: Live Budget And Cost Metering

Status: done

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

- Added shared `BudgetUsage` and `BudgetUsageRecord` contracts plus a
  `BudgetUsageRecorded` audit event for planned and observed mediated gateway
  usage. Usage records normalize kind/provider/model/operation metadata, require
  positive amounts, preserve redacted metadata values, and carry the matched
  `permissions.budget.allow` limit and policy decision.
- Extended the gateway executor and adapter contract so planned usage is
  checked before secret attachment and adapter execution, over-limit planned
  usage fails closed with `BudgetExceeded`, observed over-limit usage records a
  partial result plus a budget error, and `github_rest` records one planned
  `gateway_calls` usage per configured operation.
- Updated local event/state/report consumers so budget usage appears in
  structured event summaries, task state reads, Markdown reports, and spool
  denial status without rendering raw credentials.
- Updated README, roadmap, architecture, runtime architecture, development
  design, `docs/decisions/2026-06-07-bounded-gateway-budget-metering.md`,
  `examples/github-rest-task.yaml`, and the report golden fixture to document
  bounded gateway metering while keeping billing, team quota, chargeback, and
  broad model-provider metering unclaimed.
- Passed: `cargo fmt --all --check`; `cargo test -p taskfence-core -p
  taskfence-policy -p taskfence-audit -p taskfence-report`; `cargo test -p
  taskfence-gateway`; `git diff --check`.

### Phase 4: Local Review UI And Replay Foundation

Status: done

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

- Implemented file-backed local review contracts in `taskfence-state`:
  `LocalReviewIndex`, `LocalTaskReview`, and `ReplayPlan` assemble task lists,
  task details, structured events, logs, diffs, reports, optional-evidence
  warnings, replay inputs, blockers, last status, and deterministic limitations
  without scraping Markdown reports.
- Added CLI surfaces: `taskfence review --workspace <workspace>
  [--output <file>]`, `taskfence review --workspace <workspace> --serve
  [--port <port>]`, and `taskfence replay plan <task-id> --workspace
  <workspace>`. The static review page renders task lists, pending approvals,
  run comparison, replay readiness, timeline, diffs, logs, and reports from
  structured local evidence. The foreground loopback server resolves pending
  workspace-local approval records through explicit approve/deny POST routes.
- Kept the selected state store file-backed for this phase and documented that
  SQLite, a persistent API server, live log streaming, artifact-download
  routing, richer browser diff interaction, and deterministic replay execution
  remain future work.
- Updated README, roadmap, architecture, runtime architecture, development
  design, and `docs/decisions/2026-06-07-local-review-replay-foundation.md`
  to document the bounded local review/replay-plan behavior without
  overclaiming team-server, persistent Web/API, SQLite, or replay execution.
- Rendered UI verification used Playwright CLI fallback because the Browser
  plugin was listed but the required Node REPL Browser control tool was not
  exposed in this tool surface. Verified `http://127.0.0.1:18186/` from a
  foreground `taskfence review --serve` process: page title `TaskFence Review`,
  task content, run comparison, replay panel, report content, zero console
  errors/warnings, and no horizontal overflow at desktop and 390px mobile
  widths. Screenshots were kept under `/tmp/taskfence-phase4-playwright-final`.
- Passed: `cargo test -p taskfence-state`; `cargo test -p taskfence-cli`;
  `cargo fmt --all --check`; `git diff --check`; `cargo test --workspace`.
  The workspace suite reported the Docker integration test ignored because it
  requires a Docker daemon and a locally available test image.

### Phase 5: Runner Expansion

Status: done

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

- Added typed sandbox/runner families for `remote_ssh`, `kubernetes_job`,
  `microvm`, and `managed_cloud` while preserving `docker` as the only
  executable runner.
- Implemented runner capability reports covering availability, filesystem
  isolation, secret isolation, network disable/default-deny, domain allowlist
  enforcement, limit enforcement, and output capture. Docker reports its
  known domain-allowlist gap; future runner families report exact missing
  controls and fail closed.
- Added `ExpandedRunner` dispatch: Docker tasks delegate to `DockerRunner`;
  remote SSH, Kubernetes job, microVM, managed cloud, and unknown sandbox
  types fail closed before execution instead of falling back to Docker or a
  host-local process.
- Updated `taskfence validate` / `taskfence run` to use the expanded runner
  dispatcher, added config parsing tests for the future runner families, and
  added CLI validation coverage for unavailable remote runner contracts.
- Updated README, roadmap, architecture, runtime architecture, development
  design, and `docs/decisions/2026-06-07-expanded-runner-capability-contracts.md`
  to document the bounded runner expansion and keep live SSH/Kubernetes/
  microVM/managed-cloud execution unclaimed.
- Passed: `cargo test -p taskfence-core -p taskfence-runner -p
  taskfence-testkit`; `cargo test -p taskfence-config`; `cargo test -p
  taskfence-cli validate_rejects_unavailable_remote_runner_contracts`;
  `cargo fmt --all --check`; `git diff --check`. Runner-specific live
  integration tests were not run because no live remote SSH, Kubernetes,
  microVM, or managed cloud runner backend is implemented yet; the existing
  Docker integration test remains explicitly ignored unless a Docker daemon and
  local test image are available.

### Phase 6: Team Server Foundation

Status: done

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

- Added contract-only team-server foundations in `taskfence-state`: typed team
  API resources, organization/RBAC access decisions with fail-closed
  method/resource matching, approval-owner enforcement, deterministic
  in-memory worker leases for local development tests, Postgres state config
  validation with explicit unsupported live-backend behavior, team artifact
  root containment checks, unsupported audit-export sink behavior, and
  local-to-team migration planning from structured `.taskfence` files without
  treating rendered Markdown reports as source-of-truth state.
- Preserved local development behavior without adding a persistent API server,
  live Postgres backend, worker daemon, audit export sink, service port,
  systemd/launchd unit, or deployment command.
- Updated README, roadmap, architecture, runtime architecture, development
  design, cross-platform ops docs, and
  `docs/decisions/2026-06-08-team-server-foundation-contract.md` to document
  the bounded team foundation and keep live team-server, SSO, SIEM, object
  storage, and enterprise connector behavior unclaimed.
- Aligned the expanded runner capability error for Docker domain allowlists so
  `taskfence validate` continues to fail closed with the operator-facing
  enforcing-proxy explanation.
- Passed: `cargo test -p taskfence-state`; `cargo test -p taskfence-runner
  expanded_runner_reports_docker_capabilities_and_domain_gap`; `cargo test -p
  taskfence-cli validate_rejects_unsupported_domain_allowlist`; `cargo fmt
  --all --check`; `git diff --check`; `cargo test --workspace`. The workspace
  suite reported the Docker integration test ignored because it requires a
  Docker daemon and a locally available test image. No deployment or ops-script
  validation was needed because no service entrypoints changed.

### Phase 7: Enterprise Connectors And Audit Export

Status: done

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

- Added explicit task-file connector contracts for `github_enterprise_rest`,
  `gitlab`, `jira`, `feishu`, `wecom`, `dingtalk`, `gitee`, `coding`,
  `database`, `internal_http`, and `siem_export`. Config validation rejects
  unsafe API bases, unsafe project/repository references, inline DSNs, and
  secret-bearing sink/reference values.
- Reused the bounded GitHub REST adapter for `github_enterprise_rest` with
  gateway-side environment token handling and the same limited
  `github.read_issue`, `github.create_pr`, and `github.comment_issue`
  operations. GitHub Enterprise tests prove raw tokens are passed only to the
  mocked client and do not appear in audit events.
- Added connector-specific policy template contracts for GitHub Enterprise,
  GitLab, Jira, Feishu, WeCom, DingTalk, Gitee, CODING, database, internal
  HTTP, and SIEM export, including supported operation sets, approval-required
  operation sets, and secret scopes.
- Added contract-only unsupported adapter handling for non-GitHub enterprise
  connectors so configured actions still pass registry, policy, approval, and
  redacted secret-reference handling, then fail closed with structured
  unsupported execution evidence. Template-unsupported operations fail closed
  before secret attachment.
- Added validated audit-export sink contracts in `taskfence-state` for SIEM,
  webhook, and object storage destinations, while preserving explicit
  unsupported live export-sink behavior.
- Added `examples/enterprise-connectors-task.yaml` and
  `docs/decisions/2026-06-08-enterprise-connector-contracts.md`, and updated
  README, roadmap, architecture, runtime architecture, and development design
  to document bounded behavior without overclaiming live Slack, live
  service-specific clients, team-server execution, live SIEM export, managed
  runners, or compliance report exports.
- Passed: `cargo test -p taskfence-config -p taskfence-gateway -p
  taskfence-state`; `cargo run -p taskfence-cli -- validate
  examples/enterprise-connectors-task.yaml`; `cargo fmt --all --check`;
  `git diff --check`; `cargo clippy --workspace --all-targets -- -D warnings`;
  `cargo test --workspace`. The workspace suite reported the Docker
  integration test ignored because it requires a Docker daemon and a locally
  available test image. Live enterprise integration tests were not run because
  no operator-supplied credentials or live environments were provided, and
  non-GitHub enterprise connectors are contract-only in this phase.

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

Status: done

All seven phases are complete and recorded above with phase-specific evidence.
Final validation passed with `cargo fmt --all --check`, `cargo clippy
--workspace --all-targets -- -D warnings`, `cargo test --workspace`, and
`cargo run -p taskfence-cli -- validate examples/enterprise-connectors-task.yaml`.
The workspace suite kept the Docker integration test ignored because it requires
a Docker daemon and a locally available test image. Completed change scopes were
committed through Phase 7, including the final `feat: add enterprise connector
and audit export foundations` commit.
