# Security Model

TaskFence is a control layer around AI agent tasks. It does not make an agent
safe by itself; it constrains how a task is configured, executed, mediated,
approved, and recorded.

The current project status is local preview. This document describes the
security model for implemented local-preview surfaces and the review bar for
future beta or production-supported surfaces.

## Protected Assets

TaskFence is designed to protect:

- host workspaces and files outside explicitly allowed roots
- host home directories, SSH agents, Docker sockets, package-manager tokens,
  cloud credentials, and other host secrets
- gateway-side credentials used for live connectors
- task policy decisions and approval records
- append-only audit events, task artifacts, reports, local state indexes, and
  team-state records
- operator intent captured in task files, CLI flags, approvals, and release
  documentation

## Trust Boundaries

The host operator is trusted to choose the task file, local workspace, Docker
image, SSH target, and any gateway-side secrets. TaskFence does not protect a
machine from its own administrator, local root, a compromised Docker daemon, or
an already malicious repository checkout.

The task file is untrusted input until parsed and resolved. Config parsing
rejects unknown fields in the supported schema, normalizes paths, rejects
parent-path escapes in sensitive locations, and fails closed when a requested
runner or gateway control is unavailable.

The sandboxed agent process is untrusted. It should receive only the mounted
workspace paths, command arguments, environment variables, and gateway access
explicitly allowed by the resolved task. Host home paths, host secrets, Docker
socket access, SSH agent forwarding, package-manager credentials, and cloud
credentials must not be passed through by default.

The local Docker runner is a local preview execution boundary. It plans
workspace mounts, environment allowlists, resource limits, network mode, and
artifact capture before starting a container. Docker does not provide
domain-level allowlists by itself. When a task requests domain-level network
egress, TaskFence must fail closed unless the task uses the local gateway
egress path that keeps the container network disabled and checks HTTPS hosts at
the gateway boundary.

The remote SSH runner is a live remote backend only under an explicit operator
capability contract. The operator must declare that the remote workspace,
remote secrets, process termination, and network policy are provided by the
remote environment. Generic SSH cannot enforce disabled or default-deny network
access, domain allowlists, local gateway mounts, or remote file diffs, so those
task shapes fail closed instead of silently weakening the local Docker model.

The gateway boundary is trusted to mediate configured tool actions before a
live connector runs. Gateway-side secrets are looked up through redacted
references after policy, registry, approval, and budget checks. Raw connector
credentials must not be written to task files, sandbox parameters, audit
events, reports, artifacts, or local review pages.

The approval actor is trusted to approve or deny high-risk actions based on the
presented structured request. Non-interactive approval-required actions fail
closed unless the task explicitly selects an available approval flow. Approval
denial and timeout are terminal for the guarded action.

Audit, artifact, report, and state stores are evidence surfaces, not authority
to bypass policy. Reports must be generated from structured task evidence
rather than scraped terminal output. Artifact downloads and state import paths
must remain contained within the configured task or team roots.

The local review server is a foreground loopback operator tool. It binds to
`127.0.0.1`, rebuilds views from local evidence, and is not a production API
daemon, production Web UI, or team approval service.

## Secure Defaults

TaskFence's default behavior is fail-closed:

- unknown, malformed, unregistered, unsupported, or unclassifiable actions are
  denied
- explicit deny wins over approval-required and allow rules
- approval-required wins over allow rules
- no policy match means deny
- network defaults to deny unless explicitly configured
- secret access is denied unless the task grants a scoped gateway-side secret
  reference
- task files reject unknown supported-schema fields instead of ignoring them
- shell-wrapped commands require approval even when the raw wrapper executable
  appears in allow rules
- gateway operations run only for configured, registered, supported actions
- unsupported production surfaces return explicit unsupported errors instead
  of silently falling back to a weaker path

## In-Scope Threats

Security reports and reviews should treat these as in scope:

- path traversal, symlink escape, mount overlap, or artifact containment bugs
- command policy bypass through executable aliases, raw command text, appended
  arguments, or shell wrappers
- explicit deny being overridden by approval or allow rules
- approval-required actions running after denial, timeout, or non-interactive
  fail-closed behavior
- network egress when the task requested disabled or default-deny networking
- domain allowlist claims that are not actually enforced
- raw secrets appearing in sandbox input, logs, audit events, reports,
  artifacts, review pages, fixture files, or release documentation
- gateway live connector actions running without policy, registry, approval,
  budget, redaction, or unsupported-action checks
- local review or artifact routes serving files outside the workspace evidence
  root
- structured evidence being replaced by scraped terminal output for
  security-relevant reports

## Out Of Scope And Current Limits

The current local preview does not provide:

- protection from a malicious host administrator, compromised kernel,
  compromised Docker daemon, or container runtime escape
- proof that an arbitrary Docker image is safe
- proof that a remote SSH host really enforces its declared isolation controls
- production API daemon, deployed team server, production Web UI, production
  MCP server, arbitrary HTTP proxy, SDK/webhook connectors, SSO, object storage,
  Kubernetes, microVM, managed-cloud runner execution, or background audit
  export service support
- safety for agents run directly on the host outside TaskFence
- inspection of encrypted traffic unless traffic is routed through an explicit
  gateway or tool integration

Reports about unsupported or contract-only surfaces are still useful when they
identify documentation overclaims, unsafe defaults, or future implementation
paths that would weaken the security boundary.

## Security Review Bar

Before a surface moves from preview to beta or production-supported status, it
must have:

- success and failure tests for the security branches it owns
- documented configuration, operational limits, and unsupported behavior
- structured audit evidence for relevant policy, approval, secret, artifact,
  connector, and state decisions
- redaction tests for secret-like input and output
- release notes that identify skipped Docker, database, remote runner, or live
  connector coverage

Use [SECURITY.md](../SECURITY.md) to report vulnerabilities.
