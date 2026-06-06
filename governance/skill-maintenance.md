# Skill Maintenance

Use this document when creating, selecting, updating, merging, or retiring files under `.codex/skills/`.

## Policy

- Runtime `AGENTS.md` stays small; reusable procedures belong in skills.
- Default installed skills are limited to the helper baseline manifest. Keep `governance/skill-routing.toml`, this document, and generated output aligned when the manifest changes.
- Reusable templates are maintained as local source under `governance_manager/templates/*`; do not depend on external template-source registries.
- Managed projects should sync only the selected templates they need.
- Project-specific skills must be authored under `governance/private/skill/<name>/SKILL.md`; generated `.codex/skills/*` is build output.
- Long-lived edits to generated `.codex/skills/*` must be moved back to `governance/core/skill/*` or `governance/private/skill/*` before running build.
- Runtime governance outputs use the `generated-but-committed` policy: commit generated files for bootstrap, but keep source ownership and drift checks authoritative.
- Dependency source facts belong in `.codex-helper/local-env.toml`; stable mirror or registry policy belongs in `docs/config/*`.
- Provider tokens, registry credentials, Bearer/Auth headers, URL userinfo, and other sensitive values must not be written into skills, docs, generated governance, or local env facts.
- Baseline version, preflight status, repair dry-run output, lifecycle hints, and ADR requirements are default governance signals, not optional per-project extras.

## Create Or Select A Skill

1. Search installed skills and the helper governance catalog before creating a new workflow.
2. Prefer an existing maintained local public skill or local design template over creating a new workflow.
3. If no candidate fits in a managed project, create a narrow private skill under `governance/private/skill/<name>/SKILL.md` and add optional `agents/openai.yaml`.
4. Register the private skill in `governance/modules.toml`, then run `python3 scripts/governance/build_agents.py`.
5. Add exactly one routing entry in `governance/skill-routing.toml` only for installed default skills.
6. Keep heavy references in `references/` files so they are read only when needed.
7. If a skill records local machine facts, write them to `.codex-helper/local-env.toml` or another ignored `.codex-helper/*` file, never to generated governance.
8. If a skill changes dependency source behavior, update `dependency-source-policy` and the relevant `docs/config/*` policy in the same task.
9. If a skill changes durable governance, deployment, provider, dependency source, security, or lifecycle policy, add or update a decision record under `docs/decisions/`.

## Project Private Agents

1. Create or edit project-specific agent fragments under `governance/private/agent/*.md`.
2. Register each fragment in `governance/modules.toml`.
3. Add agent fragments that should affect runtime rules to the relevant `governance/bundles.toml` bundle.
4. Run `python3 scripts/governance/build_agents.py`, then `python3 scripts/governance/check_codex_governance.py`.

## Validation

- Run `python3 scripts/governance/check_codex_governance.py` when governance assets or installed skills change.
- Run `codex-helper governance preflight <project-id>` for release, deployment, or baseline-upgrade gates when the project is registered.
- Use `codex-helper governance repair <project-id> --dry-run` to inspect repair candidates; do not auto-apply repairs in v1.
- Run targeted tests for local template catalog, project sync, or Web governance APIs when their behavior changes.
