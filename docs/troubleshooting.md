# Troubleshooting

This guide helps local-preview users diagnose common TaskFence setup,
validation, and demo failures. It does not replace the release gate or expand
production support.

Start with the supported source-build path in [Installation](installation.md)
and the no-Docker flow in [Quickstart](quickstart.md). Use
[Testing Strategy](testing.md) when deciding whether a failure is in the core
workspace gate or an optional integration surface.

## Environment Checks

Refresh local machine facts before diagnosing toolchain problems:

```bash
bash deploy/manage.sh detect-env
bash deploy/manage.sh doctor
```

The detector writes ignored machine-local facts to
`.codex-helper/local-env.toml`. Do not commit that file and do not put local
tool paths, mirrors, proxy URLs with credentials, or secrets into stable docs.

TaskFence currently expects Rust 1.88 or newer:

```bash
rustc --version
cargo check --workspace --locked
```

If `cargo check --workspace --locked` reports that the lockfile is stale, update
the dependency graph deliberately and follow [Supply-Chain Maintenance](supply-chain.md).
If it reports that the Rust compiler is too old, upgrade Rust before changing
TaskFence code or dependency versions.

## Validation Fails Before Docker Starts

`taskfence validate <task-file>` parses the task file, resolves paths, evaluates
the planned command, and builds the runner plan without starting Docker, SSH, or
writing task artifacts.

```bash
cargo run -p taskfence-cli -- validate examples/task.yaml
```

Common validation failures:

- `agent.command must not be empty`: set `agent.command` to the executable path
  and put arguments in `agent.args`.
- path escape or outside-workspace errors: keep read and write roots inside the
  declared `workspace`; do not use `..` to reach host paths.
- denied command decisions: add only the exact executable or controlled command
  shape that the task should run; do not broaden policy to shell wrappers just
  to make validation pass.
- unsupported domain allowlists: local Docker cannot enforce domain-level
  network rules by itself. Use the documented task-scoped gateway egress path
  or keep networking disabled.
- unknown fields: remove the field or update the schema and docs in the same
  change.

Use [Task File Reference](task-file-reference.md) for the current preview
schema and fail-closed behavior.

## Docker Runner Problems

The Docker runner demo requires a Docker daemon and a locally available task
image. TaskFence uses `docker run --pull=never`, so it will not silently pull
images at task runtime.

Check Docker separately:

```bash
docker version
docker image inspect debian:bookworm-slim
```

If the image is missing, acquire it outside the task run according to your local
operator policy. If Docker is unavailable, use the no-Docker quickstart and
record Docker integration coverage as skipped in release or pull request notes.

Do not work around Docker failures by mounting the host home directory, Docker
socket, SSH agent socket, cloud credentials, or package-manager tokens into the
sandbox. Those are security-boundary changes, not troubleshooting shortcuts.

## Approval Fail-Closed Behavior

Approval-required actions fail closed by default in non-interactive runs. That
is expected.

Use one explicit mode when a local demo needs approval:

```bash
cargo run -p taskfence-cli -- run --interactive-approval examples/task.yaml
cargo run -p taskfence-cli -- run --external-approval examples/task.yaml
```

For external approval mode, list and resolve workspace-local records from
another terminal:

```bash
cargo run -p taskfence-cli -- approvals --workspace examples/repo
cargo run -p taskfence-cli -- approve <approval-id> --workspace examples/repo
cargo run -p taskfence-cli -- deny <approval-id> --workspace examples/repo
```

If an approval times out, is denied, or is missing in a non-interactive path,
treat the denied task result as correct fail-closed behavior.

## Gateway Connector Problems

The deterministic fixture gateway path does not need a live GitHub token:

```bash
cargo run -p taskfence-cli -- gateway call examples/task.yaml github read_issue --param number=1
```

Live connector examples require gateway-side environment secrets with the
documented variable names. For the example secret name `github_token`, the
runtime reads `TASKFENCE_GATEWAY_SECRET_GITHUB_TOKEN`.

Do not put raw token values in task YAML, examples, docs, command output, or
issue reports. If a live connector reports `SecretUnavailable`, confirm that
the environment variable name is set in the process environment and that the
task's gateway secret scope allows the requested action.

Unsupported connector operations should return structured unsupported-action
evidence. Do not treat that as a generic gateway failure unless the docs claim
the operation is implemented.

## Remote SSH Runner Problems

The remote SSH runner is available only under its explicit capability contract.
Validation fails closed unless the task declares an operator-isolated remote
workspace, an identity file, a known-hosts file, and the SSH runner's unsupported
control acknowledgements.

Check SSH access outside TaskFence first:

```bash
ssh -i <identity-file> -o BatchMode=yes -o IdentitiesOnly=yes <user>@<host> true
```

Do not enable SSH agent forwarding, host environment forwarding, host-home
mounts, or uncontrolled secret access to make a remote task pass. If the remote
environment cannot provide the required isolation contract, use the local
Docker path or keep the task in validation-only mode.

## Artifacts And Local State

Successful and failed runs write evidence under the task workspace:

```text
.taskfence/tasks/<task-id>/
```

Useful inspection commands:

```bash
cargo run -p taskfence-cli -- tasks --workspace examples/repo
cargo run -p taskfence-cli -- task local-demo --workspace examples/repo
cargo run -p taskfence-cli -- events local-demo --workspace examples/repo
cargo run -p taskfence-cli -- report local-demo --workspace examples/repo
```

If evidence is missing, confirm that the task reached artifact setup. Policy
denials and approval denials can still write evidence when the artifact
directory can be created; malformed task files that fail before resolution may
not have a task artifact directory.

## Governance And CI Failures

Generated governance outputs must remain in sync with source-owned governance
files:

```bash
python3 scripts/governance/build_agents.py --check
python3 scripts/governance/check_codex_governance.py
```

If these fail after changing generated files such as `AGENTS.md`,
`.codex/skills/*`, or `governance/core/*`, move the durable change to the
owning governance source before rebuilding.

For CI parity, run:

```bash
cargo fmt --all --check
cargo check --workspace --locked
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo test --workspace --locked
RUSTDOCFLAGS="-D warnings" cargo doc --workspace --no-deps --locked
cargo package --workspace --no-verify --locked
```

Docker, database, remote runner, and live connector integration coverage still
requires matching local services or credentials and should be recorded as
skipped when unavailable.

## Reporting Problems

Use [Support](../SUPPORT.md) for ordinary setup, validation, and documentation
questions. Use [Security Policy](../SECURITY.md) for vulnerabilities, sandbox
escape risks, approval bypasses, credential exposure, or audit integrity
issues.

Issue reports should include commands run, TaskFence commit or version, Rust
version, operating system, and whether Docker, SSH, database, or live connector
credentials were involved. Do not include raw secrets, private repository URLs,
customer data, or credential-bearing logs.
