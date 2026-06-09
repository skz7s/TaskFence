# CLI And Task File Reference Plan

## Goal

Continue improving TaskFence toward mature open-source readiness by adding
stable public reference documentation for the implemented CLI surface and
preview task-file schema.

## Plan Source

Continuation of the active 2026-06-09 goal:

> 继续优化项目，包括文档，将项目优化到一个成熟的开源项目的级别后，开源项目

Related archived plans:

- `docs/codex/plan_archived/2026-06-09-open-source-maturity.md`
- `docs/codex/plan_archived/2026-06-09-security-compatibility.md`

Scope:

- inspect the `taskfence` CLI command tree from `crates/taskfence-cli`
- inspect the task-file parser and resolved public types from
  `crates/taskfence-config` and `crates/taskfence-core`
- add public reference docs for implemented CLI commands and preview task-file
  fields
- link the new references from README, contributing, examples, project
  structure, changelog, and governance change-map docs

Non-goals:

- do not change CLI behavior, task-file schema, runtime behavior, dependency
  versions, release tags, or generated governance outputs
- do not document unsupported production surfaces as supported
- do not invent fields that are not accepted by the current parser
- do not replace examples or quickstart docs; references should complement
  them

Acceptance criteria:

- users can find the implemented command tree without reading Rust source
- users can identify required and optional task YAML fields, defaults, and
  fail-closed constraints
- docs distinguish preview contract status from stable production support
- validation evidence is recorded before committing

## Snapshot

- Date: 2026-06-09
- Default branch: `origin/main`
- Working branch: `codex/governance-development-plan`
- Initial worktree status: clean after commits `d1070e4` and `fb74319`
- `git pull --ff-only`: not retried for this slice because the current branch
  still has no tracking upstream
- Current observed gap: README and examples contain many runnable commands, but
  there is no concise public CLI reference or task-file schema reference

## Phases

### 1. Intake And Plan

- Status: done
- Scope: record the current branch, related archived plans, scope, non-goals,
  acceptance criteria, and next executable phase
- Verification command: `git status --short --branch`
- Verification evidence: passed on 2026-06-09. Worktree contains only the new
  active plan file for this slice.

### 2. Interface Inspection

- Status: done
- Scope: inspect CLI help output and task-file parser/source types for accepted
  fields, defaults, command names, options, and known limits
- Verification command: `target/debug/taskfence --help` plus targeted
  subcommand help and example validation
- Verification evidence: passed on 2026-06-09. Inspected top-level,
  gateway, replay, state, review, approval, evidence-query, compliance, and
  team/worker CLI help from `target/debug/taskfence`; inspected
  `crates/taskfence-cli/src/main.rs`, `crates/taskfence-config/src/lib.rs`,
  and `crates/taskfence-core/src/lib.rs`; validated all example task files
  with `target/debug/taskfence validate examples/*.yaml`.

### 3. Reference Docs

- Status: done
- Scope: add CLI and task-file reference docs and link them from public and
  governance docs
- Verification command: `git diff --check && python3 scripts/governance/check_codex_governance.py`
- Verification evidence: passed on 2026-06-09. Added `docs/cli-reference.md`
  and `docs/task-file-reference.md`; linked them from README, CONTRIBUTING,
  examples, project-structure, versioning, changelog, and change-map docs.
  `git diff --check`, `python3 scripts/governance/build_agents.py --check`,
  and `python3 scripts/governance/check_codex_governance.py` passed.

### 4. Final Review And Commit

- Status: done
- Scope: run final targeted validation, archive this plan if complete, and
  create focused local commits
- Verification command: `cargo run -p taskfence-cli -- validate examples/task.yaml`
- Verification evidence: passed on 2026-06-09 using `target/debug/taskfence
  validate examples/task.yaml`. Also captured top-level, gateway-call, and
  team-audit-export CLI help output to `/tmp` for reference-doc validation.

## Commit Plan

1. `docs: add cli and task file reference` - `2523e8b`

## Final Evidence

- Public CLI reference added for core task, gateway, evidence, review, replay,
  state, approval, and team-state commands.
- Public task-file reference added for top-level fields, agent, sandbox,
  remote SSH, permissions, secrets, approval, gateway connectors, audit, and
  maintained examples.
- README, CONTRIBUTING, examples, project structure, versioning, changelog, and
  change-map docs now route CLI/task-file changes to the new references.
- Validation passed on 2026-06-09:
  `git diff --check`, `python3 scripts/governance/build_agents.py --check`,
  `python3 scripts/governance/check_codex_governance.py`,
  `target/debug/taskfence --help`, `target/debug/taskfence gateway call
  --help`, `target/debug/taskfence team audit-export --help`, and
  `target/debug/taskfence validate examples/task.yaml`.
- Implementation commit: `2523e8b docs: add cli and task file reference`.
