use std::collections::{BTreeMap, BTreeSet};

use taskfence_core::{
    Action, ActionDecision, ApprovalDecision, ApprovalEngine, ApprovalRecord, AuditEvent,
    AuditLogger, PolicyEngine, RedactedValue, ResolvedTask, TaskFenceError, ToolAction,
};
use time::OffsetDateTime;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SecretReference {
    pub name: String,
    pub scope: String,
    pub handle: String,
}

impl SecretReference {
    pub fn as_redacted_value(&self) -> RedactedValue {
        RedactedValue::Redacted {
            reason: format!("gateway secret reference for {}", self.name),
        }
    }
}

pub trait SecretBroker {
    fn issue_reference(
        &self,
        task: &ResolvedTask,
        name: &str,
        scope: &str,
    ) -> taskfence_core::Result<SecretReference>;
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GatewayMediation {
    pub action: ToolAction,
    pub decision: ActionDecision,
    pub approval: Option<ApprovalRecord>,
}

pub struct GatewayMediator<'a> {
    policy: &'a dyn PolicyEngine,
    audit: &'a dyn AuditLogger,
    approval: Option<&'a dyn ApprovalEngine>,
    supported_protocols: BTreeSet<String>,
}

impl<'a> GatewayMediator<'a> {
    pub fn new(policy: &'a dyn PolicyEngine, audit: &'a dyn AuditLogger) -> Self {
        Self {
            policy,
            audit,
            approval: None,
            supported_protocols: BTreeSet::from(["mcp".into()]),
        }
    }

    pub fn with_approval(mut self, approval: &'a dyn ApprovalEngine) -> Self {
        self.approval = Some(approval);
        self
    }

    pub fn with_supported_protocols<I, S>(mut self, protocols: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        self.supported_protocols = protocols
            .into_iter()
            .map(|protocol| protocol.into().trim().to_ascii_lowercase())
            .filter(|protocol| !protocol.is_empty())
            .collect();
        self
    }

    pub fn mediate_tool_action(
        &self,
        task: &ResolvedTask,
        action: ToolAction,
    ) -> taskfence_core::Result<GatewayMediation> {
        let action = normalize_tool_action(action)?;
        if !self.supported_protocols.contains(&action.protocol) {
            let message = format!("gateway protocol '{}' is not supported", action.protocol);
            self.audit.record(AuditEvent::Error {
                task_id: task.id.clone(),
                at: OffsetDateTime::now_utc(),
                message: message.clone(),
            })?;
            return Err(TaskFenceError::Unsupported(message));
        }

        let wrapped = Action::ToolCall(action.clone());
        let decision = self.policy.evaluate(task, &wrapped)?;
        self.audit.record(AuditEvent::PolicyDecision {
            task_id: task.id.clone(),
            at: OffsetDateTime::now_utc(),
            action: wrapped.clone(),
            decision: decision.clone(),
        })?;

        let approval = match decision {
            ActionDecision::Allow { .. } | ActionDecision::Deny { .. } => None,
            ActionDecision::RequireApproval { .. } => match self.approval {
                Some(_) => Some(self.request_tool_approval(task, wrapped, decision.clone())?),
                // Policy-only mediation is still useful for evidence and compatibility.
                None => None,
            },
        };

        Ok(GatewayMediation {
            action,
            decision,
            approval,
        })
    }

    fn request_tool_approval(
        &self,
        task: &ResolvedTask,
        action: Action,
        decision: ActionDecision,
    ) -> taskfence_core::Result<ApprovalRecord> {
        let approval = self.approval.ok_or_else(|| {
            TaskFenceError::Approval("gateway approval engine is not configured".into())
        })?;
        let requested = approval.request(task, action, decision)?;
        self.audit.record(AuditEvent::ApprovalRequested {
            record: requested.clone(),
        })?;
        let resolved = approval.wait(&requested.id)?;
        self.audit.record(AuditEvent::ApprovalResolved {
            record: resolved.clone(),
        })?;

        match resolved.decision {
            Some(ApprovalDecision::Approved) => Ok(resolved),
            Some(ApprovalDecision::Denied) | Some(ApprovalDecision::TimedOut) | None => Err(
                TaskFenceError::Approval("gateway tool approval denied or timed out".into()),
            ),
        }
    }
}

