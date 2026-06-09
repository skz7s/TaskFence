# Publication Readiness

This document records the repository state that should be true before making
TaskFence public. It is an open-source publication checklist, not a production
support claim.

Current remote status observed from this checkout:

- Repository: `skz7s/TaskFence`
- Default branch: `main`
- GitHub visibility: private
- GitHub description: `Secure runtime and gateway for AI agent tasks`
- GitHub homepage URL: `https://github.com/skz7s/TaskFence#readme`
- GitHub topics observed: `agents`, `ai`, `gateway`, `policy`, `sandbox`
- GitHub issues: enabled
- GitHub projects: enabled
- GitHub wiki: disabled
- GitHub discussions: disabled
- GitHub vulnerability alerts: disabled when queried from this checkout
- GitHub branch protection for `main`: not confirmed; the GitHub API returned
  a private-repository plan/visibility restriction when queried from this
  checkout
- Issue template labels observed: `bug`, `enhancement`, `documentation`, and
  `question` all exist
- Supported public status after publication: local preview only

## Local Evidence Ready

Before publishing the repository, verify the current branch has passed:

```bash
bash deploy/manage.sh readiness
bash -n deploy/manage.sh
cargo fmt --all --check
cargo check --workspace --locked
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo test --workspace --locked
RUSTDOCFLAGS="-D warnings" cargo doc --workspace --no-deps --locked
cargo package --workspace --no-verify --locked
python3 scripts/governance/build_agents.py --check
python3 scripts/governance/check_codex_governance.py
```

The repository should also have:

- `README.md`, `docs/README.md`, `docs/installation.md`, and
  `docs/quickstart.md` aligned with the local-preview source-build path
- `CONTRIBUTING.md`, `CODE_OF_CONDUCT.md`, `SECURITY.md`, `SUPPORT.md`, and
  `CHANGELOG.md`
- GitHub Actions CI, Dependabot, pull request template, issue templates, and
  CODEOWNERS review routing
- `docs/security-model.md`, `docs/versioning.md`, `docs/testing.md`,
  `docs/supply-chain.md`, `docs/release.md`, and
  `docs/release-notes-template.md`
- crate metadata, crate-level rustdoc introductions, and package manifest
  inspection through the locked package gate
- generated governance checks passing and generated governance outputs in sync
- a high-confidence secret-shape scan with no real provider tokens, private
  keys, GitHub tokens, AWS access key ids, Slack tokens, or URL userinfo

Secret scanning should report only placeholder variable names, redaction tests,
or intentionally fake values. Do not print or commit matched secret values when
investigating a finding.

## External GitHub Settings

Before switching the repository to public, a maintainer should confirm GitHub
repository settings outside the worktree:

- GitHub private vulnerability reporting is enabled, or an equivalent private
  contact path exists in the repository profile
- branch protection or rulesets require the CI workflow on `main`
- GitHub secret scanning and push protection are enabled when available
- Dependabot alerts and security updates are enabled when available
- issue labels referenced by templates exist or are acceptable for GitHub to
  create on first use
- repository description, topics, homepage, and profile contact path match the
  local-preview status

These settings are external state; do not claim they are configured unless
verified in GitHub.

As of the latest local audit, the repository still needs external GitHub
settings review before publication because vulnerability alerts are disabled,
branch protection was not confirmable through the API while the repository
remained private, and private vulnerability reporting, secret scanning, push
protection, and Dependabot security updates still need maintainer confirmation.

## Publication Steps

After maintainer approval:

1. Push the working branch.
2. Open a pull request or fast-forward merge into `main` according to the
   repository policy.
3. Confirm the latest GitHub Actions run on `main` is green.
4. Confirm no secret scanning alerts or private-data findings block publication.
5. Switch repository visibility to public in GitHub.
6. Re-read `README.md`, `docs/installation.md`, `SECURITY.md`, and
   `docs/config/readiness-checklist.md` from the public URL.

Do not publish crates, tags, release binaries, containers, or package-manager
artifacts as part of the repository visibility switch unless the release
process explicitly approves those artifacts.

## Publication Boundary

Public repository visibility does not make these surfaces production supported:

- long-lived production API daemon
- deployed team server
- production Web UI
- production MCP server
- arbitrary HTTP proxy
- SDK/webhook connectors
- SSO provider integration
- object storage adapter
- Kubernetes, microVM, or managed-cloud live execution
- live replay of destructive or externally visible connector effects

Keep `docs/config/readiness-checklist.md` as the source of truth for local
preview, beta-candidate, and unsupported production surfaces.
