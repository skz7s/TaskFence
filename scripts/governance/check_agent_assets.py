# -*- coding: utf-8 -*-
from __future__ import annotations

import re
import shutil
import subprocess
import sys
from dataclasses import dataclass
from pathlib import Path
from types import ModuleType

REPO_ROOT = Path(__file__).resolve().parents[2]
VENV_PYTHON = REPO_ROOT / ".venv" / "bin" / "python"
AGENT_ROOT = REPO_ROOT / "governance"
SKILLS_ROOT = REPO_ROOT / ".codex" / "skills"
BASELINE_OVERRIDE_SKILLS_ROOT = (
    REPO_ROOT / "src" / "codex_helper" / "assets" / "agent_baseline" / "overrides" / ".codex" / "skills"
)
BASELINE_ROOT = REPO_ROOT / "src" / "codex_helper" / "assets" / "agent_baseline"
BASELINE_OVERRIDES_ROOT = BASELINE_ROOT / "overrides"
BASELINE_MANIFEST_PATH = BASELINE_ROOT / "manifest.toml"
SKILL_ROUTING_PATH = AGENT_ROOT / "skill-routing.toml"
SKILL_ROUTING_DOC_PATH = AGENT_ROOT / "private" / "agent" / "project-governance.md"

BASELINE_REQUIRED_RELATIVE_PATHS = (
    Path("README.md"),
    Path("governance/README.md"),
    Path("governance/change-map.md"),
    Path("governance/modules.toml"),
    Path("governance/skill-maintenance.md"),
    Path("governance/skill-routing.toml"),
    Path("governance/profile.toml"),
    Path("governance/bundles.toml"),
    Path("governance/core/agent"),
    Path("governance/core/skill"),
    Path("governance/private/agent"),
    Path("docs/codex/plans"),
    Path("docs/codex/plan_archived/.gitkeep"),
    Path("docs/codex/project-structure.md"),
    Path("docs/codex/structure-contract.md"),
    Path("docs/codex/runtime-architecture.md"),
    Path("docs/config/cross-platform-ops.md"),
    Path("docs/config/env-inventory.md"),
    Path("scripts/governance/check_live_doc_paths.py"),
    Path("scripts/governance/check_codex_governance.py"),
    Path("scripts/governance/check_agent_assets.py"),
    Path("scripts/governance/build_agents.py"),
    Path("scripts/system/run_io_limited.sh"),
    Path("scripts/test/run_io_limited.sh"),
)
SOURCE_REPO_ONLY_RELATIVE_PATHS = (
    Path("deploy/manage.sh"),
    Path("deploy/setup.sh"),
    Path("deploy/build.sh"),
    Path("deploy/deploy.sh"),
    Path("governance_manager"),
)
SOURCE_REPO_ONLY_BASELINE_FORCE_INCLUDE_PATHS = (
    Path("scripts/test/run_backend_checks.sh"),
    Path("scripts/test/run_full_validation.sh"),
    Path("governance/core/agent"),
    Path("governance/core/skill"),
    Path("docs/codex/plans"),
)
GOVERNANCE_MANAGER_REQUIRED_RELATIVE_PATHS = (
    Path("governance_manager/README.md"),
    Path("governance_manager/template-authoring.md"),
    Path("governance_manager/database-sync.md"),
    Path("governance_manager/catalog.toml"),
    Path("governance_manager/templates/agent"),
    Path("governance_manager/templates/skill"),
)
SOURCE_REPO_MARKERS = (
    Path("src/codex_helper/assets/agent_baseline/manifest.toml"),
)

