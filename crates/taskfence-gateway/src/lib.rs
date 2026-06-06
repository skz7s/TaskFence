use std::collections::{BTreeMap, BTreeSet};

use taskfence_core::{
    Action, ActionDecision, AuditEvent, AuditLogger, PolicyEngine, RedactedValue, ResolvedTask,
    TaskFenceError, ToolAction,
};
use time::OffsetDateTime;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GatewayMediation {
    pub action: ToolAction,
    pub decision: ActionDecision,
}

pub struct GatewayMediator<'a> {
    policy: &'a dyn PolicyEngine,
    audit: &'a dyn AuditLogger,
    supported_protocols: BTreeSet<String>,
}

impl<'a> GatewayMediator<'a> {
    pub fn new(policy: &'a dyn PolicyEngine, audit: &'a dyn AuditLogger) -> Self {
        Self {
            policy,
            audit,
            supported_protocols: BTreeSet::from(["mcp".into()]),
        }
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
            action: wrapped,
            decision: decision.clone(),
        })?;

        Ok(GatewayMediation { action, decision })
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
        AgentConfig, AgentKind, ApprovalConfig, AuditConfig, LimitConfig, PermissionConfig,
        SandboxConfig, SandboxKind, SecretConfig, TaskId,
    };

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
