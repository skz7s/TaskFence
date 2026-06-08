# Persistent Team State Boundary

## Context

TaskFence's earlier team foundation defined RBAC, approval ownership, worker
leases, artifact roots, audit-export sinks, and local evidence migration as
contracts only. Phase 6 needs those semantics to become durable state without
silently adding an operator-facing daemon or weakening local mode.

## Decision

Implement team state as state-layer service functions with two backends:

- a local JSON file backend for development and CLI workflows
- a Postgres-backed backend for organization task records, worker leases,
  artifact metadata, and audit-export plans when a configured database URL is
  available

The service boundary enforces organization-scoped RBAC before state changes,
keeps approval-owner checks in the existing team access model, rejects
duplicate, wrong-worker, unleased, and terminal worker transitions, checks
artifact writes against configured absolute roots before any backend write, and
records artifact size plus SHA-256 metadata. `taskfence team` exposes local
state inspection, structured evidence import, and worker lease commands without
requiring local task execution to use team mode.

No deployed HTTP API daemon, live worker service, SSO flow, object-storage
adapter, managed Postgres deployment, or live audit-export sink is introduced by
this decision.

## Consequences

Team semantics can now be exercised against durable local and Postgres state,
and future deployed team-server work can reuse the same state contracts instead
of inventing parallel queue or artifact metadata behavior. Operators still need
a later deployment decision before TaskFence claims a long-lived team service,
worker daemon, SSO, object storage, or live SIEM export.

## Validation And Rollback

Validation is the Phase 6 gate:
`cargo test -p taskfence-state -p taskfence-core -p taskfence-cli`, with focused
state and CLI tests for persistent leases, RBAC, artifact containment, audit
export planning, and local evidence import. Rollback is to remove the
`TeamStateBackend` implementations, `TeamStateService`, and `taskfence team`
commands while keeping the earlier contract-only types if still needed.
