# TaskFence

TaskFence is an open-source secure runtime and gateway for AI agent tasks.

It helps teams run AI agents against real repositories, tools, and internal
systems with explicit permissions, sandboxed execution, audit trails, and
human approval for high-risk actions.

TaskFence is not another agent framework. It is the control layer around agents:
agents decide what to do, TaskFence decides what they are allowed to access,
records what happened, and creates evidence that can be reviewed later.

> Status: local runner plus bounded gateway connector foundation. The Rust
> workspace contains the first usable `taskfence run <task-file>` path for
> generic commands in a local Docker sandbox, with structured audit events,
> local artifacts, and Markdown reports. It also contains a CLI-owned
> `taskfence gateway call` path for configured local fixture tools and a
> bounded GitHub REST connector, plus a request/response gateway spool
> prototype for sandboxed agents. These paths prove policy, approval,
> registry, gateway-side secret brokering, audit, and report behavior.
> The local review foundation can render workspace evidence as a static or
> foreground-served HTML page, resolve workspace-local approvals from that
> page, and plan replay inputs from saved structured evidence. MCP/HTTP
> listener or proxy servers, SDK/webhook connectors, persistent API server,
> SQLite migration, deterministic replay execution, team-server, and
> enterprise surfaces remain future work.

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
implemented first. Gateway-enhanced execution is currently limited to a
deterministic local fixture command, a bounded GitHub REST connector for three
operations, executor-backed MCP/HTTP adapter entry points, configured tool
policy decisions, optional approval mediation, known-tool registry checks,
redacted secret references, structured evidence, and unsupported-action errors.

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

The current executable gateway surfaces are intentionally narrower than that
future mode. `taskfence gateway call` executes only configured task-file tools:
deterministic local fixtures, or a bounded GitHub REST connector for
`github.read_issue`, `github.create_pr`, and `github.comment_issue`.
`taskfence gateway spool process` processes one request from a task-local
`gateway-spool/requests` directory and writes one typed response under
`gateway-spool/responses`; the generated sandbox wrapper is only a thin
request writer over that spool. These paths do not start a production MCP
server, proxy arbitrary HTTP traffic, implement SDK/webhook connectors, create
branches or commits, or expose a raw token to the sandbox.

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
  budget:
    allow:
      - kind: "tokens"
        max_amount: 100000
```

See [examples/task.yaml](examples/task.yaml) for the runnable demo.

## Local Demo

Create a starter task file in the current directory with:

```bash
cargo run -p taskfence-cli -- init taskfence.yaml
```

The init command writes one local task YAML file and refuses to overwrite an
existing target.

Validate a task file before starting the local runner with:

```bash
cargo run -p taskfence-cli -- validate examples/task.yaml
```

Validation resolves the task file, checks the planned agent command against
policy, and builds the local Docker run plan without starting Docker or writing
task artifacts.

You can also exercise the deterministic GitHub-shaped local fixture gateway
without Docker or a real GitHub token:

```bash
cargo run -p taskfence-cli -- gateway call examples/task.yaml github read_issue --param number=1
```

That command reads `examples/repo/fixtures/github.json`, mediates
`github.read_issue` through the configured tool registry and policy, and writes
structured evidence under `examples/repo/.taskfence/tasks/local-demo/`.

Approval-required fixture actions fail closed unless approval is explicitly
selected. For the local deterministic demo, `--approve` resolves the approval
inside the CLI process and produces a PR-shaped proposal artifact instead of
creating a real pull request:

```bash
cargo run -p taskfence-cli -- gateway call examples/task.yaml github create_pr --approve --param title="Fixture proposal"
```

The configured `github_token` is only a gateway-side redacted reference for the
fixture. The fixture does not read a raw token, does not expose one to the
agent process, and does not send network traffic.

A task can opt into the bounded live GitHub REST connector with
`connector.type: github_rest`. That connector currently supports
`github.read_issue`, `github.create_pr`, and `github.comment_issue` only. Raw
tokens are read by the gateway-side secret broker from
`TASKFENCE_GATEWAY_SECRET_<NORMALIZED_SECRET_NAME>` after policy, registry, and
approval checks; the token is not written to task files, sandbox parameters,
audit events, reports, or artifacts. For the example secret name
`github_token`, set `TASKFENCE_GATEWAY_SECRET_GITHUB_TOKEN`.

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
cargo run -p taskfence-cli -- approvals --workspace examples/repo
cargo run -p taskfence-cli -- approval <approval-id> --workspace examples/repo
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
cargo run -p taskfence-cli -- tasks --workspace examples/repo
cargo run -p taskfence-cli -- task local-demo --workspace examples/repo
cargo run -p taskfence-cli -- inputs local-demo --workspace examples/repo
cargo run -p taskfence-cli -- artifacts local-demo --workspace examples/repo
cargo run -p taskfence-cli -- compare <left-task-id> <right-task-id> --workspace <workspace>
cargo run -p taskfence-cli -- status local-demo --workspace examples/repo
cargo run -p taskfence-cli -- events local-demo --workspace examples/repo
cargo run -p taskfence-cli -- diff local-demo --workspace examples/repo
cargo run -p taskfence-cli -- report local-demo --workspace examples/repo
cargo run -p taskfence-cli -- logs <task-id> --workspace <workspace>
```

