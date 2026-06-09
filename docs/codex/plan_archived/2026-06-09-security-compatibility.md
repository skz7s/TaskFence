# Security And Compatibility Maturity Plan

## Goal

Continue improving TaskFence toward mature open-source readiness by making its
security model, compatibility policy, release stewardship, and supply-chain
maintenance expectations explicit in stable documentation.

## Plan Source

Continuation of the active 2026-06-09 goal:

> 继续优化项目，包括文档，将项目优化到一个成熟的开源项目的级别后，开源项目

Related archived plan:

- `docs/codex/plan_archived/2026-06-09-open-source-maturity.md`

Scope:

- add a public security model that consolidates trust boundaries, protected
  assets, in-scope threats, out-of-scope surfaces, and secure defaults
- add a public versioning and compatibility policy for preview releases, Rust
  MSRV, task-file contracts, CLI/API behavior, audit evidence, and deprecation
  expectations
- add a supply-chain maintenance policy that separates current mandatory gates
  from optional external tooling that is not yet installed or required in CI
- update README, security, release, maintainer, changelog, and structure docs
  to link the new policy surfaces
- record the durable security/versioning decision in `docs/decisions/`

Non-goals:

- do not claim production support for contract-only daemon, Web UI, MCP,
  arbitrary HTTP proxy, SDK/webhook, Kubernetes, microVM, managed-cloud, SSO,
  object storage, or background audit export surfaces
- do not introduce new supply-chain tools into CI unless they can be verified in
  this checkout and documented as mandatory
- do not change runtime behavior, dependency versions, package publication, or
  release tags in this slice
- do not edit generated governance outputs directly

Acceptance criteria:

- external users can find a concise security model without reading governance
  prompts or historical plans
- maintainers have a concrete compatibility policy for 0.x preview releases and
  first publish waves
- release docs identify supply-chain checks and the current limitations of
  missing external audit tooling
- documentation still labels unsupported production surfaces as unsupported
- validation evidence is recorded before committing

## Snapshot

- Date: 2026-06-09
- Default branch: `origin/main`
- Working branch: `codex/governance-development-plan`
- Initial worktree status: clean
- `git pull --ff-only`: attempted and failed because the local branch has no
  tracking upstream
- `git fetch --prune origin`: completed in the prior continuation turn
- Current observed gaps: `SECURITY.md` is high-level, there is no dedicated
  public security model, release docs mention semver review without a concrete
  compatibility policy, and `cargo-deny`/`cargo-audit` are not installed or
  configured as mandatory checks in this checkout

## Phases

### 1. Intake And Plan

- Status: done
- Scope: record the current branch, pull limitation, related archived plan,
  scope, non-goals, acceptance criteria, and next executable phase
- Verification command: `git status --short --branch`
- Verification evidence: passed on 2026-06-09. Worktree contains only the new
  active plan file for this slice.

### 2. Security And Compatibility Docs

- Status: done
- Scope: add security model, versioning/compatibility, supply-chain policy, and
  decision record; update stable docs and public entrypoints to link them
- Verification command: `git diff --check && python3 scripts/governance/check_codex_governance.py`
- Verification evidence: passed on 2026-06-09. Added
  `docs/security-model.md`, `docs/versioning.md`, `docs/supply-chain.md`, and
  `docs/decisions/2026-06-09-security-model-and-compatibility.md`; linked
  them from README, SECURITY, CONTRIBUTING, release, maintainer, changelog,
  readiness, project-structure, and change-map docs. Also passed
  `python3 scripts/governance/build_agents.py --check`.

### 3. Final Review And Commit

- Status: done
- Scope: run targeted metadata/docs checks, update this plan with evidence,
  archive it if complete, and create one focused local commit
- Verification command: `cargo metadata --locked --format-version 1 --no-deps && bash deploy/manage.sh readiness`
- Verification evidence: passed on 2026-06-09. `cargo metadata --locked
  --format-version 1 --no-deps` wrote 34096 bytes of metadata to `/tmp`, and
  `bash deploy/manage.sh readiness` printed the updated readiness checklist.

## Commit Plan

1. `docs: define security and compatibility policy` - `d1070e4`

## Final Evidence

- Public security model added with protected assets, trust boundaries, secure
  defaults, in-scope threats, current limits, and beta/production review bar.
- Preview compatibility policy added for Rust MSRV, workspace crate versions,
  semver during `0.x`, task-file changes, CLI behavior, structured evidence,
  and deprecations.
- Supply-chain maintenance policy added for mandatory current gates,
  Dependabot review, optional external audit tools, package publication, and
  source/secret hygiene.
- Durable decision record added for the security model and compatibility policy.
- Public entrypoints and maintainer docs now link the new policy surfaces.
- Validation passed on 2026-06-09:
  `git diff --check`, `python3 scripts/governance/build_agents.py --check`,
  `python3 scripts/governance/check_codex_governance.py`,
  `cargo metadata --locked --format-version 1 --no-deps`, and
  `bash deploy/manage.sh readiness`.
- Implementation commit: `d1070e4 docs: define security and compatibility
  policy`.
