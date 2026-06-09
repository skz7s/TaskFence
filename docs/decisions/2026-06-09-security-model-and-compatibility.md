# Security Model And Compatibility Policy

## Context

TaskFence is moving toward public open-source use. The repository had strong
runtime and governance security rules, but external users had to piece the
security model together from README status text, development design notes, and
historical plans. Release docs also referenced semver review without a concrete
preview compatibility policy or supply-chain maintenance boundary.

## Decision

Add stable public documentation for:

- TaskFence's local-preview security model, protected assets, trust boundaries,
  secure defaults, in-scope threats, and unsupported production surfaces
- preview versioning and compatibility policy for Rust MSRV, workspace crates,
  task files, CLI behavior, structured evidence, and deprecations
- supply-chain maintenance expectations that distinguish mandatory current
  gates from optional external tools such as `cargo-audit` and `cargo-deny`

Keep the current project status as local preview. Do not make unavailable audit
tools, production services, or contract-only runtime surfaces mandatory or
supported by documentation alone.

## Consequences

External reviewers can evaluate the security boundary without reading runtime
agent prompts. Maintainers have a clearer bar for release notes, compatibility
claims, MSRV changes, and dependency updates. Future beta or stable releases
must promote optional supply-chain checks into configured CI policy before
claiming that level of assurance.

The new policy docs must stay aligned with `README.md`, `SECURITY.md`,
`docs/release.md`, `docs/maintainers.md`, and
`docs/config/readiness-checklist.md` whenever supported surfaces or release
gates change.

## Validation Or Rollback Notes

Validation is documentation and governance focused:

```bash
git diff --check
python3 scripts/governance/check_codex_governance.py
cargo metadata --locked --format-version 1 --no-deps
bash deploy/manage.sh readiness
```

Rollback is to remove the new policy docs and their links. No runtime behavior,
dependency source, credential handling, release artifact, or CI gate changes are
introduced by this decision.
