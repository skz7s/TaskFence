# Open Source Maturity Plan

## Goal

Continue improving TaskFence, including documentation, until the project is
ready to operate as a mature open-source project.

## Plan Source

User request on 2026-06-09:

> 继续优化项目，包括文档，将项目优化到一个成熟的开源项目的级别后，开源项目

Scope:

- improve repository documentation, collaboration surfaces, validation, and
  release readiness
- preserve TaskFence's current core feature boundary and avoid overclaiming
  unsupported production behavior
- keep generated governance artifacts source-owned through `governance/`
- use task-sized verified changes and commit coherent scopes

Non-goals for this plan:

- do not implement unsupported production daemon, Web UI, Kubernetes, microVM,
  managed-cloud, SSO, object storage, or arbitrary proxy behavior just to make
  docs read as complete
- do not rewrite the Rust crate boundaries or helper governance baseline unless
  a concrete maturity gap requires it
- do not push, merge, or publish releases without explicit operator approval

Acceptance criteria:

- public contributors can find setup, validation, contribution, security,
  support, release, and roadmap/status information
- repository automation checks formatting, linting, tests, governance drift,
  and readiness docs on normal pull requests
- package metadata is credible for Rust publication review, even if crates are
  not published yet
- readiness docs distinguish local preview, beta candidates, and unsupported
  production surfaces
- validation evidence is recorded before commits

## Snapshot

- Date: 2026-06-09
- Default branch: `origin/main`
- Working branch: `codex/governance-development-plan`
- `git pull --ff-only`: skipped because this local branch has no tracking
  upstream
- `git fetch --prune origin`: completed successfully
- Initial worktree status: clean
- Relevant observed gaps: no `.github` workflow/templates or dependency update
  automation; no `CONTRIBUTING.md`, `CODE_OF_CONDUCT.md`, `SECURITY.md`,
  `SUPPORT.md`, `CHANGELOG.md`, release guide, or maintainer guide; Cargo
  package metadata is minimal; README docs index does not expose
  collaboration/security/release surfaces; declared Rust 1.78 MSRV is lower
  than the locked dependency tree's effective Rust 1.88 requirement

## Phases

### 1. Open-Source Release Shell

- Status: done
- Scope: add public collaboration, conduct, security, support, changelog,
  release, and maintainer docs; add GitHub Actions CI, Dependabot, and issue/PR
  templates; update package metadata, MSRV, and readiness/docs links without
  changing runtime behavior
- Verification command: `bash -n deploy/manage.sh && cargo fmt --all --check
  && cargo check --workspace --locked && cargo clippy --workspace --all-targets
  --locked -- -D warnings && cargo test --workspace --locked && python3
  scripts/governance/build_agents.py --check && python3
  scripts/governance/check_codex_governance.py`
- Verification evidence: passed on 2026-06-09. Also ran `cargo metadata
  --no-deps --format-version 1`, `bash deploy/manage.sh readiness`, and checked
  dependency metadata for effective MSRV; the locked dependency tree requires
  Rust 1.88. Docker integration test remained ignored by the existing test
  annotation because it requires Docker daemon and a locally available test
  image.

### 2. Contributor Quickstart And Examples

- Status: done
- Scope: review README and examples for first-time contributor ergonomics,
  reduce ambiguity around Docker/image prerequisites, gateway secrets, and
  unsupported surfaces
- Verification command: targeted docs/link checks plus example validation
- Verification evidence: passed on 2026-06-09. Added no-Docker
  `docs/quickstart.md`, `examples/README.md`, README links, contributing
  guidance, and structure-doc ownership notes. Verified all example task files
  with `cargo run -p taskfence-cli -- validate ...`, verified the deterministic
  fixture gateway call with `cargo run -p taskfence-cli -- gateway call
  examples/task.yaml github read_issue --param number=1`, ran `python3
  scripts/governance/check_codex_governance.py`, and ran `git diff --check`.

### 3. Codebase Hardening Review

- Status: done
- Scope: audit high-risk runtime paths for avoidable panics, weak error
  messages, under-tested fail-closed behavior, and package publication gaps
- Verification command: targeted crate tests plus workspace gate when touched
- Verification evidence: passed on 2026-06-09. Scanned non-test Rust source for
  `unwrap(`, `expect(`, `panic!`, `todo!`, and `unimplemented!`; no production
  hits were found outside `#[cfg(test)]` modules. Ran clippy with additional
  unwrap/expect/panic warnings and confirmed the warnings are test-only. Added
  version requirements to internal workspace path dependencies so package
  manifests are valid. Verified with `cargo check --workspace --locked`,
  `cargo package --workspace --no-verify --allow-dirty`, `cargo package -p
  taskfence-core --allow-dirty`, and `git diff --check`. Full package
  verification for crates above `taskfence-core` remains gated on
  dependency-order publication to crates.io.

### 4. Release Candidate Pass

- Status: done
- Scope: run release/readiness gate, update final release notes, archive this
  plan when all phases are terminal, and prepare for operator-approved merge or
  push
- Verification command: full documented release gate
- Verification evidence: passed on 2026-06-09. Ran `bash deploy/manage.sh
  readiness`, `bash -n deploy/manage.sh`, `cargo fmt --all --check`, `cargo
  check --workspace --locked`, `cargo clippy --workspace --all-targets --locked
  -- -D warnings`, `cargo test --workspace --locked`, `cargo package
  --workspace --no-verify`, `python3 scripts/governance/build_agents.py
  --check`, `python3 scripts/governance/check_codex_governance.py`, and `git
  diff --check`. Docker integration test remained ignored because it requires a
  Docker daemon and a locally available test image. Full package verification
  above `taskfence-core` remains gated on dependency-order publication.

## Commit Plan

1. `docs: add open source release shell` - `3b1e4ad`
2. `docs: improve quickstart and examples` - `7260fbf`
3. `rust: harden package metadata` - `0eea433`
4. `docs: archive open source maturity plan` - `39c3cc1`

## Final Evidence

- Public collaboration and release shell added: contribution, conduct,
  security, support, changelog, release, maintainer, issue/PR templates,
  Dependabot, and GitHub Actions.
- First-contributor path added: no-Docker quickstart, examples matrix, README
  and contributing links.
- Package metadata hardened: Rust 1.88 MSRV, repository/homepage/docs metadata,
  crate descriptions, categories, keywords, readme inheritance, and versioned
  internal path dependencies.
- Final release gate passed locally on 2026-06-09 with the limitations recorded
  above.
