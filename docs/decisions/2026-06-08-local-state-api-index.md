# Local State API And JSON Index

## Context

The local review foundation already reads structured `.taskfence/tasks`
evidence and workspace-local approval records. The next state slice needs a
durable local query boundary for review, comparison, approval, and future
migration workflows without treating rendered Markdown reports as source of
truth or prematurely introducing a long-lived service.

## Decision

Add a workspace-local JSON state index at `.taskfence/state/local-index.json`.
`taskfence state index --workspace <workspace>` rebuilds the index from
structured task evidence and prints it; `--read-only` reads the existing index
without refreshing it.

Keep the foreground `taskfence review --serve` loopback server as the local API
boundary for now. While it is running, it exposes JSON routes for the local
index, task lists/details, artifacts, events, logs, diffs, reports, replay
plans, task comparisons, approvals, and approval resolution. Artifact downloads
are limited to known evidence files and one-level custom artifact entries under
the task evidence directory, with parent-path, absolute-path, symlink, non-file,
and containment checks.

The source of truth remains structured `.taskfence` evidence and approval
records. Markdown reports remain review artifacts, not state input.

## Consequences

- Local review has a durable structured index without adding SQLite yet.
- The API is a foreground loopback operator surface, not a daemon, team server,
  remote API, or cross-workspace index.
- Contained artifact downloads can support richer review workflows without
  turning the review server into a broad file server.
- Future SQLite, Postgres, team-server, TypeScript Web UI, and replay execution
  work can consume the structured state contracts without scraping reports.

## Validation And Rollback

Validation:

- `cargo fmt --all --check`
- `cargo test -p taskfence-state -p taskfence-cli -p taskfence-core -p taskfence-report`

Rollback is to remove `taskfence state index`, the JSON index types, review API
routes, and artifact download route while keeping the existing static review
page and CLI evidence query commands.
