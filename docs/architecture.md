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
approval-required tool calls and redacted gateway secret references. Gateway
execution, credential use, Web UI, replay, team-server, and enterprise
control-plane behavior are not implemented yet.

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
- `taskfence logs <task-id> --workspace <workspace>` reads captured stdout and
  stderr logs from local task evidence when present.
- `taskfence diff <task-id> --workspace <workspace>` reads the captured
  `diff.patch` artifact from local task evidence when present.
- `taskfence report <task-id> --workspace <workspace>` reads the generated
  Markdown report from local task evidence.

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

The first implementation can use a built-in policy evaluator. Later versions can
support OPA, Cedar, or custom plugins.

### Sandbox Runner

The runner isolates black-box agents.

Initial runner:

- Docker

Future runners:

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

Current implementation is limited to typed normalization and mediation
contracts, configured `permissions.tools` policy decisions, structured
`PolicyDecision` audit events, optional `ApprovalRequested` /
`ApprovalResolved` events for approval-required tool calls, report evidence,
redacted secret references, MCP/HTTP request normalization stubs, and explicit
unsupported-protocol errors. It does not execute MCP, HTTP, CLI wrapper, SDK,
webhook, or secret-broker actions yet.

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

Current implementation only defines the gateway-side broker trait and
`SecretReference` contract. It can authorize a requested secret against
`secrets.available_to_gateway` and attach a redacted parameter reference, but it
does not read, store, or use a raw credential.

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
that workspace-local artifact directory, but it does not yet provide
cross-workspace indexing, Web UI queries, replay execution, or SQLite-backed
state.

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
