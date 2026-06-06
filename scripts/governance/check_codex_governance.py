# -*- coding: utf-8 -*-
from __future__ import annotations

import subprocess
import sys
import shutil
from pathlib import Path

REPO_ROOT = Path(__file__).resolve().parents[2]
VENV_PYTHON = REPO_ROOT / ".venv" / "bin" / "python"
CHECK_SCRIPTS = (
    "scripts/governance/check_agent_assets.py",
    "scripts/governance/check_live_doc_paths.py",
)


def _current_python_path() -> Path:
    return Path(sys.executable).resolve()


def _can_import_tomllib(python_path: Path) -> bool:
    result = subprocess.run(
        [str(python_path), "-c", "import tomllib"],
        cwd=REPO_ROOT,
        stdout=subprocess.DEVNULL,
        stderr=subprocess.DEVNULL,
        check=False,
    )
    return result.returncode == 0


def _tomllib_python_candidates() -> list[Path]:
    candidates = [VENV_PYTHON]
    for command_name in ("python3.13", "python3.12", "python3.11", "python3"):
        path = shutil.which(command_name)
        if path:
            candidates.append(Path(path))
    deduped: list[Path] = []
    seen: set[Path] = set()
    for candidate in candidates:
        try:
            resolved = candidate.resolve()
        except FileNotFoundError:
            continue
        if resolved in seen or not resolved.exists():
            continue
        seen.add(resolved)
        deduped.append(resolved)
    return deduped


def _preferred_python() -> Path:
    current = _current_python_path()
    if _can_import_tomllib(current):
        return current
    for python_path in _tomllib_python_candidates():
        if python_path != current and _can_import_tomllib(python_path):
            return python_path
    return current


def main() -> int:
    python_path = _preferred_python()
    for script_name in CHECK_SCRIPTS:
        script_path = REPO_ROOT / script_name
        print(f"[codex-governance] Running {script_name}...", flush=True)
        result = subprocess.run(
            [str(python_path), str(script_path)],
            cwd=REPO_ROOT,
            text=True,
            check=False,
        )
        if result.returncode != 0:
            return result.returncode
    print("[codex-governance] All governance checks passed.", flush=True)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