You can generate a local review page from the same file-backed evidence:

```bash
cargo run -p taskfence-cli -- review --workspace examples/repo
cargo run -p taskfence-cli -- review --workspace examples/repo --serve --port 0
```

The static page is written to `.taskfence/review/index.html` by default. The
foreground server binds only to `127.0.0.1`, rebuilds the page from local
evidence on each request, and can approve or deny pending records from the
workspace-local approval queue. It is not a persistent API server or team
approval service.

Replay planning is currently inspection-only:

```bash
cargo run -p taskfence-cli -- replay plan local-demo --workspace examples/repo
```

The command reports saved task inputs, artifact paths, last status, blockers,
and determinism limits. It does not execute a replay.

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

1. A local `taskfence` CLI with starter task-file scaffolding and pre-run task
   validation.
2. Docker-based sandbox execution.
3. One black-box CLI agent adapter.
4. File, command, network, secret, and explicit budget restrictions.
5. Non-interactive fail-closed local approval records, opt-in local interactive
   approval during `run`, and explicit local external approval through
   workspace-scoped `approvals` / `approve` / `deny` commands.
6. Audit logs, denied-action records, stdout/stderr capture, and file diff
   artifacts.
7. Markdown task reports generated from structured evidence.
8. Local CLI lookup for workspace-local task lists, single task summaries,
   resolved task inputs, artifact manifests, structured task comparisons,
   latest task statuses, structured task event summaries, captured diffs,
   generated reports, captured stdout/stderr logs, and local approval
   records/details.
9. Task-file `permissions.budget.allow` parsing and built-in policy decisions
   for typed budget actions. Budget actions are denied by default unless a
   matching kind and `max_amount` are explicitly configured. Gateway adapters
   can now record planned or observed `BudgetUsageRecorded` audit evidence;
   the bounded `github_rest` connector records one planned `gateway_calls`
   usage before secret attachment or API execution.
10. A local fixture gateway call path for configured task-file tools, including
    GitHub-shaped `github.read_issue` and `github.create_pr` fixture behavior
    with fail-closed policy, explicit approval, redacted secret references,
    structured audit events, local artifacts, and Markdown report evidence.
11. A bounded live GitHub REST connector for configured `github_rest` task-file
    tools. It supports `github.read_issue`, `github.create_pr` against an
    existing `head`/`base`, and `github.comment_issue`; it uses gateway-side
    environment secrets only after policy, registry, approval, and planned
    `gateway_calls` budget checks.
12. An agent-facing gateway spool prototype: task artifact setup creates
    `gateway-spool/requests`, `gateway-spool/responses`, and a generated
    `taskfence-gateway-submit` wrapper; the Docker runner mounts only the
    dedicated spool path for tasks with configured gateway tools; the host-side
    `taskfence gateway spool process` command validates request paths, executes
    one mediated local fixture action, and writes typed success, denied,
    timeout, cancellation, malformed-request, unsupported-action, or failure
    responses with structured evidence.
13. Task-file `permissions.tools` parsing and policy/audit/report evidence for
    future gateway-mediated tool actions, including optional approval evidence,
    optional known-tool registry checks, redacted gateway secret references, and
    MCP/HTTP adapter requests that execute through the existing gateway
    executor when explicitly configured. This is not a production MCP server or
    HTTP proxy.
14. A local review and replay-planning foundation: `taskfence review` renders a
    static HTML task review page, `taskfence review --serve` exposes it through
    a foreground loopback-only server with workspace-local approval resolution,
    and `taskfence replay plan` reports saved replay inputs, blockers, and
    deterministic limits without executing replay.

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