pub fn normalize_tool_action(action: ToolAction) -> taskfence_core::Result<ToolAction> {
    let protocol = normalize_required_segment("protocol", action.protocol)?;
    let tool = normalize_required_segment("tool", action.tool)?;
    let operation = normalize_required_segment("operation", action.operation)?;
    let parameters = normalize_parameters(action.parameters)?;

    Ok(ToolAction {
        protocol,
        tool,
        operation,
        parameters,
    })
}

pub fn gateway_secret_reference(
    task: &ResolvedTask,
    broker: &dyn SecretBroker,
    name: impl Into<String>,
    scope: impl Into<String>,
) -> taskfence_core::Result<SecretReference> {
    let name = normalize_required_segment("secret name", name.into())?;
    let scope = normalize_required_segment("secret scope", scope.into())?;
    ensure_secret_grant(task, &name, &scope)?;
    broker.issue_reference(task, &name, &scope)
}

pub fn attach_secret_reference(
    action: ToolAction,
    parameter_name: impl Into<String>,
    reference: &SecretReference,
) -> taskfence_core::Result<ToolAction> {
    let mut action = normalize_tool_action(action)?;
    let parameter_name = parameter_name.into().trim().to_owned();
    if parameter_name.is_empty() {
        return Err(TaskFenceError::Gateway(
            "secret reference parameter name must not be empty".into(),
        ));
    }
    action
        .parameters
        .insert(parameter_name, reference.as_redacted_value());
    Ok(action)
}

fn ensure_secret_grant(task: &ResolvedTask, name: &str, scope: &str) -> taskfence_core::Result<()> {
    if task.secrets.expose_to_agent {
        return Err(TaskFenceError::Gateway(
            "gateway secret references require secrets to stay out of the agent".into(),
        ));
    }

    if task
        .secrets
        .available_to_gateway
        .iter()
        .any(|grant| grant.name == name && grant.use_for.iter().any(|allowed| allowed == scope))
    {
        Ok(())
    } else {
        Err(TaskFenceError::Gateway(format!(
            "secret {name} is not available to gateway scope {scope}"
        )))
    }
}

fn normalize_required_segment(name: &str, value: String) -> taskfence_core::Result<String> {
    let normalized = value.trim().to_ascii_lowercase();
    if normalized.is_empty() {
        return Err(TaskFenceError::Gateway(format!(
            "tool action {name} must not be empty"
        )));
    }
    Ok(normalized)
}

