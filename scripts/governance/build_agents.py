# -*- coding: utf-8 -*-
from __future__ import annotations

import argparse
import re
import shutil
import subprocess
import sys
from dataclasses import dataclass
from pathlib import Path
from types import ModuleType

REPO_ROOT = Path(__file__).resolve().parents[2]
VENV_PYTHON = REPO_ROOT / ".venv" / "bin" / "python"
GOVERNANCE_ROOT = REPO_ROOT / "governance"
GOVERNANCE_MANAGER_ROOT = REPO_ROOT / "governance_manager"
GOVERNANCE_MANAGER_CATALOG_PATH = GOVERNANCE_MANAGER_ROOT / "catalog.toml"
PROFILE_PATH = GOVERNANCE_ROOT / "profile.toml"
BUNDLES_PATH = GOVERNANCE_ROOT / "bundles.toml"
SKILL_SOURCE_ROOTS = (
    GOVERNANCE_ROOT / "core" / "skill",
    GOVERNANCE_ROOT / "private" / "skill",
)
GENERATED_SKILLS_ROOT = REPO_ROOT / ".codex" / "skills"
BASELINE_MANIFEST_PATH = (
    REPO_ROOT / "src" / "codex_helper" / "assets" / "agent_baseline" / "manifest.toml"
)
PLACEHOLDER_RE = re.compile(r"{{\s*([a-zA-Z0-9_.-]+)\s*}}")
EXTERNAL_SOURCE_PREFIX = "external://"
CACHE_DIR_NAMES = {"__pycache__", ".pytest_cache", ".ruff_cache", ".mypy_cache"}
CACHE_FILE_SUFFIXES = {".pyc", ".pyo"}


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


def _reexec_with_venv_if_needed() -> ModuleType:
    try:
        import tomllib as _tomllib
    except ModuleNotFoundError:
        current = _current_python_path()
        for python_path in _tomllib_python_candidates():
            if python_path == current:
                continue
            if _can_import_tomllib(python_path):
                raise SystemExit(
                    subprocess.run(
                        [str(python_path), __file__, *sys.argv[1:]],
                        cwd=REPO_ROOT,
                        check=False,
                    ).returncode
                ) from None
        raise SystemExit(
            "Python 3.11+ or another Python with tomllib is required for governance generation."
        )
    return _tomllib


tomllib = _reexec_with_venv_if_needed()


@dataclass(frozen=True)
class BundleSpec:
    name: str
    title: str
    output: str | None
    includes: tuple[str, ...]
    extends: tuple[str, ...]
    exclude_includes: tuple[str, ...]
    emit: bool


def _load_toml(path: Path) -> dict:
    return tomllib.loads(path.read_text(encoding="utf-8"))


def load_profile(path: Path = PROFILE_PATH) -> dict:
    profile = _load_toml(path)
    template = profile.get("template")
    if not isinstance(template, dict) or not template.get("version"):
        raise ValueError(f"Missing template.version in {path}")
    profile.setdefault("render", {})
    profile["render"].setdefault("include_fragment_markers", False)
    return profile


def load_bundles(path: Path = BUNDLES_PATH) -> list[BundleSpec]:
    raw = _load_toml(path).get("bundles", {})
    bundles: list[BundleSpec] = []
    for name, payload in raw.items():
        emit = bool(payload.get("emit", True))
        bundles.append(
            BundleSpec(
                name=name,
                title=payload["title"],
                output=payload.get("output"),
                includes=tuple(payload.get("includes", ())),
                extends=tuple(payload.get("extends", ())),
                exclude_includes=tuple(payload.get("exclude_includes", ())),
                emit=emit,
            )
        )
    return bundles


def _resolve_placeholder(profile: dict, key: str) -> str:
    value: object = profile
    for part in key.split("."):
        if not isinstance(value, dict) or part not in value:
            raise KeyError(f"Unknown profile placeholder: {key}")
        value = value[part]
    if isinstance(value, (list, tuple)):
        return ", ".join(str(item) for item in value)
    return str(value)


def render_fragment(source_path: Path, profile: dict) -> str:
    raw = source_path.read_text(encoding="utf-8").strip()

    def replace(match: re.Match[str]) -> str:
        return _resolve_placeholder(profile, match.group(1))

    return PLACEHOLDER_RE.sub(replace, raw)


def _strip_duplicate_bundle_heading(rendered: str, title: str) -> str:
    lines = rendered.splitlines()
    if not lines:
        return rendered
    if lines[0].strip() != f"# {title}":
        return rendered
    remaining = "\n".join(lines[1:]).lstrip()
    return remaining or rendered


def _bundle_map() -> dict[str, BundleSpec]:
    return {bundle.name: bundle for bundle in load_bundles()}


