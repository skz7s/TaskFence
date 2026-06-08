# Bounded Local Replay Execution

## Context

TaskFence already stores resolved local task inputs and can plan replay from
structured `.taskfence/tasks` evidence. Phase 4 needs executable replay without
reusing raw secrets from prior runs or claiming deterministic reproduction of
external systems.

## Decision

Add `taskfence replay run <task-id> --workspace <workspace>` for supported
local replay inputs.

The command loads the saved `task.resolved.json`, assigns a new replay task id
(`{source}-replay` by default or `--replay-id`), runs the task through the same
local orchestrator, policy, approval, audit, artifact, runner, and report
pipeline, compares source and replay structured summaries, and writes
`artifacts/replay.json` in the replay task's artifact directory.

Replay execution fails closed for missing resolved inputs, existing replay
evidence ids, live or contract-only gateway connector effects, foreground
listener mode, domain allowlists, and default-allow network requirements.
Recorded limitations such as runner image availability, approval re-requesting,
or external state require explicit `--accept-limitations`.

## Consequences

- Local replay execution is now testable with fake runners and executable with
  the existing local runner path.
- Replay evidence is structured state, not scraped report text.
- Raw secrets from prior runs are not replayed; live connector effects remain
  blocked until connector-specific replay contracts exist.
- This does not provide cross-workspace replay, replay of live SaaS side
  effects, deterministic image snapshots, or team-server evaluation.

## Validation And Rollback

Validation:

- `cargo fmt --all --check`
- `cargo test -p taskfence-cli replay_`
- `cargo test -p taskfence-state replay_`
- `cargo test -p taskfence-core -p taskfence-state -p taskfence-runner -p taskfence-gateway -p taskfence-cli`

Rollback is to remove `taskfence replay run`, `ReplayRunRecord`,
`ReplayEvaluation`, and the `artifacts/replay.json` writer while preserving
`taskfence replay plan`.
