//! Fail-closed built-in policy decisions for TaskFence.
//!
//! This crate evaluates path, command, network, environment, gateway-tool, and
//! budget actions against resolved task permissions. Explicit deny wins over
//! approval and allow, approval-required wins over allow, and no matching rule
//! defaults to deny.

use globset::{Glob, GlobMatcher};
use serde::{Deserialize, Serialize};
use taskfence_core::{
    Action, ActionDecision, CommandAction, NetworkDefault, PolicyEngine, ResolvedTask, RiskLevel,
    TaskFenceError,
};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum PolicyLanguageStrategy {
    BuiltInEvaluator,
    OpaContractOnly,
    CedarContractOnly,
    CustomPluginContractOnly,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct VersionedSchemaContract {
    pub name: String,
    pub version: u16,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct PolicyTemplatePack {
    pub name: String,
    pub opt_in: bool,
    pub templates: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct PolicyLanguageContract {
    pub strategy: PolicyLanguageStrategy,
    pub schemas: Vec<VersionedSchemaContract>,
    pub connector_template_packs: Vec<PolicyTemplatePack>,
    pub migration_checks: Vec<String>,
    pub unsupported_strategies: Vec<PolicyLanguageStrategy>,
}

impl PolicyLanguageContract {
    pub fn current() -> Self {
        Self {
            strategy: PolicyLanguageStrategy::BuiltInEvaluator,
            schemas: vec![
                schema("task_file", 1),
                schema("audit_event", 1),
                schema("connector_policy_template", 1),
                schema("runner_capability_contract", 1),
            ],
            connector_template_packs: vec![
                pack(
                    "coding_agent",
                    ["read_workspace", "write_workspace", "bounded_commands"],
                ),
                pack(
                    "enterprise_connectors",
                    ["github", "gitlab", "jira", "chat", "database", "siem"],
                ),
            ],
            migration_checks: vec![
                "task files must reject unknown fields and unsafe path changes".into(),
                "reports must render from structured events, not scraped terminal output".into(),
                "replay inputs must preserve connector effect limitations".into(),
                "team records must preserve organization, task id, evidence dir, status, and artifact metadata"
                    .into(),
            ],
            unsupported_strategies: vec![
                PolicyLanguageStrategy::OpaContractOnly,
                PolicyLanguageStrategy::CedarContractOnly,
                PolicyLanguageStrategy::CustomPluginContractOnly,
            ],
        }
    }
}

#[derive(Debug, Default)]
pub struct BuiltInPolicyEngine;

impl PolicyEngine for BuiltInPolicyEngine {
    fn evaluate(
        &self,
        task: &ResolvedTask,
        action: &Action,
    ) -> taskfence_core::Result<ActionDecision> {
        match action {
            Action::Command(command) => evaluate_command(task, command),
            Action::FileRead { path } => {
                if task
                    .permissions
                    .paths
                    .read
                    .iter()
                    .any(|allowed| path.starts_with(allowed))
                {
                    allow("file read path matched")
                } else {
                    deny("file read path is outside allowed roots")
                }
            }
            Action::FileWrite { path } => {
                if task
                    .permissions
                    .paths
                    .write
                    .iter()
                    .any(|allowed| path.starts_with(allowed))
                {
                    allow("file write path matched")
                } else {
                    deny("file write path is outside writable roots")
                }
            }
            Action::Network { host, .. } => match task.permissions.network.default {
                NetworkDefault::Disabled => deny("network is disabled"),
                NetworkDefault::Deny => {
                    if task
                        .permissions
                        .network
                        .allow_domains
                        .iter()
                        .any(|domain| domain == &host.to_ascii_lowercase())
                    {
                        allow("network domain matched allowlist")
                    } else {
                        deny("network domain is not allowlisted")
                    }
                }
                NetworkDefault::Allow => allow("network default allow"),
            },
            Action::EnvExpose { name } => {
                if task
                    .permissions
                    .env
                    .allow
                    .iter()
                    .any(|allowed| allowed == name)
                {
                    allow("environment variable matched allowlist")
                } else {
                    deny("environment variable is not allowlisted")
                }
            }
            Action::SecretAccess { name, scope } => {
                let gateway_grant = task.secrets.available_to_gateway.iter().any(|grant| {
                    grant.name == *name && grant.use_for.iter().any(|allowed| allowed == scope)
                });
                if gateway_grant && !task.secrets.expose_to_agent {
                    require_approval(
                        "secret_access",
                        "secret access requires approval",
                        RiskLevel::High,
                    )
                } else {
                    deny("secret is not available for requested scope")
                }
            }
            Action::ToolCall(tool) => {
                let key = format!("{}.{}", tool.tool, tool.operation);
                match first_match(
                    &[key],
                    &task.permissions.tools.deny,
                    &task.permissions.tools.approval_required,
                    &task.permissions.tools.allow,
                )? {
                    MatchDecision::Deny => deny("tool call matched deny rule"),
                    MatchDecision::Approval => require_approval(
                        "tool_call",
                        "tool call matched approval rule",
                        RiskLevel::Medium,
                    ),
                    MatchDecision::Allow => allow("tool call matched allow rule"),
                    MatchDecision::NoMatch => deny("tool call did not match policy"),
                }
            }
            Action::Budget { kind, amount } => evaluate_budget(task, kind, *amount),
        }
    }
}

fn evaluate_budget(
    task: &ResolvedTask,
    kind: &str,
    amount: u64,
) -> taskfence_core::Result<ActionDecision> {
    let kind = kind.trim().to_ascii_lowercase();
    if kind.is_empty() {
        return deny("budget kind is empty");
    }

    let Some(limit) = task
        .permissions
        .budget
        .allow
        .iter()
        .find(|limit| limit.kind == kind)
    else {
        return deny("budget kind is not allowed");
    };

    if amount > limit.max_amount {
        deny("budget amount exceeds configured limit")
    } else {
        allow("budget action matched configured limit")
    }
}

fn evaluate_command(
    task: &ResolvedTask,
    command: &CommandAction,
) -> taskfence_core::Result<ActionDecision> {
    let candidates = command_match_candidates(command);
    if matches_any_candidate(&candidates, &task.permissions.commands.deny)? {
        return deny("command matched deny rule");
    }

    if command.shell_wrapped {
        return require_approval(
            "shell_wrapped_command",
            "shell-wrapped commands require approval",
            RiskLevel::High,
        );
    }

    match first_match(
        &candidates,
        &[],
        &task.permissions.commands.approval_required,
        &task.permissions.commands.allow,
    )? {
        MatchDecision::Deny => deny("command matched deny rule"),
        MatchDecision::Approval => {
            require_approval("command", "command matched approval rule", RiskLevel::High)
        }
        MatchDecision::Allow => allow("command matched allow rule"),
        MatchDecision::NoMatch => deny("command did not match policy"),
    }
}

#[derive(Debug, PartialEq, Eq)]
enum MatchDecision {
    Deny,
    Approval,
    Allow,
    NoMatch,
}

fn first_match(
    candidates: &[String],
    deny_patterns: &[String],
    approval_patterns: &[String],
    allow_patterns: &[String],
) -> taskfence_core::Result<MatchDecision> {
    if matches_any_candidate(candidates, deny_patterns)? {
        return Ok(MatchDecision::Deny);
    }
    if matches_any_candidate(candidates, approval_patterns)? {
        return Ok(MatchDecision::Approval);
    }
    if matches_any_candidate(candidates, allow_patterns)? {
        return Ok(MatchDecision::Allow);
    }
    Ok(MatchDecision::NoMatch)
}

fn matches_any_candidate(
    candidates: &[String],
    patterns: &[String],
) -> taskfence_core::Result<bool> {
    for candidate in candidates {
        if matches_any(candidate, patterns)? {
            return Ok(true);
        }
    }
    Ok(false)
}

fn matches_any(value: &str, patterns: &[String]) -> taskfence_core::Result<bool> {
    for pattern in patterns {
        if value == pattern || value.starts_with(&format!("{pattern} ")) {
            return Ok(true);
        }
        let glob = glob_matcher(pattern)?;
        if glob.is_match(value) {
            return Ok(true);
        }
    }
    Ok(false)
}

fn command_match_candidates(command: &CommandAction) -> Vec<String> {
    let mut candidates = Vec::new();
    push_unique(&mut candidates, command.raw.clone());
    push_unique(&mut candidates, command.executable.clone());
    if !command.args.is_empty() {
        push_unique(
            &mut candidates,
            std::iter::once(command.executable.clone())
                .chain(command.args.iter().cloned())
                .collect::<Vec<_>>()
                .join(" "),
        );
    }
    candidates
}

fn push_unique(values: &mut Vec<String>, value: String) {
    if !value.trim().is_empty() && !values.iter().any(|existing| existing == &value) {
        values.push(value);
    }
}

fn glob_matcher(pattern: &str) -> taskfence_core::Result<GlobMatcher> {
    Glob::new(pattern)
        .map(|glob| glob.compile_matcher())
        .map_err(|err| TaskFenceError::Policy(format!("invalid policy pattern {pattern}: {err}")))
}

fn allow(reason: impl Into<String>) -> taskfence_core::Result<ActionDecision> {
    Ok(ActionDecision::Allow {
        rule_id: None,
        reason: reason.into(),
    })
}

fn deny(reason: impl Into<String>) -> taskfence_core::Result<ActionDecision> {
    Ok(ActionDecision::Deny {
        rule_id: None,
        reason: reason.into(),
    })
}

fn require_approval(
    approval_kind: impl Into<String>,
    reason: impl Into<String>,
    risk: RiskLevel,
) -> taskfence_core::Result<ActionDecision> {
    Ok(ActionDecision::RequireApproval {
        approval_kind: approval_kind.into(),
        rule_id: None,
        reason: reason.into(),
        risk,
    })
}

fn schema(name: &str, version: u16) -> VersionedSchemaContract {
    VersionedSchemaContract {
        name: name.into(),
        version,
    }
}

fn pack<const N: usize>(name: &str, templates: [&str; N]) -> PolicyTemplatePack {
    PolicyTemplatePack {
        name: name.into(),
        opt_in: true,
        templates: templates.into_iter().map(str::to_owned).collect(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use camino::Utf8PathBuf;
    use std::collections::BTreeMap;
    use taskfence_core::{
        AgentConfig, AgentKind, ApprovalConfig, AuditConfig, BudgetLimit, BudgetPermissions,
        CommandPermissions, EnvPermissions, LimitConfig, NetworkPermissions, PathPermissions,
        PermissionConfig, SandboxConfig, SandboxKind, SecretConfig, TaskId, ToolAction,
        ToolPermissions,
    };

    fn task() -> ResolvedTask {
        ResolvedTask {
            id: TaskId("t1".into()),
            task_file: "/tmp/task.yaml".into(),
            goal: "test".into(),
            workspace_host_path: "/tmp/repo".into(),
            workspace_container_path: "/workspace".into(),
            agent: AgentConfig {
                kind: AgentKind::Generic,
                command: "codex".into(),
                args: Vec::new(),
            },
            sandbox: SandboxConfig {
                kind: SandboxKind::Docker,
                image: Some("taskfence/runner:latest".into()),
                ssh: None,
                limits: LimitConfig::default(),
            },
            permissions: PermissionConfig {
                paths: PathPermissions {
                    read: vec![Utf8PathBuf::from("/tmp/repo")],
                    write: vec![Utf8PathBuf::from("/tmp/repo/src")],
                },
                commands: CommandPermissions {
                    allow: vec!["npm test".into()],
                    approval_required: vec!["git push".into()],
                    deny: vec!["sudo *".into()],
                },
                network: NetworkPermissions::default(),
                env: EnvPermissions {
                    allow: vec!["CI".into()],
                },
                tools: Default::default(),
                budget: Default::default(),
            },
            secrets: SecretConfig::default(),
            approval: ApprovalConfig::default(),
            gateway: Default::default(),
            audit: AuditConfig::default(),
        }
    }

    fn tool_action(operation: &str) -> Action {
        Action::ToolCall(ToolAction {
            protocol: "mcp".into(),
            tool: "github".into(),
            operation: operation.into(),
            parameters: BTreeMap::new(),
        })
    }

    #[test]
    fn deny_beats_approval_and_allow() {
        let decision = BuiltInPolicyEngine
            .evaluate(
                &task(),
                &Action::Command(CommandAction::parse("sudo git push")),
            )
            .unwrap();
        assert!(matches!(decision, ActionDecision::Deny { .. }));
    }

    #[test]
    fn approval_beats_allow() {
        let decision = BuiltInPolicyEngine
            .evaluate(
                &task(),
                &Action::Command(CommandAction::parse("git push origin main")),
            )
            .unwrap();
        assert!(matches!(decision, ActionDecision::RequireApproval { .. }));
    }

    #[test]
    fn unknown_command_denied() {
        let decision = BuiltInPolicyEngine
            .evaluate(
                &task(),
                &Action::Command(CommandAction::parse("rm -rf target")),
            )
            .unwrap();
        assert!(matches!(decision, ActionDecision::Deny { .. }));
    }

    #[test]
    fn executable_only_allow_matches_command_with_args() {
        let mut task = task();
        task.permissions.commands.allow = vec!["npm".into()];

        let decision = BuiltInPolicyEngine
            .evaluate(
                &task,
                &Action::Command(CommandAction {
                    executable: "npm".into(),
                    args: vec!["test".into(), "--".into(), "--runInBand".into()],
                    raw: "npm test -- --runInBand".into(),
                    shell_wrapped: false,
                }),
            )
            .unwrap();

        assert!(matches!(decision, ActionDecision::Allow { .. }));
    }

    #[test]
    fn executable_deny_beats_raw_allow() {
        let mut task = task();
        task.permissions.commands.allow = vec!["npm test".into()];
        task.permissions.commands.deny = vec!["npm".into()];

        let decision = BuiltInPolicyEngine
            .evaluate(&task, &Action::Command(CommandAction::parse("npm test")))
            .unwrap();

        assert!(matches!(decision, ActionDecision::Deny { .. }));
    }

    #[test]
    fn shell_wrapper_requires_approval_even_when_raw_command_allowed() {
        let mut task = task();
        task.permissions.commands.allow = vec!["sh".into(), "sh -c npm test".into()];
        task.permissions.commands.approval_required.clear();
        task.permissions.commands.deny.clear();

        let decision = BuiltInPolicyEngine
            .evaluate(
                &task,
                &Action::Command(CommandAction::parse("sh -c npm test")),
            )
            .unwrap();

        assert!(matches!(decision, ActionDecision::RequireApproval { .. }));
    }

    #[test]
    fn tool_allow_matches_normalized_key() {
        let mut task = task();
        task.permissions.tools = ToolPermissions {
            allow: vec!["github.read_issue".into()],
            approval_required: Vec::new(),
            deny: Vec::new(),
        };

        let decision = BuiltInPolicyEngine
            .evaluate(&task, &tool_action("read_issue"))
            .unwrap();

        assert!(matches!(decision, ActionDecision::Allow { .. }));
    }

    #[test]
    fn tool_approval_beats_allow() {
        let mut task = task();
        task.permissions.tools = ToolPermissions {
            allow: vec!["github.create_pr".into()],
            approval_required: vec!["github.create_pr".into()],
            deny: Vec::new(),
        };

        let decision = BuiltInPolicyEngine
            .evaluate(&task, &tool_action("create_pr"))
            .unwrap();

        assert!(matches!(
            decision,
            ActionDecision::RequireApproval {
                approval_kind,
                risk: RiskLevel::Medium,
                ..
            } if approval_kind == "tool_call"
        ));
    }

    #[test]
    fn tool_deny_beats_approval_and_allow() {
        let mut task = task();
        task.permissions.tools = ToolPermissions {
            allow: vec!["github.delete_repo".into()],
            approval_required: vec!["github.delete_repo".into()],
            deny: vec!["github.delete_repo".into()],
        };

        let decision = BuiltInPolicyEngine
            .evaluate(&task, &tool_action("delete_repo"))
            .unwrap();

        assert!(matches!(decision, ActionDecision::Deny { .. }));
    }

    #[test]
    fn unmatched_tool_call_is_denied_by_default() {
        let decision = BuiltInPolicyEngine
            .evaluate(&task(), &tool_action("delete_repo"))
            .unwrap();

        assert!(matches!(decision, ActionDecision::Deny { .. }));
    }

    #[test]
    fn policy_language_contract_versions_schemas_and_keeps_packs_opt_in() {
        let contract = PolicyLanguageContract::current();

        assert_eq!(contract.strategy, PolicyLanguageStrategy::BuiltInEvaluator);
        for schema_name in [
            "task_file",
            "audit_event",
            "connector_policy_template",
            "runner_capability_contract",
        ] {
            let schema = contract
                .schemas
                .iter()
                .find(|schema| schema.name == schema_name)
                .unwrap();
            assert_eq!(schema.version, 1);
        }
        assert!(contract
            .connector_template_packs
            .iter()
            .all(|pack| pack.opt_in));
        assert!(contract
            .migration_checks
            .iter()
            .any(|check| check.contains("task files")));
        assert!(contract
            .migration_checks
            .iter()
            .any(|check| check.contains("reports")));
        assert!(contract
            .unsupported_strategies
            .contains(&PolicyLanguageStrategy::OpaContractOnly));
        assert!(contract
            .unsupported_strategies
            .contains(&PolicyLanguageStrategy::CedarContractOnly));
        assert!(contract
            .unsupported_strategies
            .contains(&PolicyLanguageStrategy::CustomPluginContractOnly));
    }

    #[test]
    fn budget_action_is_denied_without_configured_limit() {
        let decision = BuiltInPolicyEngine
            .evaluate(
                &task(),
                &Action::Budget {
                    kind: "tokens".into(),
                    amount: 100,
                },
            )
            .unwrap();

        assert!(matches!(
            decision,
            ActionDecision::Deny { reason, .. } if reason == "budget kind is not allowed"
        ));
    }

    #[test]
    fn budget_action_is_allowed_within_configured_limit() {
        let mut task = task();
        task.permissions.budget = BudgetPermissions {
            allow: vec![BudgetLimit {
                kind: "tokens".into(),
                max_amount: 1000,
            }],
        };

        let decision = BuiltInPolicyEngine
            .evaluate(
                &task,
                &Action::Budget {
                    kind: " Tokens ".into(),
                    amount: 1000,
                },
            )
            .unwrap();

        assert!(matches!(decision, ActionDecision::Allow { .. }));
    }

    #[test]
    fn budget_action_over_configured_limit_is_denied() {
        let mut task = task();
        task.permissions.budget = BudgetPermissions {
            allow: vec![BudgetLimit {
                kind: "usd_cents".into(),
                max_amount: 250,
            }],
        };

        let decision = BuiltInPolicyEngine
            .evaluate(
                &task,
                &Action::Budget {
                    kind: "usd_cents".into(),
                    amount: 251,
                },
            )
            .unwrap();

        assert!(matches!(
            decision,
            ActionDecision::Deny { reason, .. }
                if reason == "budget amount exceeds configured limit"
        ));
    }
}
