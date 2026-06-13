# Publication Readiness

This document records the repository state that should be true before making
TaskFence public. It is an open-source publication checklist, not a production
support claim.

Current remote status observed from this checkout:

- Repository: `skz7s/TaskFence`
- Default branch: `main`
- GitHub visibility: public
- GitHub description: `Secure runtime and gateway for AI agent tasks`
- GitHub homepage URL: `https://github.com/skz7s/TaskFence#readme`
- GitHub topics observed: `agents`, `ai`, `gateway`, `policy`, `sandbox`
- GitHub issues: enabled
- GitHub projects: enabled
- GitHub wiki: disabled
- GitHub discussions: disabled
- GitHub Actions workflows on `main`: CI observed and green for commit
  `ba2f73a959216d6edb773522b5384ef7d6d9f006`; run `27482469795` completed
  successfully with `Minimum supported Rust`, `Rust workspace`, and
  `Governance and readiness` jobs
- GitHub vulnerability alerts: enabled; the API returned `204 No Content` when
  queried from this checkout
- GitHub Dependabot alerts: enabled; the open-alert query returned `200 OK`
  from this checkout
- GitHub Dependabot automated security fixes: enabled; the API returned
  `200 OK` when queried from this checkout
- GitHub secret scanning: not confirmed; the secret scanning alerts API
  returned `404 Not Found` when queried from this checkout after publication
- GitHub private vulnerability reporting: enabled; the API returned `200 OK`
  when queried from this checkout after publication
- GitHub branch protection for `main`: enabled; required status checks are
  strict and require `Minimum supported Rust`, `Rust workspace`, and
  `Governance and readiness`; admins are enforced, linear history is required,
  force pushes and deletions are disabled, and conversation resolution is
  required
- GitHub repository rulesets: none observed; the rulesets API returned an empty
  list when queried from this checkout after publication
- Issue template and Dependabot labels observed: `bug`, `enhancement`,
  `documentation`, `question`, and `dependencies` all exist
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
python3 scripts/docs/check_markdown_links.py
```

The repository should also have:

- `README.md`, `docs/README.md`, `docs/installation.md`,
  `docs/quickstart.md`, `docs/troubleshooting.md`, and
  `docs/community-triage.md` aligned with the local-preview source-build and
  public issue-routing paths
- `CONTRIBUTING.md`, `CODE_OF_CONDUCT.md`, `SECURITY.md`, `SUPPORT.md`, and
  `CHANGELOG.md`
- GitHub Actions CI, Dependabot, pull request template, issue templates,
  community triage guide, and CODEOWNERS review routing
- `docs/security-model.md`, `docs/versioning.md`, `docs/testing.md`,
  `docs/supply-chain.md`, `docs/release.md`, and
  `docs/release-notes-template.md`
- crate metadata, crate-level rustdoc introductions, and package manifest
  inspection through the locked package gate
- generated governance checks passing and generated governance outputs in sync
- public Markdown relative-link checks passing for README, docs, examples,
  GitHub templates, and governance entry docs
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
- Dependabot alerts and security updates remain enabled
- issue labels referenced by templates exist or are acceptable for GitHub to
  create on first use
- repository description, topics, homepage, and profile contact path match the
  local-preview status
- CI workflow files from this branch have been merged to `main`, and the latest
  `main` workflow run is green

These settings are external state; do not claim they are configured unless
verified in GitHub.

As of the latest local audit after publication, CI is present and green on
`main`, branch protection requires the CI jobs listed above, private
vulnerability reporting is enabled, and Dependabot alerts/security updates are
enabled. Secret scanning and push protection still need maintainer confirmation
because the secret scanning alerts API returned `404 Not Found` from this
checkout.

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
