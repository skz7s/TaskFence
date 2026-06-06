---
name: dependency-source-policy
description: Use when configuring, recording, reviewing, or diagnosing Docker image registries, npm registries, pip or uv indexes, Python or Node install mirrors, proxy settings, offline/internal package sources, dependency installation commands, or dependency source facts in .codex-helper/local-env.toml.
---

# Dependency Source Policy

## Goal

Make package sources explicit and diagnosable without storing credentials or mutating operator-owned global config by default.

## Trigger Conditions

Use this skill whenever dependency acquisition can be affected by network locality or source policy, including:

- China mainland or other restricted-network environments
- slow, timed out, reset, TLS, DNS, 403/429, or mirror-related install failures
- Docker image pulls, Docker VM/runtime image downloads, or container build package installs
- `npm`, `pnpm`, `yarn`, `uv`, `pip`, Python runtime, Node runtime, apt, dnf, yum, or Homebrew dependency setup
- CI failures that mention package registries, indexes, proxies, mirrors, or unavailable upstream release assets
- GitHub Actions, Gitee CI/Gitee Go, or other CI workflows that build, cache, push, mirror, or pull
  container images
- any request to use domestic mirrors, internal mirrors, offline packages, proxy settings, or private registries

## Source Of Truth

- Stable project policy belongs in `docs/config/env-inventory.md`.
- Current machine facts belong in ignored `.codex-helper/local-env.toml` under `[dependency_sources]`.
- Do not store tokens, credentials, cookies, private registry auth headers, or full secret-bearing config file contents.
- Docker daemon, VM provider, and registry mirror facts are machine facts unless the project owns a
  documented internal mirror. Keep Colima/Lima/Docker Desktop/OrbStack mirror URLs, local image
  paths, daemon config locations, and active Docker contexts out of reusable governance and
  committed project files.

## Required Local Env Fields

`[dependency_sources]` should record:

- `docker_registry_mirror` or a project-specific Docker image prefix when relevant
- `docker_base_image` and `node_docker_image` when Dockerfiles or Compose builds expose base image overrides
- `docker_vm_image` or the local VM/runtime image mirror note when relevant
- `docker_context` and Docker daemon/provider notes when relevant
- `npm_registry`
- `uv_index`
- `pip_index`
- `pip_extra_index`
- `python_install_mirror`
- `node_install_mirror`
- `proxy_detected`
- `source`

`[verification]` should record:

- `last_detect_env_at`
- `last_doctor_at`
- `drift_detected`

## Environment Overrides

- `CODEX_HELPER_DOCKER_REGISTRY_MIRROR`: operator-selected Docker registry mirror or pull-through cache.
- `CODEX_HELPER_DOCKER_IMAGE_PREFIX`: optional image prefix for mirrored images.
- `CODEX_HELPER_DOCKER_BASE_IMAGE`: project-specific base image override when Compose or Dockerfiles support it.
- `CODEX_HELPER_NODE_DOCKER_IMAGE`: project-specific Node image override when Compose or Dockerfiles support it.
- `CODEX_HELPER_DOCKER_VM_IMAGE`: Docker VM/runtime image source or mirror note for Colima, Lima, Rancher Desktop, Docker Desktop, or similar daemon providers.
- `CODEX_HELPER_NPM_REGISTRY`: primary npm registry.
- `CODEX_HELPER_NPM_REGISTRY_MIRROR`: npm fallback registry.
- `UV_DEFAULT_INDEX` or `CODEX_HELPER_UV_DEFAULT_INDEX`: uv Python package index.
- `PIP_INDEX_URL`: pip package index observed for local facts.
- `PIP_EXTRA_INDEX_URL`: pip extra package index observed for local facts.
- `CODEX_HELPER_UV_INSTALL_URL`: uv installer URL.
- `CODEX_HELPER_UV_PYTHON_INSTALL_MIRROR`: uv Python distribution mirror.
- `CODEX_HELPER_NODE_MIRROR`: Node distribution mirror or operator-chosen mirror note.