SCOPE_RE = re.compile(r"Apply these rules to files under `([^`]+)`")
PATH_RE = re.compile(
    r"`((?:AGENTS\.md|README\.md|deploy/[^`]+|docs/[^`]+|governance/[^`]+|governance_manager/[^`]+|scripts/[^`]+|\.codex/[^`]+))`"
)
SKILL_NAME_RE = re.compile(r"`([a-z0-9][a-z0-9-]+)`")
SKILL_TAG_RE = re.compile(r"^[a-z0-9][a-z0-9/_-]*$")
OPENAI_YAML_REQUIRED_KEYS = (
    "display_name:",
    "short_description:",
    "default_prompt:",
)
SKILL_CATEGORIES = {"surface", "workflow", "provider"}
BLOCKED_BASELINE_MANIFEST_PATHS = {
    Path("governance/core/agent"),
    Path("governance/core/skill"),
}
RETIRED_RUNTIME_PATH_REFERENCES = {
    "deploy/setup.sh",
    "deploy/deploy.sh",
    "deploy/build.sh",
}
BLOCKED_POLICY_PATTERNS = (
    ("plan-first workflow", "The default development workflow is Codex plan-first"),
    ("prototype-first UI workflow", "prototype-first"),
    ("mandatory UI bitmap prototype", "Generate a bitmap prototype or mockup first"),
    ("external governance source registry", "governance_manager/sources.toml"),
    ("external template source refresh", "refresh_template_sources.py"),
)
HOST_SPECIFIC_PATTERNS = (
    "/Users/",
    "/opt/homebrew/bin/",
    'shell = "/bin/zsh"',
    'command = "/opt/homebrew/',
    'path = "/opt/homebrew/',
)
HOST_SPECIFIC_SCAN_RELATIVE_ROOTS = (
    Path("governance"),
    Path("governance_manager"),
    Path("src/codex_helper/assets/agent_baseline/overrides"),
)
EXTERNAL_SOURCE_PREFIX = "external://"
TEMPLATE_CACHE_FILE_PATTERNS = ("*.pyc", "*.pyo")
TEMPLATE_CACHE_DIR_NAMES = {"__pycache__", ".pytest_cache", ".ruff_cache", ".mypy_cache"}
PUBLIC_TEMPLATE_HELPER_LEAK_PATTERNS = (
    (
        "helper-specific public dev-flow name",
        "codex-helper-dev-flow",
    ),
    (
        "helper-specific public repository wording",
        "codex-helper repository",
    ),
    (
        "helper-specific public template source workflow",
        "When changing reusable helper templates",
    ),
)
BASELINE_HELPER_LEAK_PATTERNS = (
    (
        "helper source repository structure",
        "`governance_manager/`",
    ),
    (
        "helper-specific default dev-flow",
        "codex-helper-dev-flow",
    ),
    (
        "helper product boundary",
        "provider、Assistant、project Codex",
    ),
    (
        "helper source-repo maintenance workflow",
        "helper 源仓库维护必须直接编辑",
    ),
    (
        "helper source-repo sync input",
        "codex-helper` source repository",
    ),
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
            "Python 3.11+ or another Python with tomllib is required for governance asset checks."
        )
    return _tomllib


tomllib = _reexec_with_venv_if_needed()


@dataclass(frozen=True)
class SkillRoutingSpec:
    name: str
    category: str
    routing_tags: tuple[str, ...]
    narrower_than: tuple[str, ...]
    pairs_with: tuple[str, ...]


def _iter_agents_files() -> list[Path]:
    return sorted(
        path
        for path in REPO_ROOT.rglob("AGENTS.md")
        if "__pycache__" not in path.parts and "node_modules" not in path.parts
    )


def _iter_agent_source_docs() -> list[Path]:
    return sorted(path for path in AGENT_ROOT.rglob("*.md") if path.is_file())


def _iter_stable_docs() -> list[Path]:
    files: list[Path] = []
    for root in (
        REPO_ROOT / "docs" / "codex",
        REPO_ROOT / "docs" / "config",
    ):
        if not root.exists():
            continue
        files.extend(sorted(path for path in root.glob("*.md") if path.is_file()))
    return files


def _iter_text_files() -> list[Path]:
    files = _iter_agents_files()
    files.extend(_iter_agent_source_docs())
    files.extend(_iter_stable_docs())
    files.extend(sorted(SKILLS_ROOT.glob("**/SKILL.md")))
    files.extend(sorted(BASELINE_OVERRIDE_SKILLS_ROOT.glob("**/SKILL.md")))
    if _is_source_repo(REPO_ROOT):
        files.extend(sorted((REPO_ROOT / "governance_manager" / "templates" / "skill").glob("*/SKILL.md")))
    return sorted(dict.fromkeys(files))


