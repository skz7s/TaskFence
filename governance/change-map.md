# Change Map

Use this document when a task changes code, contracts, configuration, governance, reusable agent behavior, or runtime artifacts and you need to determine which documents must stay in sync.

## Principles

- stable project facts belong in `docs/codex/` and `docs/config/`
- runtime agent rules belong in generated `AGENTS.md` entrypoints, with editable source in `governance/`
- reusable execution workflows belong in `.codex/skills/`
- project-private governance belongs in `governance/private/*`
- machine-local environment facts belong in ignored `.codex-helper/local-env.toml`
- baseline version/source/sync metadata belongs in `governance/profile.toml`; machine facts do not
- secret risks must be diagnosed without printing sensitive values
- durable governance, deployment, provider, dependency-source, security, or lifecycle decisions belong in `docs/decisions/`
- Project Design assets belong in `.codex-helper/design/*` plus finalized `design.md`
- governance enhancement uses the local `codex` executable and must report missing runtime explicitly
- old planning/dispatch/worker/goals artifacts are legacy data and must not become new production dependencies

## Change Matrix

| Change type | Required doc updates | Usually also update |
| --- | --- | --- |
| top-level directory map or directory purpose changes | `docs/codex/project-structure.md` | `docs/codex/structure-contract.md`, `governance/private/agent/project-governance.md`, generated `AGENTS.md` |
| code ownership, module boundaries, CLI/runtime layering | `docs/codex/structure-contract.md` | `docs/codex/project-structure.md`, `docs/codex/runtime-architecture.md`, affected skills |
| governance template source, catalog, sync, recommendation, or private-governance behavior | `governance/README.md`, `governance/change-map.md`, relevant `governance/*` files | `docs/codex/runtime-architecture.md`, generated `AGENTS.md` |
| baseline version, preflight, repair dry-run, secret scanning, lifecycle hints, or ADR policy | `governance/README.md`, `governance/change-map.md`, `docs/codex/structure-contract.md` | `docs/codex/runtime-architecture.md`, `docs/decisions/`, affected skills, Web/API/CLI tests |
| Project Design template, prototype, UI library, or `design.md` behavior | `docs/codex/runtime-architecture.md`, `docs/codex/project-structure.md` | `README.md`, frontend/API tests |
| governance enhancement runtime, local `codex` availability, or missing-`codex` behavior | `docs/codex/runtime-architecture.md`, `docs/codex/structure-contract.md` | `README.md`, `docs/config/env-inventory.md`, frontend/API tests |
| local environment detection, dependency strategy, setup/dev/build scripts, or OS support | `docs/config/env-inventory.md`, `docs/config/cross-platform-ops.md`, `docs/codex/runtime-architecture.md` | `README.md`, `docs/codex/project-structure.md`, affected ops skills |
| operator quickstart, supported flow, validation commands, or removed public commands | `README.md` | `docs/codex/runtime-architecture.md` |
| removal of legacy Plan/Execution/Goals/dispatch/worker/provider/assistant/project-Codex surfaces | `docs/codex/project-structure.md`, `docs/codex/structure-contract.md` | `README.md`, `docs/codex/runtime-architecture.md`, generated `AGENTS.md` |
| skill trigger boundaries, routing tags, read-first docs, sync docs, validation steps | `governance/skill-routing.toml`, affected `.codex/skills/*` files | `governance/skill-maintenance.md`, `governance/private/agent/project-governance.md` |
| native Codex capability adoption, such as review mode, MCP, plugins, hooks, cloud tasks, or subagents | `docs/codex/runtime-architecture.md`, `docs/codex/structure-contract.md` | `docs/config/env-inventory.md`, `README.md`, affected skills only after a stable project-owned surface exists |

## Update Rules

- if the change alters both code and a fact document, update them in the same task
- if the change only affects runtime agent behavior, prefer updating `governance/*` and regenerating `AGENTS.md`
- if the change only affects repeatable workflow guidance, prefer updating the owning skill instead of broadening runtime `AGENTS.md`
- if interrupted during a large refactor, resume from the relevant project roadmap or issue tracker rather than source-repo-only helper docs