def _dedupe(items: list[str]) -> list[str]:
    return list(dict.fromkeys(items))


def resolve_bundle_includes(bundle_name: str, bundle_map: dict[str, BundleSpec]) -> tuple[str, ...]:
    resolved: dict[str, tuple[str, ...]] = {}
    resolving: set[str] = set()

    def walk(name: str) -> tuple[str, ...]:
        if name in resolved:
            return resolved[name]
        if name in resolving:
            raise ValueError(f"Bundle inheritance cycle detected at {name}")
        if name not in bundle_map:
            raise ValueError(f"Unknown bundle reference: {name}")
        resolving.add(name)
        bundle = bundle_map[name]
        includes: list[str] = []
        for parent in bundle.extends:
            includes.extend(walk(parent))
        includes.extend(bundle.includes)
        deduped = _dedupe(includes)
        if bundle.exclude_includes:
            exclude_set = set(bundle.exclude_includes)
            deduped = [item for item in deduped if item not in exclude_set]
        resolved[name] = tuple(deduped)
        resolving.remove(name)
        return resolved[name]

    return walk(bundle_name)


def render_bundle(
    bundle: BundleSpec,
    profile: dict,
    resolved_includes: tuple[str, ...],
    include_fragment_markers: bool,
) -> str:
    lines = [
        "<!-- GENERATED FILE. DO NOT EDIT DIRECTLY. -->",
        f"<!-- source bundle: {bundle.name} -->",
        "<!-- generated by: scripts/governance/build_agents.py -->",
        "",
        f"# {bundle.title}",
        "",
    ]

    for include in resolved_includes:
        source_path = GOVERNANCE_ROOT / include
        rendered = _strip_duplicate_bundle_heading(
            render_fragment(source_path, profile),
            bundle.title,
        )
        if include_fragment_markers:
            lines.append(f"<!-- BEGIN {include} -->")
        lines.append(rendered)
        if include_fragment_markers:
            lines.append(f"<!-- END {include} -->")
        lines.append("")

    return "\n".join(lines).rstrip() + "\n"


def iter_selected_bundles(selected: set[str] | None = None) -> list[BundleSpec]:
    bundles = [bundle for bundle in load_bundles() if bundle.emit]
    if not selected:
        return bundles
    return [bundle for bundle in bundles if bundle.name in selected]


def write_bundle(bundle: BundleSpec, content: str) -> None:
    if bundle.output is None:
        raise ValueError(f"Bundle {bundle.name} has no output path")
    output_path = REPO_ROOT / bundle.output
    output_path.parent.mkdir(parents=True, exist_ok=True)
    output_path.write_text(content, encoding="utf-8")


def check_bundle(bundle: BundleSpec, content: str) -> str | None:
    if bundle.output is None:
        return f"generated AGENTS output missing target path for bundle: {bundle.name}"
    output_path = REPO_ROOT / bundle.output
    if not output_path.exists():
        return f"generated AGENTS output missing: {output_path}"
    existing = output_path.read_text(encoding="utf-8")
    if existing != content:
        return f"generated AGENTS output is stale: {output_path}"
    return None


def validate_bundles(bundle_map: dict[str, BundleSpec]) -> None:
    for bundle in bundle_map.values():
        if bundle.emit and not bundle.output:
            raise ValueError(f"Renderable bundle missing output: {bundle.name}")
        if bundle.output and not bundle.output.endswith("AGENTS.md"):
            raise ValueError(f"Bundle output must target AGENTS.md: {bundle.name} -> {bundle.output}")
        for parent in bundle.extends:
            if parent not in bundle_map:
                raise ValueError(f"Bundle {bundle.name} extends unknown bundle {parent}")
        for include in bundle.includes + bundle.exclude_includes:
            source_path = GOVERNANCE_ROOT / include
            if not source_path.exists():
                raise ValueError(f"Bundle {bundle.name} references missing source fragment {include}")


def _iter_skill_source_dirs() -> list[Path]:
    skill_dirs: list[Path] = []
    default_skill_names = _load_default_skill_names()
    for root in SKILL_SOURCE_ROOTS:
        if not root.exists():
            continue
        for path in sorted(root.iterdir()):
            if not path.is_dir() or not (path / "SKILL.md").exists():
                continue
            if (
                default_skill_names is not None
                and root.name == "skill"
                and root.parent.name == "core"
                and path.name not in default_skill_names
            ):
                continue
            skill_dirs.append(path)
    return skill_dirs


def _load_default_skill_names() -> set[str] | None:
    try:
        payload = _load_toml(BASELINE_MANIFEST_PATH)
    except FileNotFoundError:
        return None
    raw_default_skills = payload.get("default_skills", [])
    if not isinstance(raw_default_skills, list):
        return set()
    return {item.strip() for item in raw_default_skills if isinstance(item, str) and item.strip()}


