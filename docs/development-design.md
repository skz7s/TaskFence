# Development Design

This document turns the product, requirements, architecture, and roadmap into a
parallel implementation design for TaskFence.

The target is not a minimum demo. The first codebase layout should establish the
long-term runtime boundaries: CLI, task orchestration, policy, approvals,
sandbox execution, gateway mediation, audit, state, artifacts, and reporting.
Phase 1 can ship a narrow local runner, but it should be built inside the global
shape so later gateway, web UI, replay, and team-server work do not require a
rewrite.

## Engineering Goals

- Keep code layers explicit and testable.
- Make module ownership clear enough for multiple agents to implement in
  parallel.
- Prefer typed internal contracts over loosely shared maps or ad hoc JSON.
- Treat every high-risk decision as an auditable policy outcome.
- Keep side effects behind ports so the orchestrator can be tested without
  Docker, Git, network access, or a real terminal.
- Design for local-first execution now and server-backed execution later.

## Technology Choices

### Core Runtime

Use Rust for the main TaskFence runtime.

Decision:

- Rust is the default implementation language for the core runtime, CLI,
  gateway contracts, policy engine, runner integration, audit pipeline, and
  report generator.
- Go is a reasonable alternative for fast iteration and Docker-heavy local
  tooling, but it is not the selected default for this repository. Do not split
  the implementation between Rust and Go unless a later architecture decision
  deliberately changes the stack.
- TypeScript is reserved for the later Web UI and browser-facing developer
  experience, not for the enforcement boundary.

Reasons:

- Single-binary distribution fits local developer and CI modes.
- Strong types help keep policy, approval, audit, and task-state contracts
  precise.
- Async runtime support is mature for streaming logs, gateway calls, and future
  API/server work.
- Security-sensitive code benefits from explicit error handling and ownership.
- Rust can support CLI, daemon, gateway proxy, and worker binaries from one
  workspace.

Recommended core crates:

- `clap` for CLI command parsing.
- `serde`, `serde_json`, and `serde_yaml` for task files and audit events.
- `schemars` for generated JSON Schema.
- `tokio` for async process, IO, and gateway execution.
- `tracing` and `tracing-subscriber` for structured runtime diagnostics.
- `thiserror` for domain errors.
- `uuid`, `time`, and `camino` for IDs, timestamps, and UTF-8 paths.
- `sqlx` with SQLite for local state once persistent task queries are needed.
- `bollard` or direct Docker CLI execution behind a runner port. Start with the
  Docker CLI adapter if it reduces platform risk; keep the interface stable so
  `bollard` can replace it without touching orchestration.
- `ignore` and `similar` or `git2` for diff and artifact collection. Prefer
  invoking Git through a narrow adapter for the first implementation because the
  product target is repository-centric and Git output is familiar to users.

### Web UI

Use TypeScript, React, Vite, and TanStack Router/Query when Phase 4 starts.

The Web UI must consume API/state contracts owned by the core runtime; it must
not become the source of truth for task state, approval state, or report
generation.

### Gateway Surface

Use Rust for the MCP gateway, HTTP proxy, and secret broker control path. Keep
tool protocol adapters as replaceable modules under a gateway boundary. The
policy engine must evaluate normalized tool actions, not protocol-specific
payloads.

### Policy Evolution

Start with a built-in evaluator backed by typed matchers. Design the policy API
so OPA, Cedar, or custom plugins can be added later as alternative evaluators.
Do not leak a future policy language into every module.

## Workspace Layout

Use a Rust workspace from the beginning.

```text
taskfence/
  Cargo.toml
  crates/
    taskfence-cli/
    taskfence-core/
    taskfence-config/
    taskfence-policy/
    taskfence-approval/
    taskfence-audit/
    taskfence-artifacts/
    taskfence-runner/
    taskfence-agent/
    taskfence-gateway/
    taskfence-report/
    taskfence-state/
    taskfence-testkit/
  docs/
  examples/
```

