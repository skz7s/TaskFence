# Maintainer Guide

This guide captures repository maintenance expectations for TaskFence.

## Review Priorities

Review runtime changes in this order:

1. Security boundary correctness and fail-closed behavior.
2. Typed contracts at crate boundaries.
3. Evidence quality for audit, reports, approvals, and artifacts.
4. Documentation accuracy and readiness claims.
5. Developer ergonomics.

## Boundary Claims

Do not describe a surface as production supported until it has:

- implementation, not only a typed contract
- tests for success and failure paths
- documentation for configuration, operation, and limitations
- release notes that identify skipped integration coverage
- security review of secrets, policy decisions, approvals, audit integrity, and
  artifact containment

## Governance Ownership

Generated governance artifacts are committed for bootstrap, but source
ownership stays in `governance/`.

- Project-specific runtime rules: `governance/private/agent/*.md`
- Project-specific skills: `governance/private/skill/<skill-name>/SKILL.md`
- Generated outputs: `AGENTS.md`, `.codex/skills/*`, `governance/core/*`

Run governance validation before merging governance changes:

```bash
python3 scripts/governance/build_agents.py --check
python3 scripts/governance/check_codex_governance.py
```

## Release Stewardship

Before tagging or publishing:

1. Confirm `docs/config/readiness-checklist.md` matches implemented behavior.
2. Update `CHANGELOG.md`.
3. Run the release gate in [docs/release.md](release.md).
4. Record unavailable integration coverage.
5. Confirm crate/package metadata and repository URLs are correct.

## Incident Handling

For security reports, avoid public discussion of exploit details until a fix or
mitigation is ready. Prefer narrowly scoped patches, clear release notes, and
documentation updates that explain the corrected boundary without exposing
secrets or private reporter data.
