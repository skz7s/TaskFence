# Requirements

This document defines the initial product requirements for TaskFence.

## Goals

- Run AI agent tasks in controlled environments.
- Support black-box CLI agents without requiring agent-specific integration.
- Provide stronger tool-level enforcement for integrated agents.
- Enforce task policies for files, commands, network, secrets, and tools.
- Record complete task evidence for review and incident reconstruction.
- Support human approval for high-risk actions.
- Keep the first developer experience simple enough for local use.

## Non-Goals

- Build a new general-purpose agent.
- Replace existing agent frameworks.
- Replace model providers or model-only gateways.
- Provide perfect semantic inspection of black-box HTTPS traffic.
- Guarantee security for agents run outside the TaskFence runtime.
- Start with a heavy Kubernetes-only deployment.

## Personas

### Developer

Wants to run an AI coding agent against a repository while protecting personal
files, shell environment, credentials, and network access.

### Team Lead

Wants the team to use coding agents without producing unreviewable changes,
unsafe scripts, or untraceable tool calls.

### Platform Engineer

Wants one execution layer for multiple agents, policies, runners, and tool
integrations.

### Security Engineer

Wants least privilege, approval for sensitive operations, secret isolation,
egress control, and audit logs.

## Core Use Cases

### Secure Local Agent Run

Run a CLI agent in a Docker sandbox with a mounted repository, limited
environment variables, restricted network access, and a final diff report.

### GitHub Issue Fix

Start a task from an issue, let the agent inspect and modify a repository, run
tests, and require approval before pushing a branch or opening a pull request.

### CI Repair

Run an agent against a failing CI job, allow test and lint commands, prevent
production secrets from being exposed, and produce a report for review.

### MCP Tool Governance

Expose GitHub, Slack, Feishu, database, browser, or internal tools through a
TaskFence gateway, then enforce operation-level permissions and approvals.

### Agent Regression Evaluation

Replay a task against different agents, models, or policies and compare success,
cost, duration, denied actions, file changes, and test results.

## P0 Functional Requirements

- `taskfence run <task-file>` starts a local task.
- Task files define goal, workspace, agent command, sandbox type, permissions,
  budgets, and approval rules.
- Docker runner isolates the agent from the host.
- Workspace mounting supports read-only and read-write paths.
- Secrets are not passed into the agent by default.
- Network access can be disabled or allowlisted by domain.
- Commands and terminal output are captured.
- File changes are collected as a diff.
- High-risk actions can pause the task for approval.
- Task logs and artifacts are stored locally.
- A Markdown or HTML report is generated at the end of each task.

## P1 Functional Requirements

- Web UI for task logs, diffs, approvals, and reports.
- MCP gateway with per-tool policy enforcement.
- GitHub integration for issues, branches, pull requests, and comments.
- Secret broker for scoped tool usage without exposing raw secrets.
- Policy templates for common coding-agent tasks.
- Replay support for deterministic task inputs and environment snapshots.
- Evaluation runner for comparing agents and policies.

## P2 Functional Requirements

- Remote runners over SSH.
- Kubernetes runner.
- Team control plane.
- SSO and RBAC.
- SIEM and audit export.
- Enterprise connectors for GitHub Enterprise, GitLab, Jira, Feishu, WeCom,
  DingTalk, Gitee, CODING, databases, and internal HTTP APIs.
- Advanced policy engines such as OPA, Cedar, or custom policy plugins.

## Security Requirements

- Agents must not receive host credentials unless explicitly configured.
- Agents must not see the host home directory by default.
- Write access must be restricted to configured paths.
- External network access must be disabled or allowlisted by default.
- Tool credentials must stay on the gateway side when possible.
- Logs must redact configured secret patterns.
- Approvals must be recorded with actor, time, action, policy, and result.
- All task artifacts must be tied to a task ID.

## Deployment Requirements

### Local Developer Mode

- Single binary or simple package install.
- Requires Docker for the first sandbox runner.
- Stores state locally.

### CI Mode

- Runs as a CI step.
- Produces artifacts and reports.
- Can comment on pull requests in later versions.

### Team Server Mode

- API server plus workers.
- SQLite for small installs, Postgres for team installs.
- Optional object storage for artifacts.

## Success Criteria

The first usable version is successful if a developer can:

1. Run a CLI coding agent against a repository without exposing the host.
2. Restrict the agent to specific paths, domains, and commands.
3. Prevent direct access to real secrets.
4. Review every file change made by the agent.
5. See a task report that explains what happened.