The first implementation does not need to fill every crate with production
behavior, but crate boundaries should exist when the contracts are already
known. Stub only with typed traits and explicit unsupported errors; do not hide
missing implementation behind generic `todo` branches in runtime paths.

## Module Boundaries

### `taskfence-cli`

Owns user-facing commands and terminal UX.

Initial commands:

- `taskfence init [path]`
- `taskfence validate <task-file>`
- `taskfence run <task-file>`
- `taskfence tasks --workspace <workspace>`
- `taskfence task <task-id> --workspace <workspace>`
- `taskfence events <task-id> --workspace <workspace>`
- `taskfence logs <task-id>`
- `taskfence diff <task-id>`
- `taskfence approvals --workspace <workspace>`
- `taskfence approval <approval-id> --workspace <workspace>`
- `taskfence approve <approval-id>`
- `taskfence deny <approval-id>`
- `taskfence report <task-id>`

Responsibilities:

- Parse arguments.
- Resolve user paths.
- Write a starter task file for `taskfence init [path]` without overwriting an
  existing target.
- Invoke the core pre-run validation path for `taskfence validate <task-file>`
  and render its result without starting Docker or writing task artifacts.
- Start the orchestrator.
- Render human-readable progress.
- Collect interactive approval input.
- Convert domain errors into actionable CLI messages.

Must not:

- Evaluate policy directly.
- Build Docker commands directly.
- Write audit events directly except through the audit port.
- Generate reports directly.

### `taskfence-core`

Owns the task lifecycle and shared domain types.

Key types:

- `TaskId`
- `TaskDefinition`
- `ResolvedTask`
- `TaskRun`
- `TaskStatus`
- `TaskContext`
- `TaskEvent`
- `Action`
- `ActionDecision`
- `TaskResult`

Orchestrator flow:

1. Load and validate task configuration.
2. Resolve workspace, paths, defaults, and policy.
3. Capture the pre-run workspace baseline.
4. Create task state and artifact directories.
5. Prepare sandbox inputs.
6. Start agent execution through the runner and adapter ports.
7. Stream stdout/stderr into audit and terminal sinks.
8. Evaluate observable actions through policy.
9. Pause for approval where required.
10. Collect diffs and artifacts.
11. Generate report.
12. Finalize task state.

Must not:

- Import Docker-specific code.
- Parse YAML directly.
- Depend on CLI-specific prompts.
- Know concrete storage paths beyond artifact/state ports.

### `taskfence-config`

Owns task-file schema, parsing, validation, defaults, and path resolution.

Responsibilities:

- Parse YAML task files.
- Validate required fields and incompatible options.
- Normalize command, path, network, environment, secret, audit, and approval
  sections.
- Generate JSON Schema for editor support.
- Produce `ResolvedTask` inputs for the orchestrator.

Important logic branches:

- Missing task ID: generate stable runtime ID or reject according to command
  mode.
- Relative workspace paths: resolve relative to task file directory, not the
  current shell after command invocation.
- Read/write overlap: write paths imply read only when explicitly configured or
  by documented default.
- Read-only parent with writable child, or writable parent with read-only child:
  reject ambiguous configurations unless the mount planner can enforce the exact
  least-privilege shape.
- Path escape attempts: reject `..`, symlink escapes, and host absolute paths
  outside allowed roots.
- Network default: default deny unless explicitly configured.
- Environment: pass only allowlisted variables; never inherit host env by
  default.
- Secrets: reject `expose_to_agent: true` unless an explicit high-risk option
  is present.

### `taskfence-policy`

Owns allow, deny, and approval-required decisions.

Decision model:

```text
Allow
RequireApproval { approval_kind, rule_id, reason, risk }
Deny { rule_id, reason }
```

Action dimensions:

- File read/write.
- Shell command.
- Network destination.
- Environment variable exposure.
- Secret access.
- Tool call.
- Budget consumption.

