# Agent Governance

This directory is the editable source layer for compiled repository governance.

## Goals

- keep runtime `AGENTS.md` compact and generated
- keep stable facts in `docs/codex/` and `docs/config/`
- keep reusable procedures in `.codex/skills/`
- use the installed governance catalog as the reusable template library
- keep project-specific agents and skills under `governance/private/*` so generated-but-committed outputs can be rebuilt safely

## Layout

- `profile.toml`
  - render variables, runtime commands, validation commands, and project boundary facts
- `bundles.toml`
  - maps source fragments to generated runtime entrypoints
- `core/agent/`
  - minimal generated-entrypoint fragments only
- `core/skill/`
  - default helper-managed skills installed into `.codex/skills/`
- `private/agent/`
  - repository-specific runtime facts and constraints
- `private/skill/`
  - repository-specific skills that should survive governance builds
- `change-map.md`
  - request-type to doc-update routing guidance
- `skill-maintenance.md`
  - rules for creating, selecting, or retiring skills
- `skill-routing.toml`
  - routing inventory for installed default skills

## Default Templates

Default helper-managed templates are intentionally small:

- `managed-project-dev-flow`
- `project-env-baseline`
- `ops-script-maintenance`
- `generated-artifact-policy`
- `dependency-source-policy`
- `secret-safety-policy`
- `managed-project-lifecycle`
- `decision-log-policy`
- `commercial-ui-constraints`
- `frontend-feedback-states`
- `i18n-localization-policy`
- `seo-visibility-policy`

Optional reusable onboarding or specialized templates should be selected explicitly when a managed project needs them, including `managed-project-onboarding`.

Additional reusable agents, workflow skills, or design skills should be selected from the governance catalog and synced into managed projects only when needed.
Saved project selections are additive over the default tier: when an older or manual selection omits default templates, project sync/build restores the full default baseline before applying optional selections.

The default development workflow is contextual: use lightweight inspect/edit/validate for small
changes and direct rule edits; use Codex plan mode and durable `docs/codex/plans/` records only for
larger work that needs phased coordination, branch integration, or multi-turn resumability. Each
new plan-sized request gets a new plan file; active plans stay in `docs/codex/plans/`, while
completed plans move to `docs/codex/plan_archived/` after final evidence is recorded. Plan source
sections preserve actionable requirements instead of collapsing them into generic summaries, and
phase statuses are updated as work starts and finishes.

Useful external ideas must be curated into local governance templates before managed projects can select them.

Keep machine-local environment facts in `.codex-helper/local-env.toml`; do not promote host-specific paths, dependency source facts, mirrors, proxies, or package-manager state into reusable governance templates.

`governance/profile.toml` records committed baseline metadata under `[baseline]`: version, source,
last sync time, and last preflight time. Health diagnostics report missing, outdated, or unknown
baseline state without treating profile data as machine truth.

Docs, governance source, generated outputs, and local env facts must not contain provider tokens,
registry credentials, Bearer/Auth headers, URL userinfo, or similar values. Health and preflight
diagnostics report only the path and category.

Durable governance, deployment, provider, dependency-source, security, or lifecycle decisions should
be recorded as lightweight ADRs under `docs/decisions/`.

Operator-requested multi-round review stays explicit and main-thread orchestrated. Before spawning
multiple subagents, inspect the active Codex configuration for `[agents].max_threads`; if it is
unset, assume a default concurrent limit of `6` and keep each delegated batch within the remaining
available slots.

When a managed repository already has a checked-in debug Docker or Compose setup, keep it aligned
with relevant code, dependency, env-contract, or ops-script changes. If no debug Docker setup
exists, ignore that surface unless the operator explicitly asks for it. Treat test Docker or
Compose as an opt-in validation path and update or run it only when the change risk or requested
verification needs container-backed testing.

## Project Private Governance

- Add project-specific agent rules under `governance/private/agent/*.md`.
- Add project-specific skills under `governance/private/skill/<name>/SKILL.md`.
- Do not edit generated `AGENTS.md`, generated `.codex/skills/*`, or synced `governance/core/*` directly; update governance sources and rebuild.
- Keep generated runtime outputs committed for bootstrap in v1; use build checks and health diagnostics to catch drift.
- Register private agent and skill sources in `governance/modules.toml`.
- Add private agent fragments to `governance/bundles.toml` when they should affect generated runtime rules.
- Keep stable deployment facts in `docs/config/*`; keep host-specific paths and current tool locations in ignored `.codex-helper/local-env.toml`.

Standard private module flow:

1. Create the private source file under `governance/private/agent/` or `governance/private/skill/<name>/SKILL.md`.
2. Register it in `governance/modules.toml`.
3. Add private agent fragments to `governance/bundles.toml` when they should appear in generated `AGENTS.md`.
4. Run `python3 scripts/governance/build_agents.py`.
5. Run `python3 scripts/governance/check_codex_governance.py`.

Read-only diagnostics:

```bash
codex-helper governance health <project-id>
codex-helper governance preflight <project-id>
codex-helper governance repair <project-id> --dry-run
```

`preflight` aggregates health, generated build checks, governance checks, ops syntax checks, local
env status, and secret scan. `repair --dry-run` prints suggested commands only.

## Template Library

Do not point managed-project sync/build at a project's private governance as reusable template truth.
Target projects should receive selected catalog templates plus their own `governance/private/*` facts,
not another repository's generated outputs or private rules.

## Build

Render generated entrypoints and installed default skills with:

```bash
python3 scripts/governance/build_agents.py
```

Check governance assets with:

```bash
python3 scripts/governance/check_agent_assets.py
python3 scripts/governance/check_codex_governance.py
```
