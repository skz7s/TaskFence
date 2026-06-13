# Maintainer Guide

This guide captures repository maintenance expectations for TaskFence.

## Review Priorities

Review runtime changes in this order:

1. Security boundary correctness and fail-closed behavior against
   [docs/security-model.md](security-model.md).
2. Typed contracts at crate boundaries.
3. Evidence quality for audit, reports, approvals, and artifacts.
4. Documentation accuracy, rustdoc health, and readiness claims.
5. Developer ergonomics.

## Review Routing

`.github/CODEOWNERS` routes default pull request review to the public
repository owner, `@skz7s`. This is a lightweight triage signal for the current
preview project. It does not create production support, hosted-service,
incident-response, or module-owner commitments.

Add narrower code owners only when maintainers are explicitly confirmed for a
subsystem. Do not use CODEOWNERS to imply support for production surfaces that
remain unsupported in `docs/config/readiness-checklist.md`.

## Issue Triage

Use [docs/community-triage.md](community-triage.md) for public issue routing,
label use, reproduction expectations, and closure decisions. Keep security
reports, approval bypasses, sandbox escapes, credential exposure, and audit
integrity concerns out of public issue threads and route them through
[SECURITY.md](../SECURITY.md).

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

Run Markdown link validation before merging public documentation or GitHub
template changes:

```bash
python3 scripts/docs/check_markdown_links.py
```

## Release Stewardship

Before tagging or publishing:

1. Confirm `docs/config/readiness-checklist.md` matches implemented behavior.
2. Confirm [docs/security-model.md](security-model.md),
   [docs/versioning.md](versioning.md), and
   [docs/supply-chain.md](supply-chain.md) match the release claims.
3. Confirm [docs/testing.md](testing.md) matches the tested surfaces.
4. Update `CHANGELOG.md`.
5. Draft release notes from
   [docs/release-notes-template.md](release-notes-template.md).
6. Run the release gate in [docs/release.md](release.md).
7. Record unavailable integration and external audit-tool coverage.
8. Confirm public Markdown relative links, crate/package metadata, and
   repository URLs are correct.

Before switching repository visibility to public, use
[docs/publication-readiness.md](publication-readiness.md) and confirm the
external GitHub settings listed there. Do not treat public repository
visibility as crate publication, production support, or artifact release.

## Compatibility Stewardship

During `0.x` preview releases, maintainers may make breaking changes when the
security boundary or unstable contracts require them. Release notes must call
out task-file, CLI, audit-event, gateway, runner, state, and MSRV impact, and
must include migration notes when users can act on them.

## Incident Handling

For security reports, avoid public discussion of exploit details until a fix or
mitigation is ready. Prefer narrowly scoped patches, clear release notes, and
documentation updates that explain the corrected boundary without exposing
secrets or private reporter data.
