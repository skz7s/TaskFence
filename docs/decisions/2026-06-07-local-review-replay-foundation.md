# Local Review And Replay Foundation

## Context

TaskFence now has stable file-backed local evidence under `.taskfence/tasks`
and workspace-local approval records under `.taskfence/approvals`. Phase 4
needs task review, approval handling, structured comparison, and replay input
capture without claiming a team server, persistent API, SQLite migration, or
deterministic replay execution before those contracts are ready.

## Decision

Use the Rust CLI as the first local review boundary. `taskfence review` renders
workspace-local evidence into static HTML, and `taskfence review --serve`
foreground-serves the same page on `127.0.0.1` with explicit approve/deny POST
routes for pending workspace-local approvals. `taskfence replay plan` reports
saved inputs, artifact paths, last status, blockers, and determinism limits
without executing replay.

The selected state store remains file-backed for this foundation. SQLite and a
persistent local API server are deferred until cross-workspace indexing, live
refresh, team-server migration, or persistent query semantics require them.

## Consequences

- Review evidence stays derived from structured files, not rendered Markdown
  scraping.
- Raw gateway credentials and tool parameter values remain out of the review
  UI; approvals operate on existing redacted approval records.
- The loopback server is a foreground local operator tool, not a daemon, team
  approval service, or remote API.
- Replay is inspectable but non-executing; external state, approvals, gateway
  mediation, network behavior, and runner image availability remain explicit
  limitations.

## Validation And Rollback

Validation should include focused state/CLI tests, rendered review-page checks
on desktop and mobile widths, and `cargo test --workspace` before claiming the
phase complete. Rollback is to keep the existing CLI evidence queries and
remove the `review` / `replay plan` commands; no durable schema migration is
required because the foundation reuses existing file-backed evidence.
