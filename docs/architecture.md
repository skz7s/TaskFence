# Architecture

TaskFence is designed around a task-level execution model.

A task has a goal, workspace, agent, policy, sandbox, approvals, logs, and
artifacts. Every action that matters should be attributable to a task.

## High-Level Flow

```text
User / CI / Webhook
        |
        v
TaskFence CLI / API
        |
        v
Task Orchestrator
        |
        +--> Policy Engine
        +--> Approval Engine
        +--> Audit Logger
        +--> Artifact Store
        |
        v
Sandbox Runner
        |
        v
Agent Process
        |
        +--> Shell / Files / Network
        +--> Tool Gateway / MCP Gateway
        |
        v
External Systems
```

## Components

Current implementation note: the local CLI path is implemented for generic
commands in Docker, and generated local reports/logs can be read back from the
task workspace. Task files can configure tool allow/approval/deny rules that
feed typed gateway mediation, policy decisions, audit events, and report
evidence, including optional approval request/resolution records for
approval-required tool calls, known-tool registry checks, and redacted gateway
secret references. The executable gateway surface is currently limited to
configured task-file tools through `taskfence gateway call`: deterministic
local fixtures, bounded GitHub/GitHub Enterprise REST connectors for
`github.read_issue`, `github.create_branch`, `github.commit_file`,
`github.create_pr`, `github.update_pr`, `github.comment_issue`, and
`github.comment_report`, and contract-only enterprise connector surfaces that
fail closed for live execution. A bounded agent-facing request/response spool prototype is processed
by `taskfence gateway spool process`. The spool path is task-local, mounted
separately into Docker only for tasks with configured gateway tools, and
produces typed responses plus structured evidence. `taskfence gateway listen`
starts a foreground loopback listener for task-scoped JSON tool actions, and
the bounded `http egress.fetch` action performs gateway-side HTTPS GET/HEAD
requests only after registry, tool, budget, and network-destination policy
checks. The team-server foundation is currently contract-only: typed API
resources, RBAC decisions, approval-owner checks, local development worker
leases, Postgres state config validation, artifact-root containment, validated
but unsupported audit-export sink contracts, and local-to-team migration
planning from structured `.taskfence` files exist without starting a server or
live database. Production MCP servers, arbitrary HTTP proxying, SDK/webhook
connectors, branch/commit behavior outside the bounded GitHub REST family,
persistent API server, live Postgres backend, deterministic replay execution,
persistent team-server, Slack, and live enterprise connector behavior beyond
the bounded GitHub REST family are not implemented yet.
The local review foundation is CLI-owned: it can render file-backed evidence as
a static or foreground-served loopback HTML page, resolve pending
workspace-local approvals from that page, and plan replay inputs without
executing them.

### CLI

The CLI is the first user interface.

Initial commands:

- `taskfence init [path]` writes one starter task file, creating parent
  directories for nested paths and refusing to overwrite an existing target.
- `taskfence validate <task-file>` resolves the task file, checks the planned
  agent command against policy, and builds the local Docker run plan without
  creating artifacts, starting Docker, or requesting approvals.
- `taskfence run <task-file>` executes the current local Docker runner path,
  failing closed for approval-required actions by default.
- `taskfence run --interactive-approval <task-file>` prompts in the local
  terminal for approval-required actions during that run.
- `taskfence run --external-approval <task-file>` writes pending approval
  records under the task workspace and waits for local `approve` / `deny`.
- `taskfence approvals --workspace <workspace>` lists workspace-local approval
  records from the file-backed approval queue.
- `taskfence approval <approval-id> --workspace <workspace>` reads one
  workspace-local approval record from the file-backed approval queue without
  rendering raw tool parameter values.
- `taskfence approve <approval-id> --workspace <workspace>` and
  `taskfence deny <approval-id> --workspace <workspace>` resolve pending local
  approval records.
- `taskfence task <task-id> --workspace <workspace>` reads a single structured
  local task summary and artifact availability from local task evidence.
- `taskfence inputs <task-id> --workspace <workspace>` reads the saved resolved
  task input from local `task.resolved.json` evidence without executing replay.
- `taskfence artifacts <task-id> --workspace <workspace>` lists known local
  evidence files and immediate custom artifact files without reading their
  contents or traversing artifact subdirectories.