def _should_skip_reference(reference: str) -> bool:
    normalized = reference.strip().rstrip("/")
    if normalized in RETIRED_RUNTIME_PATH_REFERENCES and not _is_source_repo(REPO_ROOT):
        return True
    return any(token in reference for token in ("<", ">", "{", "}", "*", "|")) or any(
        char.isspace() for char in reference
    )


def _resolve_reference(reference: str) -> Path:
    return REPO_ROOT / reference.rstrip("/")


def _resolve_text_reference(text_path: Path, reference: str) -> Path:
    if reference == "deploy/manage.sh" and not _is_source_repo(REPO_ROOT):
        return text_path
    if ".codex" in text_path.parts and "skills" in text_path.parts:
        try:
            skill_index = text_path.parts.index("skills") + 1
            skill_root = Path(*text_path.parts[: skill_index + 1])
        except (ValueError, IndexError):
            skill_root = text_path.parent
        candidate = skill_root / reference.rstrip("/")
        if candidate.exists():
            return candidate
    if "governance" in text_path.parts and "core" in text_path.parts and "skill" in text_path.parts:
        try:
            skill_index = text_path.parts.index("skill") + 1
            skill_root = Path(*text_path.parts[: skill_index + 1])
        except (ValueError, IndexError):
            skill_root = text_path.parent
        candidate = skill_root / reference.rstrip("/")
        if candidate.exists():
            return candidate
    return _resolve_reference(reference)


def _load_toml(path: Path) -> dict:
    return tomllib.loads(path.read_text(encoding="utf-8"))


def _normalize_repo_path(repo_root: Path, raw_path: str) -> Path:
    candidate = Path(raw_path).expanduser()
    if not candidate.is_absolute():
        candidate = repo_root / candidate
    return candidate.resolve()


def _is_source_repo(repo_root: Path) -> bool:
    return all((repo_root / marker).exists() for marker in SOURCE_REPO_MARKERS)


def iter_required_paths(*, repo_root: Path = REPO_ROOT) -> tuple[Path, ...]:
    relative_paths = list(BASELINE_REQUIRED_RELATIVE_PATHS)
    if _is_source_repo(repo_root):
        relative_paths.extend(SOURCE_REPO_ONLY_RELATIVE_PATHS)
    return tuple((repo_root / relative_path).resolve() for relative_path in relative_paths)


def collect_force_include_violations(violations: set[str], *, repo_root: Path = REPO_ROOT) -> None:
    pyproject_path = repo_root / "pyproject.toml"
    if not pyproject_path.exists():
        return

    try:
        payload = _load_toml(pyproject_path)
    except (FileNotFoundError, tomllib.TOMLDecodeError) as exc:
        violations.add(f"invalid pyproject.toml for wheel force-include validation: {pyproject_path}")
        if isinstance(exc, tomllib.TOMLDecodeError):
            return
        return

    tool = payload.get("tool", {})
    hatch = tool.get("hatch", {}) if isinstance(tool, dict) else {}
    build = hatch.get("build", {}) if isinstance(hatch, dict) else {}
    targets = build.get("targets", {}) if isinstance(build, dict) else {}
    wheel = targets.get("wheel", {}) if isinstance(targets, dict) else {}
    force_include = wheel.get("force-include", {}) if isinstance(wheel, dict) else {}
    if force_include in ({}, None):
        return
    if not isinstance(force_include, dict):
        violations.add(f"wheel force-include must be a TOML table in {pyproject_path}")
        return

    package_root = (repo_root / "src" / "codex_helper").resolve()
    target_sources: dict[str, str] = {}
    for source, target in force_include.items():
        if not isinstance(source, str) or not source.strip():
            violations.add(f"wheel force-include has an invalid source entry in {pyproject_path}")
            continue
        if not isinstance(target, str) or not target.strip():
            violations.add(f"wheel force-include has an invalid target entry for {source} in {pyproject_path}")
            continue

        normalized_source = _normalize_repo_path(repo_root, source.strip())
        normalized_target = Path(target.strip()).as_posix()
        relative_source = normalized_source.relative_to(repo_root.resolve()).as_posix()
        if not normalized_source.exists():
            violations.add(f"wheel force-include source path missing in {pyproject_path}: {source.strip()}")
        if normalized_source == package_root or package_root in normalized_source.parents:
            violations.add(
                "wheel force-include duplicates packaged src/codex_helper content "
                f"in {pyproject_path}: {source.strip()}"
            )
        if Path(relative_source) in SOURCE_REPO_ONLY_BASELINE_FORCE_INCLUDE_PATHS:
            violations.add(
                "wheel force-include includes source-repo-only managed baseline file "
                f"in {pyproject_path}: {source.strip()}"
            )
        if (
            Path(relative_source) in {Path("governance_manager")}
            and normalized_target != "codex_helper/assets/governance_manager"
        ):
            violations.add(
                "wheel force-include must package governance_manager as the reusable template source "
                f"in {pyproject_path}: {source.strip()} -> {normalized_target}"
            )

        previous_source = target_sources.get(normalized_target)
        if previous_source is not None:
            violations.add(
                "wheel force-include target path is duplicated in "
                f"{pyproject_path}: {normalized_target} ({previous_source}, {source.strip()})"
            )
            continue
        target_sources[normalized_target] = source.strip()


