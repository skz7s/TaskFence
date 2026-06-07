# Budget Policy Default Deny

## Context

The built-in policy engine already had a typed `Action::Budget` action, but it
allowed budget actions by default. That contradicted TaskFence's secure default
rule that unknown or unconfigured external consumption should fail closed.

The current local runner does not observe live token usage, provider cost,
billing state, or team quota consumption. Treating budget actions as implicitly
allowed would overstate the safety boundary once gateway or wrapper surfaces
start mediating cost-like actions.

## Decision

Add explicit `permissions.budget.allow` task-file configuration for typed
budget actions.

Each budget allowance has:

- `kind`: normalized to lower-case and required to be non-empty.
- `max_amount`: required to be positive.

The built-in policy allows `Action::Budget { kind, amount }` only when the
normalized kind matches a configured allowance and `amount <= max_amount`.
Missing kinds, empty kinds, and over-limit amounts are denied.

This is a policy boundary for mediated budget actions. It is not live token
metering, provider cost accounting, billing integration, or team quota
enforcement.

## Consequences

- Task files without `permissions.budget` remain parseable, but typed budget
  actions deny by default.
- Future gateway, wrapper, or model surfaces have a typed contract for checking
  budget-like actions before adding real metering.
- Documentation can mention budget limits without implying the local Docker
  runner observes token or provider spending.

## Validation And Rollback

Validation:

- `cargo test -p taskfence-config`
- `cargo test -p taskfence-policy`
- `cargo test -p taskfence-runner --tests --no-run`
- `cargo clippy --workspace --all-targets -- -D warnings`
- `cargo test --workspace`

Rollback is to remove `BudgetPermissions`, `permissions.budget` parsing, and
budget policy evaluation, returning budget actions to an unconfigured state.
Do not restore default allow unless a later decision records a stronger
runtime metering guarantee.
