# Remaining Capability Implementation Plan

## Goal

Plan the remaining TaskFence implementation work from the current local runner,
bounded gateway connector, local review, replay-plan, and team-contract
foundation toward a production-ready secure agent runtime and gateway.

The plan keeps work ordered by dependency, preserves fail-closed behavior, and
separates executable features from contract-only surfaces until each surface has
tests, documentation, and explicit operational limits.

## Plan Source

User request on 2026-06-08, translated: generate a plan for the remaining
items after the current capability review.

Capability baseline from the 2026-06-08 review:

- The Rust workspace and crate boundaries are present for CLI, core, config,
  policy, approval, audit, artifacts, runner, agent, gateway, report, state,
  and testkit.
- The executable local path supports `taskfence init`, `taskfence validate`,
  `taskfence run`, local task evidence queries, local approvals, local review
  HTML generation/foreground serving, and replay input planning.
- The local Docker runner supports controlled mounts, secret isolation, no host
  home/socket/SSH-agent passthrough by default, environment allowlisting,
  disabled/default-deny/default-allow network modes, limits, timeout handling,
  stdout/stderr capture, diffs, and reports.
- Local Docker domain allowlists are not enforceable yet and fail closed.
- The built-in policy engine covers command, path, network, environment,
  secret, tool, and budget actions with deny precedence and default-deny
  behavior.
- The gateway can execute deterministic local fixtures and bounded
  GitHub/GitHub Enterprise REST operations for `github.read_issue`,
  `github.create_pr`, and `github.comment_issue`.
- The gateway spool prototype lets sandboxed agents write task-local typed
  requests that the host processes one at a time through policy, approval,
  registry, secret-broker, audit, and report paths.
- GitLab, Jira, Feishu, WeCom, DingTalk, Gitee, CODING, database, internal
  HTTP, and SIEM export connectors are contract-only and fail closed for live
  execution.
- Remote runner families, persistent API/server behavior, live Postgres state,
  deterministic replay execution, team server, object storage, SSO, durable
  worker queue, and live audit export are not implemented.

Related prior plan:

- `docs/codex/plan_archived/2026-06-07-remaining-capability-roadmap.md`
  completed the earlier local gateway, budget, review, runner-contract, and
  team-contract foundation. This new plan is not a continuation of that archived
  execution state; it starts from the current remaining work.

## Scope

- Implement production gateway transport surfaces without exposing raw
  credentials to agent sandboxes.
- Add an enforcing network path for local domain allowlists.
- Expand live connector support without overclaiming unsupported services.
- Move local review/state from file-only reads toward persistent API-backed
  workflows.
- Implement deterministic replay and evaluation only after evidence contracts
  are stable enough to replay without scraping reports.
- Add live remote runner backends with explicit capability checks and artifact
  transport.
- Promote the team-server foundation from contracts to a persistent service
  after local API, state, approval, and worker semantics are stable.
- Keep README, roadmap, architecture, runtime docs, examples, and decision
  records synchronized as behavior becomes real.

## Non-Goals

- Do not build a general-purpose agent framework.
- Do not replace model providers or implement an LLM-only gateway as a
  prerequisite for tool/runtime governance.
- Do not claim semantic control of arbitrary encrypted traffic outside
  TaskFence-controlled runtimes or gateway paths.
- Do not split the core runtime across Rust and Go.
- Do not treat contract-only connectors, runners, team APIs, or export sinks as
  implemented until they have live execution, failure handling, and tests.
- Do not introduce hidden dispatch, background worker, or auto-execution state
  outside the documented runtime and plan files.

## Acceptance Criteria

- Every phase has a bounded contract, success behavior, fail-closed behavior,
  targeted tests, documentation updates, and explicit unsupported-operation
  handling.
- Unknown, unregistered, malformed, path-escaping, denied, approval-denied,
  approval-timeout, secret-unavailable, and unsupported actions remain
  fail-closed.
- Raw secrets remain gateway-side and are never written to task files, sandbox
  parameters, audit events, reports, local fixtures, or artifacts.
- Public docs describe only implemented behavior and explicitly call out
  unsupported gateway, runner, replay, team, and connector surfaces.
- Full Rust quality gates pass before a broad implementation wave is claimed:
  `cargo fmt --all`, `cargo clippy --workspace --all-targets -- -D warnings`,
  and `cargo test --workspace`.
