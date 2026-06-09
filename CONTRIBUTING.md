# Contributing To TaskFence

TaskFence is an open-source secure runtime and gateway for AI agent tasks. The
project is early, but its security boundary is deliberate: contributors should
prefer small, reviewable changes that preserve fail-closed behavior.

## Start Here

1. Read [README.md](README.md) for the product status.
2. Run the no-Docker path in [docs/quickstart.md](docs/quickstart.md).
3. Review [examples/README.md](examples/README.md) before changing examples or
   claiming integration coverage.
4. Read [docs/testing.md](docs/testing.md) before choosing validation scope or
   claiming integration coverage.
5. Read [docs/cli-reference.md](docs/cli-reference.md) and
   [docs/task-file-reference.md](docs/task-file-reference.md) before changing
   public commands or task YAML.
6. Read [docs/architecture.md](docs/architecture.md) and
   [docs/development-design.md](docs/development-design.md) before changing
   runtime boundaries.
7. Read [docs/security-model.md](docs/security-model.md) before changing
   policy, approval, sandbox, gateway, audit, artifact, or secret behavior.
8. Read [docs/versioning.md](docs/versioning.md) before changing MSRV,
   task-file contracts, CLI behavior, or structured evidence formats.
9. Read [docs/config/readiness-checklist.md](docs/config/readiness-checklist.md)
   before describing a surface as beta or production ready.
10. For governance or agent-rule changes, read
   [governance/change-map.md](governance/change-map.md).

## Development Setup

TaskFence uses a Rust workspace and currently targets Rust 1.88 or newer.

```bash
bash deploy/manage.sh detect-env
bash deploy/manage.sh doctor
cargo test --workspace --locked
```

The local Docker runner demo requires Docker and the task image to already be
available locally. The runner uses `--pull=never`, so demos do not silently
pull images at task runtime.

## Validation

Run the narrowest useful check while iterating. Before opening a pull request,
run the core gate. See [docs/testing.md](docs/testing.md) for focused crate
checks and integration prerequisites.

```bash
cargo fmt --all --check
cargo check --workspace --locked
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo test --workspace --locked
RUSTDOCFLAGS="-D warnings" cargo doc --workspace --no-deps --locked
```

For governance changes, also run:

```bash
python3 scripts/governance/build_agents.py --check
python3 scripts/governance/check_codex_governance.py
```

For shell script changes, run:

```bash
bash -n deploy/manage.sh
```

Dependency updates should also follow
[docs/supply-chain.md](docs/supply-chain.md), including MSRV review and release
note limitations for unavailable external audit tools.

## Security Expectations

- Fail closed when a control cannot be enforced.
- Unknown or unclassifiable actions must be denied.
- Explicit deny wins over approval and allow.
- Do not expose host secrets, Docker sockets, SSH agent sockets, package tokens,
  cloud credentials, or host home paths to the sandbox by default.
- Keep gateway credentials gateway-side whenever possible.
- Record behavior from structured evidence instead of scraped terminal output.

Report vulnerabilities through [SECURITY.md](SECURITY.md), not public issues.

## Pull Request Guidelines

- Keep changes scoped to one feature, fix, or documentation improvement.
- Include tests for runtime behavior, especially deny/approval/error branches.
- Update affected docs in the same change when commands, schema, examples,
  readiness status, or public behavior changes.
- Keep [docs/cli-reference.md](docs/cli-reference.md) and
  [docs/task-file-reference.md](docs/task-file-reference.md) aligned with
  `crates/taskfence-cli`, `crates/taskfence-config`, and examples.
- Do not claim unsupported gateway, Web UI, replay, team-server, runner, or
  enterprise behavior in docs or release notes.
- Preserve generated governance ownership. Long-lived changes to `AGENTS.md`,
  `.codex/skills/*`, or `governance/core/*` must be made in the owning
  governance source and rebuilt.

## Generated Governance

`AGENTS.md`, `.codex/skills/*`, and `governance/core/*` are
generated-but-committed. To change project-specific agent rules or skills:

1. Edit the owning source under `governance/private/*`.
2. Register new modules in `governance/modules.toml`.
3. Add private agent fragments to `governance/bundles.toml` when they should
   affect runtime rules.
4. Run:

```bash
python3 scripts/governance/build_agents.py
python3 scripts/governance/build_agents.py --check
python3 scripts/governance/check_codex_governance.py
```
