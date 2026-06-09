# Testing Strategy

TaskFence's tests are organized around the security boundary: fast local tests
cover fail-closed behavior, typed contracts, evidence generation, and CLI
parsing; environment-backed tests are explicit because they need Docker,
credentials, databases, or remote hosts.

## Default Local Gate

Run the default local gate from the repository root:

```bash
cargo fmt --all --check
cargo check --workspace --locked
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo test --workspace --locked
RUSTDOCFLAGS="-D warnings" cargo doc --workspace --no-deps --locked
python3 scripts/governance/build_agents.py --check
python3 scripts/governance/check_codex_governance.py
```

GitHub Actions runs the Rust and governance surfaces above, plus shell syntax,
readiness output, and package manifest inspection with
`cargo package --workspace --no-verify --locked`. CI cancels superseded runs on
the same ref and uses job timeouts so public pull requests cannot consume
unbounded runner time.

The default workspace test run covers all non-ignored tests and doc-tests. It
does not prove Docker, database, live connector, remote host, or deployed
service behavior unless those environments are explicitly provisioned.

## Test Inventory

Current test coverage is concentrated in:

| Area | Location | Typical checks |
| --- | --- | --- |
| Agent adapters | `crates/taskfence-agent` | generic and specialized invocation construction without host-secret inheritance |
| Approval | `crates/taskfence-approval` | fail-closed, interactive, external, timeout, and local store behavior |
| Artifacts | `crates/taskfence-artifacts` | task layout, logs, diffs, baseline, and unsafe id rejection |
| Audit | `crates/taskfence-audit` | JSONL writing, redaction, truncation, and terminal-control sanitization |
| CLI | `crates/taskfence-cli` | command parsing, task validation, run behavior, gateway calls, local evidence, review, replay, approval, and team-state commands |
| Config | `crates/taskfence-config` | task-file parsing, unknown-field rejection, path validation, gateway connectors, remote SSH contracts, and invalid config rejection |
| Core | `crates/taskfence-core` | orchestration policy/approval outcomes and report/artifact failure behavior |
| Gateway | `crates/taskfence-gateway` | tool normalization, registry, policy, approval, budget, secret references, bounded connector adapters, egress, and spool requests |
| Policy | `crates/taskfence-policy` | deny precedence, approval precedence, default deny, budget checks, and policy language contract |
| Report | `crates/taskfence-report` | Markdown/compliance output from structured evidence and redaction |
| Runner | `crates/taskfence-runner` | Docker run planning, host-secret exclusion, mount safety, remote SSH planning, unsupported runner contracts, and fake runner behavior |
| State | `crates/taskfence-state` | local evidence reads, indexes, review data, replay plans, team RBAC/state, artifact containment, and audit export records |

Use `cargo test --workspace --locked -- --list` to inspect the current test
inventory before claiming coverage.

## Focused Local Checks

Use focused checks while iterating:

```bash
cargo test -p taskfence-config
cargo test -p taskfence-policy
cargo test -p taskfence-gateway
cargo test -p taskfence-runner
cargo test -p taskfence-cli
```

Pick the crate that owns the behavior:

- task-file schema or path resolution: `taskfence-config`
- allow/deny/approval/default-deny decisions: `taskfence-policy`
- gateway tool mediation, connector boundaries, secrets, budgets, and spool:
  `taskfence-gateway`
- Docker/SSH runner planning and mount/env/network safety: `taskfence-runner`
- user-facing commands and local evidence flows: `taskfence-cli`
- local/team state, replay planning, artifact routing, and review data:
  `taskfence-state`

Security-boundary changes should include both the owning crate tests and any
CLI or report tests needed to prove the user-visible evidence path.

## Example Validation

Validate maintained examples without Docker, SSH, databases, or live
credentials:

```bash
cargo run -p taskfence-cli -- validate examples/task.yaml
cargo run -p taskfence-cli -- validate examples/github-rest-task.yaml
cargo run -p taskfence-cli -- validate examples/enterprise-connectors-task.yaml
cargo run -p taskfence-cli -- validate examples/remote-ssh-task.yaml
cargo run -p taskfence-cli -- validate examples/codex-cli-task.yaml
```

Validation parses task files, checks planned command policy, and builds runner
plans. It does not start Docker, SSH, databases, or live connectors.

The deterministic fixture gateway path is also safe for normal local testing:

```bash
cargo run -p taskfence-cli -- gateway call examples/task.yaml github read_issue --param number=1
```

It reads `examples/repo/fixtures/github.json`, writes local evidence, and does
not send network traffic or read a real token.

## Docker Integration

The workspace contains one ignored Docker integration test:

```bash
cargo test -p taskfence-runner --test docker_integration -- --ignored
```

It requires:

- Docker CLI and daemon
- locally available `debian:bookworm-slim` image
- permission to run local containers

The test itself also skips with a message if Docker or the local image is
unavailable. The normal workspace suite intentionally leaves this test ignored.
When a change affects Docker execution behavior, run it on a suitable machine
or record the unavailable-Docker limitation in the pull request and release
notes.

## Live Environment Tests

Live connector, database, remote SSH, Kubernetes, microVM, managed-cloud, SSO,
object storage, production API daemon, production Web UI, and background audit
export tests are not part of the default CI gate.

Run live tests only when the operator explicitly provides the required
environment and credentials. Do not commit real credentials, private URLs,
customer data, or credential-bearing logs. Record exactly which live surfaces
were run and which were skipped.

## Documentation And Governance Checks

Documentation-only changes should still run:

```bash
git diff --check
python3 scripts/governance/build_agents.py --check
python3 scripts/governance/check_codex_governance.py
```

Rust public API or CLI doc-comment changes should also run:

```bash
RUSTDOCFLAGS="-D warnings" cargo doc --workspace --no-deps --locked
```

Generated governance outputs must stay source-owned through `governance/`.
Do not edit `AGENTS.md`, `.codex/skills/*`, or `governance/core/*` directly for
durable changes.

## Reporting Coverage

Pull requests and release notes should state:

- focused crate tests run
- workspace gate status
- example validation or fixture gateway checks run
- Docker integration status, including skipped reason when unavailable
- live connector, database, remote runner, or deployed service coverage, if
  any
- governance and rustdoc checks when docs, policy, or public APIs changed

Do not imply integration coverage from CI unless the workflow explicitly
provisions that integration environment.
