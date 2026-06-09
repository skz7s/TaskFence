# Examples

Examples are designed to separate deterministic local validation from surfaces
that require operator-provided infrastructure.

For field-level task YAML details, see
[Task File Reference](../docs/task-file-reference.md). For command syntax, see
[CLI Reference](../docs/cli-reference.md).

## Quick Matrix

| File | Purpose | Runs Without Docker | Needs Live Credentials |
| --- | --- | ---: | ---: |
| `task.yaml` | Local Docker runner plus deterministic gateway fixture | Partly | No |
| `github-rest-task.yaml` | Bounded live GitHub REST connector and local egress contract | Validate only | Yes for live calls |
| `enterprise-connectors-task.yaml` | Enterprise connector contracts and policy templates | Validate only | Yes for live calls |
| `remote-ssh-task.yaml` | Remote SSH runner capability contract | Validate only | SSH identity and remote host |
| `codex-cli-task.yaml` | Specialized Codex CLI adapter profile | Validate only | No for validation |

## Deterministic Local Path

These commands do not require Docker or live credentials:

```bash
cargo run -p taskfence-cli -- validate examples/task.yaml
cargo run -p taskfence-cli -- gateway call examples/task.yaml github read_issue --param number=1
```

The gateway call reads `examples/repo/fixtures/github.json`, writes structured
evidence under `examples/repo/.taskfence/tasks/local-demo/`, and does not send
network traffic.

## Validate All Example Task Files

```bash
cargo run -p taskfence-cli -- validate examples/task.yaml
cargo run -p taskfence-cli -- validate examples/github-rest-task.yaml
cargo run -p taskfence-cli -- validate examples/enterprise-connectors-task.yaml
cargo run -p taskfence-cli -- validate examples/remote-ssh-task.yaml
cargo run -p taskfence-cli -- validate examples/codex-cli-task.yaml
```

Validation resolves task files, checks planned commands against policy, and
builds runner plans. It does not start Docker, SSH, databases, or live
connectors.

## Docker Runner

`examples/task.yaml` can be executed through Docker when the daemon is running
and `debian:bookworm-slim` is already present locally:

```bash
docker image inspect debian:bookworm-slim
cargo run -p taskfence-cli -- run examples/task.yaml
```

The runner uses `--pull=never`; pre-pull or pre-load images explicitly.

## Live Connector Examples

Live connector examples keep raw credentials out of task files. Export
gateway-side secrets with the `TASKFENCE_GATEWAY_SECRET_` prefix only in the
operator environment that runs the gateway command.

Example:

```bash
export TASKFENCE_GATEWAY_SECRET_GITHUB_TOKEN=...
cargo run -p taskfence-cli -- gateway call examples/github-rest-task.yaml github read_issue --param number=1
```

Do not commit real repository names, private URLs, tokens, or captured live
connector output.
