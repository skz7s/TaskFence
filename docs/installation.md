# Installation

TaskFence is currently a local preview project. There are no published
crates.io packages, release binaries, package-manager formulas, containers, or
hosted services for production use yet.

## Supported Preview Path

Use a source checkout with Rust 1.88 or newer:

```bash
git clone https://github.com/skz7s/TaskFence.git
cd TaskFence
cargo build --workspace --locked
cargo run -p taskfence-cli -- --help
```

For the no-Docker first run, continue with [Quickstart](quickstart.md).

## Local Binary From Source

Build the CLI binary locally:

```bash
cargo build -p taskfence-cli --locked
./target/debug/taskfence --help
```

For a release-mode local binary:

```bash
cargo build -p taskfence-cli --release --locked
./target/release/taskfence --help
```

Do not treat a local release build as a signed or provenance-tracked release
artifact. Published binary provenance is future work.

## Cargo Install Status

`cargo install taskfence-cli` is not supported until the internal TaskFence
crates are published to crates.io in dependency order. Before the first publish
wave, maintainers should follow [Supply-Chain Maintenance](supply-chain.md) and
[Release Process](release.md), including package metadata checks and skipped
coverage notes.

## Runtime Prerequisites

The no-Docker validation and fixture gateway path requires:

- Rust 1.88 or newer
- Git
- Python 3.11 or newer only when running governance checks

Optional surfaces require additional local dependencies:

- Docker plus a locally available task image for `taskfence run`
- SSH identity, known-hosts file, and an operator-isolated remote workspace for
  the remote SSH runner example
- Gateway-side environment secrets for live connector examples
- Postgres only for the Postgres-backed state or database connector surfaces

Dependency setup is reuse-first. `deploy/manage.sh detect-env` records local
tool facts in ignored `.codex-helper/local-env.toml`; it does not install
dependencies or rewrite package-manager configuration.

## Unsupported Distribution Paths

The following are not supported yet:

- crates.io installation
- Homebrew, apt, dnf, yum, npm, pip, or uv packages
- signed release binaries
- Docker images for production deployment
- long-lived production API daemon, team server, or hosted service deployment

Use [Readiness Checklist](config/readiness-checklist.md) before describing a
surface as beta or production ready.
