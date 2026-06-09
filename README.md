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
> generic commands in a local Docker sandbox, plus a first live remote SSH
> runner that is available only when the task declares operator-provided remote
> isolation and accepts the SSH runner's unsupported controls. Both paths write
> structured audit events, local artifacts, and Markdown reports. It also
> contains a CLI-owned
> `taskfence gateway call` path for configured local fixture tools, bounded
> GitHub/GitHub Enterprise REST operations, and prioritized live enterprise
> connectors for GitLab, Jira, Feishu, WeCom, DingTalk, Gitee, CODING,
> Postgres database, internal HTTP, and SIEM export, plus a request/response
> gateway spool prototype for sandboxed agents. These paths prove policy,
> approval, registry, gateway-side secret brokering, audit, redaction, and
> unsupported-operation behavior without exposing raw credentials to the
> sandbox.
> The local review foundation persists a workspace-local structured index at
> `.taskfence/state/local-index.json`, can render workspace evidence as a
> static or foreground-served HTML page, exposes loopback-only JSON API routes
> for task evidence, comparisons, replay inputs, approvals, and contained
> artifact downloads while serving review, resolves workspace-local approvals
> from that page or API, and plans replay inputs from saved structured
> evidence. The team foundation now exposes persistent state-layer service
> functions and `taskfence team` local commands for organization-scoped task
> records, RBAC decisions, approval-owner checks, durable worker leases,
> local JSON state, a Postgres-backed state backend, artifact-root containment
> with SHA-256 metadata, team-owned audit export artifacts, compliance reports
> rendered from structured events, and local-to-team migration from structured
> evidence. Production MCP servers, arbitrary HTTP proxying, SDK/webhook
> connectors, long-lived persistent API daemon, replay for unsupported live
> connector effects, Kubernetes, microVM, or managed cloud runner execution,
> deployed team server daemon, SSO, Slack, and object storage adapters remain
> future work.

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
deterministic local fixture command, bounded GitHub/GitHub Enterprise REST
operations, prioritized live enterprise connectors, executor-backed MCP/HTTP
adapter entry points, configured tool policy decisions, optional approval
mediation, known-tool registry checks, redacted secret references, structured
evidence, and unsupported-action errors.

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

Local Docker does not enforce domain-level allowlists by itself. If a task
configures `permissions.network.allow_domains`, TaskFence fails closed unless
the task also opts into `gateway.mode: local_listener`,
`gateway.egress.allow_domains: true`, and a registered `http egress.fetch`
gateway tool. In that mode the Docker container still runs with
`--network none`; allowlisted egress is expected to go through the task-scoped
gateway boundary, where the destination host is checked by policy before the
gateway-side request is executed.

Task files can use `sandbox.type: remote_ssh` for the first live remote runner
backend. It executes the agent command through the host `ssh` executable with
`BatchMode=yes`, strict host-key checking, identity-file authentication, no SSH
agent forwarding, no host environment forwarding, and bounded stdout/stderr
capture. The SSH runner is available only when the task declares a remote
workspace, identity file, known-hosts file, `isolated_workspace: true`,
`isolated_secrets: true`, `terminates_remote_processes: true`,
`network_policy: uncontrolled_allow`, `permissions.network.default: allow`,
and `audit.capture.file_diff: false`. Generic SSH cannot enforce disabled or
default-deny network access, domain allowlists, local gateway spool/listener
mounts, host env allowlists, or remote file diff transport, so those
configurations fail closed instead of falling back to Docker. Kubernetes,
microVM, and managed cloud runner families remain typed contracts only.

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
deterministic local fixtures, a bounded GitHub REST connector for
`github.read_issue`, `github.create_branch`, `github.commit_file`,
`github.create_pr`, `github.update_pr`, `github.comment_issue`, and
`github.comment_report`, or prioritized live enterprise connectors for
GitLab/Jira issue workflows, Feishu/WeCom/DingTalk messages, Feishu docs,
Gitee/CODING issue and merge-request workflows, bounded Postgres database
reads/writes, internal HTTP calls, and SIEM event export.
`taskfence gateway spool process` processes one request from a task-local
`gateway-spool/requests` directory and writes one typed response under
`gateway-spool/responses`; the generated sandbox wrapper is only a thin
request writer over that spool. `taskfence gateway listen` starts a foreground
loopback listener for task-scoped JSON tool actions, and the bounded
`http egress.fetch` action can perform gateway-side GET/HEAD requests only for
policy-allowlisted HTTPS hosts. These paths do not start a production MCP
server, proxy arbitrary HTTP traffic, implement SDK/webhook connectors, or
expose a raw token to the sandbox.

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

