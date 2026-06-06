---
name: managed-project-dev-flow
description: Use when changing code, governance, docs, validation, or git state in a helper-managed target project and you need a task-sized development workflow.
---

# Managed Project Dev Flow

## Goal

Deliver the requested outcome with the smallest coherent change while keeping project facts, reusable governance, local environment facts, validation evidence, and git checkpoints aligned.

## Start

- Read `README.md` for the project goal and operator commands.
- Read `governance/change-map.md` before deeper docs; then read only the stable docs that own the touched facts.
- Read `.codex-helper/local-env.toml` if it exists. If command facts are missing or stale, refresh it by probing this checkout, for example with `bash deploy/manage.sh detect-env` when available or the `project-env-baseline` skill.
- Check `git status --short` and preserve unrelated user changes.
- Before modifying code, attempt `git pull --ff-only`; if it cannot run, record the reason and continue from the current checkout.
- Choose workflow by task size and user intent before doing heavy process work.
- For small edits, direct bug fixes, narrow docs/rule updates, and direct user instructions, use the lightweight path: inspect the relevant files, edit, run the smallest validation, and summarize. Do not create a durable plan only because multiple commands are involved.
- For larger changes, use Codex plan mode and a durable plan file only when phase-by-phase execution, cross-file coordination, branch integration, or multi-turn resumability is needed.
- If the user asks to implement, execute, continue, or apply an existing plan, treat the task as plan-driven phased work unless the user explicitly asks for lightweight execution.
- If a plan-sized request needs a durable plan, generate a new `docs/codex/plans/YYYY-MM-DD-<topic>.md` file for that request. Do not overwrite, recycle, or append new request scope to an older plan file.
- Continuing execution of an existing active plan means updating status and evidence in that plan. Generating a revised plan, a new plan, or a plan for a different goal, acceptance criteria, or change scope requires a new file and a short cross-reference to the superseded or related plan when useful.
- If the user is reporting a bug, reproduce or inspect the failing path first, then fix and validate the specific behavior before broad refactors.
- If the user asks for a review, use review mode: prioritize findings with severity and file/line references; do not modify code unless the user asks you to address findings.
- When phased work needs a durable plan, write it to `docs/codex/plans/YYYY-MM-DD-<topic>.md` before implementation and keep both approved plan content and execution state in that same file.
- A chat-only plan is not durable execution state. If the plan exists only in conversation, convert it into a durable plan file before changing implementation files, preserving the approved plan content as directly as practical. Use a faithful summary only when the original source is too long, repetitive, or contains irrelevant chat noise.
- For phased work, detect the default branch from `origin/HEAD`, then local `main`, then local `master`. If the current branch is that default branch, create a `codex/<topic-slug>` branch before implementation.

## Work Rules