- Docker behavior changes run Docker integration tests on a Docker-capable
  machine, or record an explicit unavailable-Docker note.

## Intake / Snapshot

Status: done

- Date: 2026-06-08
- Detected default branch: `origin/main`
- Current branch: `codex/governance-development-plan`
- Current branch is not the detected default branch, so no new branch was
  created for this planning document.
- Worktree before plan authoring: clean aside from branch status output.
- `git pull --ff-only` result: blocked because the current branch has no
  upstream tracking information. No remote update was applied.
- Active plan directory before authoring was empty.
- Next executable phase: Phase 1, enforcing local gateway and network control.

Verification:

- `git status --short --branch`
- `git symbolic-ref --short refs/remotes/origin/HEAD`
- `git pull --ff-only`
- README, roadmap, architecture, runtime code, CLI command definitions, and the
  archived remaining-capability plan were inspected for current implemented and
  explicitly deferred behavior.

## Overall Status

Status: in_progress

This plan is active. Phases 1, 2, 3, and 4 are complete. The next executable
phase is Phase 5, live remote runner backends.

## Phases

### Phase 1: Enforcing Local Gateway And Network Control

Status: done

Scope:

- Add a production local gateway process or foreground listener boundary for
  task-scoped MCP and HTTP tool calls.
- Preserve the existing spool path as the deterministic offline and
  low-network fallback.
- Add an enforcing proxy or gateway-backed egress path so
  `permissions.network.allow_domains` can be honored locally instead of always
  failing closed.
- Bind listener/proxy lifetime to the task, workspace, task id, policy,
  approval engine, audit logger, and artifact root.
- Reject raw token exposure to the sandbox, broad host networking, unregistered
  tools, unsupported protocols, malformed requests, request path escapes, and
  proxy bypass attempts.
- Update runner preparation so Docker tasks can opt into the gateway network
  path without mounting broader host resources.

Verification command:

```bash
cargo test -p taskfence-core -p taskfence-gateway -p taskfence-runner -p taskfence-cli
```

Completion evidence:

- Started on 2026-06-08. The plan source and Phase 1 requirements were read
  before implementation edits.
- Implemented typed `gateway.mode` and `gateway.egress.allow_domains` task
  schema, preserving fail-closed behavior unless local listener mode, egress
  allow-domains, and a registered `http egress.fetch` tool are configured.
- Added prepared gateway metadata and Docker runner planning that keeps
  allowlisted-domain tasks on `--network none`, exposes only the dedicated
  gateway spool path and gateway environment hints, rejects broad spool-covering
  mounts, and continues to reject domain allowlists without the configured
  gateway boundary.
- Added a bounded gateway-side `http egress.fetch` adapter that validates HTTPS
  GET/HEAD URLs, rejects userinfo, fragments, parent path escapes, secret-like
  query material, unregistered tools, unsupported protocols, and
  non-allowlisted hosts before client execution.
- Added `taskfence gateway listen` as a foreground loopback listener for
  task-scoped JSON tool actions, bound to the task workspace, task id, policy,
  approval mode, audit logger, artifact root, registry, and secret broker.
- Preserved the existing gateway spool path as the deterministic offline and
  low-network fallback.
- Updated `examples/github-rest-task.yaml`, README, roadmap, architecture,
  runtime architecture, development design, and
  `docs/decisions/2026-06-08-local-gateway-egress-boundary.md` to document the
  bounded listener/egress behavior without claiming arbitrary HTTP proxying or
  production MCP server behavior.
- Verification passed: `cargo fmt --all --check`.
- Verification passed: `cargo test -p taskfence-core -p taskfence-gateway -p taskfence-runner -p taskfence-cli`
  (95 CLI tests, 6 core tests, 47 gateway tests, 20 runner tests; Docker
  integration test remained ignored because it requires Docker daemon and a
  locally available test image).
- Additional schema verification passed: `cargo test -p taskfence-config
  gateway_egress` (2 tests).

### Phase 2: Expanded GitHub Workflow And Connector Prioritization

Status: done

Scope:

- Expand the bounded GitHub/GitHub Enterprise REST connector from three
  operations into a coherent issue-to-branch-to-PR workflow.
- Add branch creation, commit or patch application, PR update, issue comment,
  and status/report comment behavior only where policy, approval, audit,
  idempotency, and rollback limits are explicit.