See [examples/task.yaml](examples/task.yaml) for the runnable demo and
[examples/README.md](examples/README.md) for the example matrix.

## Local Demo

For a first pass that does not require Docker, SSH, database services, live
GitHub credentials, or provider tokens, follow [Quickstart](docs/quickstart.md).

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
policy, and builds the runner plan without starting Docker, SSH, or writing
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
`github.read_issue`, `github.create_branch`, `github.commit_file`,
`github.create_pr`, `github.update_pr`, `github.comment_issue`, and
`github.comment_report` only. Write operations require explicit tool policy
and remain approval-sensitive in the example. Branch names, refs,
repository-relative file paths, optional file SHAs, report URLs, and comment
sizes are bounded before any live API call. Raw tokens are read by the
gateway-side secret broker from
`TASKFENCE_GATEWAY_SECRET_<NORMALIZED_SECRET_NAME>` after policy, registry,
approval, and planned budget checks; the token is not written to task files,
sandbox parameters, audit events, reports, or artifacts. For the example secret
name `github_token`, set `TASKFENCE_GATEWAY_SECRET_GITHUB_TOKEN`.

GitHub Enterprise task tools can opt into
`connector.type: github_enterprise_rest` with an explicit HTTPS `api_base`; it
uses the same bounded operations and credential rules as `github_rest`. Other
enterprise connector types are configuration and policy-template contracts
only: `gitlab`, `jira`, `feishu`, `wecom`, `dingtalk`, `gitee`, `coding`,
`database`, `internal_http`, and `siem_export` remain fail-closed for live
execution while still proving registry, policy, approval, redacted
secret-reference, and unsupported-action evidence. See
`examples/enterprise-connectors-task.yaml`.

For local domain allowlists, the example GitHub REST task opts into the
task-scoped gateway egress boundary. The sandbox still has Docker networking
disabled; the configured `http egress.fetch` gateway action is the only local
path that may perform gateway-side GET/HEAD requests after host policy checks.
You can validate the configuration without Docker:

```bash
cargo run -p taskfence-cli -- validate examples/github-rest-task.yaml
```

You can also start the foreground local listener for configured tool calls:

```bash
cargo run -p taskfence-cli -- gateway listen examples/github-rest-task.yaml --once --port 0
```

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

You can also refresh a durable workspace-local state index that is derived from
structured task evidence, not rendered Markdown reports:

```bash
cargo run -p taskfence-cli -- state index --workspace examples/repo
cargo run -p taskfence-cli -- state index --workspace examples/repo --read-only
```

The index is written to `.taskfence/state/local-index.json` and records task
ids, status, goals, evidence paths, artifact counts, and warnings for local
review and future migration tooling.

You can generate a local review page from the same file-backed evidence:

```bash
cargo run -p taskfence-cli -- review --workspace examples/repo
cargo run -p taskfence-cli -- review --workspace examples/repo --serve --port 0
```

The static page is written to `.taskfence/review/index.html` by default. The
foreground server binds only to `127.0.0.1`, rebuilds the page from local
evidence on each request, refreshes the structured local index through
`/api/index`, exposes JSON routes for task lists, task detail, artifacts,
events, logs, diffs, reports, replay plans, comparisons, approvals, and
approval resolution, and serves only known evidence/artifact files through
contained `/artifact/<task-id>/<relative-path>` downloads. It is a foreground
loopback operator tool, not a long-lived persistent API daemon or team approval
service.

Replay planning shows whether saved structured evidence is eligible for local
execution:

