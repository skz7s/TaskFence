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

Implemented boundaries:

- CLI `run` loads a strict task file, constructs local runtime components, and
  calls the orchestrator.
- Docker execution runs with `--pull=never`, controlled mounts, no inherited
  host environment, no host home or socket passthrough by default, captured
  stdout/stderr, timeout handling, and structured exit status.
- Local Docker domain allowlists fail closed until an enforcing proxy exists.
- Local approval is non-interactive and fail-closed by default; interactive
  approval UX remains Phase 2 work.

Minimum demo:

```bash
taskfence run examples/task.yaml
```

The demo should show an agent running in a container, modifying only allowed
paths, and producing a report.

The current demo writes `examples/repo/src/taskfence-demo.txt` and artifacts
under `examples/repo/.taskfence/tasks/local-demo/`.

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
decisions, deny precedence, approval-required command patterns,
non-interactive fail-closed approval records, timeout modeling, and audit/report
redaction. Remaining Phase 2 work is interactive approval UX and durable
approval lookup commands.

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

Minimum demo:

- agent reads a GitHub issue through TaskFence
- agent proposes a pull request
- TaskFence requires approval before creating the pull request
- raw GitHub token is not exposed to the agent process

## Phase 4: Web UI and Replay

Goal: improve review, debugging, and reproducibility.

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
