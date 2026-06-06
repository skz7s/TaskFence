# Local Runner Development Plan

## Goal

Advance TaskFence from the current skeletal Rust workspace to a first usable
local secure runner path where `taskfence run examples/task.yaml` validates a
task file, plans a Docker sandbox, runs a generic agent command through the
orchestrator, records structured evidence, and writes a Markdown report without
overclaiming unsupported gateway or team-server behavior.

## Plan Source

User request on 2026-06-06: "优化当前项目治理，然后为后续开发生成plan" followed by
"继续".

Actionable requirements captured for this plan:

- Use current project governance rather than hidden helper runtime state.
- Base the development plan on the repository's actual state after governance
  optimization.
- Preserve the core feature boundary from `governance/private/agent/project-governance.md`.
- Follow `docs/development-design.md` staged implementation and crate ownership.
- Keep implementation in Rust workspace crates unless a later architecture
  decision changes it.
- Use secure defaults: fail closed, explicit deny precedence, default deny, no
  host home mount, no host secrets or socket passthrough by default.
- Validate Rust work with `cargo fmt --all`, `cargo clippy --workspace --all-targets -- -D warnings`,
  and `cargo test --workspace` when Rust tooling is available.
- Validate Docker behavior with integration tests on a Docker-capable machine,
  or record Docker unavailability explicitly.

## Snapshot

- Default branch: `main` detected from `origin/HEAD`.
- Working branch: `codex/governance-development-plan`.
- Worktree status at intake: `README.md` modified; broad project initialization
  files under `.codex/`, `AGENTS.md`, `Cargo.toml`, `crates/`, `deploy/`,
  `docs/`, `examples/`, `governance/`, and `scripts/` were already untracked.
- Current Rust state: workspace exists with long-term crate boundaries and
  shared contracts; CLI parses commands and `run` loads task files but does not
  invoke the orchestrator; core orchestrator and ports exist; config, policy,
  approval, audit, artifacts, runner planning, agent adapter, gateway mediation,
  report, state, and testkit have initial implementations or fakes.
- Current Rust tooling: `cargo 1.96.0`, `rustc 1.96.0`, `rustfmt 1.9.0`, and
  `clippy 0.1.96` are available from Homebrew on this host after local
  environment refresh.
- Next executable phase: wire CLI `run` through the local orchestrator path.

## Overall Status

done

## Phases

### Phase 1: Toolchain And Baseline Verification

Status: done

Scope:

- Ensure Rust 1.78+ tooling is available without hard-coding host paths into
  governance or docs.
- Refresh `.codex-helper/local-env.toml` with `bash deploy/manage.sh detect-env`.
- Run baseline governance and Rust checks from the current branch.
- Record any pre-existing Rust failures before making implementation changes.

Verification command:

```bash
bash deploy/manage.sh detect-env
python3 scripts/governance/build_agents.py --check
python3 scripts/governance/check_codex_governance.py
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```

Verification evidence:

- `git pull --ff-only` was attempted before implementation and failed because
  branch `codex/governance-development-plan` has no upstream tracking branch;
  work continued from the current checkout without changing branch tracking.
- `bash deploy/manage.sh detect-env` passed and refreshed
  `.codex-helper/local-env.toml` with current macOS/Homebrew tool facts.
- Baseline `cargo clippy --workspace --all-targets -- -D warnings` initially
  failed on derivable `Default` impls in `taskfence-core` and
  `taskfence-approval`; fixed mechanically by deriving defaults while
  preserving `NetworkDefault::Deny` and `LocalApprovalMode::FailClosed`.
- `python3 scripts/governance/build_agents.py --check` passed.
- `python3 scripts/governance/check_codex_governance.py` passed.
- `cargo fmt --all --check` passed.
- `cargo clippy --workspace --all-targets -- -D warnings` passed.
- `cargo test --workspace` passed.

### Phase 2: CLI-To-Orchestrator Local Path

Status: done

Scope:

- Wire `taskfence-cli run` to construct concrete local implementations for
  policy, approval, audit, artifacts, generic agent adapter, runner, report, and
  state.
- Keep CLI terminal UX thin: argument parsing, path resolution, progress output,
  and actionable errors only.
- Ensure `init`, `logs`, `approve`, `deny`, and `report` remain explicit
  unsupported paths until backed by state and approval storage.
- Add CLI-focused tests around successful task loading, unsupported commands,
  config errors, and orchestrator failure surfacing.

Verification command:

```bash
cargo test -p taskfence-cli
cargo run -p taskfence-cli -- run examples/task.yaml
```

Verification evidence:

