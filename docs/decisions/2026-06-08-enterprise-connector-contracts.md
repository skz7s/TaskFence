# Enterprise Connector And Audit Export Contracts

## Context

TaskFence already has a bounded GitHub REST connector, gateway-side secret
references, policy and approval mediation, and a contract-only team foundation.
The next roadmap slice needs enterprise connector coverage without implying
that broad live SaaS, database, or SIEM integrations are production-ready.

## Decision

Add explicit task-file connector contracts for GitHub Enterprise REST, GitLab,
Jira, Feishu, WeCom, DingTalk, Gitee, CODING, database, internal HTTP, and SIEM
export.

`github_enterprise_rest` reuses the bounded GitHub REST adapter contract with an
explicit HTTPS API base and supports only `github.read_issue`,
`github.create_branch`, `github.commit_file`, `github.create_pr`,
`github.update_pr`, `github.comment_issue`, and `github.comment_report`. Raw
tokens remain gateway-side through
`TASKFENCE_GATEWAY_SECRET_<NORMALIZED_SECRET_NAME>`.

All other enterprise connectors are opt-in contract-only surfaces. They parse
safe configuration references, expose connector-specific policy templates,
approval-sensitive operation sets, redacted secret-reference handling, and
structured unsupported execution evidence. They do not call live services.

Audit export remains a team API/RBAC resource with validated sink contracts for
destination references and credential environment variables. No live export
sink is implemented.

## Consequences

- Operators can model enterprise tools in task files without embedding raw
  credentials or DSNs.
- Unsupported live connector execution fails closed after registry, policy,
  approval, and redacted secret-reference handling when applicable.
- GitHub Enterprise gets a narrow live path without generalizing GitHub
  semantics to GitLab, Gitee, CODING, Jira, chat, database, internal HTTP, or
  SIEM surfaces.
- Slack, live service-specific clients, team-server execution, live SIEM
  export, and compliance report exports remain future work.

## Validation

Validation is covered by task-file parsing tests for all connector contracts,
gateway tests for policy templates, GitHub Enterprise REST token redaction, and
contract-only unsupported execution, plus state tests for audit-export sink
validation and unsupported live export behavior.