def _iter_markdown_and_toml_files(root: Path) -> list[Path]:
    if not root.exists():
        return []
    return sorted(
        path
        for path in root.rglob("*")
        if path.is_file()
        and "__pycache__" not in path.parts
        and path.suffix in {".md", ".toml", ".yaml", ".yml"}
    )


def collect_governance_manager_template_violations(
    violations: set[str],
    *,
    repo_root: Path = REPO_ROOT,
) -> None:
    manager_root = repo_root / "governance_manager"
    catalog_path = manager_root / "catalog.toml"
    if not catalog_path.exists():
        return

    try:
        catalog = _load_toml(catalog_path)
    except tomllib.TOMLDecodeError:
        violations.add(f"invalid governance manager catalog: {catalog_path}")
        return

    raw_templates = catalog.get("templates", [])
    if not isinstance(raw_templates, list):
        violations.add(f"governance manager catalog must use [[templates]] entries: {catalog_path}")
        return

    catalog_keys: set[tuple[str, str]] = set()
    catalog_sources: set[Path] = set()
    catalog_skill_tiers: dict[str, str] = {}
    templates_root = manager_root / "templates"
    if templates_root.exists():
        for path in sorted(templates_root.rglob("*")):
            if path.is_dir() and path.name in TEMPLATE_CACHE_DIR_NAMES:
                violations.add(f"governance manager template tree contains cache directory: {path}")
            if path.is_file() and any(path.match(pattern) for pattern in TEMPLATE_CACHE_FILE_PATTERNS):
                violations.add(f"governance manager template tree contains generated cache file: {path}")

    for raw_template in raw_templates:
        if not isinstance(raw_template, dict):
            violations.add(f"governance manager catalog contains a non-table template entry: {catalog_path}")
            continue

        module_id = raw_template.get("id")
        kind = raw_template.get("kind")
        source_path = raw_template.get("source_path")
        sync_path = raw_template.get("sync_path")
        tier = raw_template.get("tier", "optional")
        if not isinstance(module_id, str) or not module_id.strip():
            violations.add(f"governance manager template missing id: {catalog_path}")
            continue
        if kind not in {"agent", "skill"}:
            violations.add(f"governance manager template has unsupported kind: {module_id}")
            continue
        key = (kind, module_id)
        if key in catalog_keys:
            violations.add(f"governance manager catalog duplicates template: {kind}/{module_id}")
        catalog_keys.add(key)
        if not isinstance(source_path, str) or not source_path.strip():
            violations.add(f"governance manager template missing source_path: {kind}/{module_id}")
            continue
        if not isinstance(sync_path, str) or not sync_path.strip():
            violations.add(f"governance manager template missing sync_path: {kind}/{module_id}")
        expected_prefix = f"governance/core/{kind}/"
        if isinstance(sync_path, str) and not sync_path.startswith(expected_prefix):
            violations.add(
                f"governance manager template sync_path must start with {expected_prefix}: "
                f"{kind}/{module_id} -> {sync_path}"
            )
        if tier not in {"default", "optional"}:
            violations.add(f"governance manager template has unsupported tier: {kind}/{module_id} -> {tier}")
        if kind == "skill" and isinstance(tier, str):
            catalog_skill_tiers[module_id] = tier
        if source_path.startswith(EXTERNAL_SOURCE_PREFIX):
            continue

        source_file = (manager_root / source_path).resolve()
        try:
            relative_source = source_file.relative_to(manager_root.resolve())
        except ValueError:
            violations.add(f"governance manager template source escapes root: {kind}/{module_id} -> {source_path}")
            continue
        catalog_sources.add(relative_source)
        if not source_file.is_file():
            violations.add(f"governance manager template source is missing: {kind}/{module_id} -> {source_path}")
            continue
        if kind == "skill" and module_id == "codex-helper-dev-flow" and tier == "default":
            violations.add(
                "governance manager default skills must be reusable managed-project templates, "
                "not helper-repository private workflows: codex-helper-dev-flow"
            )
        if kind == "skill":
            if source_file.name != "SKILL.md":
                violations.add(f"governance manager skill template must point to SKILL.md: {kind}/{module_id}")
            openai_yaml = source_file.parent / "agents" / "openai.yaml"
            if not openai_yaml.is_file():
                violations.add(
                    f"governance manager skill template missing agents/openai.yaml: {kind}/{module_id}"
                )
        if kind == "skill" and module_id != "codex-helper-dev-flow":
            content = source_file.read_text(encoding="utf-8", errors="ignore")
            for label, pattern in PUBLIC_TEMPLATE_HELPER_LEAK_PATTERNS:
                if pattern in content:
                    violations.add(
                        f"helper-specific {label} leaked into public governance template: {source_file}"
                    )

    local_agent_sources = {
        path.relative_to(manager_root)
        for path in sorted((manager_root / "templates" / "agent").glob("*.md"))
        if path.is_file()
    }
    local_skill_sources = {
        path.relative_to(manager_root)
        for path in sorted((manager_root / "templates" / "skill").glob("*/SKILL.md"))
        if path.is_file()
    }
    for missing_catalog in sorted((local_agent_sources | local_skill_sources) - catalog_sources):
        violations.add(f"governance manager local template missing catalog entry: {missing_catalog}")

    manifest_path = repo_root / "src" / "codex_helper" / "assets" / "agent_baseline" / "manifest.toml"
    if manifest_path.exists():
        try:
            manifest = _load_toml(manifest_path)
        except tomllib.TOMLDecodeError:
            violations.add(f"invalid agent baseline manifest: {manifest_path}")
            return
        default_skills = manifest.get("default_skills", [])
        if not isinstance(default_skills, list) or not all(isinstance(item, str) for item in default_skills):
            violations.add(f"agent baseline manifest default_skills must be a list of strings: {manifest_path}")
            return
        for skill_name in default_skills:
            tier = catalog_skill_tiers.get(skill_name)
            if tier is None:
                violations.add(
                    f"agent baseline default skill is missing from governance_manager catalog: {skill_name}"
                )
            elif tier != "default":
                violations.add(
                    f"agent baseline default skill must be default tier in governance_manager catalog: {skill_name}"
                )


