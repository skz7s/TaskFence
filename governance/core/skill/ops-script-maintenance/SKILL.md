---
name: ops-script-maintenance
description: Use when creating, updating, reviewing, or debugging repository operations scripts for setup, deployment, dev servers, validation, dependency installation, service files, systemd, macOS dev environments, Ubuntu/CentOS/RHEL/Fedora deployment, uv, Python, npm, Node, Vite, Vitest, or Codex runtime commands.
---

# Ops Script Maintenance

## Goal

Keep operational scripts portable, dependency-aware, and safe without replacing project-owned deployment facts with generic platform assumptions.

## Start

- First use `project-env-baseline`: read `.codex-helper/local-env.toml` or refresh it.
- Inspect existing script entrypoints before editing. Preserve the documented project entrypoint contract and any intentional legacy wrapper contract.
- Preserve operator-owned environment choices such as mirrors, Homebrew paths, nvm, conda, or system packages unless the user asks to replace them.

## Script Contract

- Use `deploy/manage.sh` as the repository operations entrypoint when the project documents it as supported.
- When a managed project has no operations entrypoint, scaffold a project-local one with:
  - `python3 .codex/skills/ops-script-maintenance/scripts/scaffold_manage.py --root . --check`
  - Review the generated `deploy/manage.sh` before extending deployment behavior.
- Do not copy `codex-helper`'s own `deploy/manage.sh` into other repositories; use the scaffold or write a project-local script from the target project's facts.
- Generate old public script wrappers only when explicitly requested with `--with-wrappers`; keep or add wrappers only when compatibility is a confirmed project requirement.
- Split commands by intent:
  - `detect-env`: detect and write local facts only.
  - `setup`: prepare repo-local dependencies.
  - `dev`: foreground hot reload for macOS or Linux.
  - `build`: package build and explicitly configured deployment steps.
  - `doctor`: explain current environment and missing dependencies.
- When a checked-in debug Docker or Compose stack runs the app service, wire that service to the
  same documented `dev` entrypoint so mounted source keeps hot reload.
- Keep deployment Docker or Compose entrypoints separate from debug services. Production containers
  should use build/start commands, not the reusable `dev` flow.
- Default dependency strategy is reuse-first. Do not install missing tools unless the operator explicitly asks or a flag such as `--install-missing` is present.
- Keep third-party environments project-isolated: `.venv`, `web/node_modules`, repo-local cache directories, and helper-managed `CODEX_HOME`.
- Keep stable deployment facts in `docs/config/cross-platform-ops.md`; keep host-specific tool paths, current package-manager availability, and local bin versions in `.codex-helper/local-env.toml`.
- Do not generalize a project's documented deployment target. For example, preserve Ubuntu-only deployment as Ubuntu-only; do not rewrite it as generic Linux systemd.
- Do not downgrade an explicitly documented single entrypoint into a preferred entrypoint.
- Keep package source choices explicit: pass npm registries, uv/pip indexes, Python install mirrors, and Node mirrors through environment variables; do not rewrite global package-manager config by default.
- For Docker-backed debug scripts, prefer reusable images and idempotent dependency sync over
  rebuilding or reinstalling dependencies on every run. Expose explicit `--rebuild` or
  `--force-sync` style flags for toolchain changes or clean dependency installs.
- For Docker deployment scripts, keep CI-built immutable image deployment separate from local build
  fallback. Normal deployment should pull configured image tags from the deployment registry and
  skip source rebuilds when prebuilt images are available.
- If a script learns a repeatable project-specific operating rule, add it to `governance/private/agent/*.md` or `governance/private/skill/<name>/SKILL.md`, then rebuild generated governance.

## Platform Rules

- macOS: support detection and development startup. Do not add launchd deployment unless explicitly requested.
- Ubuntu/Debian: use apt only when the project supports those deployment targets and privileged system prerequisites are required.
- CentOS/RHEL/Rocky/Alma/Fedora: use dnf/yum only when the project explicitly supports those deployment targets.
- WSL: treat as Linux for command detection, but record `is_wsl` and avoid assuming full systemd availability.

## Dependency Rules

- Python: prefer recorded repo `.venv/bin/python`; otherwise use recorded Python path, then PATH discovery.
- uv: prefer recorded path; use `uv sync --frozen` for Python dependencies when available.
- Node/npm: prefer recorded Node/npm path; use `npm ci` when a lockfile exists, otherwise `npm install`.
- Vite/Vitest: run through npm scripts with `--prefix web` unless the repository documents another path.
- Codex: record executable path separately; do not depend on repo-local Codex unless the project explicitly documents it.
- Dependency sources: record local facts in `[dependency_sources]`, redact secret-bearing URLs, and surface read-only diagnostics from `doctor`.

## Scaffold Rules

- The scaffold is a starting point, not a universal deployment contract.
- It detects Python, Node, Vite-style `dev` scripts, package build scripts, and creates `deploy/manage.sh` by default.
- Legacy `setup.sh`, `deploy.sh`, and `build.sh` wrappers are generated only with `--with-wrappers`.
- It does not copy `codex-helper`'s own deploy script into other projects.
- It refuses to overwrite non-scaffolded scripts unless `--force` is passed after review.
- Systemd service generation requires explicit `--systemd --exec-command`; do not infer long-running service commands or OS support for unfamiliar projects.

## Validation

- Shell changes require `bash -n` for each changed script.
- Python helper changes require the narrowest pytest covering the behavior.
- Governance or default skill changes require `python3 scripts/governance/check_codex_governance.py`.
- Managed project ops onboarding should pass the governance health check or clearly document remaining warnings before deployment.
- For broad suites on constrained hosts, prefer the repository's I/O-limited wrapper when available.
