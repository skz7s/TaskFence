# Task File Reference

TaskFence task files are YAML documents parsed by `taskfence-config`. The
current task-file contract is preview-level; see
[docs/versioning.md](versioning.md) before changing accepted fields or
defaults.

Task files reject unknown fields in supported objects. Unsupported runner or
gateway families remain explicit fail-closed contracts unless documented as
implemented.

## Minimal Shape

```yaml
goal: "Create a demo file"
workspace: "./repo"

agent:
  type: "generic"
  command: "/usr/bin/true"

sandbox:
  type: "docker"
  image: "debian:bookworm-slim"

permissions:
  commands:
    allow:
      - "/usr/bin/true"
  network:
    default: "disabled"
```

`id` is optional. When omitted, the resolved task id defaults to `task`.

## Top-Level Fields

| Field | Required | Default | Notes |
| --- | ---: | --- | --- |
| `id` | No | `task` | Task id used for local evidence directories. |
| `goal` | Yes | none | Must not be empty. |
| `workspace` | Yes | none | Existing workspace path, relative to the task file or absolute. Must not contain `..`. |
| `agent` | Yes | none | Agent invocation configuration. |
| `sandbox` | Yes | none | Runner configuration. |
| `permissions` | No | deny-oriented defaults | Path, command, network, env, tool, and budget policy input. |
| `secrets` | No | no agent exposure, no gateway grants | Gateway-side secret references only. |
| `approval` | No | timeout 60 minutes | Approval policy metadata and timeout. |
| `gateway` | No | `spool_only`, no tools | Configured tool connectors and local egress mode. |
| `audit` | No | Markdown report and default capture | Report format and capture toggles. |

## Agent

```yaml
agent:
  type: "generic"
  command: "/usr/bin/true"
  args: []
```

| Field | Required | Default | Notes |
| --- | ---: | --- | --- |
| `type` | No | `generic` | `generic` or a specialized profile such as `codex_cli`, `claude_code`, `gemini_cli`, or `openhands`. Unknown values are preserved as specialized agent kinds. |
| `command` | Yes | none | Must not be empty. |
| `args` | No | `[]` | Argument array passed to the agent command. |

Specialized agent profiles add non-secret planning hints only. They do not
automatically grant host credentials or broaden sandbox access.

## Sandbox

```yaml
sandbox:
  type: "docker"
  image: "debian:bookworm-slim"
  limits:
    timeout_minutes: 5
    cpu: 1
    memory: "512m"
    disk: "1g"
```

| Field | Required | Default | Notes |
| --- | ---: | --- | --- |
| `type` | Yes | none | `docker`, `remote_ssh`, `kubernetes_job`, `microvm`, `managed_cloud`, or another unsupported name. Unsupported families fail closed when required controls are unavailable. |
| `image` | Docker tasks | none | Docker image name. The local runner uses `--pull=never`. |
| `limits.timeout_minutes` | No | none | Must be positive when set. |
| `limits.cpu` | No | none | Numeric CPU limit input for runner planning. |
| `limits.memory` | No | none | Memory limit string. |
| `limits.disk` | No | none | Disk limit string. |
| `ssh` | `remote_ssh` only | none | Required for `remote_ssh`; rejected for other sandbox types. |

Remote SSH configuration:

```yaml
sandbox:
  type: "remote_ssh"
  ssh:
    host: "runner.example"
    user: "taskfence"
    port: 22
    workspace: "/srv/taskfence/workspaces/demo"
    identity_file: "/tmp/taskfence/id_ed25519"
    known_hosts_file: "/tmp/taskfence/known_hosts"
    isolated_workspace: true
    isolated_secrets: true
    terminates_remote_processes: true
    enforces_resource_limits: false
    network_policy: "uncontrolled_allow"
```

`host` is required. `host` and `user` must be non-empty SSH segments without
control characters or `@`. `port`, when set, must be positive. `workspace`,
`identity_file`, and `known_hosts_file` must be absolute paths without `..` or
NUL. The only accepted `network_policy` value is `uncontrolled_allow`.

Generic SSH cannot enforce disabled/default-deny network access, domain
allowlists, local gateway mounts, or remote file diffs. Task shapes requiring
those controls fail closed.

## Permissions

```yaml
permissions:
  paths:
    read:
      - "./repo/README.md"
    write:
      - "./repo/src"
  commands:
    allow:
      - "/usr/bin/true"
    approval_required:
      - "git push"
    deny:
      - "sudo *"
  network:
    default: "disabled"
    allow_domains:
      - "api.github.com"
  env:
    allow:
      - "CI"
  tools:
    allow:
      - "github.read_issue"
    approval_required:
      - "github.create_pr"
    deny:
      - "github.delete_repo"
  budget:
    allow:
      - kind: "gateway_calls"
        max_amount: 8
```

Path permissions are resolved relative to the task file unless absolute. Paths
must stay inside `workspace`, must not contain `..`, and canonical existing
paths must not escape the workspace through symlinks. Missing paths are allowed
only when the normalized path is still inside the workspace.

