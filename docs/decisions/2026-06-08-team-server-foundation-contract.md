# Team Server Foundation Contract

## Context

TaskFence needs a future team control plane for shared policy, approvals,
worker execution, audit export, and server-owned state. The local review and
replay foundation already reads structured `.taskfence` evidence, but starting
a real API server or Postgres backend before the state and security boundaries
are explicit would overclaim production behavior.

## Decision

Add a contract-only team-server foundation in `taskfence-state`:

- typed API resources for task evidence, approvals, replay inputs, and audit
  export
- organization policy and RBAC decisions with fail-closed method/resource
  matching
- optional approval-owner enforcement for approval resolution
- deterministic in-memory worker leases for local development tests
- Postgres state configuration validation for a future database URL env var and
  schema, with explicit unsupported live-backend behavior
- artifact-root containment checks for future team artifact storage
- local `.taskfence` to team migration planning from structured files only
- explicit unsupported errors for persistent team server start and audit export
  sinks

No API listener, worker daemon, Postgres storage backend, service entrypoint,
audit export sink, SSO, or live team execution is implemented by this slice.

## Consequences

Future team-server work has a reviewed contract for RBAC, approval ownership,
worker state transitions, artifact storage boundaries, and migration inputs.
Local development can exercise those contracts without inventing a service
deployment surface. Documentation must continue to say that persistent team
server, live Postgres, durable workers, SSO, SIEM export, and enterprise
connectors remain future work until each has its own implementation and tests.

## Validation And Rollback

Validation for the contract slice is `cargo test -p taskfence-state` plus the
phase workspace gate. Rollback is to remove the contract types and tests before
any live server code depends on them; no deployment or persistent data
migration is introduced.
