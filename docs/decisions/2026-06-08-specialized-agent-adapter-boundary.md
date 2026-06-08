# Specialized Agent Adapter Boundary

## Context

TaskFence already ran generic black-box agent commands through policy,
approval, runner, audit, artifact, and report boundaries. Phase 8 adds the
first real coding-agent profiles and packages the common operator checks
through the existing repository entrypoint.

## Decision

Support specialized profiles for Codex CLI, Claude Code, Gemini CLI, and
OpenHands while keeping `generic` as the default adapter. Specialized profiles
may choose a default executable and add TaskFence-generated, non-secret runner
hints for profile, prompt, workspace, and configured gateway mode.

Specialized profiles do not pass host environment variables, provider tokens,
cloud credentials, SSH agent sockets, Docker sockets, or host home paths into
the sandbox. Coding-agent policy templates are exposed as conservative guidance
for explicit task-file policy and are not applied automatically.

`deploy/manage.sh` remains the supported operator entrypoint. `setup` verifies
the Rust toolchain, `dev` exposes the targeted Phase 8 Rust checks and example
validation, `build` runs the Rust workspace build, and `doctor` reports the
Rust workspace fact without creating a deployed service or hard-coding
machine-local paths.

## Consequences

- Basic sandboxing still works with `agent.type: generic`; operators do not
  need agent-specific integration to run a command safely.
- Specialized profiles improve setup and reporting for known coding agents
  without weakening the existing policy, approval, runner, audit, or secret
  boundaries.
- Policy templates remain opt-in guidance. Task files must still declare
  concrete command, path, network, tool, and budget permissions.
- The operator workflow stays under one script and one stable docs surface
  instead of adding long-lived compatibility wrappers or service managers.

## Validation Or Rollback Notes

Validation is covered by targeted agent, config, CLI, and core tests, shell
syntax validation for `deploy/manage.sh`, example validation for
`examples/codex-cli-task.yaml`, and the `deploy/manage.sh dev --skip-start`
operator dry run.

Rollback is to route all specialized `agent.type` values back to unsupported
adapter errors, remove the coding-agent template helper, and return the
operator script to generic detection-only Rust handling.
