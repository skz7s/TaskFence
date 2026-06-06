# Product Positioning

TaskFence is an open-source secure runtime and gateway for AI agent tasks.

It sits between agents and the real systems they act on: files, shell commands,
networks, secrets, MCP tools, internal APIs, SaaS products, repositories, issue
trackers, databases, and business systems.

## One-Line Positioning

TaskFence lets teams run AI agents with bounded permissions, isolated execution,
human approval, audit trails, and reproducible task reports.

## What It Is

TaskFence is:

- a secure runtime for black-box CLI agents
- a gateway for agent tool calls
- a task-level policy enforcement layer
- an audit and approval system for high-risk actions
- a foundation for evaluating agent behavior across runs

## What It Is Not

TaskFence is not:

- an agent brain
- a prompt framework
- a general chat product
- a model provider
- a model-only API gateway
- a dashboard that only observes after the fact

The core promise is enforcement, not just visibility.

## Market Thesis

Organizations are adopting LLMs and AI agents in stages:

1. Model access: teams need routing, cost control, logging, and data policies.
2. Tool access: agents call internal systems, SaaS APIs, and MCP servers.
3. Real execution: agents edit files, run commands, open PRs, and operate
   workflows.

Most existing tools cover one layer:

- LLM gateways manage model calls.
- MCP gateways manage tool calls.
- Sandboxes isolate execution.
- Observability products trace behavior.
- Human-in-the-loop tools approve selected actions.

TaskFence combines these concerns around a single unit: the task.

## Target Users

### Individual Developers

Developers who want to run coding agents without exposing their whole machine,
home directory, credentials, and network.

### Engineering Teams

Teams that want to adopt coding agents, issue-fixing bots, dependency update
agents, or CI repair agents while keeping review and audit boundaries.

### Platform Teams

Teams building internal AI agent platforms that need one policy and runner layer
across multiple agents.

### Security Teams

Teams responsible for data leakage, secret exposure, least privilege,
production access, and auditability.

## Initial Wedge

The first wedge is secure execution for coding agents:

- run Codex, Claude Code, Gemini CLI, OpenHands, OpenClaw, Hermes, or internal
  CLI agents in a sandbox
- restrict files, commands, network, and secrets
- record diffs, logs, approvals, and reports

This wedge is concrete, testable, and useful for developers.

The broader product can expand into enterprise agent gateways:

- model access governance
- MCP and internal tool permissioning
- approval workflows
- enterprise connectors
- self-hosted control plane
- team-level policy and audit

## Differentiation

TaskFence should not compete by claiming to be the smartest agent.

It should compete by making agent execution safe enough to run in real
workspaces and enterprise systems.

Key differentiators:

- works with black-box CLI agents without mandatory adaptation
- adds deeper control when agents use the TaskFence gateway
- treats the task as the audit and policy unit
- joins sandboxing, policy, approval, and audit into one execution flow
- is local-first and self-hostable

## Positioning Statement

For teams that want to use AI agents against real code and internal systems,
TaskFence is an open-source secure runtime and gateway that isolates execution,
enforces least-privilege policies, mediates tool calls, records audit evidence,
and enables human approval for risky actions.

Unlike agent frameworks or observability dashboards, TaskFence controls what the
agent can access before actions reach the outside world.