```bash
cargo run -p taskfence-cli -- replay plan local-demo --workspace examples/repo
cargo run -p taskfence-cli -- replay run local-demo --workspace examples/repo --accept-limitations
```

`replay run` reuses the saved `task.resolved.json` through the same local
orchestrator, writes a new task evidence directory with a default
`<task-id>-replay` id, compares structured source/replay summaries, and writes
`artifacts/replay.json`. It fails closed for missing replay inputs, existing
replay evidence ids, live or contract-only gateway connector effects,
foreground listener mode, and network allow/default-allow requirements. Use
`--accept-limitations` when the plan records nondeterministic limits such as
runner image availability, re-requested approvals, or external state.

The team-state foundation is currently a Rust state-layer service and local
CLI surface, not a deployed daemon. It validates Postgres state configuration
from an environment variable name and schema, can connect a Postgres-backed
state store when that database is available, models RBAC and approval-owner
decisions, persists team task records and durable worker leases, bounds team
artifact writes to configured absolute roots with SHA-256 metadata, plans
validated audit exports, writes completed/failed audit export payload artifacts
from structured events, and imports structured `.taskfence/tasks` files into
team state. Rendered Markdown reports are explicitly treated as migration
artifacts rather than source-of-truth state. Local task execution remains
independent and does not require team state.

## Documentation

- [Documentation Index](docs/README.md)
- [Product Positioning](docs/positioning.md)
- [Requirements](docs/requirements.md)
- [Architecture](docs/architecture.md)
- [Roadmap](docs/roadmap.md)
- [Development Design](docs/development-design.md)
- [Quickstart](docs/quickstart.md)
- [CLI Reference](docs/cli-reference.md)
- [Task File Reference](docs/task-file-reference.md)
- [Testing Strategy](docs/testing.md)
- [Security Model](docs/security-model.md)
- [Versioning And Compatibility](docs/versioning.md)
- [Supply-Chain Maintenance](docs/supply-chain.md)
- [Readiness Checklist](docs/config/readiness-checklist.md)
- [Release Process](docs/release.md)
- [Maintainer Guide](docs/maintainers.md)
- [Decision Records](docs/decisions/README.md)

## Community

- [Contributing Guide](CONTRIBUTING.md)
- [Code Of Conduct](CODE_OF_CONDUCT.md)
- [Security Policy](SECURITY.md)
- [Support Policy](SUPPORT.md)
- [Changelog](CHANGELOG.md)

## Development

TaskFence uses a Rust workspace rooted at `Cargo.toml`. The workspace currently
targets Rust 1.88 or newer. See
[Versioning And Compatibility](docs/versioning.md) before changing MSRV,
task-file contracts, CLI behavior, or structured evidence shapes.

Core validation gates:

```bash
cargo fmt --all
cargo check --workspace --locked
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo test --workspace --locked
RUSTDOCFLAGS="-D warnings" cargo doc --workspace --no-deps --locked
```

Governance validation:

```bash
python3 scripts/governance/build_agents.py
python3 scripts/governance/build_agents.py --check
python3 scripts/governance/check_codex_governance.py
```

Release/readiness checklist:

```bash
bash deploy/manage.sh readiness
```

The checklist separates local preview, beta-candidate, and not-production
supported surfaces. It is read-only and does not start a daemon or install
dependencies.

GitHub Actions runs the Rust workspace gate, rustdoc generation with warnings
denied, generated-governance drift checks, shell syntax check, and readiness
output on pull requests. See [Testing Strategy](docs/testing.md) for focused
checks, example validation, Docker integration prerequisites, and live coverage
reporting. Docker, database, remote runner, and live connector integration
tests still require matching local services or credentials and must be called
out explicitly when skipped.
See [Supply-Chain Maintenance](docs/supply-chain.md) for dependency update and
external advisory-tool expectations.

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
    tools. It supports `github.read_issue`, `github.create_branch`,
    `github.commit_file`, `github.create_pr`, `github.update_pr`,
    `github.comment_issue`, and `github.comment_report`; it uses gateway-side
    environment secrets only after policy, registry, approval, and planned
    `gateway_calls` budget checks.
