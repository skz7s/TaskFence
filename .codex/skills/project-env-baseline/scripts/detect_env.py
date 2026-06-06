#!/usr/bin/env python3
from __future__ import annotations

import json
import os
import platform
import shutil
import subprocess
from datetime import datetime, timezone
from pathlib import Path
from urllib.parse import unquote, urlsplit


def _repo_root() -> Path:
    try:
        result = subprocess.run(
            ["git", "rev-parse", "--show-toplevel"],
            text=True,
            capture_output=True,
            check=True,
        )
    except Exception:
        return Path.cwd().resolve()
    return Path(result.stdout.strip()).resolve()


def _toml(value: object) -> str:
    if isinstance(value, bool):
        return "true" if value else "false"
    if isinstance(value, int):
        return str(value)
    if isinstance(value, (list, tuple)):
        return "[" + ", ".join(_toml(item) for item in value) + "]"
    text = "" if value is None else str(value)
    return json.dumps(text)


def _utc_now() -> str:
    return datetime.now(timezone.utc).strftime("%Y-%m-%dT%H:%M:%SZ")


def _command_path(name: str) -> str:
    return shutil.which(name) or ""


def _version(argv: list[str]) -> str:
    if not argv or not shutil.which(argv[0]):
        return ""
    try:
        result = subprocess.run(argv, text=True, capture_output=True, timeout=5, check=False)
    except Exception:
        return ""
    output = (result.stdout or result.stderr).splitlines()
    return output[0].strip() if output else ""


def _python_version(command: str) -> str:
    if not command:
        return ""
    try:
        result = subprocess.run(
            [command, "-c", "import sys; print('.'.join(map(str, sys.version_info[:3])))"],
            text=True,
            capture_output=True,
            timeout=5,
            check=False,
        )
    except Exception:
        return ""
    return result.stdout.strip()


def _select_python(repo_root: Path) -> str:
    for candidate in (
        repo_root / ".venv" / "bin" / "python",
        repo_root / ".venv" / "bin" / "python3",
    ):
        if candidate.exists() and os.access(candidate, os.X_OK):
            return str(candidate)
    for command in ("python3", "python"):
        path = _command_path(command)
        if path:
            return path
    return ""


def _relative_repo_dir(repo_root: Path, *parts: str) -> str:
    path = repo_root.joinpath(*parts)
    try:
        return path.relative_to(repo_root).as_posix()
    except ValueError:
        return path.as_posix()


def _read_pyproject_python(repo_root: Path) -> str:
    pyproject_path = repo_root / "pyproject.toml"
    try:
        text = pyproject_path.read_text(encoding="utf-8")
    except FileNotFoundError:
        return ""
    for line in text.splitlines():
        stripped = line.strip()
        if stripped.startswith("requires-python") and "=" in stripped:
            return stripped.split("=", 1)[1].strip().strip('"').strip("'")
    return ""


def _tool_source(path: str, repo_root: Path) -> str:
    if not path:
        return "missing"
    try:
        resolved = Path(path).resolve()
        if resolved.is_relative_to((repo_root / ".venv").resolve()):
            return "repo-venv"
        if resolved.is_relative_to((repo_root / "node_modules").resolve()):
            return "repo-node_modules"
        if resolved.is_relative_to((repo_root / "web" / "node_modules").resolve()):
            return "repo-node_modules"
    except Exception:
        pass
    text = path.lower()
    if ".nvm" in text:
        return "nvm"
    if "conda" in text or "miniforge" in text or "anaconda" in text:
        return "conda"
    if path.startswith("/opt/homebrew") or "homebrew" in text:
        return "homebrew"
    if path.startswith(("/usr/bin", "/bin", "/usr/local/bin", "/opt/")):
        return "system"
    return "path"


def _dependency_source_contains_secret(value: str) -> bool:
    text = unquote(value).lower()
    secret_markers = (
        "_authtoken",
        "authorization",
        "bearer ",
        "password=",
        "passwd=",
        "token=",
        "secret=",
    )
    if any(marker in text for marker in secret_markers):
        return True
    try:
        parsed = urlsplit(value)
    except ValueError:
        return False
    return bool(parsed.username or parsed.password)


