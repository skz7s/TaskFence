# 环境变量约定

本文档记录托管项目默认治理 baseline 使用的环境事实、依赖源和运维入口约定。

## Repo-Local Environment Facts

- `.codex-helper/local-env.toml`
  - ignored machine-local fact file, not a Git-tracked source of truth
  - created by `bash deploy/manage.sh detect-env` or the `project-env-baseline` skill
  - records current OS, shell, service manager, package managers, tool paths/versions, dependency sources, and verification metadata
  - guides Python/uv/Node/npm/Codex/Vite/Vitest/deploy command selection
  - must not contain provider tokens, Codex config contents, cookies, SSH keys, private registry credentials, or other secrets

`governance/profile.toml` stores committed governance baseline metadata under `[baseline]`; keep
machine-local paths, dependency sources, proxy state, and tool versions in `.codex-helper/local-env.toml`.
Run `codex-helper governance preflight <project-id>` before release or deployment gates to combine
local env/schema validation with governance and secret-risk checks.

Minimum schema:

```toml
[meta]
schema_version = 1
project_root = "/abs/path/to/repo"
generated_at = "2026-05-10T00:00:00Z"
generated_by = "deploy/manage.sh detect-env"
last_verified_at = "2026-05-10T00:00:00Z"

[system]
os = "Darwin"
os_family = "macos"
distribution = "macos"
version = "15.4.1"
arch = "arm64"
shell = "<detected-shell>"
is_wsl = false
service_manager = "launchd"

[tools.python]
command = "/abs/path/.venv/bin/python"
path = "/abs/path/.venv/bin/python"
version = "3.13.x"
source = "repo-venv"
verified_at = "2026-05-10T00:00:00Z"

[requirements]
python = ">=3.13"
node_major = ""
codex_package = ""
python_source = "pyproject.toml"
node_source = ""

[paths]
python_bin_dir = "/abs/path/.venv/bin"
node_bin_dir = "/abs/path/web/node_modules/.bin"
extra_bin_dirs = []
venv_dir = "/abs/path/.venv"
state_dir = "/home/user/.codex-helper/state"

[package_managers.homebrew]
available = true
path = "<detected-package-manager-path>"

[dependency_sources]
docker_registry_mirror = ""
docker_image_prefix = ""
docker_base_image = ""
node_docker_image = ""
docker_vm_image = ""
debian_mirror = ""
node_apt_repo = ""
npm_registry = ""
npm_registry_mirror = ""
uv_index = ""
pip_index = ""
pip_extra_index = ""
python_install_mirror = ""
uv_install_url = ""
node_install_mirror = ""
proxy_detected = false
source = "deploy/manage.sh detect-env"

[policy]
dependency_strategy = "reuse-first"
auto_install = false
project_isolation = true

[verification]
last_detect_env_at = "2026-05-10T00:00:00Z"
last_doctor_at = ""
drift_detected = false
```

Secret safety applies to this file even though it is ignored: dependency source URLs should be
recorded without registry credentials, URL userinfo, Bearer tokens, or private query parameters.

## Dependency Source Policy

- Stable mirror, registry, proxy, or package-source policy belongs in `docs/config/*`.
- Actual local Docker registry mirror, image prefix, base image, Node Docker image, apt mirror, npm registry, uv/pip index, pip extra index, Python installer mirror, Node mirror, proxy detection, and tool paths belong only in ignored `.codex-helper/local-env.toml`.
- `detect-env` records dependency source facts and never installs dependencies.
- `doctor` is read-only: it prints diagnostics and suggested commands, but does not rewrite global Docker, npm, pip, uv, nvm, conda, apt, dnf, or yum config.
- Docker image pulls and Docker build package installs are dependency acquisition. Compose and Dockerfiles should expose mirror-friendly image, apt, npm, uv, pip, Python, and Node source overrides instead of assuming public registries are reachable.
- GitHub/Gitee repository hosting and Docker image delivery are separate decisions. Default to GitHub Actions for primary CI when available, keep Gitee as a mirror or domestic collaboration entrypoint when useful, and place deployment images in an internal or regionally close registry.
- CI Docker builds should use BuildKit/buildx cache where available and should publish immutable release image tags. Deployment hosts should pull those tags instead of rebuilding source during normal production deploys.
- URLs containing credentials, auth tokens, `Authorization`, `Bearer`, `password=`, `token=`, or similar markers must not be promoted into docs or generated governance.

Supported overrides:

- `CODEX_HELPER_NPM_REGISTRY`
- `CODEX_HELPER_NPM_REGISTRY_MIRROR`
- `UV_DEFAULT_INDEX`
- `CODEX_HELPER_UV_DEFAULT_INDEX`
- `PIP_INDEX_URL`
- `PIP_EXTRA_INDEX_URL`
- `CODEX_HELPER_UV_INSTALL_URL`
- `CODEX_HELPER_UV_PYPI_SPEC`
- `CODEX_HELPER_UV_PYTHON_INSTALL_MIRROR`
- `CODEX_HELPER_NODE_MIRROR`
- `CODEX_HELPER_DOCKER_REGISTRY_MIRROR`
- `CODEX_HELPER_DOCKER_IMAGE_PREFIX`
- `CODEX_HELPER_DOCKER_BASE_IMAGE`
- `CODEX_HELPER_NODE_DOCKER_IMAGE`
- `CODEX_HELPER_DOCKER_VM_IMAGE`
- `CODEX_HELPER_DEBIAN_MIRROR`
- `CODEX_HELPER_NODE_APT_REPO`

## Cross-Platform Operations Scripts

- Record the actual project-local operations entrypoint and supported commands in `docs/config/cross-platform-ops.md`.
- If `deploy/manage.sh` is the only supported entrypoint, keep that fact explicit; do not downgrade it to a preferred entrypoint.
- Record the exact supported deployment target, such as Ubuntu-only systemd, instead of generalizing it to generic Linux systemd.
- Dependency setup is reuse-first: detect existing tools and print install guidance when something is missing; install only with an explicit operator flag such as `--install-missing`.
- If no operations entrypoint exists, use `python3 .codex/skills/ops-script-maintenance/scripts/scaffold_manage.py --root . --check` to review a project-local scaffold. It writes only `deploy/manage.sh` by default; pass `--with-wrappers` only for confirmed legacy wrapper compatibility.
- Stable deployment requirements, service names, ports, and operator runbooks belong in `docs/config/*`.

## Sync Rules

Update this file when the project adds or removes:

- runtime environment variables
- dependency source or mirror policy
- secret injection policy
- CLI flags that change setup, dev, validation, or deployment behavior
