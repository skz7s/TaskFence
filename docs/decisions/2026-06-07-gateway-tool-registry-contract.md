# Gateway Tool Registry Contract

## Context

Gateway mediation already normalized protocol-shaped tool actions, checked
supported protocols, evaluated configured tool policy, recorded audit evidence,
and could request approval when an approval engine was explicitly attached.

It did not yet have a known-tool boundary. That meant a future gateway adapter
could ask policy about a normalized tool action without first proving that the
operation was part of an expected gateway catalog.

## Decision

Add a gateway-owned tool registry contract.

The contract includes typed `ToolKey` and `RegisteredTool` values that normalize
protocol, tool, and operation segments. `GatewayMediator` can now be configured
with a `ToolRegistry`; when present, mediation rejects unregistered tool actions
before policy evaluation, records an audit error, and returns a gateway error.

No registry remains the compatibility path and preserves existing policy
mediation behavior.

This is a contract boundary only. It does not implement production MCP, HTTP,
CLI wrapper, SDK, webhook, secret-broker execution, dynamic tool discovery, or
team-server registry state.

## Consequences

- Future gateway adapters can fail closed for unknown operations before asking
  policy for a decision.
- Tests and local composition can use `InMemoryToolRegistry` without committing
  to storage, service APIs, or dynamic discovery.
- Documentation can describe a known-tool registry boundary without claiming
  real tool execution.

## Validation And Rollback

Validation:

- `cargo test -p taskfence-gateway`
- `cargo clippy --workspace --all-targets -- -D warnings`
- `cargo test --workspace`

Rollback is to remove the registry types and optional mediator registry check,
returning gateway mediation to supported-protocol checks followed directly by
policy evaluation. Do not add production gateway execution without a separate
decision that records the execution and credential boundary.
