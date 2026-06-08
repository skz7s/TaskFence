# Roadmap

This roadmap is intentionally staged. TaskFence should prove a narrow secure
execution workflow before expanding into a full enterprise agent gateway.

## Phase 0: Design and Repository Setup

Status: complete for initial repository setup.

- project positioning
- requirements
- architecture
- sample task policy
- Apache-2.0 license
- public README

## Phase 1: Local Secure Runner

Goal: run a black-box CLI coding agent safely against a local repository.

Status: initial local implementation complete.

Deliverables:

- `taskfence init [path]`
- `taskfence validate <task-file>`
- `taskfence run <task-file>`
- task file parser and validator
- Docker sandbox runner
- generic command adapter
- workspace mount controls
- basic network mode controls
- explicit environment variable allowlist
- local task state directory
- stdout and stderr capture
- file diff capture
- Markdown report generation
- local report and captured-log lookup commands

Implemented boundaries:

- `taskfence init [path]` writes one starter task file, creates parent
  directories for nested paths, and refuses to overwrite an existing target. It
  does not execute the task or generate a larger project structure.
- `taskfence validate <task-file>` resolves the task file, checks the planned
  agent command against policy, and builds the local Docker run plan without
  starting Docker, writing artifacts, or requesting approvals.
- CLI `run` loads a strict task file, constructs local runtime components, and
  calls the orchestrator.
- Docker execution runs with `--pull=never`, controlled mounts, no inherited
  host environment, no host home or socket passthrough by default, captured
  stdout/stderr, timeout handling, and structured exit status.
- Local Docker domain allowlists fail closed unless the task explicitly opts
  into the task-scoped local gateway egress boundary with
  `gateway.mode: local_listener`, `gateway.egress.allow_domains: true`, and a
  registered `http egress.fetch` tool. Docker still runs with `--network none`
  for allowlisted domains; gateway-side egress checks the destination host
  through policy before performing bounded HTTPS GET/HEAD requests.
- Local approval is fail-closed by default; `taskfence run --interactive-approval`
  enables in-process terminal approval for approval-required actions, and
  `taskfence run --external-approval` waits for workspace-local
  `taskfence approve` / `taskfence deny` resolution from another terminal.
- `taskfence approvals --workspace <workspace>` lists workspace-local approval
  records from `.taskfence/approvals` without a service-side approval system.
- `taskfence approval <approval-id> --workspace <workspace>` reads one
  workspace-local approval record from `.taskfence/approvals` without a
  service-side approval system or raw tool parameter rendering.
- Policy-denied and approval-denied local runs stop before the runner starts,
  but still write resolved task evidence, structured audit events, and a
  Markdown report when artifact creation succeeds.
- `taskfence task <task-id> --workspace <workspace>`,
  `taskfence inputs <task-id> --workspace <workspace>`,
  `taskfence artifacts <task-id> --workspace <workspace>`,
  `taskfence compare <left-task-id> <right-task-id> --workspace <workspace>`,
  `taskfence status <task-id> --workspace <workspace>`,
  `taskfence events <task-id> --workspace <workspace>`,
  `taskfence report <task-id> --workspace <workspace>`, `taskfence diff
  <task-id> --workspace <workspace>`, and `taskfence logs <task-id> --workspace
  <workspace>` read structured summaries and generated local task evidence from
  `.taskfence/tasks/<task-id>/` when those artifacts exist.
- `taskfence approve <approval-id> --workspace <workspace>` and
  `taskfence deny <approval-id> --workspace <workspace>` resolve pending local
  approval records under `.taskfence/approvals/<approval-id>.json`.

Minimum demo:

```bash
taskfence init taskfence.yaml
taskfence validate examples/task.yaml
taskfence run examples/task.yaml
```

The demo should show an agent running in a container, modifying only allowed
paths, and producing a report.

The current demo writes `examples/repo/src/taskfence-demo.txt` and artifacts
under `examples/repo/.taskfence/tasks/local-demo/`.

The generated Markdown report can be viewed with:

```bash
taskfence report local-demo --workspace examples/repo
```

## Phase 2: Policy and Approval

Goal: make high-risk actions visible and reviewable.

Deliverables:

- built-in policy evaluator
- command allowlist and denylist
- approval-required command patterns
- budget limits
- interactive CLI approval
- approval records in audit logs
- denied action records
- secret redaction rules

