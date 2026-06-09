# Quickstart

This guide gives new contributors a deterministic path that does not require
Docker, SSH, database services, live GitHub credentials, or provider tokens.

## Prerequisites

- Rust 1.88 or newer
- Git
- Python 3.11 or newer for governance checks

Optional surfaces:

- Docker plus a locally available `debian:bookworm-slim` image for
  `taskfence run examples/task.yaml`
- SSH identity, known-hosts file, and an operator-isolated remote workspace for
  the remote SSH example
- Gateway-side environment secrets for live connector examples

## First Successful Commands

Run from the repository root:

```bash
cargo check --workspace --locked
cargo run -p taskfence-cli -- validate examples/task.yaml
cargo run -p taskfence-cli -- gateway call examples/task.yaml github read_issue --param number=1
```

Expected result:

- validation reports `Task file valid`
- the fixture gateway call reports `Gateway call finished`
- evidence is written under
  `examples/repo/.taskfence/tasks/local-demo/`

The fixture gateway path uses `examples/repo/fixtures/github.json`. It does not
need Docker, does not call GitHub, and does not read a real token.

## Local Evidence Commands

After running the fixture gateway command, inspect the recorded evidence:

```bash
cargo run -p taskfence-cli -- tasks --workspace examples/repo
cargo run -p taskfence-cli -- task local-demo --workspace examples/repo
cargo run -p taskfence-cli -- events local-demo --workspace examples/repo
cargo run -p taskfence-cli -- report local-demo --workspace examples/repo
```

Generated evidence lives under ignored `.taskfence/` directories and should not
be committed.

## Docker Runner Demo

The Docker runner demo is intentionally offline at runtime. It uses
`docker run --pull=never`, so the image must already be present:

```bash
docker image inspect debian:bookworm-slim
cargo run -p taskfence-cli -- run examples/task.yaml
```

If the image or Docker daemon is unavailable, use validation and fixture gateway
commands instead of claiming Docker coverage.

## Full Local Gate

Before opening a pull request, run:

```bash
bash -n deploy/manage.sh
cargo fmt --all --check
cargo check --workspace --locked
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo test --workspace --locked
RUSTDOCFLAGS="-D warnings" cargo doc --workspace --no-deps --locked
python3 scripts/governance/build_agents.py --check
python3 scripts/governance/check_codex_governance.py
```

For focused crate checks, example validation, Docker integration prerequisites,
and live coverage reporting, see [docs/testing.md](testing.md).

The workspace test suite contains a Docker integration test that is ignored by
default because it requires a Docker daemon and a locally available test image.
Record that limitation in release notes or pull requests when Docker was not
available.
