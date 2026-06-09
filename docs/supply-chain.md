# Supply-Chain Maintenance

TaskFence is a Rust workspace with committed lockfile-based validation and
GitHub Actions checks. This document defines current supply-chain expectations
without claiming unavailable tooling as mandatory.

## Current Mandatory Gates

Pull requests and release candidates should run:

```bash
cargo fmt --all --check
cargo check --workspace --locked
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo test --workspace --locked
RUSTDOCFLAGS="-D warnings" cargo doc --workspace --no-deps --locked
cargo package --workspace --no-verify
python3 scripts/governance/build_agents.py --check
python3 scripts/governance/check_codex_governance.py
```

The `--locked` checks ensure CI and local release gates use the committed
`Cargo.lock` dependency graph.

## Dependency Updates

Dependabot proposes Cargo and GitHub Actions dependency updates weekly. Review
dependency updates like code changes:

- inspect the changed package, version, and transitive impact
- run the core validation gate with `--locked`
- check whether the update changes the effective MSRV
- update docs when the supported Rust version or release gate changes
- note unavailable Docker, database, remote runner, or live connector coverage
  in the pull request or release notes

Do not add registry credentials, private mirrors, proxy URLs with credentials,
or package-manager tokens to docs, governance, task files, examples, or
`.codex-helper/local-env.toml`.

## External Audit Tools

`cargo-audit` and `cargo-deny` are useful release checks, but they are not
currently installed or configured as mandatory CI gates in this checkout.

When available locally, maintainers should run and record:

```bash
cargo audit
cargo deny check advisories bans licenses sources
```

If these tools are unavailable for a preview release, record the limitation in
release notes instead of implying the checks passed. Before beta or stable
support, add a reviewed `deny.toml` policy or an equivalent documented
advisory/license/source gate and wire it into CI.

## Package Publication Review

Before publishing crates:

- confirm each crate has license, repository, homepage, documentation, readme,
  description, keyword, category, and `rust-version` metadata
- run `cargo package -p taskfence-core` as the first full package verification
- use `cargo package --workspace --no-verify` before the first publish wave to
  inspect package manifests and included files without pretending unpublished
  internal crates already exist on crates.io
- publish internal crates in dependency order
- do not publish artifacts, push tags, or merge release branches without
  operator approval

## Source And Secret Hygiene

Release and security reviews must confirm:

- examples use placeholder or environment variable names, not real secrets
- gateway secret values remain gateway-side and redacted in evidence
- generated governance outputs are in sync with source-owned governance files
- local machine facts stay in ignored `.codex-helper/local-env.toml`
- dependency source facts with credentials are redacted before being recorded
- release notes identify skipped integration coverage accurately

## Future Hardening

Before a stable release, maintainers should add mandatory advisory, license,
source, and duplicate-version policy checks, decide whether signed releases are
required, and document artifact provenance for published binaries or packages.