def collect_policy_regression_violations(
    violations: set[str],
    *,
    repo_root: Path = REPO_ROOT,
) -> None:
    manifest_path = repo_root / "src" / "codex_helper" / "assets" / "agent_baseline" / "manifest.toml"
    has_baseline_source = manifest_path.exists()
    if manifest_path.exists():
        try:
            manifest = _load_toml(manifest_path)
        except tomllib.TOMLDecodeError:
            violations.add(f"invalid agent baseline manifest: {manifest_path}")
        else:
            raw_files = manifest.get("files", [])
            if isinstance(raw_files, list):
                manifest_paths = {
                    Path(item.strip())
                    for item in raw_files
                    if isinstance(item, str) and item.strip()
                }
                for blocked_path in sorted(BLOCKED_BASELINE_MANIFEST_PATHS & manifest_paths):
                    violations.add(
                        "agent baseline manifest must not sync helper generated governance "
                        f"to target projects: {blocked_path}"
                    )
            else:
                violations.add(f"agent baseline manifest files must be a list: {manifest_path}")

    if has_baseline_source:
        for plan_dir_name in ("plans", "plan_archived"):
            plans_override = (
                repo_root
                / "src"
                / "codex_helper"
                / "assets"
                / "agent_baseline"
                / "overrides"
                / "docs"
                / "codex"
                / plan_dir_name
            )
            if not (plans_override / ".gitkeep").exists():
                violations.add(f"baseline {plan_dir_name} override must seed only a tracked empty directory: {plans_override}")
            if plans_override.exists():
                plan_payloads = [
                    path for path in plans_override.rglob("*") if path.is_file() and path.name != ".gitkeep"
                ]
                for path in plan_payloads:
                    violations.add(f"baseline {plan_dir_name} override must not include helper plan history: {path}")
        for path in sorted(BASELINE_OVERRIDES_ROOT.rglob("*")):
            if path.is_dir() and path.name in TEMPLATE_CACHE_DIR_NAMES:
                violations.add(f"agent baseline overrides contain cache directory: {path}")
            if path.is_file() and any(path.match(pattern) for pattern in TEMPLATE_CACHE_FILE_PATTERNS):
                violations.add(f"agent baseline overrides contain generated cache file: {path}")
        for path in _iter_markdown_and_toml_files(BASELINE_OVERRIDES_ROOT):
            content = path.read_text(encoding="utf-8", errors="ignore")
            for label, pattern in BASELINE_HELPER_LEAK_PATTERNS:
                if pattern in content:
                    violations.add(f"helper-specific {label} leaked into baseline override: {path}")

    for relative_root in HOST_SPECIFIC_SCAN_RELATIVE_ROOTS:
        root = repo_root / relative_root
        for path in _iter_markdown_and_toml_files(root):
            content = path.read_text(encoding="utf-8", errors="ignore")
            for pattern in HOST_SPECIFIC_PATTERNS:
                if pattern in content:
                    violations.add(f"host-specific fact leaked into reusable governance: {path} -> {pattern}")

    for path in [
        repo_root / "governance" / "README.md",
        repo_root / "governance" / "modules.toml",
        repo_root / "governance_manager" / "README.md",
        *(_iter_markdown_and_toml_files(repo_root / "governance_manager" / "templates")),
        *(_iter_markdown_and_toml_files(repo_root / "governance" / "private" / "skill")),
    ]:
        if not path.exists() or not path.is_file():
            continue
        content = path.read_text(encoding="utf-8", errors="ignore")
        for label, pattern in BLOCKED_POLICY_PATTERNS:
            if pattern in content:
                violations.add(f"blocked {label} wording in reusable governance source: {path}")

    pyproject_path = repo_root / "pyproject.toml"
    has_governance_manager_source = (repo_root / "governance_manager" / "catalog.toml").exists()
    if pyproject_path.exists():
        payload = _load_toml(pyproject_path)
        force_include = (
            payload.get("tool", {})
            .get("hatch", {})
            .get("build", {})
            .get("targets", {})
            .get("wheel", {})
            .get("force-include", {})
        )
        if isinstance(force_include, dict):
            if has_governance_manager_source and "governance_manager" not in force_include:
                violations.add("wheel force-include must package governance_manager template sources")
            for blocked_path in (
                "governance/core/agent",
                "governance/core/skill",
                "docs/codex/plans",
                "docs/codex/plan_archived",
            ):
                if blocked_path in force_include:
                    violations.add(
                        "wheel force-include must not package helper project generated/history path "
                        f"as managed baseline: {blocked_path}"
                    )

    scaffold_paths = [
        repo_root
        / "governance_manager"
        / "templates"
        / "skill"
        / "ops-script-maintenance"
        / "scripts"
        / "scaffold_manage.py",
        repo_root
        / "governance"
        / "core"
        / "skill"
        / "ops-script-maintenance"
        / "scripts"
        / "scaffold_manage.py",
    ]
    for scaffold_path in scaffold_paths:
        if not scaffold_path.exists():
            continue
        content = scaffold_path.read_text(encoding="utf-8")
        if "with_wrappers: bool = False" not in content:
            violations.add(f"ops scaffold wrappers must be opt-in in {scaffold_path}")
        if 'wrapper_group.add_argument(\n        "--with-wrappers"' not in content:
            violations.add(f"ops scaffold CLI must expose explicit --with-wrappers in {scaffold_path}")
        if 'files[deploy_dir / "deploy.sh"] = render_wrapper("build")' not in content:
            violations.add(f"ops scaffold deploy.sh wrapper must map to manage.sh build in {scaffold_path}")
        if 'files[deploy_dir / "deploy.sh"] = render_wrapper("dev")' in content:
            violations.add(f"ops scaffold deploy.sh wrapper must not map to manage.sh dev in {scaffold_path}")