Partially implemented: built-in command/file/network/env/secret/tool/budget
policy decisions, task-file `permissions.tools` and `permissions.budget`
parsing, deny precedence, approval-required command/tool patterns,
non-interactive fail-closed approval records, timeout modeling, and audit/report
redaction. Budget actions are denied by default unless a typed action matches a
configured `permissions.budget.allow` kind and positive `max_amount`. Gateway
adapters can record planned or observed budget usage as structured audit
evidence; the bounded `github_rest` connector records one planned
`gateway_calls` usage before secret attachment or live API execution. This is
not billing, team quota, chargeback, or broad model-provider cost metering.
Opt-in local interactive approval during `run` is implemented, as is an explicit
workspace-local external approval mode through `taskfence approvals`,
`taskfence approval`, `taskfence approve`, and `taskfence deny`.
Denied command and approval denial evidence/reporting is implemented for local
pre-run decisions; configured tool-call policy decisions are covered through
typed gateway mediation and structured report evidence, including optional
known-tool registry checks and approval request/resolution records for
approval-required tool calls. Remaining Phase 2 work is observing or mediating
real agent tool actions through an enforcing gateway or wrapper path.

Minimum demo:

- agent can run tests automatically
- agent must request approval before pushing code or calling a write tool
- report shows approved and denied actions

## Phase 3: Tool Gateway

Goal: add semantic control for integrated agents and tools.

Deliverables:

- MCP gateway prototype
- HTTP tool proxy prototype
- tool registry
- per-tool policy rules
- GitHub tool integration
- secret broker for GitHub token usage
- structured tool call audit logs

Current implemented gateway coverage:

- task files can configure `permissions.tools.allow`,
  `permissions.tools.approval_required`, and `permissions.tools.deny`
- typed gateway mediation normalizes supported `mcp` tool actions, evaluates
  policy, and writes structured `PolicyDecision` audit events
- typed registered-tool keys normalize protocol, tool, and operation segments;
  the local executable path builds a registry from `gateway.tools`, and
  unregistered tool actions fail before policy evaluation with audit evidence
- approval-required executable fixture tool calls write `ApprovalRequested` /
  `ApprovalResolved` audit events and fail closed on denial or timeout before
  any adapter execution
- gateway secret broker contracts can authorize configured
  `secrets.available_to_gateway` grants and attach redacted secret references
  to tool parameters after policy, registry, and approval checks; local fixture
  execution receives only redacted handles, while live GitHub REST execution
  receives raw tokens only through the gateway-side secret broker
- `taskfence gateway call` can execute deterministic local fixture tools and a
  bounded GitHub REST connector from a task file, then write structured
  `.taskfence/tasks/<task-id>/` evidence and a Markdown report
- task artifact setup creates a task-local `gateway-spool/requests`,
  `gateway-spool/responses`, and generated `taskfence-gateway-submit` wrapper
  for configured gateway tasks
- the Docker runner mounts only the dedicated gateway spool path at
  `/taskfence/gateway-spool` when gateway tools are configured, and rejects
  broader read/write mounts that would also expose that spool
- `taskfence gateway listen <task-file>` starts a foreground loopback listener
  for task-scoped JSON tool actions and executes them through the same registry,
  policy, approval, secret-broker, audit, and report path as `gateway call`
- the bounded `http egress.fetch` gateway action supports gateway-side HTTPS
  GET/HEAD requests only after URL validation, network destination policy, tool
  policy, budget policy, and registry checks; it rejects userinfo, fragments,
  parent-path escapes, secret-like query material, unregistered tools, and
  non-allowlisted domains
- `taskfence gateway spool process <task-file> <request-file>` validates one
  request file under the task spool, executes one mediated local fixture action,
  writes one typed response, and records structured evidence for success,
  denied, timeout, cancellation, malformed request, unsupported action, and
  adapter/policy/approval failures
- the GitHub-shaped fixture supports `github.read_issue` from local JSON and
  `github.create_pr` as a PR proposal artifact after explicit approval; it does
  not call live GitHub or use a real token
- the live `github_rest` and `github_enterprise_rest` connector contracts
  support `github.read_issue`, `github.create_branch`, `github.commit_file`,
  `github.create_pr`, `github.update_pr`, `github.comment_issue`, and
  `github.comment_report`; write operations require explicit tool policy,
  gateway-side secrets, planned `gateway_calls` budget checks, and bounded
  parameters before any live API call
