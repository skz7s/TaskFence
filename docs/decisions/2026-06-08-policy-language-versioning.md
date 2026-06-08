# Policy Language And Schema Versioning

## Context

TaskFence already has a built-in Rust policy evaluator for command, path,
network, environment, secret, tool, and budget decisions. Production readiness
requires schema/version contracts before considering broad external policy
engine adoption.

## Decision

Keep the built-in evaluator as the current production-readiness strategy.
Record OPA, Cedar, and custom plugin integration as contract-only future
options until their trust boundary, data model, audit evidence, migration path,
and failure behavior are implemented and tested.

Version the task-file, audit-event, connector-policy-template, and
runner-capability contracts before broad external adoption. Department or
use-case policy packs remain explicit opt-in templates, not defaults.

## Consequences

Task files, reports, replay inputs, team records, connector templates, and
runner capability contracts must preserve compatibility through explicit
migration checks. Unsupported external policy engines must fail closed instead
of silently bypassing the built-in evaluator.

## Validation Or Rollback Notes

Validation is covered by the `PolicyLanguageContract` tests in
`taskfence-policy`. A later decision may replace or extend this strategy only
after the external policy boundary has tests for unknown input, deny precedence,
approval precedence, default deny, redaction, and audit evidence.