- Keep `AGENTS.md` compact; put repeatable procedures in skills and stable project facts in `docs/codex/`, `docs/config/`, or project-private governance.
- Treat `AGENTS.md`, `.codex/skills/*`, and `governance/core/*` as generated-but-committed outputs. If a generated edit needs to live, move it back to the owning source before build.
- Project-private agent constraints belong under `governance/private/agent/*.md`; project-private skills belong under `governance/private/skill/<name>/SKILL.md`.
- When adding a private agent or skill, create the source, register it in `governance/modules.toml`, add private agent fragments to `governance/bundles.toml` when they should affect runtime rules, run `python3 scripts/governance/build_agents.py`, then run `python3 scripts/governance/check_codex_governance.py`.
- Reusable public templates are selected from the installed governance catalog and should not be edited inside target-project generated outputs. Make project-specific changes private unless the operator explicitly asks to improve the reusable template library upstream.
- When changing package registries, mirrors, proxies, or dependency source policy, keep stable policy in `docs/config/*` and actual machine facts in `.codex-helper/local-env.toml`.
- Treat `.codex-helper/planning/*` and `.codex-helper/runtime.json` as legacy data unless this project explicitly documents a current production dependency on them.
- Treat Codex plan mode as conversation control only; do not turn it into hidden runtime state, queue orchestration, background automation, or worker handoff.
- Do not use an existing chat plan, dirty worktree, partial implementation, failing tests, or a closeout request as a reason to skip durable planning. First snapshot the current branch and worktree, reconcile the existing changes with the plan, then continue.
- Treat `.codex-helper/local-env.toml` as the ignored machine-local fact cache; refresh and maintain it by probing the current checkout instead of hard-coding shell, OS, absolute tool paths, package-manager paths, dependency sources, local bin directories, or current-machine facts in reusable agents, skills, generated governance, or stable docs.
- Stable deployment facts and operator procedures belong in `docs/config/*`; host-specific paths, local bin locations, package-manager state, dependency source facts, and current OS facts stay in `.codex-helper/local-env.toml`.
- If the repository has a checked-in debug Docker or Compose environment such as `compose.debug.yaml`, reuse it as the container-backed environment for both debugging and tests, and keep it in sync with relevant code, dependency, env-contract, or ops-script changes. If no debug Docker setup exists, do not create or update one unless the operator explicitly asks for it.
- Do not keep a separate reusable test Docker or Compose stack as managed-project governance policy. Container-backed tests should reuse the debug stack and its networking model.
- If the debug Docker or Compose stack runs the app itself, start that app service with the documented `dev` command so source mounts preserve hot reload. Keep production deployment Docker or Compose manifests separate from the debug stack.
- For substantial new UI design, major redesign, or product-facing pre-design, prefer GPT image generation as the first visual artifact when image generation is available. Use existing design-system implementation directly for narrow fixes, copy edits, and small layout adjustments. Do not substitute an HTML mockup plus screenshot for visual direction; screenshots verify implementation after the design direction is clear.
- For product feature design, include commercial analysis proportional to the change: target users, monetization or business value, adjacent competitors, market positioning, and why the proposed feature belongs inside the current project boundary.
- A durable execution plan should record the goal, approved plan content or plan-source summary, detected default branch, working branch, overall status, ordered phases, phase verification evidence, and a numbered commit plan.
- Every durable execution plan must include an `Approved Plan` or `Plan Source` section that preserves the initial approved plan, including scope, non-goals, assumptions, constraints, acceptance criteria, and tradeoffs when present. Do not replace the initial plan with only a phase list.
- The `Approved Plan` or `Plan Source` section should preserve the operator's native structure, wording, requirements, constraints, acceptance criteria, and edge cases when it is usable. Do not shrink a concrete multi-item operator request into a generic one-paragraph summary, and do not drop requirements just because they are inconvenient or belong to a later phase.
- If the original request is too long, repetitive, or contains irrelevant chat noise, write a faithful structured summary that keeps every actionable requirement and explicitly notes what was omitted and why.
- Every durable execution plan must include an initial `Intake / Snapshot` phase that records the plan source, current branch, worktree status, already-present changes, and the next executable phase.
- Each phase should record scope, status, verification command, and verification evidence. Use `pending`, `in_progress`, `done`, or `blocked`; do not leave phase status implicit.
- When a phase starts, mark it `in_progress` before making phase-owned edits. When a phase finishes, immediately update its status to `done` with verification evidence, or `blocked` with the blocker and next needed input, before starting another phase or claiming progress.
- The commit plan should list intended commit messages as `1.`, `2.`, `3.` by coherent change scope, not by phase count.
- Commit after the relevant coherent change scope passes verification. Avoid one-commit-per-phase fragmentation unless a phase is independently reviewable and maps cleanly to one change scope.
- Do not commit a change scope until its recorded verification has passed or the skipped validation risk is documented.
- When every phase in a durable plan is terminal and the requested work is complete, update the overall status and final evidence, then move the plan file from `docs/codex/plans/` to `docs/codex/plan_archived/` using the same filename. Keep `docs/codex/plans/` for active or blocked plans only, with `.gitkeep` preserving the directory.
- After all phases are complete, confirm before branch integration, then try `git merge --ff-only` into the detected default branch and `git push`.
- If merge or push fails, branch protection blocks the update, or history requires rebase or a merge commit, stop and report the exact blocker. Do not force-push, auto-rebase, or rewrite history.

## Code Organization

- Split work by stable feature or domain boundaries first. Avoid dumping unrelated behavior into one page, service, hook, module, or utility file just because the change is small.
- Keep business logic, data access, orchestration, rendering, and one-off view glue separated. If a file starts owning multiple layers at once, split it before adding more behavior.
- Prefer small modules with one clear responsibility over broad catch-all helpers. Name directories and files by the responsibility they own, not by vague labels such as `misc`, `common`, or `utils2`.
- Keep reusable code in explicit shared locations only after a second real use appears or a near-term shared use is already clear. Do not extract premature abstractions that make local behavior harder to read.
- When extracting shared code, preserve the product boundary: shared modules should hold stable, generic behavior, while product-specific branching stays in the calling feature.
- Keep components focused. Large page components should compose smaller presentational or workflow components instead of accumulating data fetching, modal state, table rendering, forms, and mutations in one file.
- Prefer directory structures that reveal intent, for example feature-scoped folders with nearby tests, small subcomponents, and local helpers, rather than very deep generic type-based trees.
- Avoid oversized files. When a file becomes hard to scan, mixes multiple workflows, or requires long scrolling to understand one change, split it by responsibility instead of adding another section.
- Keep public module interfaces narrow. Export the minimum surface a caller needs, and avoid cross-feature imports that bypass the intended boundary.
- Co-locate tests with the module or feature layer they verify, and add targeted tests when extracting shared logic or splitting a large file so the new boundary is exercised directly.

## Subagent Use