def _read_str_list(payload: dict, key: str, path: Path) -> tuple[str, ...]:
    value = payload.get(key, [])
    if not isinstance(value, list) or not all(isinstance(item, str) for item in value):
        raise ValueError(f"{path} field '{key}' must be a list of strings")
    return tuple(value)


def load_skill_routing_specs(path: Path = SKILL_ROUTING_PATH) -> list[SkillRoutingSpec]:
    raw = _load_toml(path)
    if raw.get("schema_version") != 1:
        raise ValueError(f"{path} must set schema_version = 1")
    entries = raw.get("skills", [])
    if not isinstance(entries, list):
        raise ValueError(f"{path} must define [[skills]] entries")

    specs: list[SkillRoutingSpec] = []
    seen: set[str] = set()
    for entry in entries:
        if not isinstance(entry, dict):
            raise ValueError(f"{path} contains a non-table skill entry")
        name = entry.get("name")
        category = entry.get("category")
        if not isinstance(name, str) or not name:
            raise ValueError(f"{path} has a skill entry missing 'name'")
        if name in seen:
            raise ValueError(f"{path} contains duplicate skill entry: {name}")
        if not isinstance(category, str) or category not in SKILL_CATEGORIES:
            raise ValueError(
                f"{path} skill '{name}' must use category one of {sorted(SKILL_CATEGORIES)}"
            )
        seen.add(name)
        specs.append(
            SkillRoutingSpec(
                name=name,
                category=category,
                routing_tags=_read_str_list(entry, "routing_tags", path),
                narrower_than=_read_str_list(entry, "narrower_than", path),
                pairs_with=_read_str_list(entry, "pairs_with", path),
            )
        )
    return specs


