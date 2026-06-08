# Local Gateway Egress Boundary

## Context

TaskFence previously failed closed when a local Docker task configured
`permissions.network.allow_domains`, because Docker bridge networking cannot
enforce domain-level policy by itself. Phase 1 of the remaining capability plan
requires a local gateway/network control path without exposing raw credentials
or broad host networking to agent sandboxes.

## Decision

Local domain allowlists are executable only when the task explicitly configures
all of:

- `gateway.mode: local_listener`
- `gateway.egress.allow_domains: true`
- a registered `http egress.fetch` gateway tool

Even then, the Docker runner keeps container networking disabled with
`--network none`. The sandbox can reach the task-scoped gateway boundary through
the mounted gateway spool path or foreground loopback listener metadata, and the
gateway performs bounded HTTPS GET/HEAD requests only after registry, tool
policy, network destination policy, approval, budget, audit, and URL validation
checks.

## Consequences

Tasks with `permissions.network.allow_domains` still fail closed unless the
gateway egress boundary is explicitly configured. The implemented egress action
is not an arbitrary HTTP proxy, production MCP server, SDK/webhook connector,
or persistent API service. Raw secrets remain gateway-side.

## Validation

Targeted validation covers config parsing, old fail-closed behavior, the new
Docker `--network none` gateway egress plan, egress URL rejection, network
policy denial before client execution, and listener request parsing.
