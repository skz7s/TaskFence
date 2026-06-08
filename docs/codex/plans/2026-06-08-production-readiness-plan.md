# Production Readiness Follow-Up Plan

## Goal

Plan the next TaskFence work after the completed local runner, bounded gateway,
local review/replay, remote SSH runner, persistent team-state foundation,
enterprise connector, audit export, compliance report, specialized adapter, and
operator-packaging slices.

The goal is to move from a broad local-capability foundation toward production
readiness without overclaiming unsupported daemon, UI, runner, connector,
policy, deployment, or enterprise behavior.

## Plan Source

User request on 2026-06-08: "生成后续plan".

Current capability baseline from the immediately preceding capability review and
the archived
`docs/codex/plan_archived/2026-06-08-remaining-capability-implementation.md`:

- The Rust workspace has the intended crate boundaries for CLI, core, config,
  policy, approval, audit, artifacts, runner, agent, gateway, report, state,
  and testkit.
- Local `taskfence init`, `validate`, `run`, local evidence query commands,
  local approvals, compliance reporting, review page generation/foreground
  serving, bounded replay execution, and local/team state commands exist.
- Docker local execution and first live remote SSH execution are implemented
  behind runner capability checks.
- The gateway has local fixture, bounded GitHub/GitHub Enterprise REST,
  prioritized live enterprise connectors, local listener, and spool processing
  surfaces.
- Local review and loopback JSON API are foreground CLI-owned surfaces, not a
  long-lived product daemon.
- Team state, RBAC, worker leases, audit export records, local JSON storage,
  and Postgres-backed state storage exist as state-layer/CLI foundations, not
  a deployed team server.
- Production MCP servers, arbitrary HTTP proxying, SDK/webhook connectors,
  long-lived persistent API daemon, production Web UI, Kubernetes, microVM,
  managed cloud runner execution, SSO, object storage adapter, background audit
  export service, team quota/chargeback, Slack, department policy packs, and
  replay of live connector side effects remain future work.

Related prior plan:

- `docs/codex/plan_archived/2026-06-08-remaining-capability-implementation.md`
  completed the previous eight-phase capability implementation wave. This file
  is a new follow-up plan and must not rewrite that archived execution state.

## Scope

- Convert selected foreground/local surfaces into production-grade service
  boundaries only after contracts, security controls, and tests are explicit.
- Harden gateway transports and connector behavior before adding broad proxy or
  SDK/webhook reach.
- Expand runner families only when each backend can state and enforce its
  isolation, network, secret, limit, cancellation, and artifact guarantees.
- Improve review, replay, and team workflows using structured state and audit
  evidence rather than rendered report scraping.
- Add release, deployment, and operations readiness without hard-coding
  machine-local facts or weakening the existing `deploy/manage.sh` contract.
- Keep docs, examples, decision records, runtime facts, and tests aligned with
  actual implemented behavior.

## Non-Goals

- Do not build a general-purpose agent framework.
- Do not claim production MCP, arbitrary HTTP proxy, SDK/webhook, deployed team
  server, SSO, object storage, Kubernetes, microVM, managed cloud, or Slack
  support until implementation, failure handling, tests, and docs exist.
- Do not expose raw provider or business-tool credentials to agent sandboxes.
- Do not treat local review HTML or foreground loopback serving as a production
  Web UI without a separate service and deployment boundary.
- Do not make local task execution require team mode or a deployed service.
- Do not introduce hidden worker dispatch, background automation, or
  out-of-band state outside documented runtime contracts.
- Do not split the core runtime across Rust and Go.

## Assumptions

- Rust remains the core runtime and enforcement language.
- The current branch is suitable for planning; implementation phases may need a
  new topic branch depending on operator direction.
- Existing local JSON state and Postgres-backed state-layer code are acceptable
  foundations for service work, but not themselves a deployed daemon.
- Docker integration tests require a Docker-capable host with the expected
  local image; unavailable Docker must be recorded explicitly when relevant.

## Acceptance Criteria

- Each implementation phase has a narrow contract, explicit non-goals,
  fail-closed behavior, targeted tests, and documentation updates.