Command policy must parse command invocations into executable, arguments, shell
mode, and raw text. A plain string equality check is not enough for patterns such
as `git push origin main`, `/usr/bin/git push`, or `sh -c "git push"`.

Decision precedence:

1. Invalid or unclassifiable action is denied.
2. Explicit deny wins over approval and allow.
3. Approval-required wins over allow.
4. Explicit allow permits the action.
5. Default policy applies, normally deny for external access and write actions.

Must not:

- Prompt humans directly.
- Mutate task state directly.
- Redact logs directly; it returns policy metadata used by audit/redaction.

### `taskfence-approval`

Owns approval requests, wait/resume behavior, and approval records.

Responsibilities:

- Create approval records with actor, task ID, action, rule, reason, and risk.
- Support interactive CLI approvals for local mode.
- Support timeout and default result.
- Persist approval decisions through the state/audit ports.
- Expose future API/web approval hooks.

Branches:

- Approver approves before timeout: resume action.
- Approver denies: record denial and skip/fail according to action semantics.
- Timeout: use task policy default, normally deny.
- Non-interactive mode: fail closed unless explicit auto-deny or preapproval
  exists.
- Local interactive approval records must include at least task ID, approval ID,
  local actor string, terminal/session source when available, timestamp, action,
  rule, reason, decision, and any redacted parameters shown to the approver.

### `taskfence-audit`

Owns append-only evidence.

Responsibilities:

- Record task input and resolved policy.
- Record agent command, sandbox image, limits, and environment summary.
- Record stdout/stderr chunks after redaction.
- Record policy decisions, approval requests, approval results, denied actions,
  network destinations, tool calls, file diffs, costs, duration, and artifacts.
- Produce a stable event schema.

Log handling must account for large output, binary-looking chunks, terminal
control characters, partial lines, and redaction before persistence. The raw
agent stream must not be allowed to inject misleading report formatting or hide
secret-like values.

Must not:

- Decide whether actions are allowed.
- Store raw secrets.
- Treat terminal logs as the only audit source.

### `taskfence-artifacts`

Owns task artifact directories and files.

Initial local layout:

```text
.taskfence/
  tasks/
    <task-id>/
      task.resolved.json
      events.jsonl
      stdout.log
      stderr.log
      diff.patch
      report.md
      artifacts/
```

Responsibilities:

- Create artifact directories.
- Capture pre-run baseline metadata before the agent starts.
- Provide atomic writes for final artifacts.
- Track paths included in reports.
- Support future object storage through the same port.

Diff collection must distinguish pre-existing dirty workspace changes from
agent-produced changes. If the workspace is dirty before the task starts, the
baseline and final report must say so explicitly.

### `taskfence-runner`

Owns sandbox execution.

Runner trait:

```text
prepare(resolved_task) -> PreparedRun
start(prepared_run, agent_invocation) -> RunningTask
stream_logs(running_task) -> LogStream
stop(task_id)
collect_exit(task_id) -> ExitStatus
```

Docker runner responsibilities:

- Mount only configured paths.
- Apply read-only and read-write mounts correctly.
- Avoid host home mount.
- Build a minimal environment from allowlisted variables.
- Apply CPU, memory, disk, and timeout limits where Docker supports them.
- Apply network disabled, default deny, or allowlisted mode.
- Capture stdout, stderr, and exit code.

Branches:

- Docker missing: return environment error with installation guidance.
- Image missing: pull only if policy allows network/image pull, otherwise fail.
- Network allowlist unsupported on local Docker: fail closed or start a gateway
  proxy mode that enforces domains.
- Container exits non-zero: task can still produce audit/report; do not discard
  artifacts.
- Timeout: stop container, record timeout, collect partial artifacts.
- User interrupt or process cancellation: stop container, collect partial logs,
  collect final diff where possible, and generate a failure report.

### `taskfence-agent`

Owns agent command construction.

Responsibilities:

- Convert generic and specialized adapter config into a runner invocation.
- Validate command and args against policy before execution.
- Provide adapter metadata for reports.