fn normalize_parameters(
    parameters: BTreeMap<String, RedactedValue>,
) -> taskfence_core::Result<BTreeMap<String, RedactedValue>> {
    let mut normalized = BTreeMap::new();
    for (key, value) in parameters {
        let key = key.trim().to_owned();
        if key.is_empty() {
            return Err(TaskFenceError::Gateway(
                "tool action parameter names must not be empty".into(),
            ));
        }
        normalized.insert(key, value);
    }
    Ok(normalized)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;
    use taskfence_core::{
        AgentConfig, AgentKind, ApprovalConfig, ApprovalId, AuditConfig, LimitConfig,
        PermissionConfig, SandboxConfig, SandboxKind, SecretConfig, SecretGrant, TaskId,
        ToolPermissions,
    };
    use taskfence_policy::BuiltInPolicyEngine;

    #[derive(Debug)]
    struct StaticPolicy {
        decision: ActionDecision,
        seen_actions: Mutex<Vec<Action>>,
    }

    impl StaticPolicy {
        fn new(decision: ActionDecision) -> Self {
            Self {
                decision,
                seen_actions: Mutex::new(Vec::new()),
            }
        }
    }

    impl PolicyEngine for StaticPolicy {
        fn evaluate(
            &self,
            _task: &ResolvedTask,
            action: &Action,
        ) -> taskfence_core::Result<ActionDecision> {
            self.seen_actions.lock().unwrap().push(action.clone());
            Ok(self.decision.clone())
        }
    }

    #[derive(Debug, Default)]
    struct RecordingAudit {
        events: Mutex<Vec<AuditEvent>>,
    }

    impl AuditLogger for RecordingAudit {
        fn record(&self, event: AuditEvent) -> taskfence_core::Result<()> {
            self.events.lock().unwrap().push(event);
            Ok(())
        }
    }

    #[derive(Debug)]
    struct StaticApproval {
        decision: ApprovalDecision,
        requested: Mutex<Vec<ApprovalRecord>>,
    }

    impl StaticApproval {
        fn new(decision: ApprovalDecision) -> Self {
            Self {
                decision,
                requested: Mutex::new(Vec::new()),
            }
        }
    }

    impl ApprovalEngine for StaticApproval {
        fn request(
            &self,
            task: &ResolvedTask,
            action: Action,
            policy_decision: ActionDecision,
        ) -> taskfence_core::Result<ApprovalRecord> {
            let record = ApprovalRecord {
                id: ApprovalId("approval-tool-1".into()),
                task_id: task.id.clone(),
                actor: "gateway-test".into(),
                source: Some("gateway".into()),
                requested_at: OffsetDateTime::now_utc(),
                resolved_at: None,
                action,
                policy_decision,
                decision: None,
            };
            self.requested.lock().unwrap().push(record.clone());
            Ok(record)
        }

        fn wait(&self, approval_id: &ApprovalId) -> taskfence_core::Result<ApprovalRecord> {
            let mut record = self.requested.lock().unwrap()[0].clone();
            record.id = approval_id.clone();
            record.resolved_at = Some(OffsetDateTime::now_utc());
            record.decision = Some(self.decision.clone());
            Ok(record)
        }
    }

    #[derive(Debug, Default)]
    struct StaticSecretBroker {
        issued: Mutex<Vec<(String, String)>>,
    }

    impl SecretBroker for StaticSecretBroker {
        fn issue_reference(
            &self,
            task: &ResolvedTask,
            name: &str,
            scope: &str,
        ) -> taskfence_core::Result<SecretReference> {
            self.issued
                .lock()
                .unwrap()
                .push((name.into(), scope.into()));
            Ok(SecretReference {
                name: name.into(),
                scope: scope.into(),
                handle: format!("taskfence://{}/{name}/{scope}", task.id.0),
            })
        }
    }

    fn allow() -> ActionDecision {
        ActionDecision::Allow {
            rule_id: Some("tools.allow".into()),
            reason: "allowed by test".into(),
        }
    }

    fn deny() -> ActionDecision {
        ActionDecision::Deny {
            rule_id: Some("tools.deny".into()),
            reason: "denied by test".into(),
        }
    }

    fn task() -> ResolvedTask {
        ResolvedTask {
            id: TaskId("task-1".into()),
            task_file: "/tmp/task.yaml".into(),
            goal: "test gateway".into(),
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
                limits: LimitConfig::default(),
            },
            permissions: PermissionConfig::default(),
            secrets: SecretConfig::default(),
            approval: ApprovalConfig::default(),
            audit: AuditConfig::default(),
        }
    }

    fn tool_action(protocol: &str) -> ToolAction {
        ToolAction {
            protocol: protocol.into(),
            tool: " GitHub ".into(),
            operation: " CREATE_PR ".into(),
            parameters: BTreeMap::from([(
                " title ".into(),
                RedactedValue::Plain("ship bounded slice".into()),
            )]),
        }
    }

    fn task_with_gateway_secret() -> ResolvedTask {
        let mut task = task();
        task.secrets.available_to_gateway = vec![SecretGrant {
            name: "github_token".into(),
            use_for: vec!["github.create_pr".into()],
        }];
        task
    }

    #[test]
    fn normalizes_tool_action_before_policy_and_audit() {
        let policy = StaticPolicy::new(allow());
        let audit = RecordingAudit::default();
        let mediator = GatewayMediator::new(&policy, &audit);

        let result = mediator
            .mediate_tool_action(&task(), tool_action(" MCP "))
            .unwrap();

        assert_eq!(result.action.protocol, "mcp");
        assert_eq!(result.action.tool, "github");
        assert_eq!(result.action.operation, "create_pr");
        assert!(result.action.parameters.contains_key("title"));
        assert!(result.approval.is_none());

        let seen = policy.seen_actions.lock().unwrap();
        assert_eq!(seen.len(), 1);
        assert!(matches!(
            &seen[0],
            Action::ToolCall(action)
                if action.tool == "github" && action.operation == "create_pr"
        ));

        let events = audit.events.lock().unwrap();
        assert_eq!(events.len(), 1);
        assert!(matches!(
            events.first(),
            Some(AuditEvent::PolicyDecision { .. })
        ));
    }

    #[test]
    fn returns_policy_decision_without_executing_gateway_action() {
        let policy = StaticPolicy::new(deny());
        let audit = RecordingAudit::default();
        let mediator = GatewayMediator::new(&policy, &audit);

        let result = mediator
            .mediate_tool_action(&task(), tool_action("mcp"))
            .unwrap();

        assert!(matches!(result.decision, ActionDecision::Deny { .. }));
        assert!(result.approval.is_none());
    }

    #[test]
    fn records_configured_tool_policy_decision_without_execution() {
        let policy = BuiltInPolicyEngine;
        let audit = RecordingAudit::default();
        let mediator = GatewayMediator::new(&policy, &audit);
        let mut task = task();
        task.permissions.tools = ToolPermissions {
            allow: vec!["github.read_issue".into()],
            approval_required: vec!["github.create_pr".into()],
            deny: vec!["github.delete_repo".into()],
        };

        let result = mediator
            .mediate_tool_action(&task, tool_action("mcp"))
            .unwrap();

        assert!(matches!(
            result.decision,
            ActionDecision::RequireApproval {
                approval_kind,
                ..
            } if approval_kind == "tool_call"
        ));
        assert!(result.approval.is_none());
        let events = audit.events.lock().unwrap();
        assert_eq!(events.len(), 1);
        assert!(matches!(
            events.first(),
            Some(AuditEvent::PolicyDecision {
                action: Action::ToolCall(action),
                decision: ActionDecision::RequireApproval { .. },
                ..
            }) if action.tool == "github" && action.operation == "create_pr"
        ));
    }

    #[test]
    fn approved_tool_call_records_approval_events_without_execution() {
        let policy = BuiltInPolicyEngine;
        let approval = StaticApproval::new(ApprovalDecision::Approved);
        let audit = RecordingAudit::default();
        let mediator = GatewayMediator::new(&policy, &audit).with_approval(&approval);
        let mut task = task();
        task.permissions.tools.approval_required = vec!["github.create_pr".into()];

        let result = mediator
            .mediate_tool_action(&task, tool_action("mcp"))
            .unwrap();

        assert!(matches!(
            result.decision,
            ActionDecision::RequireApproval { .. }
        ));
        assert!(matches!(
            result.approval,
            Some(ApprovalRecord {
                decision: Some(ApprovalDecision::Approved),
                ..
            })
        ));
        let events = audit.events.lock().unwrap();
        assert_eq!(events.len(), 3);
        assert!(matches!(
            events.as_slice(),
            [
                AuditEvent::PolicyDecision {
                    action: Action::ToolCall(_),
                    decision: ActionDecision::RequireApproval { .. },
                    ..
                },
                AuditEvent::ApprovalRequested { record: requested },
                AuditEvent::ApprovalResolved { record: resolved },
            ] if requested.decision.is_none()
                && resolved.decision == Some(ApprovalDecision::Approved)
        ));
    }

    #[test]
    fn denied_tool_approval_fails_closed_after_audit_resolution() {
        let policy = BuiltInPolicyEngine;
        let approval = StaticApproval::new(ApprovalDecision::Denied);
        let audit = RecordingAudit::default();
        let mediator = GatewayMediator::new(&policy, &audit).with_approval(&approval);
        let mut task = task();
        task.permissions.tools.approval_required = vec!["github.create_pr".into()];

        let err = mediator
            .mediate_tool_action(&task, tool_action("mcp"))
            .unwrap_err();

        assert!(matches!(
            err,
            TaskFenceError::Approval(message) if message.contains("denied or timed out")
        ));
        let events = audit.events.lock().unwrap();
        assert_eq!(events.len(), 3);
        assert!(matches!(
            events.last(),
            Some(AuditEvent::ApprovalResolved { record })
                if record.decision == Some(ApprovalDecision::Denied)
        ));
    }

    #[test]
    fn timed_out_tool_approval_fails_closed_after_audit_resolution() {
        let policy = BuiltInPolicyEngine;
        let approval = StaticApproval::new(ApprovalDecision::TimedOut);
        let audit = RecordingAudit::default();
        let mediator = GatewayMediator::new(&policy, &audit).with_approval(&approval);
        let mut task = task();
        task.permissions.tools.approval_required = vec!["github.create_pr".into()];

        let err = mediator
            .mediate_tool_action(&task, tool_action("mcp"))
            .unwrap_err();

        assert!(matches!(err, TaskFenceError::Approval(_)));
        let events = audit.events.lock().unwrap();
        assert!(matches!(
            events.last(),
            Some(AuditEvent::ApprovalResolved { record })
                if record.decision == Some(ApprovalDecision::TimedOut)
        ));
    }

    #[test]
    fn issues_redacted_gateway_secret_reference_for_allowed_scope() {
        let task = task_with_gateway_secret();
        let broker = StaticSecretBroker::default();

        let reference =
            gateway_secret_reference(&task, &broker, " GitHub_Token ", " GitHub.Create_Pr ")
                .unwrap();

        assert_eq!(reference.name, "github_token");
        assert_eq!(reference.scope, "github.create_pr");
        assert_eq!(
            broker.issued.lock().unwrap().as_slice(),
            &[("github_token".into(), "github.create_pr".into())]
        );
        assert!(matches!(
            reference.as_redacted_value(),
            RedactedValue::Redacted { reason } if reason.contains("github_token")
        ));
    }

    #[test]
    fn gateway_secret_reference_denies_unavailable_secret_or_scope() {
        let task = task_with_gateway_secret();
        let broker = StaticSecretBroker::default();

        let missing = gateway_secret_reference(&task, &broker, "slack_token", "github.create_pr")
            .unwrap_err();
        let wrong_scope =
            gateway_secret_reference(&task, &broker, "github_token", "github.delete_repo")
                .unwrap_err();

        assert!(
            matches!(missing, TaskFenceError::Gateway(message) if message.contains("slack_token"))
        );
        assert!(
            matches!(wrong_scope, TaskFenceError::Gateway(message) if message.contains("github.delete_repo"))
        );
        assert!(broker.issued.lock().unwrap().is_empty());
    }

    #[test]
    fn gateway_secret_reference_requires_secrets_to_stay_out_of_agent() {
        let mut task = task_with_gateway_secret();
        task.secrets.expose_to_agent = true;
        let broker = StaticSecretBroker::default();

        let err = gateway_secret_reference(&task, &broker, "github_token", "github.create_pr")
            .unwrap_err();

        assert!(matches!(
            err,
            TaskFenceError::Gateway(message) if message.contains("stay out of the agent")
        ));
        assert!(broker.issued.lock().unwrap().is_empty());
    }

    #[test]
    fn attaches_secret_reference_without_raw_secret_parameter_value() {
        let task = task_with_gateway_secret();
        let broker = StaticSecretBroker::default();
        let reference =
            gateway_secret_reference(&task, &broker, "github_token", "github.create_pr").unwrap();

        let action =
            attach_secret_reference(tool_action("mcp"), " authorization ", &reference).unwrap();

        assert!(matches!(
            action.parameters.get("authorization"),
            Some(RedactedValue::Redacted { reason })
                if reason == "gateway secret reference for github_token"
        ));
        assert!(!format!("{:?}", action.parameters).contains(&reference.handle));
        assert!(!format!("{:?}", action.parameters).contains("raw"));
    }

    #[test]
    fn unsupported_protocol_returns_explicit_error_and_audit_event() {
        let policy = StaticPolicy::new(allow());
        let audit = RecordingAudit::default();
        let mediator = GatewayMediator::new(&policy, &audit);

        let err = mediator
            .mediate_tool_action(&task(), tool_action("http"))
            .unwrap_err();

        assert!(matches!(
            err,
            TaskFenceError::Unsupported(message) if message.contains("http")
        ));
        assert!(policy.seen_actions.lock().unwrap().is_empty());
        let events = audit.events.lock().unwrap();
        assert_eq!(events.len(), 1);
        assert!(matches!(events.first(), Some(AuditEvent::Error { .. })));
    }

    #[test]
    fn empty_tool_segment_is_gateway_error() {
        let err = normalize_tool_action(ToolAction {
            protocol: "mcp".into(),
            tool: " ".into(),
            operation: "read_issue".into(),
            parameters: BTreeMap::new(),
        })
        .unwrap_err();

        assert!(matches!(err, TaskFenceError::Gateway(message) if message.contains("tool")));
    }
}
