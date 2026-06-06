## Governance Standard

- Treat `governance/` as the only buildable source for runtime agent and skill rules.
- Add or edit project-specific agent fragments only under `governance/private/agent/*.md`.
- Add or edit project-specific skills only under `governance/private/skill/<skill-name>/SKILL.md`.
- For helper repository maintenance, edit the reusable template-library source first, then sync generated core output through the governance build path.
- Do not create long-lived agent prompts or skills directly in generated outputs such as `AGENTS.md`, `.codex/skills/*`, or `governance/core/*`.
- The default generated artifact policy is `generated-but-committed`: keep runtime generated outputs in Git for bootstrap, but keep source ownership in governance files and detect drift with build checks.
- Long-lived edits to generated outputs must be moved back to source before running build; direct generated edits are temporary debugging only.
- When project development discovers a new repeatable workflow or constraint, classify it into project private governance before running build, otherwise the next build may prune or overwrite it.
- Keep private governance compact: merge duplicates, prefer one narrowly named module per concern, and move stable project facts into `docs/codex/` or `docs/config/`.
- Keep machine-local runtime facts in `.codex-helper/local-env.toml`; do not commit that file or promote host-specific paths into generated governance.
- Keep dependency source facts such as registries, indexes, mirrors, and proxy detection in `.codex-helper/local-env.toml`; keep stable dependency source policy in `docs/config/*`.
- To add a project-private agent or skill: create the `governance/private/*` source, register it in `governance/modules.toml`, add private agent fragments to the relevant `governance/bundles.toml` bundle, run `python3 scripts/governance/build_agents.py`, then run `python3 scripts/governance/check_codex_governance.py`.
- Before publishing or relying on generated governance, run the project governance check command recorded in `governance/profile.toml`.
