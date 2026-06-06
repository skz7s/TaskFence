---
name: project-env-baseline
description: Use when starting work in a repository, changing commands or validation, moving between macOS/Linux/WSL/CentOS/Ubuntu environments, or when local Python, uv, Node, npm, Codex, shell, package manager, or service-manager facts are needed. Reads or creates the repo-local .codex-helper/local-env.toml environment fact file without storing secrets.
---

# Project Env Baseline

## Goal

Keep machine-specific environment facts out of Git while making Codex command choices predictable across macOS development, Linux deployment, WSL, and low-resource hosts.

## Local Env File

- Use `.codex-helper/local-env.toml` as the repo-local environment fact cache.
- Treat it as ignored, machine-local state. Do not commit it and do not copy it into `governance/profile.toml`.
- Never store tokens, provider secrets, private config file contents, cookies, or SSH keys in it.
- Record only facts detected from the current checkout or values explicitly configured by the operator. Do not seed helper-owned default commands, package specs, registry URLs, mirrors, or state paths into this file just to keep fields non-empty.
- Stable project facts belong in `docs/codex/`, `docs/config/`, or `governance/private/*`.

## Workflow

1. If `.codex-helper/local-env.toml` exists, read it before choosing commands.
2. If the checkout moved machines, the target switched between macOS/Linux/WSL/CentOS/Ubuntu, deployment is about to run, or recorded facts contradict `uname` / tool paths, refresh it.
3. Prefer the project script when present:
   - `bash deploy/manage.sh detect-env`
4. If no project script exists, use the bundled detector:
   - `python3 .codex/skills/project-env-baseline/scripts/detect_env.py`
5. Use recorded paths for validation and setup commands when they still exist.
6. If a recorded tool path is missing, refresh the file before falling back to PATH.
7. Verify `.codex-helper/local-env.toml` is ignored by Git; if it is not ignored, stop and add the ignore rule before recording machine facts.

## Required Schema

The file should contain:

- `[meta]`: `schema_version`, `project_root`, `generated_at`, `generated_by`, `last_verified_at`
- `[system]`: `os`, `os_family`, `distribution`, `version`, `arch`, `shell`, `is_wsl`, `service_manager`
- `[tools.<name>]`: for `python`, `uv`, `node`, `npm`, `codex`, `git`; include `command`, `path`, `version`, `source`, `verified_at`
- `[requirements]`: project-required Python range or version, Node major when known, and Codex package spec when managed by npm
- `[paths]`: `python_bin_dir`, `node_bin_dir`, `extra_bin_dirs`, `venv_dir`, `state_dir`
- `[package_managers.<name>]`: for `homebrew`, `apt`, `dnf`, `yum`, `nvm`, `conda`; include `available` and `path`
- `[dependency_sources]`: Docker image and registry overrides when relevant, `npm_registry`, `uv_index`, `pip_index`, `pip_extra_index`, `python_install_mirror`, `node_install_mirror`, `proxy_detected`, and `source`
- `[policy]`: `dependency_strategy = "reuse-first"`, `auto_install = false`, `project_isolation = true`
- `[verification]`: `last_detect_env_at`, `last_doctor_at`, and `drift_detected`

## Command Policy

- Prefer repo-local `.venv/bin/*` and `web/node_modules/.bin/*` over global tools when present.
- Prefer reuse-first dependency setup: detect and record first; install only when an operator explicitly asks or passes an install flag.
- Keep Python virtualenvs, Node installs, and generated caches isolated per project.
- Record package registries, Python indexes, installer mirrors, and proxy detection in `[dependency_sources]`; put stable source policy in `docs/config/*`.
- Do not write local interpreter paths, package-manager paths, or current OS facts into `AGENTS.md`, `governance/profile.toml`, or reusable governance templates.
- Stable deployment requirements, ports, service names, and operator runbooks belong in `docs/config/*`; local tool locations stay in `.codex-helper/local-env.toml`.
- On macOS, assume development only unless the repository documents launchd support.
- On Linux deployment, prefer systemd only when `systemctl` is available and the operator requested deployment.
- Use `bash deploy/manage.sh doctor` for read-only drift diagnostics; do not auto-fix or install from doctor.
- Run governance preflight before deployment or after switching machines so stale local env facts are surfaced next to baseline, secret, and ops readiness signals.