- Unknown, malformed, unregistered, unsupported, denied, approval-denied,
  approval-timeout, secret-unavailable, path-escaping, and policy-mismatched
  actions remain fail-closed.
- Gateway secrets remain gateway-side and never appear in task files, sandbox
  inputs, audit events, reports, review UI output, replay artifacts, or
  connector fixtures.
- Service and runner behavior is described only after it is executable and
  tested; contract-only surfaces stay labeled as unsupported.
- Broad readiness claims require `cargo fmt --all`,
  `cargo clippy --workspace --all-targets -- -D warnings`, and
  `cargo test --workspace`.
- Docker, database, remote runner, and live connector behavior changes either
  run their relevant integration tests or record an explicit unavailable-host /
  unavailable-service limitation.

## Intake / Snapshot

Status: done

- Date: 2026-06-08
- Detected default branch: `origin/main`
- Current branch: `codex/governance-development-plan`
- Current branch is not the detected default branch, so no new branch was
  created for this planning-only task.
- Worktree before plan authoring: clean aside from branch status output.
- `git pull --ff-only` result: blocked because the current branch has no
  upstream tracking information. No remote update was applied.
- Active plan directory before authoring contained only `.gitkeep`.
- Prior completed capability plan inspected:
  `docs/codex/plan_archived/2026-06-08-remaining-capability-implementation.md`.
- Next executable phase after approval: Phase 1, production service boundary
  selection and API daemon contract.

Verification:

```bash
git status --short --branch
git symbolic-ref --short refs/remotes/origin/HEAD
git pull --ff-only
rg --files docs/codex/plans docs/codex/plan_archived
```

## Overall Status

Status: pending

This is a follow-up execution plan. No implementation phase has started.

## Phases

### Phase 1: Production Service Boundary And API Daemon Contract

Status: pending

Scope:

- Decide the first production daemon boundary: local API daemon, team API
  daemon, or a combined binary with clearly separated local/team modes.
- Define stable HTTP API resources for task lists/details, events, logs, diffs,
  reports, artifacts, approvals, replay inputs, team task records, worker
  leases, and audit exports.
- Add lifecycle, config, bind-address, auth placeholder, graceful shutdown,
  health/readiness, and structured diagnostics contracts.
- Preserve foreground `taskfence review --serve` as a low-dependency local
  review path until the daemon is production-ready.
- Keep local execution independent from daemon availability.

Verification command:

```bash
cargo test -p taskfence-core -p taskfence-state -p taskfence-cli
```

Verification evidence:

- Pending.

### Phase 2: Production Gateway Transport Hardening

Status: pending

Scope:

- Implement the next production gateway transport in priority order, likely MCP
  server first, then bounded HTTP adapter routes, then SDK/webhook entry points.
- Keep arbitrary HTTP proxying unsupported until request inspection, destination
  policy, secret handling, streaming, audit, and bypass controls are concrete.
- Add request authentication, request size limits, response redaction,
  structured error schemas, rate-limit behavior, and connector timeout
  behavior.
- Maintain compatibility with existing `gateway call`, `gateway listen`, and
  spool processing for deterministic tests and low-dependency local operation.

Verification command:

```bash
cargo test -p taskfence-gateway -p taskfence-policy -p taskfence-audit -p taskfence-report -p taskfence-cli
```

Verification evidence:

- Pending.

### Phase 3: Production Review UI And Operator Workflows

Status: pending

Scope:

- Introduce a production Web UI only after the daemon/API contract is stable.
- Build reviewer workflows for task evidence, approval queues, report/log/diff
  inspection, contained artifact downloads, comparisons, replay planning, and
  compliance views.
- Preserve Rust/state contracts as the source of truth; browser code must not
  own policy, approval, audit, or artifact enforcement.
- Add access-control-aware UI states, loading/error/empty states, and
  destructive-action confirmations.
- Keep the static generated review page available as fallback until the Web UI
  is deployable.

Verification command:

```bash
cargo test -p taskfence-state -p taskfence-cli
```

Additional UI verification will be defined after the frontend stack is chosen.

Verification evidence:

- Pending.

### Phase 4: Runner Expansion Beyond SSH

Status: pending

Scope:

- Implement Kubernetes job execution only after namespace, service account,
  volume, secret, network policy, log, timeout, cancellation, and artifact
  collection guarantees are explicit.