def _read_tree(root: Path) -> dict[Path, bytes]:
    return {
        path.relative_to(root): path.read_bytes()
        for path in sorted(root.rglob("*"))
        if path.is_file() and "__pycache__" not in path.parts and path.suffix not in {".pyc", ".pyo"}
    }


def _should_skip_template_path(path: Path) -> bool:
    return bool(set(path.parts) & CACHE_DIR_NAMES) or path.suffix in CACHE_FILE_SUFFIXES


def _expected_governance_manager_core_tree() -> tuple[dict[Path, bytes], list[str]]:
    if not GOVERNANCE_MANAGER_CATALOG_PATH.exists():
        return {}, []

    failures: list[str] = []
    expected: dict[Path, bytes] = {}
    try:
        catalog = _load_toml(GOVERNANCE_MANAGER_CATALOG_PATH)
    except tomllib.TOMLDecodeError as exc:
        return {}, [f"invalid governance_manager catalog: {GOVERNANCE_MANAGER_CATALOG_PATH}: {exc}"]

    raw_templates = catalog.get("templates", [])
    if not isinstance(raw_templates, list):
        return {}, [f"governance_manager catalog must use [[templates]]: {GOVERNANCE_MANAGER_CATALOG_PATH}"]

    manager_root = GOVERNANCE_MANAGER_ROOT.resolve()
    for raw_template in raw_templates:
        if not isinstance(raw_template, dict):
            failures.append(f"governance_manager catalog contains a non-table template entry: {GOVERNANCE_MANAGER_CATALOG_PATH}")
            continue
        kind = raw_template.get("kind")
        module_id = raw_template.get("id")
        source_path = raw_template.get("source_path")
        sync_path = raw_template.get("sync_path")
        if kind not in {"agent", "skill"}:
            failures.append(f"governance_manager template has unsupported kind: {module_id}")
            continue
        if not isinstance(module_id, str) or not module_id.strip():
            failures.append(f"governance_manager template missing id: {GOVERNANCE_MANAGER_CATALOG_PATH}")
            continue
        if not isinstance(source_path, str) or not source_path.strip():
            failures.append(f"governance_manager template missing source_path: {kind}/{module_id}")
            continue
        if not isinstance(sync_path, str) or not sync_path.strip():
            failures.append(f"governance_manager template missing sync_path: {kind}/{module_id}")
            continue
        if source_path.startswith(EXTERNAL_SOURCE_PREFIX):
            continue
        if not sync_path.startswith(f"governance/core/{kind}/"):
            failures.append(
                f"governance_manager template sync_path must target governance/core/{kind}: "
                f"{kind}/{module_id} -> {sync_path}"
            )
            continue
        source_file = (GOVERNANCE_MANAGER_ROOT / source_path).resolve()
        try:
            source_file.relative_to(manager_root)
        except ValueError:
            failures.append(f"governance_manager template source escapes root: {kind}/{module_id} -> {source_path}")
            continue
        if not source_file.is_file():
            failures.append(f"governance_manager template source is missing: {kind}/{module_id} -> {source_path}")
            continue

        core_relative = Path(sync_path).relative_to("governance/core")
        if kind == "agent":
            expected[core_relative] = source_file.read_bytes()
            continue

        source_dir = source_file.parent
        for path in sorted(source_dir.rglob("*")):
            if not path.is_file() or _should_skip_template_path(path):
                continue
            expected[core_relative / path.relative_to(source_dir)] = path.read_bytes()

    return expected, failures


def _sync_governance_manager_core(*, check_only: bool) -> list[str]:
    expected_tree, failures = _expected_governance_manager_core_tree()
    if not GOVERNANCE_MANAGER_CATALOG_PATH.exists():
        return failures

    core_root = GOVERNANCE_ROOT / "core"
    for relative_path, content in expected_tree.items():
        target_path = core_root / relative_path
        if check_only:
            if not target_path.exists():
                failures.append(f"governance core output missing from governance_manager: {target_path}")
            elif target_path.read_bytes() != content:
                failures.append(f"governance core output is stale from governance_manager: {target_path}")
            continue
        target_path.parent.mkdir(parents=True, exist_ok=True)
        if not target_path.exists() or target_path.read_bytes() != content:
            target_path.write_bytes(content)

    actual_files = {
        path.relative_to(core_root)
        for root in (core_root / "agent", core_root / "skill")
        if root.exists()
        for path in sorted(root.rglob("*"))
        if path.is_file() and not _should_skip_template_path(path)
    }
    for relative_path in sorted(actual_files - set(expected_tree)):
        target_path = core_root / relative_path
        if check_only:
            failures.append(f"governance core output is not sourced from governance_manager: {target_path}")
            continue
        target_path.unlink()

    if not check_only:
        for root in (core_root / "agent", core_root / "skill"):
            if not root.exists():
                continue
            for path in sorted(root.rglob("*"), reverse=True):
                if path.is_dir() and not any(path.iterdir()):
                    path.rmdir()

    return failures


