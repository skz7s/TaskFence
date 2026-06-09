# Rustdoc Release Gate

## Context

TaskFence is being prepared as a mature open-source Rust project. The existing
release gate covered formatting, type checking, linting, tests, package
manifest inspection, and governance checks, but it did not verify that public
Rust documentation can be generated warning-clean. A trial run of
`RUSTDOCFLAGS="-D warnings" cargo doc --workspace --no-deps --locked` exposed
an invalid rustdoc HTML tag warning in a CLI doc comment.

## Decision

Make warning-clean rustdoc generation part of the documented release and CI
gate:

- fix the current invalid rustdoc warning
- add `RUSTDOCFLAGS="-D warnings" cargo doc --workspace --no-deps --locked` to
  GitHub Actions
- add the same command to release, readiness, contributor, maintainer,
  supply-chain, and operations documentation

This remains a documentation generation gate. It does not publish API docs,
release artifacts, or stable production API promises.

## Consequences

Public API and CLI documentation comments now need to remain valid rustdoc.
Warnings in public Rust documentation will block CI and release candidates
instead of being discovered after publication. Maintainers should keep rustdoc
healthy when changing exported crate surfaces or doc-commented CLI commands.

## Validation Or Rollback Notes

Validation is:

```bash
RUSTDOCFLAGS="-D warnings" cargo doc --workspace --no-deps --locked
git diff --check
python3 scripts/governance/check_codex_governance.py
```

Rollback is to remove the CI step and release-doc references. The doc comment
fix can remain because it is valid independently.
