# Support

TaskFence is currently a local-preview open-source project.

## Community Help

Use GitHub issues for:

- reproducible bugs
- documentation gaps
- feature proposals inside the TaskFence core boundary
- validation or setup problems

Use the matching issue template for bug reports, feature requests,
documentation issues, or support questions. Blank issues are disabled so
security reports and support requests are routed through the documented paths.
For local-preview setup and validation failures, check
[docs/troubleshooting.md](docs/troubleshooting.md) before opening an issue.
Maintainers triage public issues using
[docs/community-triage.md](docs/community-triage.md).

Include:

- TaskFence command and task file involved
- operating system and Rust version
- validation command output summary
- whether Docker, SSH, database, or live connector credentials were involved

Do not include secrets, tokens, private repository URLs, customer data, or raw
credential-bearing logs.

## Security Issues

Use [SECURITY.md](SECURITY.md) for vulnerabilities, approval bypasses, sandbox
escape risks, secret exposure, gateway credential leaks, or audit integrity
issues.

## Production Support

TaskFence does not currently offer production support, hosted services, managed
deployment, or service-level commitments. See
[docs/config/readiness-checklist.md](docs/config/readiness-checklist.md) for the
current local-preview, beta-candidate, and unsupported production surfaces.