- `taskfence compare <left-task-id> <right-task-id> --workspace <workspace>`
  compares two local task summaries from structured evidence without reading
  report text or artifact contents.
- `taskfence status <task-id> --workspace <workspace>` reads the latest
  structured local task status from task evidence.
- `taskfence events <task-id> --workspace <workspace>` reads a structured local
  event timeline from the task `events.jsonl` evidence without rendering raw
  tool parameter values.
- `taskfence review --workspace <workspace>` writes a static local HTML review
  page under `.taskfence/review/index.html`, or to `--output` when supplied,
  using file-backed task, event, diff, log, report, approval, comparison, and
  replay-plan evidence.
- `taskfence review --workspace <workspace> --serve --port <port>` serves the
  same local review page on `127.0.0.1` in the foreground and resolves pending
  workspace-local approvals through explicit approve/deny POST routes.
- `taskfence replay plan <task-id> --workspace <workspace>` reports saved task
  inputs, artifact paths, last status, blockers, and replay limitations without
  executing replay.
- `taskfence logs <task-id> --workspace <workspace>` reads captured stdout and
  stderr logs from local task evidence when present.
- `taskfence diff <task-id> --workspace <workspace>` reads the captured
  `diff.patch` artifact from local task evidence when present.
- `taskfence report <task-id> --workspace <workspace>` reads the generated
  Markdown report from local task evidence.
- `taskfence gateway call <task-file> <tool> <operation>` executes a configured
  deterministic local fixture tool call and writes structured local evidence.
- `taskfence gateway spool process <task-file> <request-file>` processes one
  validated request from the task-local gateway spool, writes one typed
  response, and records structured evidence.

### Task Orchestrator

The orchestrator owns the task lifecycle:

1. Load and validate the task file.
2. Resolve workspace and policy.
3. Create the local task evidence directory.
4. Evaluate command policy and handle approvals.
5. Prepare sandbox inputs.
6. Start the runner.
7. Stream logs.
8. Collect artifacts.
9. Generate the report.

### Policy Engine

The policy engine evaluates whether an action is allowed, denied, or requires
approval.

Initial policy dimensions:

- file read paths
- file write paths
- shell commands
- network domains
- environment variables
- secrets
- tool calls
- budget limits

The current built-in budget policy evaluates typed `Action::Budget` values
only. Task files can configure `permissions.budget.allow` entries with a
normalized budget kind and positive `max_amount`. Budget actions with no
matching kind, an empty kind, or an amount above the configured limit are
denied. Gateway adapters can attach planned or observed `BudgetUsage` records
to mediated execution; the gateway executor evaluates each record through the
same budget policy, writes `BudgetUsageRecorded` audit evidence with the matched
limit and decision, and fails closed before secret attachment when planned usage
is over limit. Observed over-limit usage is recorded with the partial tool
result and a budget error. This is not billing, team quota, chargeback, or broad
model-provider cost metering.

The first implementation can use a built-in policy evaluator. Later versions can
support OPA, Cedar, or custom plugins.

### Sandbox Runner

The runner isolates black-box agents.

Initial runner:

- Docker

Runner contract families:

- local process with reduced guarantees
- SSH remote runner
- Kubernetes job
- microVM runner
- managed cloud runner

The Docker runner should:

- mount only configured workspace paths
- set read-only mounts where possible
- avoid mounting the host home directory
- avoid passing host environment variables by default
- apply CPU, memory, disk, and time limits
- apply network controls where supported
- capture stdout, stderr, and exit code

The current Docker runner uses `docker run --pull=never`, bind mounts the
configured paths, passes only allowlisted environment variables, supports
disabled/default-deny/default-allow Docker network modes, and captures stdout,
stderr, timeout, and exit status. Domain-level network allowlists are not
enforceable by local Docker alone; tasks that configure allowlisted domains fail
closed until an enforcing proxy is implemented.

The expanded runner dispatcher can parse and classify `remote_ssh`,
`kubernetes_job`, `microvm`, and `managed_cloud` sandbox types. These runner
families are currently capability contracts, not executable backends. They
report missing isolation, network control, secret boundary, limit enforcement,
and artifact collection guarantees and fail closed before task execution. Docker
remains the only implemented runner.

