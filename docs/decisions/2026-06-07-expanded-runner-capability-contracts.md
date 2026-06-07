# Expanded Runner Capability Contracts

## Context

TaskFence needs future runners for remote SSH, Kubernetes jobs, microVMs, and
managed cloud execution, but the current executable sandbox path is local
Docker. Adding live remote runners before their isolation, network control,
secret boundary, limit enforcement, and artifact transport contracts are
testable would weaken the secure-default model.

## Decision

Add typed sandbox families for `remote_ssh`, `kubernetes_job`, `microvm`, and
`managed_cloud`, plus runner capability reports. The expanded runner dispatcher
delegates Docker tasks to the existing Docker runner and fails closed for all
future runner families with explicit missing capabilities. Unknown sandbox types
remain unsupported.

The capability contract records whether a runner is available and whether it can
isolate filesystem and secrets, disable or default-deny network, enforce domain
allowlists when configured, enforce limits, and capture output.

## Consequences

- Docker remains the only executable runner.
- Operators can start writing and validating task files against future runner
  family names, but execution fails closed until those backends are implemented
  and tested.
- Remote runner work can be added one family at a time without changing the
  core policy, approval, audit, artifact, report, or state semantics.
- TaskFence does not silently downgrade an unavailable remote runner to Docker
  or a host-local process.

## Validation And Rollback

Validation should include config parsing tests, runner capability/fail-closed
tests, and core/testkit compilation. Runner-specific integration tests are
required only when a live runner backend is implemented or an operator supplies
the target environment. Rollback is to remove the typed future sandbox families
and expanded runner dispatcher while leaving the Docker runner contract intact.
