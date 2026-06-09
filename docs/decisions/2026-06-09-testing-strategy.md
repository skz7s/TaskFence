# Testing Strategy And Coverage Reporting

## Context

TaskFence's release and pull request docs already mentioned skipped Docker,
database, remote runner, and live connector coverage, but contributors did not
have one public testing matrix that explained default CI, focused crate tests,
example validation, ignored Docker integration tests, and live-environment
coverage reporting. Mature open-source maintenance needs those boundaries to be
explicit so reviewers do not infer integration coverage from local unit tests.

## Decision

Add `docs/testing.md` as the public testing strategy. It defines:

- the default local Rust, rustdoc, and governance gate
- CI's additional shell, readiness, and package manifest checks
- focused crate test ownership by subsystem
- example validation commands that do not require Docker or live credentials
- the ignored Docker integration test and its Docker/image prerequisites
- live connector, database, remote host, and deployed service coverage as
  explicit operator-provided environments
- coverage reporting expectations for pull requests and release notes

This decision does not unignore Docker tests, add live credentials to CI, or
claim integration coverage for unavailable environments.

## Consequences

Contributors can pick narrower validation while still knowing the full gate.
Maintainers have a shared rule for when Docker or live integration coverage must
be run and when a skipped-coverage note is acceptable. CI remains deterministic
and credential-free by default.

The testing strategy must stay aligned with `.github/workflows/ci.yml`,
`.github/pull_request_template.md`, `docs/release.md`,
`docs/config/readiness-checklist.md`, and the actual test inventory.

## Validation Or Rollback Notes

Validation is:

```bash
cargo test --workspace --locked -- --list
git diff --check
python3 scripts/governance/check_codex_governance.py
```

Rollback is to remove `docs/testing.md`, its links, and this decision record.
No runtime behavior, test implementation, credentials, or CI environment changes
are introduced by this decision.