Adapters:

- `generic` first.
- `codex`, `claude-code`, `gemini`, and other specialized adapters later.

Must not:

- Execute commands directly.
- Bypass runner policy checks.
- Inject host secrets.

### `taskfence-gateway`

Owns MCP, HTTP, CLI wrapper, SDK, and webhook mediation for integrated agents.

Responsibilities:

- Normalize protocol-specific requests into `Action::ToolCall`.
- Evaluate tool actions with policy.
- Request approval for high-risk tool actions when an approval engine is
  explicitly attached.
- Define secret broker references without exposing raw secrets to the agent.
- Normalize MCP/HTTP-shaped adapter stub requests into tool actions and return
  explicit unsupported execution errors until real protocol transports exist.
- Emit structured audit events.

This crate can start mostly as contracts and a local stub, but the contracts
must be aligned with Phase 3 from day one so policy and audit are not limited to
shell commands.

### `taskfence-report`

Owns human-readable report generation.

Report sections:

- Summary.
- Task input.
- Agent and model.
- Policy summary.
- Sandbox summary.
- Timeline.
- Commands.
- Tool calls.
- Approvals.
- Denied actions.
- Network destinations.
- File changes.
- Test results.
- Artifacts.
- Residual risks.

Must consume audit/state/artifact records. It must not scrape terminal output as
its primary source of truth.

### `taskfence-state`

Owns queryable task state.

Phase 1 can be filesystem-backed. Introduce SQLite when logs, approvals, replay,
or Web UI queries require structured persistence.

Current filesystem-backed state can read reports, structured event summaries,
captured diffs, captured logs, and workspace-local task summaries from
`.taskfence/tasks`. Task summaries use structured `task.resolved.json` and
`events.jsonl` evidence and do not infer status from rendered report text.

Responsibilities:

- Task status.
- Approval status.
- Artifact index.
- Replay input index.
- Future team-server migration path.

### `taskfence-testkit`

Owns reusable test helpers.

Responsibilities:

- Temporary repositories and task files.
- Fake runner.
- Fake approval provider.
- Fake audit sink.
- Golden report fixtures.
- Policy decision fixtures.

Every module that touches side effects should have unit tests through testkit
fakes before integration tests use Docker.

## End-to-End Logic Flow

```text
CLI
  -> ConfigLoader parses and validates task YAML
  -> Orchestrator creates TaskContext
  -> PolicyEngine evaluates planned sandbox and agent command
  -> ArtifactStore creates task directory
  -> AuditLogger records resolved task and sandbox summary
  -> AgentAdapter builds invocation
  -> Runner starts sandboxed process
  -> Log streams are redacted and appended to audit/artifacts
  -> Observable actions are evaluated by PolicyEngine
  -> ApprovalEngine pauses on RequireApproval
  -> Runner finishes or is stopped
  -> ArtifactStore collects diff and logs
  -> ReportGenerator writes report.md
  -> StateStore marks task complete, failed, denied, or timed out
```

## Status Model

Use explicit task states:

- `Created`
- `Validating`
- `Preparing`
- `Running`
- `WaitingForApproval`
- `Stopping`
- `CollectingArtifacts`
- `Reporting`
- `Succeeded`
- `Failed`
- `Denied`
- `TimedOut`
- `Cancelled`

No module should infer final status from only the process exit code. Policy
denials, approval denials, runner setup failures, timeouts, and report
generation failures are distinct outcomes.

## Configuration Contract

The initial YAML in `examples/task.yaml` should map to typed config structs.

Top-level sections:

- `id`
- `goal`
- `workspace`
- `agent`
- `sandbox`
- `permissions`
- `secrets`
- `approval`
- `audit`

Validation rules:

- `goal`, `workspace`, `agent.command`, and `sandbox.type` are required.
- Unknown top-level fields should be rejected by default.
- Unknown nested fields should be rejected once schema generation exists.
- Command patterns must be parsed into typed matchers.
- Domain allowlists must be normalized to lower-case hostnames.
- Report format must be one of the supported formats.
- Budget and timeout values must be positive and bounded.

