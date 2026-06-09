# Security Policy

TaskFence exists to constrain AI agent execution, so security reports are
treated as high priority even while the project is in local preview.

For the trust boundaries, protected assets, secure defaults, and current
preview limitations, read [docs/security-model.md](docs/security-model.md).

## Supported Versions

TaskFence has not made a stable production release yet. Security fixes are
handled on the main development line until the first versioned release branch
exists. Compatibility expectations for preview releases are documented in
[docs/versioning.md](docs/versioning.md).

## Reporting A Vulnerability

Do not file public issues for vulnerabilities.

Use GitHub private vulnerability reporting when it is enabled for the
repository. If it is not enabled, contact the repository maintainers through the
maintainer contact path listed in the GitHub repository profile and include
only the minimum detail needed to establish contact. Avoid posting exploit
details, secrets, tokens, private URLs, or customer data in public channels.

Useful report details:

- affected TaskFence command, crate, connector, or runner
- task file or policy shape needed to reproduce the issue, with secrets removed
- expected fail-closed behavior
- observed behavior and impact
- whether the issue exposes host files, credentials, network access, audit
  integrity, approval bypass, gateway-side secrets, or sandbox escape risk

## Response Targets

These are targets, not legal commitments:

- acknowledge high-impact reports within 3 business days
- provide an initial assessment within 10 business days
- publish a fix or mitigation once it has been validated and coordinated

## Security Boundaries

The current supported surface is local preview. The following are not production
supported yet:

- long-lived production API daemon
- deployed team server
- production Web UI
- production MCP server
- arbitrary HTTP proxy
- SDK/webhook connectors
- SSO provider integration
- object storage adapter
- Kubernetes, microVM, or managed-cloud live execution

Reports about these contract-only surfaces are still useful when they identify
documentation overclaims, unsafe defaults, or implementation paths that would
make the future production boundary weaker.