- opt-in enterprise connector contracts exist for GitLab, Jira, Feishu, WeCom,
  DingTalk, Gitee, CODING, database, internal HTTP, and SIEM export; these
  currently validate configuration, connector-specific policy templates,
  approval-sensitive operation sets, redacted secret references, and explicit
  unsupported live execution rather than calling live services
- MCP and HTTP adapter entry points normalize protocol-shaped requests into
  `ToolAction` values and execute through the existing gateway executor when
  the action is explicitly configured
- reports render tool-call decisions, approvals, local fixture executions, and
  denied tool actions from structured audit events without rendering raw
  parameter values
- production MCP servers, arbitrary HTTP proxying, SDK/webhook connectors,
  branch/commit behavior outside the bounded GitHub REST family, persistent
  Web/API server behavior, replay for unsupported live connector effects,
  team-server, and live enterprise connector execution beyond the bounded
  GitHub REST family remain future work

Current local fixture demo:

- `taskfence gateway call examples/task.yaml github read_issue --param number=1`
  reads a GitHub-shaped issue fixture through TaskFence
- `taskfence gateway call examples/task.yaml github create_pr --approve --param
  title="Fixture proposal"` writes a local PR proposal artifact after explicit
  approval
- a sandbox-visible `taskfence-gateway-submit` wrapper can write request files
  into the mounted spool; the host-side `taskfence gateway spool process`
  command processes one request file and writes its response under the same task
  evidence root
- raw GitHub token values are not read, used, logged, reported, or exposed to
  the agent process by local fixtures; live `github_rest` tools read raw token
  values only from `TASKFENCE_GATEWAY_SECRET_<NORMALIZED_SECRET_NAME>` in the
  host gateway process after policy, registry, and approval checks

## Phase 4: Web UI and Replay

Goal: improve review, debugging, and reproducibility.

Current local coverage:

- `taskfence tasks --workspace <workspace>` lists workspace-local task
  summaries from `.taskfence/tasks`, using structured resolved-task JSON and
  JSONL status events rather than rendered report text
- `taskfence task <task-id> --workspace <workspace>` reads a single
  workspace-local task summary and artifact availability without a report text
  scrape
- `taskfence inputs <task-id> --workspace <workspace>` reads the saved
  workspace-local `task.resolved.json` as replay input evidence without replay
  execution or report scraping
- `taskfence artifacts <task-id> --workspace <workspace>` lists known
  workspace-local evidence files and immediate custom artifact files without
  reading their contents or recursively traversing artifact subdirectories
- `taskfence state index --workspace <workspace>` rebuilds and prints a
  durable local JSON index at `.taskfence/state/local-index.json` from
  structured task evidence; `--read-only` prints the existing index without
  refreshing it
- `taskfence compare <left-task-id> <right-task-id> --workspace <workspace>`
  compares two workspace-local task summaries from structured evidence without
  replay execution, report scraping, or artifact content diffing
- `taskfence status <task-id> --workspace <workspace>` reads the latest
  workspace-local task status from structured status events without a report
  text scrape
- `taskfence events <task-id> --workspace <workspace>` reads a structured
  workspace-local event timeline from `events.jsonl` without replay execution
  or raw tool parameter rendering
- `taskfence diff <task-id> --workspace <workspace>` reads the
  workspace-local `diff.patch` artifact without a browser diff viewer
- `taskfence approvals --workspace <workspace>` lists workspace-local approval
  records from `.taskfence/approvals`
- `taskfence approval <approval-id> --workspace <workspace>` reads one
  workspace-local approval record from `.taskfence/approvals` without raw tool
  parameter rendering
- `taskfence review --workspace <workspace>` renders a static local HTML review
  page from file-backed task summaries, pending approvals, event timelines,
  diffs, logs, reports, replay plans, and a structured run-comparison table
- `taskfence review --workspace <workspace> --serve --port <port>` serves that
  review page on loopback in the foreground and resolves pending
  workspace-local approvals through explicit approve/deny POST routes. While
  running, it exposes loopback-only JSON routes under `/api/...` for the local
  index, task evidence, comparisons, replay inputs, approvals, and approval
  resolution, plus contained artifact download routes for known evidence and
  artifact files.
- `taskfence replay plan <task-id> --workspace <workspace>` reports saved
  replay inputs, last status, blockers, and limitations without executing
  replay