## Concurrency Strategy

Multiple agents can implement independent slices after the shared contracts are
accepted. The safest pattern is contract-first, then parallel implementation by
crate, then integration.

### Sequential Gate 0: Contract Baseline

Owner: lead agent.

Deliverables:

- Workspace `Cargo.toml`.
- Empty crates with public module declarations.
- Shared domain types in `taskfence-core`.
- Trait definitions for config, policy, approval, audit, artifacts, runner,
  agent adapter, report, and state.
- Error type conventions.
- Testkit skeleton.
- A work queue file or issue list assigning each parallel worker a disjoint
  write scope and expected test command.

Why this must be sequential:

- It establishes compile targets and prevents sub-agents from inventing
  incompatible interfaces.
- It defines disjoint write boundaries for parallel work.

Acceptance:

- `cargo test --workspace` compiles.
- Each crate has a clear `lib.rs`.
- Public traits are documented enough for implementers.

### Parallel Wave 1: Independent Foundations

Run these sub-agents concurrently after Gate 0.

Scheduling rule:

- The lead agent keeps the critical path local: workspace creation, shared
  contracts, and final integration.
- Worker agents receive bounded write scopes only after shared traits compile.
- No worker edits another worker's crate unless the lead agent changes the
  assignment.
- If a worker needs a shared type change, it reports the exact contract gap and
  pauses that part instead of creating a second local version.

#### Agent A: Config and Schema

Write scope:

- `crates/taskfence-config/`
- config fixtures under `crates/taskfence-testkit/fixtures/config/`
- generated schema location if configured.

Tasks:

- Implement YAML parser.
- Validate `examples/task.yaml`.
- Resolve workspace-relative paths.
- Produce typed `ResolvedTask` input.
- Add invalid-config tests for path escapes, missing fields, unknown fields,
  network defaults, env allowlists, and secret exposure.

Acceptance:

- Unit tests cover valid and invalid task files.
- No Docker, CLI, or audit dependencies beyond shared traits/types.

#### Agent B: Policy Engine

Write scope:

- `crates/taskfence-policy/`
- policy fixtures under `crates/taskfence-testkit/fixtures/policy/`

Tasks:

- Implement typed matchers for commands, paths, domains, env vars, secrets, and
  tool calls.
- Enforce deny > approval > allow > default.
- Add decision metadata with rule IDs and reasons.
- Cover shell-wrapper command attempts such as `sh -c`, absolute executable
  paths, arguments appended to approved prefixes, and deny patterns that must
  override approval patterns.
- Add table-driven tests for all policy dimensions.

Acceptance:

- Policy tests are deterministic and do not perform side effects.
- Unclassifiable actions deny by default.

#### Agent C: Audit, Artifacts, and Report

Write scope:

- `crates/taskfence-audit/`
- `crates/taskfence-artifacts/`
- `crates/taskfence-report/`
- report fixtures under `crates/taskfence-testkit/fixtures/report/`

Tasks:

- Implement local artifact directory layout.
- Implement JSONL audit writer with redaction hooks.
- Implement pre-run baseline capture and final diff metadata.
- Implement Markdown report generation from structured events.
- Add golden report tests.

Acceptance:

- Reports are generated from audit/state/artifact data, not scraped logs.
- Secret-like fixture values are redacted.
- Dirty initial workspaces are represented separately from agent changes.

#### Agent D: Runner and Agent Adapter

Write scope:

- `crates/taskfence-runner/`
- `crates/taskfence-agent/`
- runner fixtures under `crates/taskfence-testkit/fixtures/runner/`

Tasks:

- Implement generic command adapter.
- Implement runner trait and fake runner.
- Implement Docker runner behind a feature flag or runtime availability check.
- Add tests for invocation construction, env allowlist, mount planning, and
  timeout handling.