### Agent Adapter

An adapter starts an agent inside the runner.

The generic adapter accepts a command string and treats the agent as a black-box
process. Specialized adapters can improve setup and reporting for specific
agents.

Possible adapters:

- generic command
- Codex CLI
- Claude Code
- Gemini CLI
- OpenHands
- OpenClaw
- Hermes

Adapters should not be required for basic sandboxing.

### Tool Gateway

The gateway mediates tool calls for integrated agents.

Current implementation includes typed normalization and mediation contracts,
configured `permissions.tools` policy decisions, structured `PolicyDecision`
audit events, known-tool registry validation before policy evaluation,
`ApprovalRequested` / `ApprovalResolved` events for approval-required tool
calls, report evidence, redacted secret references, MCP/HTTP adapter entry
points that execute through the existing gateway executor when explicitly
configured, and a CLI-owned local fixture execution path. The local fixture path
executes only configured task-file tools and currently models GitHub-shaped
`github.read_issue` and `github.create_pr` behavior without network access or
raw credentials. The live `github_rest` and `github_enterprise_rest` connector
contracts support only `github.read_issue`, `github.create_branch`,
`github.commit_file`, `github.create_pr`, `github.update_pr`,
`github.comment_issue`, and `github.comment_report`. Write operations require
explicit tool policy and bounded parameters before a gateway-side token can be
used. GitLab, Jira, Feishu, WeCom, DingTalk, Gitee, CODING, database, internal
HTTP, and SIEM export are opt-in connector contracts that validate
configuration, policy templates, approval-sensitive operation sets, redacted
secret references, and structured unsupported live execution. The agent-facing spool prototype creates
`gateway-spool/requests`, `gateway-spool/responses`, and a generated
`taskfence-gateway-submit` wrapper under the task evidence directory; Docker
mounts only that dedicated spool path at `/taskfence/gateway-spool` when
gateway tools are configured, rejecting broader permission mounts that would
also expose it. The host-side spool processor validates request paths against
the task spool root, rejects parent components and symlink escapes, executes one
mediated configured action, and writes typed success, denied, timeout,
cancellation, malformed-request, unsupported-action, or failure responses with
structured evidence. When a registry is configured, unregistered tool actions
fail before policy evaluation and record an audit error. The foreground
`gateway listen` path accepts task-scoped JSON tool actions on loopback and the
bounded `http egress.fetch` action can make gateway-side HTTPS GET/HEAD
requests for policy-allowlisted hosts. It does not implement production MCP
servers, arbitrary HTTP proxying, SDK/webhook connectors, branch/commit
behavior outside the bounded GitHub REST family, persistent Web/API server
behavior, deterministic replay execution, team-server, Slack, or live
enterprise connector behavior beyond the bounded GitHub REST family yet.

Supported gateway surfaces can include:

- MCP server proxy
- HTTP API proxy
- CLI wrapper
- SDK
- webhook receiver

The gateway enables semantic policy decisions such as:

- allow `github.read_issue`
- require approval for `github.create_pr`
- deny `github.delete_repo`
- redact `slack.post_message` content before logging
- require approval for `database.write`

### Secret Broker

The secret broker keeps raw credentials out of the agent process whenever
possible.

The agent should request an action through a tool. The gateway performs the
action with a scoped credential. Logs contain references and redacted values, not
raw secrets.

Current implementation defines the gateway-side broker trait and
`SecretReference` contract. It can authorize a requested secret against
`secrets.available_to_gateway` and attach a redacted parameter reference before
adapter execution. The local fixture broker issues only redacted handles; it
does not read, store, validate, or use a raw credential. The environment-backed
broker is used for configured live GitHub/GitHub Enterprise REST tools and reads
`TASKFENCE_GATEWAY_SECRET_<NORMALIZED_SECRET_NAME>` only inside the host gateway
process after policy, registry, and approval checks. Raw token values are passed
out-of-band to the connector client and are not written to task files, sandbox
parameters, audit events, reports, or artifacts.

### Approval Engine

The approval engine pauses actions that require human review.

Approval records should include:

- task ID
- action type
- actor
- requested parameters
- policy rule
- risk reason
- approval result
- approver identity
- timestamp