## Regional Build Workflow

When an operator asks to build with China mainland, domestic, internal, or regional sources:

1. Read `.codex-helper/local-env.toml` and inspect the relevant override environment variables before running a build.
2. Treat empty overrides as an explicit fact, not as approval to silently use public upstreams. Surface which required source layer is unset, such as Docker image prefix/base image, Node image, npm registry, uv index, pip index, Python distribution mirror, Node mirror, Debian mirror, or proxy.
3. If the project does not document stable internal sources, pass operator-selected source values through environment variables for that command rather than committing mirror URLs into reusable governance, Dockerfiles, Compose files, or global package-manager config.
4. For Docker builds, prefer parameterized image references and build args, such as `${CODEX_HELPER_DOCKER_IMAGE_PREFIX}`, `${CODEX_HELPER_DOCKER_BASE_IMAGE}`, and `${CODEX_HELPER_NODE_DOCKER_IMAGE}`, before assuming Docker Hub or public language images are reachable.
5. For Node and Python dependency installation, pass `CODEX_HELPER_NPM_REGISTRY`, `UV_DEFAULT_INDEX` or `CODEX_HELPER_UV_DEFAULT_INDEX`, `PIP_INDEX_URL`, and `PIP_EXTRA_INDEX_URL` into the build or setup process when those tools are used.
6. Keep CI cache and registry choices explicit. Prefer BuildKit or buildx cache metadata and
   project-owned registries over changing package-manager global configuration on runners.
7. Record the detected source facts with `bash deploy/manage.sh detect-env` or the `project-env-baseline` detector after changing overrides, then rerun the narrow build or setup command.

## Workflow

1. Read `.codex-helper/local-env.toml` before dependency setup.
2. If `[dependency_sources]` or `[verification]` is missing, run `bash deploy/manage.sh detect-env` when available, or the `project-env-baseline` detector.
3. Prefer project-local installs: `.venv`, `web/node_modules`, and repo-local cache directories.
4. Use environment variables to pass temporary mirror choices into scripts.
5. For Docker Compose or Dockerfiles, make image names and build-time package sources
   parameterizable before assuming public registries are reachable.
6. For CI-based image builds, keep repository platform and image distribution separate: GitHub
   Actions may build images while deployment hosts pull from an internal or domestic registry.
   Gitee mirrors may improve code access without becoming the only CI source of truth.
7. Diagnose regional or restricted-network failures at the source layer first: Docker VM image
   download, daemon startup, registry DNS/TLS, proxy, registry mirror, package registry, and
   language runtime mirrors.
8. Do not run `docker config`, `npm config set`, `pip config set`, or global package-manager
   rewrites unless the operator explicitly asks.
9. When the operator explicitly asks for global Docker or VM mirror configuration, write only
   non-secret mirror settings to the local provider config, verify with a read-only command, and
   record the fact in `.codex-helper/local-env.toml` rather than stable governance.
10. Use `bash deploy/manage.sh doctor` for read-only diagnostics and recommended commands.

## Runtime Capability Boundaries

- Missing optional host tools inside a container are dependency-source or runtime-capability facts,
  not necessarily application failures. Passive dashboards, health checks, and settings pages should
  show degraded capability state while continuing to render.
- Only commands that directly require the missing tool should fail. Preserve actionable error text
  for those active paths so operators know whether to install the tool in the image, mount a
  supported binary, or set an override such as `CODEX_HELPER_CODEX_BIN`.
- Never assume a host-installed macOS binary can satisfy a Linux container requirement. Record the
  container-compatible install source or mark the capability unavailable.

## Secret Safety

- Treat URLs containing `user:password@`, `token=`, `password=`, `_authToken`, `Authorization`, or `Bearer` as secret-bearing.
- Redact or reject secret-bearing dependency source values before writing stable docs or generated governance.
- If a private registry requires credentials, document only the variable name or secret manager path, not the value.