- Implement microVM and managed cloud runners only after image, isolation,
  network, secret, limit, artifact, and teardown contracts are concrete.
- Add runner-specific capability reports and fail-closed tests before each live
  backend is exposed.
- Keep unsupported runner families returning explicit capability errors rather
  than falling back to Docker or SSH.

Verification command:

```bash
cargo test -p taskfence-runner -p taskfence-core -p taskfence-cli -p taskfence-config
```

Integration verification must be recorded per runner backend.

Verification evidence:

- Pending.

### Phase 5: Team Server, Workers, SSO, And Artifact Storage

Status: pending

Scope:

- Promote the state-layer team foundation into a deployed API service after
  Phase 1 service boundaries are stable.
- Add live worker service behavior with durable leases, duplicate protection,
  wrong-worker rejection, terminal-state protection, and clear retry semantics.
- Add SSO only after role/resource/method decisions are stable and covered by
  tests.
- Add object storage adapters only after containment, integrity metadata,
  credential isolation, and signed-download behavior are defined.
- Add team quota or chargeback only after budget events and task ownership are
  reliable enough to support them.

Verification command:

```bash
cargo test -p taskfence-state -p taskfence-core -p taskfence-cli
```

Postgres, worker, SSO, and object-storage integration verification must be
recorded when those surfaces are implemented.

Verification evidence:

- Pending.

### Phase 6: Replay Of Connector Effects And Evaluation Workflows

Status: pending

Scope:

- Define which live connector effects can be replayed safely through fixture,
  dry-run, or recorded-response modes.
- Keep destructive or externally visible connector side effects blocked from
  replay unless explicit operator confirmation, idempotency, and rollback
  constraints exist.
- Add evaluation summaries across agents, policies, connector modes, runner
  backends, and task versions using structured evidence.
- Avoid reusing raw secrets from prior runs; replay must request fresh
  gateway-side credentials when live execution is allowed.

Verification command:

```bash
cargo test -p taskfence-core -p taskfence-state -p taskfence-gateway -p taskfence-cli
```

Verification evidence:

- Pending.

### Phase 7: Policy Language, Templates, And Governance Packs

Status: pending

Scope:

- Decide whether to keep extending the built-in evaluator or add OPA, Cedar, or
  a custom plugin boundary.
- Version policy schemas, audit event schemas, connector policy templates, and
  runner capability contracts before broad external adoption.
- Add department/use-case policy packs only as explicit opt-in templates, not
  defaults.
- Add migration and compatibility checks for task files, reports, replay
  inputs, and team records.

Verification command:

```bash
cargo test -p taskfence-config -p taskfence-policy -p taskfence-core -p taskfence-gateway
```

Verification evidence:

- Pending.

### Phase 8: Release, Distribution, And Operational Readiness

Status: pending

Scope:

- Define supported install, upgrade, build, doctor, dev, and deployment flows
  through `deploy/manage.sh` and stable docs.
- Add release artifacts, version reporting, compatibility checks, and operator
  diagnostics without storing machine-local facts in committed docs.
- Add security-oriented validation for generated reports, redaction,
  containment, path handling, unsupported-operation errors, and secret scans.
- Prepare a public readiness checklist that separates local preview, beta, and
  production-supported surfaces.

Verification command:

```bash
bash -n deploy/manage.sh
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```

Verification evidence:

- Pending.

## Commit Plan

1. `server: define production api daemon boundary`
2. `gateway: harden production transport surfaces`
3. `ui: add production review workflows`
4. `runner: add next isolated execution backend`
5. `team: deploy team service and worker lifecycle`
6. `replay: support bounded connector-effect evaluation`
7. `policy: version policy schemas and templates`
8. `ops: add release and readiness checks`

## Current Plan-Authoring Evidence

Status: done

- Plan file was created under `docs/codex/plans/` as a new active durable plan.
- Readback confirmed the plan source, snapshot, eight pending phases,
  verification commands, and commit plan were written.
- `git diff --check -- docs/codex/plans/2026-06-08-production-readiness-plan.md`
  passed.
- `git status --short --branch` showed only this new plan file as untracked
  before staging.