Current local approval behavior is fail-closed by default. The CLI can opt into
in-process interactive approval with `taskfence run --interactive-approval`, or
explicitly wait for workspace-local file-backed approval records with
`taskfence run --external-approval`. In external approval mode, pending records
are written under `.taskfence/approvals/<approval-id>.json` in the task
workspace and can be resolved by `taskfence approve` or `taskfence deny` from
another terminal. The CLI can list those workspace-local approval records with
`taskfence approvals --workspace <workspace>` and read one record with
`taskfence approval <approval-id> --workspace <workspace>`; these are local file
queue queries, not a service-side approval system. Preconfigured decisions can
model approved, denied, or timed-out outcomes in tests.

Policy-denied and approval-denied local runs stop before the runner starts, but
still write resolved task evidence, structured audit events, and a Markdown
report when the artifact directory can be created.

### Audit Logger

The audit logger records task evidence:

- task file and resolved policy
- agent command
- sandbox image and limits
- stdout and stderr
- shell commands when available
- tool calls
- network destinations where available
- approvals
- denied actions
- file diffs
- artifacts
- costs and duration when available

### Artifact Store

Artifacts include:

- logs
- file diffs
- generated reports
- task metadata
- workspace snapshots where configured
- replay inputs

The first implementation can store artifacts on the local filesystem. Team
deployments can use object storage.

Current local artifacts are written under `.taskfence/tasks/<task-id>/` in the
task workspace and include the resolved task JSON, JSONL audit events,
stdout/stderr logs when present, a diff artifact, and a Markdown report. The
local CLI can list workspace-local task summaries and read structured event
summaries, resolved task inputs, artifact manifests, task summary comparisons,
latest task statuses, captured diffs, generated reports, or captured logs from
that workspace-local artifact directory. The local review page and replay-plan
command consume these same structured files. They do not yet provide
cross-workspace indexing, a persistent API server, replay execution, or
SQLite-backed state.

The team-state contract can build a migration plan from local `.taskfence`
evidence by listing task ids and artifact roots only when structured task input
or event files are present. Rendered Markdown reports are carried as artifacts,
not interpreted as source-of-truth state. Team artifact writes are checked
against configured absolute roots with parent-directory and containment guards.
The Postgres config type validates a future database URL environment variable
and schema name, but live Postgres storage returns an explicit unsupported
error until a backend exists.

### Team Control Plane

The future team control plane is modeled as a state-layer contract before any
API process is started. The current boundary lists task, event, log, diff,
report, artifact, approval, replay-input, and audit-export resources. RBAC
roles are explicit: viewers read task evidence, approvers can read and resolve
approval records, operators can enqueue and resolve task work, auditors can
read evidence and request audit export, and admins can administer all modeled
actions. Method/resource mismatches fail closed even for admins.

The local development worker model is an in-memory lease queue. Tasks can be
enqueued, leased by one worker id, completed, or failed; wrong-worker,
duplicate, unleased, and already-terminal transitions are rejected. This is a
contract for future team execution semantics, not a persistent queue or live
worker service. Audit export is similarly an RBAC/API boundary with validated
sink contracts and an unsupported live export-sink error until a concrete sink
is implemented.

## Security Boundary

TaskFence can only enforce access for agents that run inside its controlled
environment or call through its gateway.

If an agent runs directly on the host with real secrets and unrestricted network
access, TaskFence cannot reliably prevent bypasses.

The enforcement strategy is therefore:

- do not give direct access to host secrets
- do not mount more filesystem than needed
- do not provide unrestricted egress by default
- expose high-risk capabilities through the gateway
- use approvals for sensitive actions
- record all available evidence

## Task Report

Each task should produce a human-readable report.

Suggested sections:

- summary
- task input
- agent and model
- policy summary
- sandbox summary
- timeline
- commands
- tool calls
- approvals
- denied actions
- network destinations
- file changes
- test results
- artifacts
- residual risks

## Design Principle

TaskFence should provide useful safety without requiring agents to adapt, and
better safety when they do adapt.

The product should work in layers:

1. generic sandboxing for any CLI agent
2. gateway-mediated tools for structured control
3. team control plane for policy, audit, and collaboration
