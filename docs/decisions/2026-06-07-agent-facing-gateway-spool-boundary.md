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

Do not promote a long-lived agent-facing gateway interface in this wave.

The next implementation should prefer a request/response spool prototype before
a sidecar service. A spool can keep raw credentials gateway-side, keep Docker
networking disabled, and make request, approval wait, resolution, response, and
failure evidence inspectable as files under the task evidence boundary. It also
keeps path validation and artifact ownership close to the existing runner and
artifact-store model.

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
- Future spool work must validate request and response paths against the task
  artifact root, reject symlink escapes, reject unknown operation files, and
  fail closed on missing approvals, timeouts, malformed requests, or unsupported
  actions.
- Raw gateway credentials still must not enter the sandbox, task parameters,
  audit logs, reports, or fixture artifacts.
- Wrapper, spool, and sidecar support must not be documented as implemented
  until the request lifecycle and failure behavior are testable.

## Validation And Rollback

Validation for this bounded spike is documentation plus the existing runtime
surface tests:

- `cargo test -p taskfence-runner -p taskfence-gateway -p taskfence-core`

Rollback is to remove this decision record and leave the local gateway
foundation as CLI-only fixture execution. Any future spool or sidecar
implementation should create a new durable plan before changing runner or
gateway process boundaries.