12. Enterprise connector foundations: `github_enterprise_rest` reuses the
    bounded GitHub REST adapter contract, while `gitlab`, `jira`, `feishu`,
    `wecom`, `dingtalk`, `gitee`, `coding`, `database`, `internal_http`, and
    `siem_export` provide opt-in live adapters with connector-specific policy
    templates, approval-sensitive operation sets, gateway-side credentials,
    bounded parameters, redacted results, and explicit unsupported-operation
    evidence.
13. An agent-facing gateway spool prototype: task artifact setup creates
    `gateway-spool/requests`, `gateway-spool/responses`, and a generated
    `taskfence-gateway-submit` wrapper; the Docker runner mounts only the
    dedicated spool path for tasks with configured gateway tools; the host-side
    `taskfence gateway spool process` command validates request paths, executes
    one mediated local fixture action, and writes typed success, denied,
    timeout, cancellation, malformed-request, unsupported-action, or failure
    responses with structured evidence.
14. Task-file `permissions.tools` parsing and policy/audit/report evidence for
    future gateway-mediated tool actions, including optional approval evidence,
    optional known-tool registry checks, redacted gateway secret references, and
    MCP/HTTP adapter requests that execute through the existing gateway
    executor when explicitly configured. This is not a production MCP server or
    HTTP proxy.
15. A local state, review, and replay-planning foundation: `taskfence state
    index` persists `.taskfence/state/local-index.json` from structured task
    evidence, `taskfence review` renders a static HTML task review page,
    `taskfence review --serve` exposes it through a foreground loopback-only
    server with structured JSON API routes, contained artifact downloads, and
    workspace-local approval resolution, `taskfence replay plan` reports saved
    replay inputs, blockers, and deterministic limits, and `taskfence replay
    run` executes supported local replays into a new task evidence directory
    with structured evaluation evidence.
16. Expanded runner contracts for `remote_ssh`, `kubernetes_job`, `microvm`,
    and `managed_cloud` sandbox families. Remote SSH is the first live remote
    backend, gated by explicit filesystem/secret isolation declarations,
    identity and known-hosts files, default-allow network policy, timeout
    termination support, stdout/stderr capture, and a local-report-only
    artifact return path. Kubernetes, microVM, and managed cloud families still
    provide typed capability reports and fail-closed validation when required
    controls are unavailable.
17. A persistent team-state foundation in `taskfence-state` and `taskfence
    team`: typed team API resource boundaries, role-based access decisions,
    approval-owner enforcement, durable local JSON and Postgres state backends,
    worker lease storage with duplicate, wrong-worker, unleased, and terminal
    state protections, team artifact root containment checks with SHA-256
    metadata, completed/failed audit-export records, contained export payload
    artifacts, and migration from structured local `.taskfence` evidence
    without treating rendered reports as source of truth. A deployed team HTTP
    daemon, SSO, object-store adapter, and background export service are still
    not implemented.
18. Specialized coding-agent adapter profiles for `codex_cli`, `claude_code`,
    `gemini_cli`, and `openhands`. They build runner invocations with
    non-secret profile, prompt, workspace, and gateway-mode hints while keeping
    `generic` as the default adapter. The agent crate also exposes
    conservative coding-agent policy templates for explicit task-file policy;
    they are guidance and are not applied automatically. `deploy/manage.sh
    setup`, `dev`, `build`, and `doctor` now expose Rust-workspace-oriented
    operator flows without adding a deployed service.
19. Production-readiness contracts for the next wave: a combined local/team API
    daemon boundary with health/readiness and structured diagnostics, gateway
    transport hardening priority for MCP then bounded HTTP then SDK/webhook
    surfaces, production review UI workflow prerequisites, runner expansion
    contracts for Kubernetes/microVM/managed-cloud, team service/worker/SSO/
    object-storage/quota prerequisites, connector-effect replay rules, and
    policy/schema versioning. These are contract and readiness surfaces; they
    do not make the unsupported production daemon, Web UI, arbitrary HTTP
    proxy, deployed team service, or new runner backends live.

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
