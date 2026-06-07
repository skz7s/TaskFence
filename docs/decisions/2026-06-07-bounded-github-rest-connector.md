# Bounded GitHub REST Connector

## Context

TaskFence now has typed gateway mediation, known-tool registry checks,
approval handling, redacted secret references, deterministic local fixtures,
and a task-local gateway spool prototype. The next production connector slice
needs to prove that a real remote tool can be invoked without handing raw
credentials to the agent or widening the gateway into an unreviewed MCP/HTTP
server.

## Decision

Add a bounded `github_rest` connector for configured task-file tools only.

The connector supports:

- `github.read_issue`
- `github.create_pr`
- `github.comment_issue`

`github.create_pr` calls the GitHub pull request creation API with an existing
`head` and `base`. It does not create branches, commits, pushes, or compare
changes.

Raw GitHub tokens stay gateway-side. The environment-backed secret broker reads
`TASKFENCE_GATEWAY_SECRET_<NORMALIZED_SECRET_NAME>` only after registry, policy,
and approval checks pass. The raw token is passed out-of-band to the GitHub REST
client and is not inserted into the audited `ToolAction`, task file, sandbox
environment, reports, local fixture artifacts, or task artifacts.

MCP and HTTP adapter entry points may execute through the existing
`GatewayExecutor` when the normalized action is explicitly configured. This is
not an MCP listener, HTTP proxy, arbitrary HTTP relay, SDK connector, webhook
receiver, or sidecar.

## Consequences

- Deterministic local fixtures remain the default demo and test surface.
- Live GitHub behavior is opt-in through task-file `connector.type:
  github_rest` plus gateway-scoped secret grants.
- Missing environment secrets fail closed with structured
  `SecretUnavailable` evidence before any live API call.
- Unsupported GitHub operations fail closed as `UnsupportedTool`.
- Future branch/commit creation, GitHub Enterprise, GitLab, Jira, SDK,
  webhook, listener, sidecar, and arbitrary HTTP behavior need separate
  connector slices and tests.

## Validation And Rollback

Validation:

- `cargo test -p taskfence-config -p taskfence-gateway`
- `cargo test -p taskfence-cli gateway_call_github_rest_missing_env_secret_fails_closed_with_evidence`
- `cargo test -p taskfence-core -p taskfence-gateway -p taskfence-policy -p taskfence-audit -p taskfence-report`
- `cargo clippy --workspace --all-targets -- -D warnings`

Rollback is to remove `GatewayConnectorConfig::GitHubRest`, the environment
secret broker, the GitHub REST adapter/client, parser support for
`connector.type: github_rest`, and CLI adapter selection for live connectors.
Keep deterministic local fixtures and the spool prototype intact.