def collect_skill_routing_violations(
    violations: set[str],
    *,
    skills_root: Path = SKILLS_ROOT,
    routing_path: Path = SKILL_ROUTING_PATH,
    routing_doc_path: Path = SKILL_ROUTING_DOC_PATH,
) -> None:
    try:
        specs = load_skill_routing_specs(routing_path)
    except ValueError as exc:
        violations.add(str(exc))
        return

    actual_skills = {
        path.name
        for path in sorted(skills_root.glob("*"))
        if path.is_dir() and (path / "SKILL.md").exists()
    }
    actual_skills.update(
        path.name
        for path in sorted(BASELINE_OVERRIDE_SKILLS_ROOT.glob("*"))
        if path.is_dir() and (path / "SKILL.md").exists()
    )
    declared_skills = {spec.name for spec in specs}

    for extra in sorted(declared_skills - actual_skills):
        violations.add(f"skill routing inventory references missing skill directory in {routing_path}: {extra}")

    tag_owners: dict[str, set[str]] = {}
    pair_map = {spec.name: set(spec.pairs_with) for spec in specs}
    for spec in specs:
        if not spec.routing_tags:
            violations.add(f"skill routing inventory has no routing_tags for {spec.name} in {routing_path}")
        for relation_name, relations in (
            ("narrower_than", spec.narrower_than),
            ("pairs_with", spec.pairs_with),
        ):
            if len(relations) != len(set(relations)):
                violations.add(
                    f"skill routing inventory duplicates {relation_name} entries for {spec.name} in {routing_path}"
                )
            for relation in relations:
                if relation == spec.name:
                    violations.add(
                        f"skill routing inventory has self-reference in {relation_name} for {spec.name} in {routing_path}"
                    )
                if relation not in declared_skills:
                    violations.add(
                        f"skill routing inventory references unknown skill '{relation}' from {spec.name} in {routing_path}"
                    )
        for tag in spec.routing_tags:
            normalized = tag.strip().lower()
            if not SKILL_TAG_RE.fullmatch(normalized):
                violations.add(
                    f"skill routing tag must use lowercase letters, digits, slash, underscore, or hyphen in {routing_path}: {spec.name} -> {tag}"
                )
                continue
            tag_owners.setdefault(normalized, set()).add(spec.name)

    for tag, owners in sorted(tag_owners.items()):
        if len(owners) > 1:
            violations.add(
                f"skill routing tag conflict in {routing_path}: {tag} claimed by {', '.join(sorted(owners))}"
            )

    for spec in specs:
        for paired_skill in spec.pairs_with:
            if spec.name not in pair_map.get(paired_skill, set()):
                violations.add(
                    f"skill routing inventory pairs_with must be reciprocal in {routing_path}: {spec.name} -> {paired_skill}"
                )

    if routing_doc_path.exists():
        doc_text = routing_doc_path.read_text(encoding="utf-8")
        for spec in specs:
            count = doc_text.count(f"`{spec.name}`")
            if count == 0:
                violations.add(
                    f"skill routing doc sync mismatch in {routing_doc_path}: expected `{spec.name}` at least once, found 0"
                )
        mentioned_skills = {match.group(1) for match in SKILL_NAME_RE.finditer(doc_text)}
        for extra in sorted(mentioned_skills - declared_skills):
            if extra in actual_skills:
                violations.add(
                    f"skill routing doc references unknown registry skill in {routing_doc_path}: {extra}"
                )


