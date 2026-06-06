---
name: generated-artifact-policy
description: Use when reviewing, changing, committing, ignoring, pruning, or diagnosing generated governance artifacts such as AGENTS.md, .codex/skills/*, governance/core/*, build_agents.py outputs, generated-but-committed runtime rules, or governance drift checks.
---

# Generated Artifact Policy

## Goal

Keep runtime governance reproducible while preserving a bootstrappable checkout.

## Policy

- Treat `AGENTS.md`, `.codex/skills/*`, and `governance/core/*` as generated artifacts.
- The v1 repository policy is `generated-but-committed`: generated artifacts stay in Git so a fresh checkout has runnable Codex instructions before any build step.
- Do not make long-lived edits directly in generated artifacts. Move the change back to `governance_manager/` for reusable core templates or `governance/private/*` for project-owned rules, then rebuild.
- Direct edits to generated files are temporary debugging only and must be removed or promoted to source before final validation.
- Do not add generated artifacts to `.gitignore` or remove them from Git unless the repository also adds a bootstrap `AGENTS.md`, CI build enforcement, packaging/install regeneration, and health diagnostics for missing generated outputs.

## Workflow

1. Identify the generated file that appears wrong.
2. Find its owning source:
   - reusable helper core: `governance_manager/templates/*`
   - project private agent: `governance/private/agent/*.md`
   - project private skill: `governance/private/skill/<name>/SKILL.md`
3. Edit only the owning source.
4. Run `python3 scripts/governance/build_agents.py`.
5. Run `python3 scripts/governance/build_agents.py --check`.
6. Run `python3 scripts/governance/check_codex_governance.py`.
7. Run governance preflight when available before committing or deploying generated governance.

## Health Signals

- `generated_agents_stale` means generated output differs from source and must be rebuilt or source-corrected.
- `scattered_skill_source_missing` means a skill exists only in `.codex/skills/*`; move it to `governance/private/skill/<name>/SKILL.md` or remove it.
- `duplicate_managed_skill` means a default managed skill may have an old generated copy; review and prune through the governance build path.
- `baseline_outdated` means the managed project should sync the current helper baseline before relying on generated output.

## Boundaries

- Stable project facts belong in `docs/codex/*`, `docs/config/*`, or `governance/private/*`.
- Machine-local facts belong in `.codex-helper/local-env.toml`.
- Generated governance should never become the only source of a long-lived rule.