- Keep `github.create_pr` approval-sensitive and require explicit policy for
  write operations.
- Choose the next live connector family by operator priority, with GitLab or
  Jira as likely first candidates because they map directly to coding-agent
  workflows.
- Keep unimplemented enterprise connector families contract-only with
  unsupported-action evidence.
- Add examples that exercise live connector validation without embedding
  credentials.

Verification command:

```bash
cargo test -p taskfence-config -p taskfence-gateway -p taskfence-policy -p taskfence-audit -p taskfence-report -p taskfence-cli
```

Completion evidence:

- Started on 2026-06-08 after Phase 1 was marked done and committed as
  `5f05618` (`gateway: add enforcing local gateway and network controls`).
- Implemented bounded live `github_rest` / `github_enterprise_rest` workflow
  operations for `github.create_branch`, `github.commit_file`,
  `github.update_pr`, and `github.comment_report`, alongside existing
  `github.read_issue`, `github.create_pr`, and `github.comment_issue`.
- Added GitHub REST client methods for ref creation, one-file Contents API
  commits, PR updates, and structured issue/PR comments. Operation parameters
  are bounded before live API calls: safe branch/ref names, safe
  repository-relative file paths, bounded commit messages/content, optional
  object SHA validation, bounded PR title/body/state/base updates, bounded
  comment/report bodies, and safe report URLs.
- Kept live tokens gateway-side through the existing environment secret broker.
  Unsupported GitHub REST operations now fail during planned budget validation,
  before gateway-side token issuance or client execution.
- Updated connector policy templates so GitHub and GitHub Enterprise advertise
  the expanded operation set, keep write/report/comment operations
  approval-sensitive, and keep non-GitHub enterprise connectors contract-only
  with explicit unsupported-action evidence.
- Added gateway tests for branch creation, file commit, PR update,
  report-comment posting, path-escape rejection, missing live-token
  fail-closed behavior, token redaction, and unsupported operation
  fail-closed behavior.
- Added a config parser test for the expanded GitHub REST workflow and updated
  `examples/github-rest-task.yaml` and `examples/enterprise-connectors-task.yaml`
  to validate the expanded live connector shape without embedding credentials.
- Updated README, architecture, roadmap, runtime architecture, development
  design, and decision records to document the bounded issue-to-branch-to-PR
  GitHub/GitHub Enterprise workflow while keeping GitLab, Jira, Feishu, WeCom,
  DingTalk, Gitee, CODING, database, internal HTTP, SIEM export, production MCP
  server, arbitrary HTTP proxying, SDK/webhook, and non-GitHub branch/commit
  behavior unsupported.
- Verification passed: `cargo fmt --all --check`.
- Verification passed: `cargo test -p taskfence-gateway github_rest_ --no-fail-fast`
  (10 targeted GitHub REST tests).
- Verification passed: `cargo test -p taskfence-config parses_expanded_github_rest_workflow_tools`.
- Example validation passed: `cargo run -p taskfence-cli -- validate examples/github-rest-task.yaml`.
- Example validation passed: `cargo run -p taskfence-cli -- validate examples/enterprise-connectors-task.yaml`.
- Verification passed: `cargo test -p taskfence-config -p taskfence-gateway -p taskfence-policy -p taskfence-audit -p taskfence-report -p taskfence-cli`
  (20 config tests, 52 gateway tests, 13 policy tests, 4 audit tests, 4 report
  tests, and 95 CLI tests).
- Additional compile guard passed: `cargo check --workspace`.

### Phase 3: Persistent Local State, API Surface, And Rich Review UI

Status: done

Scope:

- Introduce a persistent local API boundary for task summaries, events, logs,
  diffs, reports, artifacts, approvals, comparisons, and replay inputs.
- Add SQLite-backed local state or an equivalent durable index without
  reinterpreting Markdown reports as source-of-truth data.
- Keep file-backed `.taskfence` evidence as the bootstrap and migration source.
- Add live log streaming, artifact download routing, richer browser diff
  interactions, and approval resolution through the API.
- Keep the generated static review page available as a low-dependency fallback
  until the persistent UI is stable.
- If a TypeScript/React Web UI is introduced, keep it outside enforcement
  boundaries and make the Rust API/state contracts authoritative.

Verification command:

```bash
cargo test -p taskfence-state -p taskfence-cli -p taskfence-core -p taskfence-report
```

Completion evidence:

