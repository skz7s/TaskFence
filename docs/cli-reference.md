# CLI Reference

This reference describes the implemented `taskfence` local-preview CLI. The CLI
is a preview contract; see [docs/versioning.md](versioning.md) before changing
documented commands or options.

Run commands from the repository root with:

```bash
cargo run -p taskfence-cli -- <command>
```

Installed binaries use the shorter form:

```bash
taskfence <command>
```

## Core Task Commands

| Command | Purpose |
| --- | --- |
| `taskfence init [PATH]` | Create a starter task file. Defaults to `taskfence.yaml` and refuses to overwrite an existing file. |
| `taskfence validate <TASK_FILE>` | Parse and validate a task file, evaluate the planned command policy, and build the runner plan without starting Docker, SSH, or live connectors. |
| `taskfence run [--interactive-approval | --external-approval] <TASK_FILE>` | Run the task orchestration boundary. `--interactive-approval` prompts in the running terminal. `--external-approval` waits for `taskfence approve` or `taskfence deny`. The two approval modes are mutually exclusive. |

The Docker runner uses `docker run --pull=never`; images must already be
available locally. Validation does not require Docker.

## Gateway Commands

| Command | Purpose |
| --- | --- |
| `taskfence gateway call [OPTIONS] <TASK_FILE> <TOOL> <OPERATION>` | Mediate and execute one configured gateway tool action. |
| `taskfence gateway listen [OPTIONS] <TASK_FILE>` | Start a foreground task-scoped loopback listener. |
| `taskfence gateway spool process [OPTIONS] <TASK_FILE> <REQUEST_FILE>` | Process one agent-facing gateway spool request file. |

`gateway call` options:

- `--protocol <PROTOCOL>`: gateway protocol shape, default `mcp`
- `--param <KEY=VALUE>`: plain parameter; values are redacted in summaries
- `--approve`: resolve approval-required calls with a local approved decision
- `--external-approval`: wait for `taskfence approve` or `taskfence deny`

`gateway listen` options:

- `--approve`: resolve approval-required listener calls with a local approved
  decision
- `--external-approval`: wait for `taskfence approve` or `taskfence deny`
- `--port <PORT>`: loopback port, default `0` for an OS-assigned port
- `--once`: stop after one request; without it the server runs in the
  foreground until interrupted

`gateway spool process` options:

- `--approve`: resolve approval-required spool calls with a local approved
  decision
- `--external-approval`: wait for `taskfence approve` or `taskfence deny`

`--approve` and `--external-approval` are mutually exclusive.

## Local Evidence Commands

All local evidence commands read from the workspace that owns `.taskfence/`.
The default workspace is `.`.

| Command | Purpose |
| --- | --- |
| `taskfence tasks [--workspace <WORKSPACE>]` | List locally recorded tasks. |
| `taskfence task <TASK_ID> [--workspace <WORKSPACE>]` | Show one task summary. |
| `taskfence inputs <TASK_ID> [--workspace <WORKSPACE>]` | Show the resolved task input saved for a run. |
| `taskfence artifacts <TASK_ID> [--workspace <WORKSPACE>]` | List saved evidence and artifact files. |
| `taskfence status <TASK_ID> [--workspace <WORKSPACE>]` | Show the latest task status. |
| `taskfence events <TASK_ID> [--workspace <WORKSPACE>]` | Show the structured event timeline. |
| `taskfence logs <TASK_ID> [--workspace <WORKSPACE>]` | Show captured stdout/stderr logs. |
| `taskfence diff <TASK_ID> [--workspace <WORKSPACE>]` | Show the captured diff artifact. |
| `taskfence report <TASK_ID> [--workspace <WORKSPACE>]` | Show or generate a task report. |
| `taskfence compare <LEFT_TASK_ID> <RIGHT_TASK_ID> [--workspace <WORKSPACE>]` | Compare two local task summaries. |
| `taskfence compliance <TASK_ID> [--workspace <WORKSPACE>] [--output <OUTPUT>]` | Render compliance evidence from structured local task events. |

