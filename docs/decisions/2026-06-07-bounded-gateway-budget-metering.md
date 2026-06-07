# Bounded Gateway Budget Metering

## Context

TaskFence already had explicit `permissions.budget.allow` policy for typed
budget actions, but gateway execution did not record observed usage evidence.
The next safe step is to meter mediated gateway actions without coupling policy
to one model provider or introducing team billing state.

## Decision

Add a shared `BudgetUsage` contract for gateway adapters. Each record carries a
budget kind, positive amount, optional provider/model/operation metadata, and
redacted metadata values.

The gateway executor evaluates every planned or observed usage record through
the existing budget policy and writes `BudgetUsageRecorded` audit evidence with
the matched limit and decision. Over-limit planned usage fails closed before
secret attachment and adapter execution. Over-limit observed usage records the
partial tool result plus a `BudgetExceeded` error.

The bounded `github_rest` connector reports one planned `gateway_calls` usage
for each configured operation. Task files must explicitly allow that kind under
`permissions.budget.allow`.

## Consequences

- Budget evidence is structured JSONL audit data and report data, not scraped
  terminal output.
- Raw credentials remain gateway-side because planned budget checks run before
  secret attachment.
- This is not billing, team quota, chargeback, or broad model-provider cost
  metering.

## Validation And Rollback

Validation:

- `cargo test -p taskfence-gateway`
- `cargo test -p taskfence-core -p taskfence-policy -p taskfence-audit -p taskfence-report`

Rollback is to remove `BudgetUsage`, `BudgetUsageRecorded`, adapter usage
hooks, and the `github_rest` `gateway_calls` planned usage while preserving the
existing explicit budget policy boundary.