- `taskfence replay run <task-id> --workspace <workspace>
  [--replay-id <task-id>] [--accept-limitations]` executes supported local
  replay inputs through the same orchestrator and runner boundary, writes a new
  local task evidence directory, compares source/replay structured summaries,
  and writes `artifacts/replay.json`; it fails closed for missing inputs,
  existing replay ids, live or contract-only connector effects, foreground
  listener mode, and network allow/default-allow requirements

Deliverables:

- implemented: task list, evidence detail page, pending approval review,
  approve/deny actions, report/log/diff/event viewing, durable local JSON state
  index, foreground loopback JSON API routes, contained artifact downloads,
  replay input planning, bounded local replay execution, and structured
  comparison for multiple runs
- remaining: live log streaming, richer browser diff interactions, long-lived
  local API daemon, SQLite-backed state migration, and replay support for live
  connector effects or other non-deterministic external state

Minimum demo:

- reviewer can inspect a local task, read its report/log/diff/event evidence,
  download known evidence/artifact files, compare multiple runs, and approve or
  deny a pending workspace-local action from the loopback review page or JSON
  API

## Phase 5: Runner Expansion

Goal: preserve identical task semantics across future runner families.

- implemented: typed `sandbox.type` parsing for `remote_ssh`,
  `kubernetes_job`, `microvm`, and `managed_cloud`
- implemented: runner capability reports for Docker and future runner
  families, including whether the runner can isolate filesystem/secrets,
  disable or default-deny network, enforce domain allowlists, enforce limits,
  and capture output
- implemented: expanded runner dispatch for Docker plus fail-closed capability
  checks for unavailable remote runner families
- remaining: live SSH execution, Kubernetes job execution, microVM lifecycle,
  managed cloud provider execution, runner-specific artifact transport, and
  runner-specific integration tests on configured hosts

## Phase 6: Team and Enterprise Foundation

Goal: support teams running multiple agents and policies.

Current team-server foundation:

- implemented: typed team API resource boundaries for task lists/details,
  event/log/diff/report/artifact reads, approval queues/details, replay inputs,
  and audit export
- implemented: RBAC and organization-policy decision contracts, including
  method/resource mismatch denial and optional approval-owner enforcement
- implemented: deterministic in-memory worker lease model for local
  development tests, with fail-closed duplicate, wrong-worker, unleased, and
  already-terminal transitions
- implemented: Postgres-backed team state configuration validation for a future
  database URL environment variable and schema, with explicit unsupported live
  backend errors
- implemented: team artifact storage root containment checks for absolute,
  canonical roots
- implemented: local `.taskfence` to team migration planning from structured
  task input and event files without treating rendered Markdown reports as
  source-of-truth state
- implemented: audit export as an RBAC/API boundary with validated sink
  contracts and an explicit unsupported live export-sink error
- remaining: persistent team API server, live Postgres backend, durable worker
  queue, SSO, team quota/chargeback, object storage, remote-runner-backed team
  execution, live SIEM export sink, and live GitHub Enterprise/GitLab team
  integrations

## Phase 7: Broader Enterprise Agent Gateway

Goal: expand beyond coding agents.

Deliverables:

- model gateway integration
- GitHub Enterprise, GitLab, Jira, Feishu, WeCom, DingTalk, Gitee, CODING,
  database, internal HTTP API, and SIEM/export connectors
- business tool approval workflows
- policy templates by department and use case
- managed runner option
- compliance-oriented audit reports

Current bounded connector foundation:

- `github_enterprise_rest` reuses the bounded GitHub REST adapter contract with
  an explicit HTTPS API base, gateway-side token lookup, and the same
  `github.read_issue`, `github.create_branch`, `github.commit_file`,
  `github.create_pr`, `github.update_pr`, `github.comment_issue`, and
  `github.comment_report` operation limits.
- GitLab, Jira, Feishu, WeCom, DingTalk, Gitee, CODING, database, internal HTTP,
  and SIEM export connectors are opt-in configuration and policy-template
  contracts. They fail closed for live execution with structured unsupported
  evidence after registry, policy, approval, and redacted secret-reference
  handling.
- Live service-specific clients, Slack, department-level policy packs, managed
  runner execution, and compliance report exports remain future work.

## Open Questions

- Which first CLI agent should receive a specialized adapter?
- Should the default runner image include common language runtimes or stay
  minimal?
- How strict should default network isolation be on macOS?
- Which policy language should be supported after the built-in evaluator?
- Which audit log schema should become the stable public contract?
- Which enterprise connectors matter most for the first commercial users?
