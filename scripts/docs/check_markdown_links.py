# -*- coding: utf-8 -*-
from __future__ import annotations

import re
import sys
from pathlib import Path
from urllib.parse import unquote, urlsplit

REPO_ROOT = Path(__file__).resolve().parents[2]

ROOT_MARKDOWN = (
    "README.md",
    "CONTRIBUTING.md",
    "SECURITY.md",
    "SUPPORT.md",
    "CODE_OF_CONDUCT.md",
    "CHANGELOG.md",
)
DOC_DIRS = (
    REPO_ROOT / "docs",
    REPO_ROOT / "examples",
)
GOVERNANCE_DOCS = (
    REPO_ROOT / "governance" / "README.md",
    REPO_ROOT / "governance" / "change-map.md",
    REPO_ROOT / "governance" / "skill-maintenance.md",
)
EXCLUDED_DOC_PARTS = {
    ".git",
    ".codex",
    ".codex-helper",
    "target",
    "plan_archived",
    "plans",
}

INLINE_LINK_RE = re.compile(r"!?\[[^\]\n]*\]\(([^)\n]+)\)")
REFERENCE_LINK_RE = re.compile(r"^\s{0,3}\[[^\]\n]+\]:\s*(.+?)\s*$")
INLINE_CODE_RE = re.compile(r"`[^`]*`")
SCHEME_RE = re.compile(r"^[a-zA-Z][a-zA-Z0-9+.-]*:")


def _is_excluded(path: Path) -> bool:
    try:
        relative = path.relative_to(REPO_ROOT)
    except ValueError:
        return True
    return any(part in EXCLUDED_DOC_PARTS for part in relative.parts)


def _iter_markdown_files() -> list[Path]:
    files: set[Path] = set()
    for name in ROOT_MARKDOWN:
        path = REPO_ROOT / name
        if path.exists():
            files.add(path)

    for doc_dir in DOC_DIRS:
        if doc_dir.exists():
            files.update(path for path in doc_dir.rglob("*.md") if not _is_excluded(path))

    github_dir = REPO_ROOT / ".github"
    if github_dir.exists():
        files.update(path for path in github_dir.rglob("*.md") if not _is_excluded(path))

    for path in GOVERNANCE_DOCS:
        if path.exists():
            files.add(path)

    return sorted(files)


def _strip_destination(raw: str) -> str:
    destination = raw.strip()
    if destination.startswith("<"):
        end = destination.find(">")
        if end != -1:
            return destination[1:end].strip()
    return destination.split(maxsplit=1)[0].strip()


def _should_skip_destination(destination: str) -> bool:
    if not destination or destination.startswith("#"):
        return True
    if SCHEME_RE.match(destination):
        return True
    return False


def _resolve_destination(doc_path: Path, destination: str) -> Path | None:
    clean = _strip_destination(destination)
    if _should_skip_destination(clean):
        return None

    split = urlsplit(clean)
    if split.scheme or split.netloc:
        return None
    if not split.path:
        return None

    target_path = unquote(split.path)
    if target_path.startswith("/"):
        return REPO_ROOT / target_path.lstrip("/")
    return (doc_path.parent / target_path).resolve()


def _iter_link_destinations(doc_path: Path) -> list[tuple[int, str]]:
    destinations: list[tuple[int, str]] = []
    in_fence = False

    for line_number, line in enumerate(doc_path.read_text(encoding="utf-8").splitlines(), start=1):
        stripped = line.lstrip()
        if stripped.startswith(("```", "~~~")):
            in_fence = not in_fence
            continue
        if in_fence:
            continue

        line_without_code = INLINE_CODE_RE.sub("", line)
        for match in INLINE_LINK_RE.finditer(line_without_code):
            destinations.append((line_number, match.group(1)))
        reference_match = REFERENCE_LINK_RE.match(line_without_code)
        if reference_match is not None:
            destinations.append((line_number, reference_match.group(1)))

    return destinations


def collect_missing_links() -> list[str]:
    violations: list[str] = []
    for doc_path in _iter_markdown_files():
        for line_number, destination in _iter_link_destinations(doc_path):
            resolved = _resolve_destination(doc_path, destination)
            if resolved is None:
                continue
            if not resolved.exists():
                relative_doc = doc_path.relative_to(REPO_ROOT)
                violations.append(f"{relative_doc}:{line_number}: missing link target: {destination}")
    return sorted(violations)


def main() -> int:
    violations = collect_missing_links()
    if not violations:
        print("Markdown link check passed.")
        return 0

    print("Markdown link check failed:")
    for violation in violations:
        print(f"- {violation}")
    return 1


if __name__ == "__main__":
    raise SystemExit(main())