def _redact_dependency_source(value: str) -> str:
    if value and _dependency_source_contains_secret(value):
        return "[redacted-secret-bearing-url]"
    return value


def _os_release() -> dict[str, str]:
    path = Path("/etc/os-release")
    if not path.exists():
        return {}
    data: dict[str, str] = {}
    for line in path.read_text(encoding="utf-8", errors="ignore").splitlines():
        if "=" not in line:
            continue
        key, value = line.split("=", 1)
        data[key] = value.strip().strip('"')
    return data


def _service_manager() -> str:
    if shutil.which("systemctl"):
        return "systemd"
    if platform.system().lower() == "darwin":
        return "launchd"
    return "none"


def _is_wsl() -> bool:
    try:
        text = Path("/proc/version").read_text(encoding="utf-8", errors="ignore")
    except FileNotFoundError:
        return False
    return "microsoft" in text.lower()


def main() -> int:
    repo_root = _repo_root()
    output_path = repo_root / ".codex-helper" / "local-env.toml"
    output_path.parent.mkdir(parents=True, exist_ok=True)
    now = _utc_now()
    os_release = _os_release()
    system_name = platform.system()
    os_family = "macos" if system_name == "Darwin" else "linux" if system_name == "Linux" else "unknown"
    distribution = "macos" if os_family == "macos" else os_release.get("ID", "")
    version = platform.mac_ver()[0] if os_family == "macos" else os_release.get("VERSION_ID", "")
    tools = {
        "python": _select_python(repo_root),
        "uv": _command_path("uv"),
        "node": _command_path("node"),
        "npm": _command_path("npm"),
        "codex": _command_path("codex"),
        "git": _command_path("git"),
    }
    versions = {
        "python": _python_version(tools["python"]),
        "uv": _version(["uv", "--version"]),
        "node": _version(["node", "--version"]),
        "npm": _version(["npm", "--version"]),
        "codex": _version(["codex", "--version"]),
        "git": _version(["git", "--version"]),
    }

    lines: list[str] = []
    lines.extend(
        [
            "[meta]",
            "schema_version = 1",
            f"project_root = {_toml(str(repo_root))}",
            f"generated_at = {_toml(now)}",
            'generated_by = "project-env-baseline/scripts/detect_env.py"',
            f"last_verified_at = {_toml(now)}",
            "",
            "[system]",
            f"os = {_toml(system_name)}",
            f"os_family = {_toml(os_family)}",
            f"distribution = {_toml(distribution)}",
            f"version = {_toml(version)}",
            f"arch = {_toml(platform.machine())}",
            f"shell = {_toml(os.environ.get('SHELL', ''))}",
            f"is_wsl = {_toml(_is_wsl())}",
            f"service_manager = {_toml(_service_manager())}",
            "",
        ]
    )
    for name, path in tools.items():
        lines.extend(
            [
                f"[tools.{name}]",
                f"command = {_toml(path)}",
                f"path = {_toml(path)}",
                f"version = {_toml(versions[name])}",
                f"source = {_toml(_tool_source(path, repo_root))}",
                f"verified_at = {_toml(now)}",
                "",
            ]
        )
    lines.extend(
        [
            "[requirements]",
            f"python = {_toml(_read_pyproject_python(repo_root))}",
            f"node_major = {_toml(os.environ.get('CODEX_HELPER_NODE_MAJOR', ''))}",
            f"codex_package = {_toml(os.environ.get('CODEX_HELPER_CODEX_PACKAGE_SPEC', ''))}",
            'python_source = "pyproject.toml"',
            'node_source = ""',
            "",
            "[paths]",
            f"python_bin_dir = {_toml(_relative_repo_dir(repo_root, '.venv', 'bin'))}",
            f"node_bin_dir = {_toml(_relative_repo_dir(repo_root, 'web', 'node_modules', '.bin') if (repo_root / 'web' / 'package.json').exists() else _relative_repo_dir(repo_root, 'node_modules', '.bin'))}",
            f"extra_bin_dirs = {_toml([])}",
            f"venv_dir = {_toml(_relative_repo_dir(repo_root, '.venv'))}",
            f"state_dir = {_toml('')}",
            "",
        ]
    )
    package_managers = {
        "homebrew": "brew",
        "apt": "apt-get",
        "dnf": "dnf",
        "yum": "yum",
        "nvm": "nvm",
        "conda": "conda",
    }
    for name, command in package_managers.items():
        path = _command_path(command)
        lines.extend(
            [
                f"[package_managers.{name}]",
                f"available = {_toml(bool(path))}",
                f"path = {_toml(path)}",
                "",
            ]
        )
    lines.extend(
        [
            "[dependency_sources]",
            f"docker_registry_mirror = {_toml(_redact_dependency_source(os.environ.get('CODEX_HELPER_DOCKER_REGISTRY_MIRROR', '')))}",
            f"docker_image_prefix = {_toml(_redact_dependency_source(os.environ.get('CODEX_HELPER_DOCKER_IMAGE_PREFIX', '')))}",
            f"docker_base_image = {_toml(_redact_dependency_source(os.environ.get('CODEX_HELPER_DOCKER_BASE_IMAGE', '')))}",
            f"node_docker_image = {_toml(_redact_dependency_source(os.environ.get('CODEX_HELPER_NODE_DOCKER_IMAGE', '')))}",
            f"docker_vm_image = {_toml(_redact_dependency_source(os.environ.get('CODEX_HELPER_DOCKER_VM_IMAGE', '')))}",
            f"debian_mirror = {_toml(_redact_dependency_source(os.environ.get('CODEX_HELPER_DEBIAN_MIRROR', '')))}",
            f"node_apt_repo = {_toml(_redact_dependency_source(os.environ.get('CODEX_HELPER_NODE_APT_REPO', '')))}",
            f"npm_registry = {_toml(_redact_dependency_source(os.environ.get('CODEX_HELPER_NPM_REGISTRY', '')))}",
            f"npm_registry_mirror = {_toml(_redact_dependency_source(os.environ.get('CODEX_HELPER_NPM_REGISTRY_MIRROR', '')))}",
            f"uv_index = {_toml(_redact_dependency_source(os.environ.get('UV_DEFAULT_INDEX') or os.environ.get('CODEX_HELPER_UV_DEFAULT_INDEX', '')))}",
            f"pip_index = {_toml(_redact_dependency_source(os.environ.get('PIP_INDEX_URL', '')))}",
            f"pip_extra_index = {_toml(_redact_dependency_source(os.environ.get('PIP_EXTRA_INDEX_URL', '')))}",
            f"python_install_mirror = {_toml(_redact_dependency_source(os.environ.get('CODEX_HELPER_UV_PYTHON_INSTALL_MIRROR', '')))}",
            f"uv_install_url = {_toml(_redact_dependency_source(os.environ.get('CODEX_HELPER_UV_INSTALL_URL', '')))}",
            f"node_install_mirror = {_toml(_redact_dependency_source(os.environ.get('CODEX_HELPER_NODE_MIRROR', '')))}",
            f"proxy_detected = {_toml(bool(os.environ.get('HTTPS_PROXY') or os.environ.get('https_proxy') or os.environ.get('HTTP_PROXY') or os.environ.get('http_proxy')))}",
            'source = "project-env-baseline/scripts/detect_env.py"',
            "",
        ]
    )
    lines.extend(
        [
            "[policy]",
            'dependency_strategy = "reuse-first"',
            "auto_install = false",
            "project_isolation = true",
            "",
            "[verification]",
            f"last_detect_env_at = {_toml(now)}",
            'last_doctor_at = ""',
            "drift_detected = false",
            "",
        ]
    )
    output_path.write_text("\n".join(lines), encoding="utf-8")
    print(output_path)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