- Add tests for read-only/read-write mount overlap, host home exclusion, host
  env exclusion, Docker socket exclusion, and unsupported domain allowlists.

Acceptance:

- Unit tests pass without Docker.
- Docker integration tests are separately marked and skipped when Docker is
  unavailable.
- The runner never silently claims domain-level network enforcement when the
  selected backend cannot enforce it.

#### Agent E: CLI and Approval UX

Write scope:

- `crates/taskfence-cli/`
- `crates/taskfence-approval/`

Tasks:

- Implement command parsing.
- Implement local interactive approval provider.
- Implement non-interactive fail-closed behavior.
- Wire CLI commands to orchestration traits through test fakes.

Acceptance:

- CLI snapshot tests cover success, validation failure, approval timeout/deny,
  and report lookup.
- Approval records include actor, time, action, rule, reason, and result.

#### Agent F: Gateway Contracts

Write scope:

- `crates/taskfence-gateway/`

Tasks:

- Define normalized tool action model.
- Carry configured task-file tool permissions through policy decisions.
- Define secret broker trait and redacted secret references.
- Keep MCP/HTTP adapter stubs returning explicit unsupported errors aligned with
  the normalized tool action model.
- Add tests proving gateway actions route through policy and audit traits.

Acceptance:

- No production gateway behavior is claimed as implemented.
- Contracts are usable by policy and audit without protocol-specific coupling.

### Sequential Gate 1: Compile and Contract Integration

Owner: lead agent.

Tasks:

- Merge parallel branches or patches.
- Reconcile public types.
- Remove duplicate local abstractions.
- Ensure all crates compile together.
- Ensure testkit fakes satisfy orchestrator needs.

Acceptance:

- `cargo fmt --all`
- `cargo clippy --workspace --all-targets -- -D warnings`
- `cargo test --workspace`

### Parallel Wave 2: Orchestration, Integration Tests, and Docs

Start after Gate 1.

#### Agent G: Orchestrator Implementation

Write scope:

- `crates/taskfence-core/`
- orchestrator integration tests.

Tasks:

- Implement task lifecycle state machine.
- Wire config, policy, approval, runner, audit, artifacts, report, and state
  traits.
- Add fake-runner end-to-end tests.

Acceptance:

- Tests cover success, denied command, approval-required command approved,
  approval denied, runner failure, timeout, and report failure.

#### Agent H: Docker Integration and Local Demo

Write scope:

- Docker runner integration tests.
- `examples/` additions only if needed.
- local runner docs.

Tasks:

- Run `taskfence run examples/task.yaml` against a controlled fixture repo.
- Verify mount and env behavior.
- Capture stdout/stderr/diff/report artifacts.

Acceptance:

- Demo works on a machine with Docker.
- Without Docker, CLI fails with a clear environment error.

#### Agent I: Developer Documentation

Write scope:

- `README.md`
- `docs/architecture.md`
- `docs/roadmap.md`
- new implementation docs as needed.

Tasks:

- Document actual commands, artifact layout, config schema, and current limits.
- Keep roadmap and architecture consistent with implemented behavior.

Acceptance:

- Docs do not overclaim unsupported gateway, web UI, or enterprise features.

### Sequential Gate 2: Release Readiness

Owner: lead agent.

Tasks:

- Run full local quality gate.
- Check generated artifacts are ignored if needed.
- Review report output manually.
- Confirm security defaults.
- Produce release notes or phase-completion summary.

Acceptance:

- All required tests pass.
- Manual demo produces a report with policy, logs, approvals, denied actions,
  diffs, and residual risks.
- No raw secrets appear in logs, reports, or fixtures.

## Sub-Agent Prompt Template

Use this template when starting implementation workers:

```text
You are implementing one bounded slice of TaskFence.

You are not alone in the codebase. Other agents may be editing different crates
in parallel. Do not revert unrelated changes. Keep your writes inside the scope
assigned below. If a shared contract is insufficient, stop and report the exact
contract change needed instead of inventing a parallel abstraction.

Global quality rules:
- Keep code layered by the crate boundaries in docs/development-design.md.
- Use typed structs/enums and domain errors; avoid stringly typed control flow.
- Side effects must sit behind traits so tests can use fakes.
- Security defaults fail closed.
- Deny beats approval; approval beats allow; default deny applies when no rule
  matches.
- Never expose host secrets or host home by default.
- Add focused tests for success and failure branches.
- Run the relevant tests before final handoff and report what passed or failed.

Assigned write scope:
<paths>

Task:
<concrete implementation task>

Expected output:
- Files changed.
- Tests added.
- Commands run.
- Any unresolved contract questions.
```

## Integration Risks

- Docker network allowlisting is platform-dependent. Treat domain allowlists as
  a policy requirement and implement the enforcement path explicitly; do not
  imply Docker can enforce domains by itself unless the code actually routes
  traffic through a controlled proxy.
- Black-box CLI agents may run shell commands internally that TaskFence cannot
  semantically observe unless execution goes through a controlled shell wrapper
  or gateway. Phase 1 audit can capture terminal output and diffs, but command
  policy enforcement must be honest about what is mediated.
- Path restrictions must account for symlinks and canonical paths before mounts
  are created.
- Approval denial semantics must be explicit: some denied actions can be skipped,
  while others should fail the task.
- Reports must survive partial failures. A failed task still needs evidence.
- Gateway credentials must stay gateway-side. A tool integration that passes raw
  tokens into the sandbox violates the product boundary.

## Quality Gates

Every implementation wave should pass:

- Formatting.
- Linting with warnings denied.
- Unit tests for modified crates.
- Contract tests for shared event/config/schema changes.
- Golden report tests when report output changes.
- Integration tests for orchestrator behavior with fakes.
- Docker integration tests only when Docker is available.

Security-specific checks:

- No raw fixture secret appears in logs or reports.
- Host environment is not inherited by default.
- Host home is not mounted by default.
- Unknown config fields fail validation.
- Unknown policy actions deny by default.
- Approval timeout fails closed by default.

## Definition of Done for the First Usable Version

The first usable version is complete when:

- `taskfence init [path]` writes a valid starter task YAML file and refuses to
  overwrite existing files. This is implemented as local task-file scaffolding,
  not as a project generator or task execution path.
- `taskfence validate <task-file>` performs local pre-run checks for task file
  resolution, generic agent command shape, planned command policy, and Docker
  runner plan construction without starting Docker, writing artifacts, or
  requesting approvals.
- `taskfence run examples/task.yaml` executes through the orchestrator, not a
  one-off demo path. This is implemented for the local Docker runner demo.
- The task file is parsed, validated, and resolved with documented defaults.
- The agent runs inside the Docker runner with controlled mounts and env.
- Policy decisions are recorded for planned command/tool actions. Observation
  of arbitrary in-container shell commands remains future gateway or wrapper
  work. Task-file `permissions.tools` values are already parsed into policy,
  and typed gateway mediation can record configured tool-call decisions plus
  optional approval request/resolution evidence and redacted secret references
  into audit/report output without executing the external tool.
- Approval-required actions fail closed by default in local mode. Operators can
  opt into in-process terminal approval with `taskfence run --interactive-approval`
  or explicitly wait for workspace-local external approval with
  `taskfence run --external-approval` plus `taskfence approvals`,
  `taskfence approval`, `taskfence approve`, and `taskfence deny`.
- Artifacts are written under `.taskfence/tasks/<task-id>/`; policy-denied and
  approval-denied pre-run decisions also write local evidence and a report when
  artifact creation succeeds.
- A Markdown report is generated from structured evidence.
- Tests cover the main success path and the important failure branches.

Current unsupported surfaces must remain explicit in docs and errors: Docker
domain allowlists without an enforcing proxy, gateway protocol execution, Web
UI, replay, team server, and enterprise behavior.