Command and tool decisions use deny-over-approval-over-allow precedence. No
match means deny. Shell-wrapped commands require approval even when the wrapper
executable appears in allow rules.

Network `default` accepts `allow`, `deny`, or `disabled`; default is `deny`.
`allow_domains` entries are normalized to lowercase, trim a trailing dot, and
must not include `/`, `:`, `*`, empty labels, or wildcard syntax. Local Docker
does not enforce domain allowlists by itself; domain egress requires the local
gateway egress boundary described below.

Budget limits require non-empty `kind` and positive `max_amount`. Budget kinds
are normalized to lowercase. Gateway call budgeting currently uses
`gateway_calls`; examples also show `tokens` as a policy input.

## Secrets

```yaml
secrets:
  expose_to_agent: false
  available_to_gateway:
    - name: "github_token"
      use_for:
        - "github.read_issue"
```

`expose_to_agent: true` is rejected by the current parser because no explicit
high-risk override is implemented. Gateway secrets are grants by reference:
raw values are looked up from the operator environment by the gateway-side
secret broker and must not be placed in task files.

For a secret named `github_token`, the live gateway broker expects an
environment variable named `TASKFENCE_GATEWAY_SECRET_GITHUB_TOKEN`.

## Approval

```yaml
approval:
  require_for:
    - "git_push"
  timeout_minutes: 60
```

`timeout_minutes` defaults to `60` and must be positive when set.
Approval-required actions fail closed without an interactive, external, or
local approved approval flow.

## Gateway

```yaml
gateway:
  mode: "spool_only"
  egress:
    allow_domains: false
  tools:
    - protocol: "mcp"
      tool: "github"
      operation: "read_issue"
      connector:
        type: "local_fixture"
        kind: "github"
        path: "./repo/fixtures/github.json"
      secret_refs:
        - name: "github_token"
          parameter: "authorization"
          scope: "github.read_issue"
```

`gateway.mode` accepts `spool_only` or `local_listener`; default is
`spool_only`. `gateway.egress.allow_domains: true` requires
`gateway.mode: local_listener`.

`protocol`, `tool`, `operation`, `secret_refs.name`, and `secret_refs.scope`
are trimmed and normalized to lowercase. Secret reference `parameter` is
trimmed but preserves case. Empty values are rejected.

Connector types accepted by the parser:

| Type | Required fields | Notes |
| --- | --- | --- |
| `local_fixture` | `kind`, `path` | `path` must resolve inside the workspace and must not contain `..`. |
| `github_rest` | `repository`, optional `api_base` | `api_base` defaults to `https://api.github.com`. `repository` must be safe `owner/repo`. |
| `github_enterprise_rest` | `repository`, `api_base` | Uses the same bounded GitHub operation contract with explicit HTTPS API base. |
| `gitlab` | `api_base`, `project` | Connector contract with safe slash-separated project path. |
| `jira` | `api_base`, `project_key` | Connector contract with safe token value. |
| `feishu` | `api_base`, `app` | Connector contract with safe token value. |
| `wecom` | `api_base`, `corp_id` | Connector contract with safe token value. |
| `dingtalk` | `api_base`, `tenant` | Connector contract with safe token value. |
| `gitee` | `api_base`, `repository` | Connector contract with safe slash-separated repository path. |
| `coding` | `api_base`, `project` | Connector contract with safe slash-separated project path. |
| `database` | `engine`, `database_ref` | `database_ref` must be a non-secret reference, not an inline DSN or credential. |
| `internal_http` | `api_base`, `service` | Bounded internal HTTP connector contract. |
| `siem_export` | `api_base`, `sink` | Audit export connector contract. |
| `unsupported` | `kind` | Explicit unsupported connector marker. |

All `api_base` values must be non-empty safe HTTPS base URLs without userinfo,
query strings, fragments, or whitespace.

Current live connector support is intentionally bounded. See
[README.md](../README.md) and [docs/security-model.md](security-model.md) for
the supported and unsupported execution boundary.

## Audit

```yaml
audit:
  report:
    format: "markdown"
  capture:
    stdout: true
    stderr: true
    file_diff: true
    network_destinations: true
    approvals: true
```

`audit.report.format` accepts `markdown` or `html`; default is `markdown`.

Capture flags default to `true`:

- `stdout`
- `stderr`
- `file_diff`
- `network_destinations`
- `approvals`

Remote SSH tasks that cannot return remote file diffs must set
`audit.capture.file_diff: false`.

## Examples

See [examples/README.md](../examples/README.md) for the maintained example
matrix:

- `examples/task.yaml`: local Docker runner plus deterministic fixture gateway
- `examples/github-rest-task.yaml`: bounded GitHub REST connector and local
  egress contract
- `examples/enterprise-connectors-task.yaml`: enterprise connector contracts
- `examples/remote-ssh-task.yaml`: remote SSH runner capability contract
- `examples/codex-cli-task.yaml`: specialized Codex CLI adapter profile
