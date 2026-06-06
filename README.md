# TaskFence

TaskFence is an open-source secure runtime and gateway for AI agent tasks.

It helps teams run AI agents against real repositories, tools, and internal
systems with explicit permissions, sandboxed execution, audit trails, and
human approval for high-risk actions.

TaskFence is not another agent framework. It is the control layer around agents:
agents decide what to do, TaskFence decides what they are allowed to access,
records what happened, and creates evidence that can be reviewed later.

> Status: local runner implementation. The Rust workspace now contains the
> first usable `taskfence run <task-file>` path for generic commands in a local
> Docker sandbox, with structured audit events, local artifacts, and Markdown
> reports. Task files can configure tool allow/approval/deny policy for typed
> gateway mediation evidence, including optional approval request/resolution
> records for approval-required tool calls, but real gateway/tool execution,
> Web UI, replay, team-server, and enterprise surfaces remain future work.

## Why TaskFence

AI agents are starting to move from chat into real execution. They can edit
files, run shell commands, call APIs, access internal tools, create pull
requests, update tickets, send messages, and query databases.

That creates a practical problem:

- If an agent runs freely on a developer machine, it can access too much.
- If an agent holds real secrets, it can bypass governance.
- If tool calls are not mediated, dangerous actions cannot be approved first.
- If logs are fragmented, incidents cannot be reconstructed.
- If every agent has its own safety model, organizations cannot apply one
  consistent policy.

TaskFence exists to make agent execution bounded, inspectable, and repeatable.

## Positioning

TaskFence combines several capabilities into one task-level execution path:

- **Sandbox runner**: run black-box CLI agents in isolated environments.
- **Tool gateway**: proxy MCP, HTTP, CLI, and internal tool calls through policy.
- **Policy engine**: enforce path, command, network, secret, and tool rules.
- **Approval engine**: pause high-risk actions for human review.
- **Audit layer**: record prompts, tool calls, commands, file diffs, network
  access, approvals, costs, and task artifacts.
- **Evaluation layer**: replay or compare agent runs across models, agents, and
  policy versions.

The first wedge is secure execution for coding agents. The broader direction is
an enterprise AI agent gateway for model access, tool permissions, audit, and
execution isolation.

## Execution Modes

TaskFence is designed around two complementary modes. The local Docker runner is
implemented first; gateway-enhanced execution is intentionally still limited to
typed contracts, configured tool policy decisions, optional approval mediation,
structured evidence, and unsupported-action errors.

### 1. Generic Sandbox Mode

Run any CLI agent as a black-box process inside a controlled environment.

This mode does not require the agent to integrate with TaskFence.

The current local runner can enforce:

- mounted workspace boundaries
- read/write path restrictions
- secret isolation
- network disabled/default-deny/default-allow modes
- CPU, memory, disk, time, and budget limits
- terminal logs
- file diffs
- task reports

Local Docker does not enforce domain-level allowlists. If a task configures
`permissions.network.allow_domains`, TaskFence fails closed until an enforcing
proxy or gateway-backed network path is implemented.

This mode answers the basic question:

> Can I run this agent without giving it my whole machine, home directory,
> credentials, and unrestricted network access?

### 2. Gateway-Enhanced Mode

Agents that can use MCP, HTTP tools, SDKs, or CLI wrappers are expected to call
tools through TaskFence instead of calling external systems directly.

This future mode enables finer control:

- tool-level permissions
- parameter inspection
- secret brokering
- sensitive data redaction
- semantic approvals
- structured audit logs
- tool-specific policy templates

This mode answers the stronger question:

> Can I understand and approve the agent's high-risk actions before they reach
> GitHub, Slack, Feishu, a database, or another internal system?

## Example Task

```yaml
goal: "Create a demo file inside the allowed workspace path"
workspace: "./repo"

agent:
  type: "generic"
  command: "/usr/bin/touch"
  args:
    - "/workspace/src/taskfence-demo.txt"

sandbox:
  type: "docker"
  image: "debian:bookworm-slim"

permissions:
  paths:
    read:
      - "./repo/README.md"
    write:
      - "./repo/src"
  commands:
    allow:
      - "/usr/bin/touch"
  network:
    default: "disabled"
  tools:
    allow:
      - "github.read_issue"
    approval_required:
      - "github.create_pr"
    deny:
      - "github.delete_repo"
```

See [examples/task.yaml](examples/task.yaml) for the runnable demo.

## Local Demo

The demo requires Docker and the configured image to be available locally. The
runner uses `docker run --pull=never`, so it does not silently acquire images at
task runtime.

```bash
cargo run -p taskfence-cli -- run examples/task.yaml
```

Denied and approval-denied runs stop before the agent process starts, but still
write local evidence and a report when the artifact directory can be created.
Approval-required actions fail closed by default for non-interactive runs. To
approve or deny those actions in the running terminal session, opt in explicitly:

```bash
cargo run -p taskfence-cli -- run --interactive-approval examples/task.yaml
```

To let another local terminal resolve approval-required actions, opt into the
workspace-local file-backed approval queue:

```bash
cargo run -p taskfence-cli -- run --external-approval examples/task.yaml
cargo run -p taskfence-cli -- approve <approval-id> --workspace examples/repo
cargo run -p taskfence-cli -- deny <approval-id> --workspace examples/repo
```

On success, TaskFence writes artifacts under
`examples/repo/.taskfence/tasks/local-demo/`, including:

- `task.resolved.json`
- `events.jsonl`
- `stdout.log` and `stderr.log` when the agent emits captured output
- `diff.patch`
- `report.md`

You can read local task evidence from the workspace that owns the `.taskfence`
directory:

```bash
cargo run -p taskfence-cli -- report local-demo --workspace examples/repo
cargo run -p taskfence-cli -- logs <task-id> --workspace <workspace>
```

## Documentation

- [Product Positioning](docs/positioning.md)
- [Requirements](docs/requirements.md)
- [Architecture](docs/architecture.md)
- [Roadmap](docs/roadmap.md)
- [Development Design](docs/development-design.md)

## Development

TaskFence uses a Rust workspace rooted at `Cargo.toml`. The workspace currently
targets Rust 1.78 or newer.

Core validation gates:

```bash
cargo fmt --all
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```

Governance validation:

```bash
python3 scripts/governance/build_agents.py
python3 scripts/governance/build_agents.py --check
python3 scripts/governance/check_codex_governance.py
```

## Initial Scope

The first implementation currently includes:

1. A local `taskfence` CLI.
2. Docker-based sandbox execution.
3. One black-box CLI agent adapter.
4. File, command, network, and secret restrictions.
5. Non-interactive fail-closed local approval records, opt-in local interactive
   approval during `run`, and explicit local external approval through
   workspace-scoped `approve` / `deny` commands.
6. Audit logs, denied-action records, stdout/stderr capture, and file diff
   artifacts.
7. Markdown task reports generated from structured evidence.
8. Local CLI lookup for generated reports and captured stdout/stderr logs.
9. Task-file `permissions.tools` parsing and policy/audit/report evidence for
   future gateway-mediated tool actions, including optional approval evidence,
   without real tool execution.

## Non-Goals

TaskFence does not aim to:

- build a new general-purpose AI agent
- replace existing coding agents
- replace LLM gateways such as LiteLLM
- replace observability tools such as Langfuse or Phoenix
- guarantee safety for agents running outside the TaskFence runtime
- inspect encrypted traffic without an explicit proxy or tool integration

## License

Apache License 2.0. See [LICENSE](LICENSE).
