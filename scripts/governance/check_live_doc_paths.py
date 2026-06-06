# -*- coding: utf-8 -*-
from __future__ import annotations

import re
from pathlib import Path

REPO_ROOT = Path(__file__).resolve().parents[2]


def _iter_live_docs() -> list[Path]:
    docs = [REPO_ROOT / "README.md"]
    docs.extend(sorted((REPO_ROOT / "docs" / "agent").rglob("*.md")))
    docs.extend(sorted((REPO_ROOT / "docs" / "codex").glob("*.md")))
    docs.extend(sorted((REPO_ROOT / "docs" / "config").glob("*.md")))
    docs.extend(sorted((REPO_ROOT / "governance_manager").glob("*.md")))
    docs.extend(sorted((REPO_ROOT / ".codex" / "skills").glob("*/SKILL.md")))
    docs.extend(
        sorted(
            (
                REPO_ROOT
                / "src"
                / "codex_helper"
                / "assets"
                / "agent_baseline"
                / "overrides"
                / ".codex"
                / "skills"
            ).glob("*/SKILL.md")
        )
    )
    return docs


INLINE_PATH_RE = re.compile(
    r"`((?:AGENTS\.md|README\.md|deploy/[^`]+|docs/[^`]+|governance_manager/[^`]+|scripts/[^`]+|\.codex/[^`]+|\.codex-helper/[^`]+))`"
)
LINK_TARGET_RE = re.compile(r"\[[^\]]+\]\(([^)]+)\)")
LINE_SUFFIX_RE = re.compile(r"^(?P<path>.+):(?P<line>\d+)(?::(?P<column>\d+))?$")
RETIRED_ARTIFACT_REFERENCES = frozenset(
    {
        ".codex-helper/planning/dispatches",
        ".codex-helper/planning/plans",
        ".codex-helper/runtime.json",
    }
)
RUNTIME_ARTIFACT_REFERENCES = frozenset(
    {
        ".codex-helper/design",
        ".codex-helper/design/draft",
        ".codex-helper/design/ui-library",
        ".codex-helper/docker-debug",
        ".codex-helper/local-env.toml",
    }
)
RETIRED_RUNTIME_PATH_REFERENCES = frozenset(
    {
        "deploy/setup.sh",
        "deploy/deploy.sh",
        "deploy/build.sh",
    }
)
SOURCE_REPO_MARKERS = (
    Path("src/codex_helper/assets/agent_baseline/manifest.toml"),
)


def _is_source_repo() -> bool:
    return all((REPO_ROOT / marker).exists() for marker in SOURCE_REPO_MARKERS)


def _should_skip_reference(reference: str) -> bool:
    normalized = reference.strip().rstrip("/")
    if (
        normalized in RETIRED_ARTIFACT_REFERENCES
        or normalized in RUNTIME_ARTIFACT_REFERENCES
        or (normalized in RETIRED_RUNTIME_PATH_REFERENCES and not _is_source_repo())
    ):
        return True
    if normalized.startswith(".codex-helper/design/"):
        return True
    if normalized.startswith(".codex-helper/docker-debug/"):
        return True
    if normalized == "deploy/manage.sh" and not _is_source_repo():
        return True
    if any(token in reference for token in ("<", ">", "{", "}", "*", "|")):
        return True
    if any(char.isspace() for char in reference):
        return True
    if reference.startswith(("http://", "https://", "#")):
        return True
    return False


def _resolve_reference(reference: str, *, doc_path: Path | None = None) -> Path:
    normalized = reference.strip()
    if normalized.startswith("<") and normalized.endswith(">"):
        normalized = normalized[1:-1].strip()
    match = LINE_SUFFIX_RE.match(normalized)
    if match is not None:
        normalized = match.group("path")
    candidate = Path(normalized.rstrip("/"))
    if candidate.is_absolute():
        return candidate
    if doc_path is not None:
        doc_candidate = doc_path.parent / candidate
        if doc_candidate.exists():
            return doc_candidate
        if ".codex" in doc_path.parts and "skills" in doc_path.parts:
            try:
                skill_index = doc_path.parts.index("skills") + 1
                skill_root = Path(*doc_path.parts[: skill_index + 1])
            except (ValueError, IndexError):
                skill_root = doc_path.parent
            skill_candidate = skill_root / candidate
            if skill_candidate.exists():
                return skill_candidate
        if "governance" in doc_path.parts and "core" in doc_path.parts and "skill" in doc_path.parts:
            try:
                skill_index = doc_path.parts.index("skill") + 1
                skill_root = Path(*doc_path.parts[: skill_index + 1])
            except (ValueError, IndexError):
                skill_root = doc_path.parent
            skill_candidate = skill_root / candidate
            if skill_candidate.exists():
                return skill_candidate
    return REPO_ROOT / candidate


def collect_live_doc_path_violations() -> list[str]:
    violations: set[str] = set()

    for doc_path in _iter_live_docs():
        if not doc_path.exists():
            violations.add(f"live doc missing: {doc_path}")
            continue

        content = doc_path.read_text(encoding="utf-8")

        for match in INLINE_PATH_RE.finditer(content):
            reference = match.group(1)
            if _should_skip_reference(reference):
                continue
            resolved = _resolve_reference(reference, doc_path=doc_path)
            if not resolved.exists():
                violations.add(f"missing inline path reference in {doc_path}: {reference}")

        for match in LINK_TARGET_RE.finditer(content):
            target = match.group(1)
            if _should_skip_reference(target):
                continue
            resolved = _resolve_reference(target, doc_path=doc_path)
            if not resolved.exists():
                violations.add(f"missing markdown link target in {doc_path}: {target}")

    return sorted(violations)


def main() -> int:
    violations = collect_live_doc_path_violations()
    if not violations:
        print("Live doc path check passed.")
        return 0

    print("Live doc path check failed:")
    for violation in violations:
        print(f"- {violation}")
    return 1


if __name__ == "__main__":
    raise SystemExit(main())
