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
- Local Docker domain allowlists fail closed until an enforcing proxy exists.
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

Partially implemented: built-in command/file/network/env/secret/tool policy
decisions, task-file `permissions.tools` parsing, deny precedence,
approval-required command/tool patterns, non-interactive fail-closed approval
records, timeout modeling, and audit/report redaction. Opt-in local interactive
approval during `run` is implemented, as is an explicit workspace-local
external approval mode through `taskfence approvals`, `taskfence approval`,
`taskfence approve`, and `taskfence deny`.
Denied command and approval denial evidence/reporting is implemented for local
pre-run decisions; configured tool-call policy decisions are covered through
typed gateway mediation and structured report evidence, including optional
approval request/resolution records for approval-required tool calls. Remaining
Phase 2 work is observing or mediating real agent tool actions through an
enforcing gateway or wrapper path.

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

Current contract coverage before production gateway execution:

- task files can configure `permissions.tools.allow`,
  `permissions.tools.approval_required`, and `permissions.tools.deny`
- typed gateway mediation normalizes supported `mcp` tool actions, evaluates
  policy, and writes structured `PolicyDecision` audit events
- when an approval engine is explicitly attached, approval-required tool calls
  write `ApprovalRequested` / `ApprovalResolved` audit events and fail closed
  on denial or timeout
- gateway secret broker contracts can authorize configured
  `secrets.available_to_gateway` grants and attach redacted secret references
  to tool parameters without reading or using raw credentials
- MCP and HTTP adapter stubs normalize protocol-shaped requests into
  `ToolAction` values and return explicit unsupported execution errors
- reports render tool-call decisions, approvals, and denied tool actions from
  structured audit events without rendering raw parameter values

Minimum demo:

- agent reads a GitHub issue through TaskFence
- agent proposes a pull request
- TaskFence requires approval before creating the pull request
- raw GitHub token is not exposed to the agent process

## Phase 4: Web UI and Replay

Goal: improve review, debugging, and reproducibility.

Current local coverage before Web UI and SQLite:

- `taskfence tasks --workspace <workspace>` lists workspace-local task
  summaries from `.taskfence/tasks`, using structured resolved-task JSON and
  JSONL status events rather than rendered report text
- `taskfence task <task-id> --workspace <workspace>` reads a single
  workspace-local task summary and artifact availability without a report text
  scrape, Web UI, API server, or SQLite index
- `taskfence inputs <task-id> --workspace <workspace>` reads the saved
  workspace-local `task.resolved.json` as replay input evidence without replay
  execution, Web UI, API server, SQLite index, or report scraping
- `taskfence artifacts <task-id> --workspace <workspace>` lists known
  workspace-local evidence files and immediate custom artifact files without an
  artifact download flow, recursive browser, Web UI, API server, or SQLite index
- `taskfence status <task-id> --workspace <workspace>` reads the latest
  workspace-local task status from structured status events without a report
  text scrape, Web UI, API server, or SQLite index
- `taskfence events <task-id> --workspace <workspace>` reads a structured
  workspace-local event timeline from `events.jsonl` without replay execution,
  Web UI, API server, SQLite index, or raw tool parameter rendering
- `taskfence diff <task-id> --workspace <workspace>` reads the workspace-local
  `diff.patch` artifact without a browser diff viewer or SQLite index
- `taskfence approvals --workspace <workspace>` lists workspace-local approval
  records from `.taskfence/approvals` without an approval UI, API server, or
  SQLite index
- `taskfence approval <approval-id> --workspace <workspace>` reads one
  workspace-local approval record from `.taskfence/approvals` without an
  approval UI, API server, SQLite index, or raw tool parameter rendering

Deliverables:

- task list
- live logs
- diff viewer
- approval UI
- report viewer
- task replay inputs
- local SQLite state
- comparison view for multiple runs

Minimum demo:

- reviewer can inspect a task, approve an action, and download the report

## Phase 5: Team and Enterprise Foundation

Goal: support teams running multiple agents and policies.

Deliverables:

- API server
- worker model
- Postgres backend
- remote runner
- RBAC
- SSO
- organization policies
- audit export
- SIEM integration
- GitHub Enterprise and GitLab support

## Phase 6: Broader Enterprise Agent Gateway

Goal: expand beyond coding agents.

Deliverables:

- model gateway integration
- Feishu, WeCom, DingTalk, Jira, Slack, and database connectors
- business tool approval workflows
- policy templates by department and use case
- managed runner option
- compliance-oriented audit reports

## Open Questions

- Which first CLI agent should receive a specialized adapter?
- Should the default runner image include common language runtimes or stay
  minimal?
- How strict should default network isolation be on macOS?
- Which policy language should be supported after the built-in evaluator?
- Which audit log schema should become the stable public contract?
- Which enterprise connectors matter most for the first commercial users?