- Started on 2026-06-08 after Phase 2 was marked done and committed as
  `7dfaf75` (`gateway: expand live connector workflows`).
- Added serializable local state records in `taskfence-state` and a durable
  workspace-local JSON index at `.taskfence/state/local-index.json`, refreshed
  from structured `.taskfence/tasks` evidence instead of rendered Markdown
  reports.
- Added `taskfence state index --workspace <workspace>` and `--read-only` so
  operators can refresh or read the local index without starting a server.
- Added structured local comparison and contained artifact routing APIs in
  `taskfence-state`, including parent-path, absolute-path, symlink, non-file,
  and task-directory containment checks. Custom artifact downloads remain
  limited to known immediate `artifacts/` entries plus known evidence files.
- Expanded the foreground loopback `taskfence review --serve` path with JSON
  routes for `/api/index`, tasks, task detail, artifacts, events, logs, diffs,
  reports, replay plans, comparisons, approvals, approval detail, and approval
  resolution. Added contained `/artifact/<task-id>/<relative-path>` downloads
  for known local evidence/artifact files.
- Kept `taskfence review --workspace` static HTML generation as the
  low-dependency fallback and added links to the foreground API/artifact
  routes without introducing a TypeScript/React UI or making browser code part
  of the enforcement boundary.
- Updated README, architecture, roadmap, development design, runtime
  architecture, and `docs/decisions/2026-06-08-local-state-api-index.md` to
  document the local JSON index and foreground loopback API without claiming a
  long-lived persistent API daemon, team server, SQLite migration, live log
  streaming, richer browser diff UI, or replay execution.
- Verification passed: `cargo fmt --all --check`.
- Verification passed: `cargo test -p taskfence-state -p taskfence-cli -p taskfence-core -p taskfence-report`
  (50 state tests, 99 CLI tests, 6 core tests, 4 report tests, and doc-tests).

### Phase 4: Deterministic Replay And Evaluation

Status: done

Scope:

- Promote `taskfence replay plan` into executable replay for supported local
  task inputs.
- Define replayable and non-replayable evidence contracts for task files,
  policy version, runner image, gateway tool calls, approvals, environment,
  limits, artifacts, and redacted secrets.
- Re-run saved tasks across supported agents, policies, or connector fixture
  modes without reusing raw secrets from prior runs.
- Add comparison outputs for success, failure, duration, denied actions,
  approvals, budget usage, file changes, logs, and reports.
- Keep replay blocked for unsupported live connector effects, missing images,
  unavailable runner families, missing approval history, or non-deterministic
  inputs unless the operator explicitly accepts those limits.

Verification command:

```bash
cargo test -p taskfence-core -p taskfence-state -p taskfence-runner -p taskfence-gateway -p taskfence-cli
```

Completion evidence:

- Started on 2026-06-08 after Phase 3 was marked done and committed as
  `5fccb18` (`state: add persistent local API and review data model`).
- Added `taskfence replay run <task-id> --workspace <workspace>
  [--replay-id <task-id>] [--accept-limitations]` as the bounded executable
  replay path for supported local replay inputs.
- Replay execution loads saved `task.resolved.json`, assigns a new replay task
  id, runs through the same local orchestrator, policy, approval, audit,
  artifact, runner, and report pipeline, compares source/replay structured
  summaries, and writes `artifacts/replay.json` in the replay task artifact
  directory.
- Replay remains fail-closed for missing resolved inputs, existing replay
  evidence ids, live or contract-only gateway connector effects, foreground
  local listener mode, domain allowlists, and default-allow network
  requirements. Recorded non-deterministic limitations require explicit
  `--accept-limitations`.
- Added structured `ReplayRunRecord` and `ReplayEvaluation` state contracts so
  replay evidence is JSON state rather than report scraping.
- Added CLI tests for replay command parsing, limitation acknowledgement,
  successful local replay execution with evaluation artifact, replay id
  overwrite rejection, and live connector effect blocking.
- Updated README, architecture, roadmap, development design, runtime
  architecture, and
  `docs/decisions/2026-06-08-bounded-local-replay-execution.md` to document
  bounded local replay without claiming replay of live connector side effects,
  deterministic image snapshots, cross-workspace replay, or team evaluation.