def _check_generated_outputs(violations: set[str]) -> None:
    python_path = _current_python_path()
    result = subprocess.run(
        [str(python_path), str(REPO_ROOT / "scripts" / "governance" / "build_agents.py"), "--check"],
        cwd=REPO_ROOT,
        text=True,
        capture_output=True,
        check=False,
    )
    if result.returncode == 0:
        return
    output = result.stdout.strip()
    error = result.stderr.strip()
    if output:
        violations.add(output)
    if error:
        violations.add(error)


def collect_agent_asset_violations() -> list[str]:
    violations: set[str] = set()

    for required_path in iter_required_paths():
        if not required_path.exists():
            violations.add(f"required agent asset missing: {required_path}")
    if _is_source_repo(REPO_ROOT):
        for relative_path in GOVERNANCE_MANAGER_REQUIRED_RELATIVE_PATHS:
            required_path = (REPO_ROOT / relative_path).resolve()
            if not required_path.exists():
                violations.add(f"required governance manager asset missing: {required_path}")

    collect_force_include_violations(violations)
    collect_governance_manager_template_violations(violations)
    collect_policy_regression_violations(violations)
    skill_roots = sorted(SKILLS_ROOT.glob("*")) + sorted(BASELINE_OVERRIDE_SKILLS_ROOT.glob("*"))
    for skill_path in skill_roots:
        if not skill_path.is_dir():
            continue
        skill_md_path = skill_path / "SKILL.md"
        openai_yaml_path = skill_path / "agents" / "openai.yaml"
        if not skill_md_path.exists():
            violations.add(f"skill directory missing SKILL.md: {skill_path}")
            continue
        if not openai_yaml_path.exists():
            violations.add(f"skill directory missing agents/openai.yaml: {skill_path}")
            continue
        openai_yaml = openai_yaml_path.read_text(encoding="utf-8")
        for required_key in OPENAI_YAML_REQUIRED_KEYS:
            if required_key not in openai_yaml:
                violations.add(
                    f"skill openai metadata missing {required_key.rstrip(':')} in {openai_yaml_path}"
                )

    collect_skill_routing_violations(violations)
    _check_generated_outputs(violations)

    for text_path in _iter_text_files():
        content = text_path.read_text(encoding="utf-8")

        for match in SCOPE_RE.finditer(content):
            scope = match.group(1)
            if _should_skip_reference(scope):
                continue
            resolved = _resolve_reference(scope)
            if not resolved.exists():
                violations.add(f"scope path missing in {text_path}: {scope}")

        for match in PATH_RE.finditer(content):
            reference = match.group(1)
            if _should_skip_reference(reference):
                continue
            resolved = _resolve_text_reference(text_path, reference)
            if not resolved.exists():
                violations.add(f"referenced path missing in {text_path}: {reference}")

    return sorted(violations)


def main() -> int:
    violations = collect_agent_asset_violations()
    if not violations:
        print("Agent asset check passed.")
        return 0

    print("Agent asset check failed:")
    for violation in violations:
        print(f"- {violation}")
    return 1


if __name__ == "__main__":
    raise SystemExit(main())