- `cargo test -p taskfence-cli` passed with 13 tests covering parsing,
  unsupported non-run commands, config error surfacing, orchestrator failure
  surfacing, and a successful injected-runner path that writes resolved task and
  report artifacts.
- `cargo fmt --all --check` passed after formatting the CLI edits.
- `cargo run -p taskfence-cli -- run examples/task.yaml` now enters the
  orchestrator and reaches Docker runner planning; it fails closed with
  `runner error: local Docker cannot enforce domain allowlists; configure an
  enforcing proxy before allowing domains`, which is the expected current
  runner-boundary failure before Phase 3/Phase 6 demo alignment.
- `examples/task.yaml` was tightened so the configured agent launch command is
  allowed and read/write mounts no longer contain an ambiguous parent/child
  overlap.

### Phase 3: Docker Runner Execution

Status: done

Scope:

- Move `taskfence-runner` from run-plan construction to actual Docker process
  execution behind the existing runner trait.
- Preserve fail-closed behavior for unsupported domain allowlists unless an
  enforcing proxy is implemented.
- Enforce mount validation, no host home mount, no Docker socket/SSH agent/cloud
  credential passthrough, and environment allowlist behavior.
- Capture stdout, stderr, exit code, timeout, missing Docker, missing image, and
  non-zero agent exit outcomes.

Verification command:

```bash
cargo test -p taskfence-runner
cargo test --workspace
# On Docker-capable hosts:
cargo test -p taskfence-runner --test docker_integration -- --ignored
```

Verification evidence:

- `taskfence-core` runner contract now returns typed `RunOutput` containing
  `ExitStatus`, stdout, and stderr; `ArtifactStore` now owns stdout/stderr log
  persistence through `write_log`.
- `taskfence-runner` now executes `docker run --pull=never` behind the existing
  runner trait, validates Docker arguments, applies bind mounts, env allowlist,
  network mode, CPU/memory/disk options where configured, captures stdout and
  stderr, reports missing Docker, reports missing image as non-zero Docker
  output, and kills/removes timed-out containers.
- Local Docker domain allowlists remain fail-closed with the explicit
  `local Docker cannot enforce domain allowlists` error until an enforcing proxy
  exists.
- `cargo test -p taskfence-runner` passed with 13 unit tests and the ignored
  Docker integration test compiled.
- `cargo test -p taskfence-runner --test docker_integration -- --ignored`
  passed on this Docker-capable host using local `debian:bookworm-slim`, covering
  stdout/stderr capture, non-zero exit, missing image failure, and timeout.
- `cargo test --workspace` passed.

### Phase 4: Evidence Pipeline And Reports

Status: done

Scope:

- Ensure audit events, artifact writes, stdout/stderr capture, baseline, diff,
  runner exit, approval events, and report generation are driven by structured
  records rather than scraped terminal output.
- Make report generation resilient to partial failures and non-zero agent exit.
- Add tests for dirty workspace baseline, binary-looking or large logs, redaction,
  artifact write failure, report write failure, and partial failure reporting.

Verification command:

```bash
cargo test -p taskfence-audit -p taskfence-artifacts -p taskfence-report -p taskfence-core
cargo test --workspace
```

Verification evidence:

- `taskfence-core` now keeps an in-memory structured audit event list while
  recording each event through the audit port, then passes that structured event
  list to report generation.
- Stdout/stderr runner output is written through the artifact port and recorded
  as structured `Artifact` and `Log` audit events before report generation.
- Non-zero runner exits and timeouts still collect diff evidence and generate
  reports with final task status represented in the event list.
- Artifact log-write and diff-collection failures are recorded as structured
  errors, preserve the runner exit status, attempt report generation, and return
  a failed task result instead of dropping evidence.
- Report-generation failures are recorded as structured errors and return a
  failed task result with the original runner exit preserved.
- `cargo test -p taskfence-audit -p taskfence-artifacts -p taskfence-report -p taskfence-core` passed.
- `cargo test --workspace` passed.

### Phase 5: Policy, Approval, And Config Hardening

Status: done

Scope:

- Close remaining config validation gaps for unknown fields, nested schema
  strictness, ambiguous mount overlaps, path escapes, secret exposure, network
  defaults, and unsupported sandbox types.
- Harden command policy parsing for executable, args, shell wrappers, raw text,
  deny-overrides-allow, approval-overrides-allow, and unclassifiable commands.
- Expand local approval behavior for approved, denied, timeout, and
  non-interactive fail-closed outcomes.
- Keep gateway behavior limited to explicit contracts and mediation stubs until
  Phase 3 gateway work is intentionally started.

Verification command:

```bash
cargo test -p taskfence-config -p taskfence-policy -p taskfence-approval -p taskfence-gateway
cargo test --workspace
```

Verification evidence:

- Config parsing now canonicalizes existing workspace paths, validates
  configured path roots stay inside the workspace, rejects parent-path escapes,
  rejects invalid network defaults/domains, rejects zero approval timeout, and
  rejects unsupported report formats instead of silently defaulting.
- Shared config structs now use `serde(deny_unknown_fields)` so nested typed
  structs fail closed when deserialized.
- Command policy matching now evaluates raw command, executable, and
  executable-plus-args candidates; explicit deny still wins over approval and
  allow, executable-only allow rules can match commands with args, and
  shell-wrapped commands require approval even when their raw shape is allowed.
- Local approval now has explicit approved, denied, timeout, and non-interactive
  fail-closed behaviors covered by tests.
- Gateway behavior remains limited to explicit normalization/mediation contracts
  and unsupported protocol errors.
- `cargo test -p taskfence-config -p taskfence-policy -p taskfence-approval -p taskfence-gateway` passed.
- `cargo test --workspace` passed.

### Phase 6: Demo, Documentation, And Release Readiness

Status: done

Scope:

- Make the minimum demo command produce a local task directory and report:
  `taskfence run examples/task.yaml`.
- Update `README.md`, `docs/architecture.md`, `docs/roadmap.md`,
  `docs/development-design.md`, `docs/codex/runtime-architecture.md`, and
  `docs/config/*` to match implemented behavior.
- Document unsupported gateway, Web UI, replay, team-server, and enterprise
  surfaces explicitly.
- Run final quality gates and Docker validation, or record Docker unavailability.

Verification command:

```bash
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
python3 scripts/governance/build_agents.py --check
python3 scripts/governance/check_codex_governance.py
bash deploy/manage.sh doctor
```

Verification evidence:

- `examples/task.yaml` is now a runnable local Docker demo using
  `debian:bookworm-slim`, disabled networking, a generic `/usr/bin/touch`
  command, and a fixture workspace under `examples/repo/`.
- `cargo run -p taskfence-cli -- run examples/task.yaml` passed and produced
  `.taskfence/tasks/local-demo/report.md` before generated runtime evidence was
  removed from the worktree.
- `README.md`, `docs/architecture.md`, `docs/roadmap.md`,
  `docs/development-design.md`, `docs/codex/runtime-architecture.md`, and
  `docs/config/cross-platform-ops.md` now describe the implemented local runner
  path and explicitly keep Docker domain allowlists, gateway execution, Web UI,
  replay, team-server, and enterprise behavior unsupported/future.
- `cargo fmt --all --check` passed.
- `cargo clippy --workspace --all-targets -- -D warnings` passed.
- `cargo test --workspace` passed.
- `cargo test -p taskfence-runner --test docker_integration -- --ignored`
  passed on this Docker-capable host using local `debian:bookworm-slim`.
- `python3 scripts/governance/build_agents.py --check` passed.
- `python3 scripts/governance/check_codex_governance.py` passed.
- `bash deploy/manage.sh doctor` passed and refreshed ignored
  `.codex-helper/local-env.toml`.

## Commit Plan

1. `chore: establish local runner baseline validation`
2. `feat: wire CLI run through local orchestrator`
3. `feat: execute generic agents with Docker runner`
4. `feat: complete structured evidence and report pipeline`
5. `fix: harden config policy and approval fail-closed behavior`
6. `docs: document local runner demo and supported surfaces`

## Open Risks

- Docker domain allowlists may not be enforceable directly with local Docker; the
  implementation must fail closed or add an enforcing proxy before claiming
  domain-level network policy.
- The current implementation fails closed for Docker domain allowlists until an
  enforcing proxy exists.
- Local CLI approval is still non-interactive and fail-closed by default;
  interactive approval UX and durable approval lookup commands remain future
  Phase 2 work.
- Gateway execution, Web UI, replay, team-server, and enterprise behavior remain
  future work and are intentionally not claimed by docs or runtime output.

## Final Evidence

- All phases are terminal with verification evidence.
- The local runner demo succeeded through the CLI and orchestrator, wrote a
  report from structured evidence, and generated runtime artifacts were removed
  before final staging.
- Final validation commands passed:
  `cargo fmt --all --check`,
  `cargo clippy --workspace --all-targets -- -D warnings`,
  `cargo test --workspace`,
  `cargo test -p taskfence-runner --test docker_integration -- --ignored`,
  `python3 scripts/governance/build_agents.py --check`,
  `python3 scripts/governance/check_codex_governance.py`, and
  `bash deploy/manage.sh doctor`.