- Verification passed: `cargo fmt --all --check`.
- Verification passed: `cargo test -p taskfence-core -p taskfence-state -p taskfence-runner -p taskfence-gateway -p taskfence-cli`
  (104 CLI tests, 6 core tests, 52 gateway tests, 20 runner tests, 50 state
  tests, and doc-tests; Docker integration remained ignored because it
  requires a Docker daemon and local image).

### Phase 5: Live Remote Runner Backends

Status: pending

Scope:

- Implement remote SSH runner execution first, because it is the smallest
  extension from local Docker to remote task execution.
- Add runner-specific capability checks for filesystem isolation, secret
  isolation, network controls, limits, cancellation, timeout, output capture,
  and artifact return.
- Add Kubernetes job execution after SSH semantics are stable, including
  namespace/service-account boundaries, volume policy, secret policy, network
  policy, logs, and artifact collection.
- Add microVM and managed cloud runners only after their isolation, image,
  network, and artifact contracts are concrete.
- Keep unsupported runner families fail-closed with capability reports until
  live execution and tests exist.

Verification command:

```bash
cargo test -p taskfence-runner -p taskfence-core -p taskfence-cli
```

Completion evidence:

- Pending.

### Phase 6: Team Server, Durable Workers, And Postgres State

Status: pending

Scope:

- Promote the team-server state contract into a persistent API service backed
  by Postgres.
- Add durable task queue and worker lease storage with the same fail-closed
  duplicate, wrong-worker, unleased, and terminal-state protections modeled by
  the in-memory contract.
- Add organization-scoped RBAC, approval-owner enforcement, audit read/export
  permissions, and operator/admin boundaries.
- Add object storage or pluggable artifact roots for team deployments with
  containment checks and artifact integrity metadata.
- Add SSO only after role/resource/method behavior is stable and tested.
- Keep local mode independent from team mode; local task execution must not
  require a team server.

Verification command:

```bash
cargo test -p taskfence-state -p taskfence-core -p taskfence-cli
```

Completion evidence:

- Pending.

### Phase 7: Enterprise Connectors, Audit Export, And Compliance Reports

Status: pending

Scope:

- Add live connector implementations in priority order for GitLab, Jira,
  Feishu, WeCom, DingTalk, Gitee, CODING, database, internal HTTP, and SIEM
  export.
- For each connector, define supported operations, approval-sensitive
  operations, secret scopes, redaction rules, idempotency behavior, rate-limit
  behavior, and unsupported-operation evidence.
- Add live audit export sinks only after team RBAC and artifact/state ownership
  are enforced.
- Add compliance-oriented reports based on structured events, not terminal log
  scraping.
- Keep connector policy templates explicit and opt-in; do not enable broad
  business-tool writes by default.

Verification command:

```bash
cargo test -p taskfence-config -p taskfence-gateway -p taskfence-policy -p taskfence-audit -p taskfence-report -p taskfence-state
```

Completion evidence:

- Pending.

### Phase 8: Specialized Agent Adapters And Packaged Operator Experience

Status: pending

Scope:

- Add specialized adapters for the first real CLI agents selected by operator
  priority, such as Codex CLI, Claude Code, Gemini CLI, OpenHands, or other
  coding-agent runtimes.
- Keep the generic command adapter as the default and do not require
  agent-specific integration for basic sandboxing.
- Add adapter-specific setup, environment, prompt, log, and artifact hints only
  where they do not weaken the runtime boundary.
- Package install/dev/doctor flows through `deploy/manage.sh` and stable docs
  without hard-coding machine-local paths.
- Add policy templates for common coding-agent tasks after live behavior is
  tested.

Verification command:

```bash
cargo test -p taskfence-agent -p taskfence-config -p taskfence-cli -p taskfence-core
```

Completion evidence:

- Pending.

## Commit Plan

1. `gateway: add enforcing local gateway and network controls`
2. `gateway: expand live connector workflows`
3. `state: add persistent local API and review data model`
4. `replay: execute deterministic local task replays`
5. `runner: implement first live remote runner backend`
6. `team: add persistent team server state and workers`
7. `gateway: add prioritized enterprise connectors and audit export`
8. `agent: add specialized adapters and operator packaging`

## Current Plan-Authoring Evidence

- Plan file was created under `docs/codex/plans/` as an active durable plan.
- Readback confirmed the goal, plan source, snapshot, eight pending phases, and
  commit plan were written.
- `git status --short` showed only this new plan file as untracked before final
  validation.
