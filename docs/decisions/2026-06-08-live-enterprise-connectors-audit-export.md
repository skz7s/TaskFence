# Live Enterprise Connectors And Audit Export Boundary

## Context

TaskFence already mediated local fixture tools and bounded GitHub/GitHub
Enterprise REST operations through policy, approval, gateway-side secrets,
budget checks, audit, and reports. Enterprise connector contracts existed for
GitLab, Jira, Feishu, WeCom, DingTalk, Gitee, CODING, database, internal HTTP,
and SIEM export, but live execution and audit export were still deferred.

## Decision

Add opt-in live adapters for the prioritized enterprise connector families with
connector-specific operation sets, parameter validation, approval-sensitive
templates, gateway-side credentials, response redaction, and explicit
unsupported-operation failures.

SIEM export uses an explicit HTTPS `api_base` plus sink reference instead of a
synthetic URI, so live execution targets a normal configured endpoint while raw
credentials remain in the gateway process.

Team audit export remains owned by the team state boundary. The service reads
structured task events, writes a contained JSON payload artifact under an
allowed artifact root, records size and SHA-256 metadata, and persists
completed or failed export records. Compliance reports are rendered from
structured events, not terminal logs or Markdown report scraping.

## Consequences

- Task files must opt into each live connector and provide gateway-side secret
  references. Raw credentials are not accepted in task configuration.
- The implemented connector behavior is bounded to documented operations; it is
  not an arbitrary HTTP proxy, production MCP server, SDK/webhook layer, Slack
  adapter, or replay contract for live side effects.
- Team audit export now has durable evidence records and artifacts without
  claiming a deployed daemon, background export service, object storage adapter,
  or SSO flow.

## Validation Or Rollback Notes

Validation is covered by connector, config, policy, audit, report, and state
tests, plus example validation for `examples/enterprise-connectors-task.yaml`.
Rollback is to return the non-GitHub enterprise connector families to
`UnsupportedGatewayAdapter`, remove live audit export completion/failure
records, and keep the existing contract-only parsing and policy templates.