Reports and compliance output are generated from structured evidence, not
scraped terminal output.

## Review, Replay, And State

| Command | Purpose |
| --- | --- |
| `taskfence review [--workspace <WORKSPACE>] [--output <OUTPUT>]` | Build a static local review page. |
| `taskfence review --serve [--workspace <WORKSPACE>] [--port <PORT>]` | Serve the local review page on `127.0.0.1` in the foreground. |
| `taskfence replay plan <TASK_ID> [--workspace <WORKSPACE>]` | Show replay inputs, blockers, and determinism limits. |
| `taskfence replay run <TASK_ID> [--workspace <WORKSPACE>] [--replay-id <REPLAY_ID>] [--accept-limitations]` | Execute a supported local replay from saved structured evidence. |
| `taskfence state index [--workspace <WORKSPACE>] [--read-only]` | Build or read the workspace-local structured state index. |

`review --serve` is a foreground loopback operator tool. It is not a production
API daemon or team approval service.

`replay run` fails closed for missing replay inputs, existing replay evidence
ids, live or contract-only gateway connector effects, foreground listener mode,
and network allow/default-allow requirements. Use `--accept-limitations` only
after reviewing the replay plan.

## Approval Commands

Approval commands read and write workspace-local approval records. The default
workspace is `.`.

| Command | Purpose |
| --- | --- |
| `taskfence approvals [--workspace <WORKSPACE>]` | List recorded approval requests. |
| `taskfence approval <APPROVAL_ID> [--workspace <WORKSPACE>]` | Show one approval request. |
| `taskfence approve <APPROVAL_ID> [--workspace <WORKSPACE>]` | Approve a pending request. |
| `taskfence deny <APPROVAL_ID> [--workspace <WORKSPACE>]` | Deny a pending request. |

Non-interactive approval-required actions fail closed unless a run or gateway
command explicitly selects an available approval flow.

## Team-State Commands

The team-state commands are local CLI/state-layer surfaces, not a deployed team
server.

| Command | Purpose |
| --- | --- |
| `taskfence team state [--state-file <STATE_FILE>] [--organization <ORGANIZATION>]` | Show or initialize durable local team state. |
| `taskfence team migrate-local [--workspace <WORKSPACE>] [--state-file <STATE_FILE>] [--organization <ORGANIZATION>] [--actor <ACTOR>]` | Import structured local evidence into team state. |
| `taskfence team audit-export --destination-ref <DESTINATION_REF> <TASK_ID> [OPTIONS]` | Export a registered task's structured audit events to a team-owned sink artifact. |
| `taskfence team worker enqueue <TASK_ID> [OPTIONS]` | Enqueue a task id for team execution. |
| `taskfence team worker lease --worker-id <WORKER_ID> [OPTIONS]` | Lease the next pending task for a worker. |
| `taskfence team worker complete --worker-id <WORKER_ID> <TASK_ID> [OPTIONS]` | Mark a leased task complete. |
| `taskfence team worker fail --worker-id <WORKER_ID> --reason <REASON> <TASK_ID> [OPTIONS]` | Mark a leased task failed. |

Common team options:

- `--state-file <STATE_FILE>`: default `.taskfence/team/state.json`
- `--organization <ORGANIZATION>`: default `default`
- `--actor <ACTOR>`: default varies by command, usually `operator` or
  `auditor`

`team audit-export` also accepts:

- `--sink-kind <SINK_KIND>`: `siem`, `webhook`, or `object-storage`; default
  `siem`
- `--destination-ref <DESTINATION_REF>`: required non-secret destination
  reference
- `--credential-env <CREDENTIAL_ENV>`: environment variable name for future
  deployment credentials, default `TASKFENCE_AUDIT_EXPORT_TOKEN`

## Exit Behavior

Commands return a non-zero exit code on configuration, policy, approval,
runner, gateway, state, artifact, or report errors. Unsupported or unsafe
configurations are expected to fail closed rather than falling back to a weaker
runtime path.
