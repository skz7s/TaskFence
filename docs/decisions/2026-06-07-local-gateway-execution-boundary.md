# Local Gateway Execution Boundary

## Context

TaskFence Phase 3 had typed gateway mediation contracts for MCP and HTTP-shaped
tool requests, policy decisions, approval records, optional known-tool registry
checks, and redacted gateway secret references. It did not have an executable
local gateway path: adapter `execute` methods intentionally returned unsupported
errors, and reports could only show policy and approval evidence for tool calls.

The next implementation wave needs to prove gateway policy, approval, registry,
redaction, audit, report, and deterministic fixture behavior without exposing
host credentials or claiming production connector support.

## Decision

TaskFence will add a local executable gateway boundary with typed request,
adapter, result, and failure contracts in `taskfence-core`, while keeping
adapter selection and execution orchestration in `taskfence-gateway`.

The first executable adapters are local deterministic fixtures. They may model
GitHub-shaped operations for demos, but they must not call live GitHub, use raw
credentials, open a production MCP server, proxy HTTP traffic, or expose gateway
credentials to the sandboxed agent. Gateway secret references remain redacted
handles and are not raw secret values.

The existing `GatewayMediator` remains policy-only compatible. Executable paths
must pass through the same mediation, approval, registry, audit, and report
contracts before an adapter runs. Unsupported, denied, unregistered, malformed,
approval-denied, and approval-timeout actions fail closed with structured
evidence.

## Consequences

- Reports and local evidence queries can distinguish policy-only tool decisions
  from executable local gateway results.
- Deterministic fixture connectors can be demonstrated without remote network
  access or real credentials.
- Production GitHub, MCP, HTTP, SDK, webhook, sidecar, Web UI, replay,
  team-server, and enterprise connector behavior still require separate plans
  and review before implementation.
- Task-file gateway fixture configuration must validate empty segments, path
  escapes, symlink escapes, and unsupported connector kinds before execution.

## Validation And Rollback

Focused validation is `cargo test -p taskfence-core -p taskfence-gateway -p
taskfence-report` after contract changes. Later executable slices add config,
CLI, approval, state, and fixture tests.

Rollback is to remove the executable adapter path and its audit event variants
while keeping policy-only `GatewayMediator` behavior unchanged.