def _sync_skill_outputs(*, check_only: bool) -> list[str]:
    failures: list[str] = []
    seen_names: set[str] = set()
    expected_output_dirs: set[Path] = set()
    for source_dir in _iter_skill_source_dirs():
        skill_name = source_dir.name
        if skill_name in seen_names:
            failures.append(f"Duplicate governance skill source: {skill_name}")
            continue
        seen_names.add(skill_name)
        output_dir = GENERATED_SKILLS_ROOT / skill_name
        expected_output_dirs.add(output_dir.resolve())
        source_tree = _read_tree(source_dir)
        if check_only:
            if not output_dir.exists():
                failures.append(f"generated skill output missing: {output_dir}")
                continue
            output_tree = _read_tree(output_dir)
            if output_tree != source_tree:
                failures.append(f"generated skill output is stale: {output_dir}")
            continue

        output_dir.mkdir(parents=True, exist_ok=True)
        current_tree = _read_tree(output_dir) if output_dir.exists() else {}
        for extra_path in sorted(set(current_tree) - set(source_tree)):
            (output_dir / extra_path).unlink()
        for relative_path, content in source_tree.items():
            target_path = output_dir / relative_path
            target_path.parent.mkdir(parents=True, exist_ok=True)
            target_path.write_bytes(content)
    if not check_only and GENERATED_SKILLS_ROOT.exists():
        for output_dir in sorted(GENERATED_SKILLS_ROOT.iterdir()):
            if not output_dir.is_dir():
                continue
            if output_dir.resolve() in expected_output_dirs:
                continue
            shutil.rmtree(output_dir)
    if check_only and GENERATED_SKILLS_ROOT.exists():
        for output_dir in sorted(GENERATED_SKILLS_ROOT.iterdir()):
            if not output_dir.is_dir():
                continue
            if output_dir.resolve() in expected_output_dirs:
                continue
            if not (output_dir / "SKILL.md").exists():
                continue
            failures.append(f"generated skill output is not sourced from governance: {output_dir}")
    return failures


def _prune_stale_agents_outputs(expected_outputs: set[Path]) -> None:
    for path in sorted(REPO_ROOT.rglob("AGENTS.md")):
        if "node_modules" in path.parts:
            continue
        if path.resolve() in expected_outputs:
            continue
        path.unlink()


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description="Render runtime AGENTS.md files and managed skills from governance/")
    parser.add_argument(
        "--check",
        action="store_true",
        help="verify generated files are current instead of writing them",
    )
    parser.add_argument(
        "--debug-markers",
        action="store_true",
        help="include fragment boundary comments in generated output",
    )
    parser.add_argument(
        "--bundle",
        action="append",
        dest="bundles",
        help="render or check only the named bundle",
    )
    args = parser.parse_args(argv)

    selected = set(args.bundles or [])

    failures: list[str] = []
    failures.extend(_sync_governance_manager_core(check_only=args.check))

    try:
        profile = load_profile()
        bundle_map = _bundle_map()
        validate_bundles(bundle_map)
        bundles = iter_selected_bundles(selected or None)
    except ValueError as exc:
        print(str(exc), file=sys.stderr)
        return 1

    if selected and len(bundles) != len(selected):
        known = {bundle.name for bundle in iter_selected_bundles()}
        missing = ", ".join(sorted(selected - known))
        print(f"Unknown bundle name(s): {missing}", file=sys.stderr)
        return 1

    include_fragment_markers = bool(profile["render"]["include_fragment_markers"]) or args.debug_markers
    expected_outputs: set[Path] = set()
    for bundle in bundles:
        resolved_includes = resolve_bundle_includes(bundle.name, bundle_map)
        content = render_bundle(
            bundle,
            profile,
            resolved_includes,
            include_fragment_markers=include_fragment_markers,
        )
        if args.check:
            failure = check_bundle(bundle, content)
            if failure:
                failures.append(failure)
            continue
        if bundle.output is not None:
            expected_outputs.add((REPO_ROOT / bundle.output).resolve())
        write_bundle(bundle, content)
        print(f"Rendered {bundle.output}")

    failures.extend(_sync_skill_outputs(check_only=args.check))
    if not args.check:
        _prune_stale_agents_outputs(expected_outputs)

    if failures:
        print("Agent bundle check failed:")
        for failure in failures:
            print(f"- {failure}")
        return 1

    if args.check:
        print("Agent bundle check passed.")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
