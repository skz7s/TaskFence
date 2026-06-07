# Agent-Facing Gateway Wrapper And Spool Boundary

## Context

The local executable gateway foundation proves policy, registry, approval,
redacted secret references, audit events, reports, and deterministic fixture
execution through the CLI-owned `taskfence gateway call` path. Black-box agents
running in a sandbox still do not have a first-class way to call that local
gateway path without receiving host credentials or depending on an unreviewed
host listener.

Three candidate integration shapes were considered for sandboxed agents:

- a generated CLI wrapper mounted into the sandbox
- a mounted request/response spool directory
- a local sidecar service or listener

## Decision

Promote a bounded request/response spool prototype as the first agent-facing
gateway interface, without treating it as a production MCP/HTTP transport or a
long-lived listener.

The prototype keeps raw credentials gateway-side, keeps Docker networking
disabled-compatible, and makes request, approval wait, resolution, response,
and failure evidence inspectable as files under the task evidence boundary. It
also keeps path validation and artifact ownership close to the existing runner
and artifact-store model.

Task artifact setup now creates `gateway-spool/requests`,
`gateway-spool/responses`, and a generated `taskfence-gateway-submit` wrapper
under `.taskfence/tasks/<task-id>/`. The Docker runner mounts only that
dedicated spool path at `/taskfence/gateway-spool` for tasks with configured
gateway tools and rejects broader read/write mounts that would also expose the
spool. The host-side `taskfence gateway spool process <task-file>
<request-file>` command validates one request path under the task spool,
executes one mediated local fixture action, and writes one typed response.

A generated CLI wrapper remains useful as the agent-visible command surface, but
it should be a thin client over the spool rather than a direct host gateway
executor. A wrapper-only approach would still need a trusted host process or
shared executable path to perform mediation, approvals, and adapter execution,
so it does not remove the hard boundary questions by itself.

A sidecar or host listener is deferred. It would require a separate plan and
decision covering listener lifecycle, port binding, Docker networking,
authentication between sandbox and host, approval wait semantics, cancellation,
timeouts, and denial behavior.

## Consequences

- The current `taskfence gateway call` command remains a local demo and
  operator/test surface, not the production agent-facing gateway interface.
- The spool prototype validates request and response paths against the task
  artifact root, rejects parent components and symlink escapes, rejects unknown
  request files, and fails closed on missing approvals, timeouts, cancellations,
  malformed requests, unsupported actions, denied actions, unavailable secrets,
  and adapter failures.
- Raw gateway credentials still must not enter the sandbox, task parameters,
  audit logs, reports, or fixture artifacts.
- Wrapper and spool support may be documented only as this bounded local
  prototype. Sidecar/listener support, production MCP/HTTP/GitHub execution,
  SDK/webhook execution, and raw secret-broker actions remain unimplemented.

## Validation And Rollback

Validation for this bounded prototype is documentation plus runtime surface
tests:

- `cargo test -p taskfence-core -p taskfence-gateway -p taskfence-runner -p taskfence-cli`

Rollback is to remove the generated wrapper creation, dedicated Docker spool
mount, spool request/response types, and `taskfence gateway spool process`
command, leaving the local gateway foundation as CLI-only fixture execution.
Any future sidecar, listener, production MCP/HTTP transport, or live GitHub
implementation should create a new durable plan before changing runner or
gateway process boundaries.