- Use subagents only when the operator explicitly asks for subagents, delegated work, parallel agents, worker agents, or multiple review rounds. Requests for depth, thoroughness, investigation, or a large review do not by themselves authorize subagents.
- Use subagents to protect context quality, not to outsource ownership. Good uses include bounded codebase exploration, independent review slices, disjoint implementation slices, or verification that can run while the main thread continues useful work.
- Before spawning multiple subagents, inspect the active Codex configuration for `[agents].max_threads`.
- If `[agents].max_threads` is not configured, assume Codex's default concurrent subagent limit is `6`.
- Do not launch a parallel batch that exceeds the configured limit or the remaining available slots after accounting for already-open delegated agents.
- Do not delegate the immediate blocker on the critical path when the next local step depends on the result. Keep tightly coupled design decisions, conflict resolution, final integration, and final user communication in the main thread.
- Keep delegated tasks concrete and self-contained. State the target files or subsystem, expected output, validation command, write ownership, and that the subagent must preserve unrelated user changes.
- For parallel implementation, split by disjoint write sets. Tell each implementation subagent it is not alone in the codebase, must not revert others' edits, and should adapt to concurrent changes.
- For delegated review, keep the main thread responsible for scope, sequencing, deduplication, and the final findings. Run review rounds serially; within one large authorized round, split by subsystem or review angle.
- When spawning a full-history forked subagent with `fork_context=true`, do not pass `agent_type`, `model`, or `reasoning_effort`; full-history forks inherit those values. If a specialized role or reasoning effort is required, spawn without a full-history fork and provide only the needed context.
- If subagent spawning fails or returns no agent id, treat that slice as not delegated: report the failure briefly, continue in the main thread when practical, and do not leave user-facing state as if an agent is still being generated.
- Close delegated agents after their result is integrated or no longer needed. Do not keep dormant agents open across unrelated work.

## Temporary Evidence

- Keep screenshots, traces, recordings, HTML reports, downloaded fixtures, and ad hoc test scripts out of the repository unless the operator explicitly asks to commit them.
- Store temporary verification artifacts under an ignored temp location such as `/tmp`, a tool-provided artifact location, or an ignored project-local path. Do not place them under docs, source, tests, or governance directories as a convenience.
- If a validation workflow creates temporary evidence inside the repository, delete it before final status and before staging unless the artifact is intentionally part of the change.
- In final summaries, describe the evidence and commands used. Do not keep screenshots or reports solely to prove that the task was completed.
- Before committing, run `git status --short` and confirm the staged files do not include temporary screenshots, traces, reports, logs, cache files, or one-off scripts.

## Operations Facts

- Treat `docs/config/cross-platform-ops.md` as the project-owned stable operations fact document.
- Preserve the project's documented deployment target, entrypoint, and legacy-wrapper contract during governance sync or script maintenance.
- Do not generalize documented Ubuntu-only, Debian-only, macOS-only, WSL-specific, or other OS-specific deployment facts into generic Linux systemd support.
- Do not downgrade a documented single supported entrypoint such as `deploy/manage.sh` into a preferred entrypoint.
- Generate or keep legacy `setup.sh`, `deploy.sh`, or `build.sh` wrappers only when the project explicitly requires compatibility.

## Validation

- Run the smallest executable validation for the touched surface.
- Prefer Python, uv, Node, npm, and Codex commands recorded in `.codex-helper/local-env.toml` when they still exist.
- Governance or skill asset changes require `python3 scripts/governance/check_codex_governance.py`.
- Shell script changes require `bash -n` for affected scripts, including compatibility wrappers.
- Python changes usually require targeted pytest and the project's configured lint command.
- Web changes usually require the smallest relevant unit/component/page test plus rendered UI verification when layout changed.
- When testing browser input, do not use the clipboard. Prefer `tab.playwright.locator(...).fill()` or `tab.playwright.locator(...).type()` for standard inputs; for custom input controls, click to focus and then use `tab.cua.type({ text })`. If a paste path is blocked or restricted, stop retrying the clipboard path and validate the business flow with direct input.
- On low-resource hosts, run static checks first, then the narrowest unit/component test, then split suites. Avoid unthrottled full pytest or Vitest suites when the project documents low-resource constraints.

## Finish

- Summarize changed files and validation evidence.
- For phased work, make sure the durable plan reflects approved plan content or plan-source summary, final phase statuses, verification evidence, numbered commit plan, and resulting commit SHAs before claiming completion.
- If the task produced code, governance, docs, or validation changes requested by the operator, create one focused local commit after validation unless the operator explicitly opted out. Stage only files changed for the task; if unrelated user changes make staging ambiguous, stop and report the exact blocker instead of committing.
- If review findings are addressed after delegated or non-delegated review, create one focused local commit for the review-fix scope after validation unless the operator explicitly opted out.
- If final merge or push cannot complete, state the exact limitation and leave the worktree readable.
