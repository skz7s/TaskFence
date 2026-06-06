use std::collections::BTreeMap;
use std::io::{self, Write};
use std::sync::Arc;
use std::sync::Mutex;

use taskfence_core::{
    Action, ActionDecision, ApprovalDecision, ApprovalEngine, ApprovalId, ApprovalRecord,
    ResolvedTask, RiskLevel, TaskFenceError,
};
use time::OffsetDateTime;

const MAX_INTERACTIVE_ATTEMPTS: usize = 3;

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub enum LocalApprovalMode {
    Preconfigured(ApprovalDecision),
    TimedOut,
    Interactive,
    #[default]
    FailClosed,
}

pub struct LocalApprovalEngine {
    actor: String,
    source: Option<String>,
    mode: LocalApprovalMode,
    prompt: Option<Arc<dyn LocalApprovalPrompt>>,
    records: Mutex<BTreeMap<ApprovalId, ApprovalRecord>>,
}

impl std::fmt::Debug for LocalApprovalEngine {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("LocalApprovalEngine")
            .field("actor", &self.actor)
            .field("source", &self.source)
            .field("mode", &self.mode)
            .field("has_prompt", &self.prompt.is_some())
            .finish_non_exhaustive()
    }
}

impl LocalApprovalEngine {
    pub fn fail_closed() -> Self {
        Self {
            actor: "local".into(),
            source: None,
            mode: LocalApprovalMode::FailClosed,
            prompt: None,
            records: Mutex::new(BTreeMap::new()),
        }
    }

    pub fn preconfigured(decision: ApprovalDecision) -> Self {
        Self {
            actor: "local".into(),
            source: Some("preconfigured".into()),
            mode: LocalApprovalMode::Preconfigured(decision),
            prompt: None,
            records: Mutex::new(BTreeMap::new()),
        }
    }

    pub fn timed_out() -> Self {
        Self {
            actor: "local".into(),
            source: Some("timeout".into()),
            mode: LocalApprovalMode::TimedOut,
            prompt: None,
            records: Mutex::new(BTreeMap::new()),
        }
    }

    pub fn interactive() -> Self {
        Self::interactive_with_prompt(Arc::new(TerminalApprovalPrompt))
    }

    pub fn interactive_with_prompt(prompt: Arc<dyn LocalApprovalPrompt>) -> Self {
        Self {
            actor: "local".into(),
            source: Some("interactive".into()),
            mode: LocalApprovalMode::Interactive,
            prompt: Some(prompt),
            records: Mutex::new(BTreeMap::new()),
        }
    }

    pub fn with_actor(mut self, actor: impl Into<String>) -> Self {
        self.actor = actor.into();
        self
    }

    pub fn with_source(mut self, source: impl Into<String>) -> Self {
        self.source = Some(source.into());
        self
    }

    fn resolve_decision(
        &self,
        record: &ApprovalRecord,
    ) -> taskfence_core::Result<ApprovalDecision> {
        match &self.mode {
            LocalApprovalMode::Preconfigured(decision) => Ok(decision.clone()),
            LocalApprovalMode::TimedOut => Ok(ApprovalDecision::TimedOut),
            LocalApprovalMode::Interactive => match &self.prompt {
                Some(prompt) => prompt.prompt(record),
                None => Ok(ApprovalDecision::Denied),
            },
            LocalApprovalMode::FailClosed => Ok(ApprovalDecision::Denied),
        }
    }
}

impl Default for LocalApprovalEngine {
    fn default() -> Self {
        Self::fail_closed()
    }
}

pub trait LocalApprovalPrompt: Send + Sync {
    fn prompt(&self, record: &ApprovalRecord) -> taskfence_core::Result<ApprovalDecision>;
}

#[derive(Debug, Default)]
pub struct TerminalApprovalPrompt;

impl LocalApprovalPrompt for TerminalApprovalPrompt {
    fn prompt(&self, record: &ApprovalRecord) -> taskfence_core::Result<ApprovalDecision> {
        let rendered = render_approval_request(record);
        let mut stderr = io::stderr();
        let stdin = io::stdin();

        stderr
            .write_all(rendered.as_bytes())
            .map_err(approval_io_error)?;

        for attempt in 1..=MAX_INTERACTIVE_ATTEMPTS {
            stderr
                .write_all(b"Approve this action? Type 'approve' or 'deny': ")
                .map_err(approval_io_error)?;
            stderr.flush().map_err(approval_io_error)?;

            let mut response = String::new();
            let bytes = stdin.read_line(&mut response).map_err(approval_io_error)?;
            if bytes == 0 {
                stderr
                    .write_all(b"\nNo approval input received; denying by default.\n")
                    .map_err(approval_io_error)?;
                return Ok(ApprovalDecision::Denied);
            }

            if let Some(decision) = parse_approval_response(&response) {
                return Ok(decision);
            }

            if attempt == MAX_INTERACTIVE_ATTEMPTS {
                stderr
                    .write_all(b"Approval response was not explicit; denying by default.\n")
                    .map_err(approval_io_error)?;
            } else {
                stderr
                    .write_all(b"Response must be explicit: approve or deny.\n")
                    .map_err(approval_io_error)?;
            }
        }

        Ok(ApprovalDecision::Denied)
    }
}

impl ApprovalEngine for LocalApprovalEngine {
    fn request(
        &self,
        task: &ResolvedTask,
        action: Action,
        decision: ActionDecision,
    ) -> taskfence_core::Result<ApprovalRecord> {
        let record = ApprovalRecord {
            id: ApprovalId::new(),
            task_id: task.id.clone(),
            actor: self.actor.clone(),
            source: self.source.clone(),
            requested_at: OffsetDateTime::now_utc(),
            resolved_at: None,
            action,
            policy_decision: decision,
            decision: None,
        };

        let mut records = self
            .records
            .lock()
            .map_err(|_| TaskFenceError::Approval("approval record store is poisoned".into()))?;
        records.insert(record.id.clone(), record.clone());
        Ok(record)
    }

    fn wait(&self, approval_id: &ApprovalId) -> taskfence_core::Result<ApprovalRecord> {
        let pending = {
            let records = self.records.lock().map_err(|_| {
                TaskFenceError::Approval("approval record store is poisoned".into())
            })?;
            let record = records.get(approval_id).ok_or_else(|| {
                TaskFenceError::Approval(format!("unknown approval id {}", approval_id.0))
            })?;
            if record.decision.is_some() {
                return Ok(record.clone());
            }
            record.clone()
        };

        let resolved_decision = self.resolve_decision(&pending)?;
        let mut records = self
            .records
            .lock()
            .map_err(|_| TaskFenceError::Approval("approval record store is poisoned".into()))?;
        let record = records.get_mut(approval_id).ok_or_else(|| {
            TaskFenceError::Approval(format!("unknown approval id {}", approval_id.0))
        })?;

        if record.decision.is_none() {
            record.decision = Some(resolved_decision);
            record.resolved_at = Some(OffsetDateTime::now_utc());
        }

        Ok(record.clone())
    }
}

pub fn render_approval_request(record: &ApprovalRecord) -> String {
    let mut rendered = String::new();
    rendered.push_str("\nTaskFence approval required\n");
    rendered.push_str(&format!("  task: {}\n", record.task_id.0));
    rendered.push_str(&format!("  approval: {}\n", record.id.0));
    rendered.push_str(&format!("  action: {}\n", action_summary(&record.action)));
    rendered.push_str(&format!(
        "  policy: {}\n",
        decision_summary(&record.policy_decision)
    ));
    rendered.push_str(&format!("  actor: {}\n", record.actor));
    if let Some(source) = &record.source {
        rendered.push_str(&format!("  source: {source}\n"));
    }
    rendered.push('\n');
    rendered
}

pub fn parse_approval_response(input: &str) -> Option<ApprovalDecision> {
    match input.trim().to_ascii_lowercase().as_str() {
        "approve" | "approved" | "yes" | "y" => Some(ApprovalDecision::Approved),
        "deny" | "denied" | "no" | "n" => Some(ApprovalDecision::Denied),
        _ => None,
    }
}

fn action_summary(action: &Action) -> String {
    match action {
        Action::FileRead { path } => format!("file read {path}"),
        Action::FileWrite { path } => format!("file write {path}"),
        Action::Command(command) => {
            let shell_suffix = if command.shell_wrapped {
                " (shell wrapped)"
            } else {
                ""
            };
            format!("command `{}`{shell_suffix}", command.raw)
        }
        Action::Network { host, port } => match port {
            Some(port) => format!("network {host}:{port}"),
            None => format!("network {host}"),
        },
        Action::EnvExpose { name } => format!("environment variable {name}"),
        Action::SecretAccess { name, scope } => {
            format!("secret access {name} for scope {scope}")
        }
        Action::ToolCall(tool) => format!(
            "tool call {} {}.{} with {} parameter(s)",
            tool.protocol,
            tool.tool,
            tool.operation,
            tool.parameters.len()
        ),
        Action::Budget { kind, amount } => format!("budget {kind} amount {amount}"),
    }
}

fn decision_summary(decision: &ActionDecision) -> String {
    match decision {
        ActionDecision::Allow { rule_id, reason } => {
            format!("allow{}: {reason}", rule_suffix(rule_id))
        }
        ActionDecision::RequireApproval {
            approval_kind,
            rule_id,
            reason,
            risk,
        } => format!(
            "requires {approval_kind} approval{}: {reason}; risk {}",
            rule_suffix(rule_id),
            risk_label(risk)
        ),
        ActionDecision::Deny { rule_id, reason } => {
            format!("deny{}: {reason}", rule_suffix(rule_id))
        }
    }
}

fn rule_suffix(rule_id: &Option<String>) -> String {
    rule_id
        .as_deref()
        .map(|id| format!(" by rule {id}"))
        .unwrap_or_default()
}

fn risk_label(risk: &RiskLevel) -> &'static str {
    match risk {
        RiskLevel::Low => "low",
        RiskLevel::Medium => "medium",
        RiskLevel::High => "high",
        RiskLevel::Critical => "critical",
    }
}

fn approval_io_error(err: std::io::Error) -> TaskFenceError {
    TaskFenceError::Approval(format!("approval prompt IO failed: {err}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use taskfence_core::{
        AgentConfig, AgentKind, ApprovalConfig, AuditConfig, LimitConfig, PermissionConfig,
        RiskLevel, SandboxConfig, SandboxKind, SecretConfig, TaskId,
    };

    fn task() -> ResolvedTask {
        ResolvedTask {
            id: TaskId("task-1".into()),
            task_file: "/tmp/task.yaml".into(),
            goal: "test approvals".into(),
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

    fn approval_decision() -> ActionDecision {
        ActionDecision::RequireApproval {
            approval_kind: "tool_call".into(),
            rule_id: Some("rule-1".into()),
            reason: "needs review".into(),
            risk: RiskLevel::High,
        }
    }

    #[test]
    fn request_stores_unresolved_record() {
        let engine =
            LocalApprovalEngine::preconfigured(ApprovalDecision::Approved).with_actor("alice");
        let requested = engine
            .request(
                &task(),
                Action::Budget {
                    kind: "tokens".into(),
                    amount: 10,
                },
                approval_decision(),
            )
            .unwrap();

        assert_eq!(requested.actor, "alice");
        assert_eq!(requested.decision, None);
        assert!(requested.resolved_at.is_none());
    }

    #[derive(Debug)]
    struct FixedPrompt {
        decision: ApprovalDecision,
    }

    impl LocalApprovalPrompt for FixedPrompt {
        fn prompt(&self, _record: &ApprovalRecord) -> taskfence_core::Result<ApprovalDecision> {
            Ok(self.decision.clone())
        }
    }

    #[test]
    fn interactive_mode_uses_prompt_decision() {
        let engine = LocalApprovalEngine::interactive_with_prompt(Arc::new(FixedPrompt {
            decision: ApprovalDecision::Approved,
        }));
        let requested = engine
            .request(
                &task(),
                Action::Budget {
                    kind: "tokens".into(),
                    amount: 10,
                },
                approval_decision(),
            )
            .unwrap();

        let resolved = engine.wait(&requested.id).unwrap();

        assert_eq!(resolved.decision, Some(ApprovalDecision::Approved));
        assert_eq!(resolved.source.as_deref(), Some("interactive"));
    }

    #[test]
    fn wait_uses_preconfigured_decision_without_blocking() {
        let engine = LocalApprovalEngine::preconfigured(ApprovalDecision::Approved);
        let requested = engine
            .request(
                &task(),
                Action::Budget {
                    kind: "tokens".into(),
                    amount: 10,
                },
                approval_decision(),
            )
            .unwrap();

        let resolved = engine.wait(&requested.id).unwrap();

        assert_eq!(resolved.decision, Some(ApprovalDecision::Approved));
        assert!(resolved.resolved_at.is_some());
    }

    #[test]
    fn fail_closed_denies_without_blocking() {
        let engine = LocalApprovalEngine::fail_closed();
        let requested = engine
            .request(
                &task(),
                Action::Budget {
                    kind: "tokens".into(),
                    amount: 10,
                },
                approval_decision(),
            )
            .unwrap();

        let resolved = engine.wait(&requested.id).unwrap();

        assert_eq!(resolved.decision, Some(ApprovalDecision::Denied));
    }

    #[test]
    fn timeout_mode_resolves_as_timed_out_without_approval() {
        let engine = LocalApprovalEngine::timed_out();
        let requested = engine
            .request(
                &task(),
                Action::Budget {
                    kind: "tokens".into(),
                    amount: 10,
                },
                approval_decision(),
            )
            .unwrap();

        let resolved = engine.wait(&requested.id).unwrap();

        assert_eq!(resolved.decision, Some(ApprovalDecision::TimedOut));
        assert!(resolved.resolved_at.is_some());
    }

    #[test]
    fn unknown_approval_id_returns_approval_error() {
        let err = LocalApprovalEngine::fail_closed()
            .wait(&ApprovalId("missing".into()))
            .unwrap_err();

        assert!(matches!(err, TaskFenceError::Approval(message) if message.contains("missing")));
    }

    #[test]
    fn parses_only_explicit_approval_responses() {
        assert_eq!(
            parse_approval_response("approve\n"),
            Some(ApprovalDecision::Approved)
        );
        assert_eq!(
            parse_approval_response("DENY"),
            Some(ApprovalDecision::Denied)
        );
        assert_eq!(parse_approval_response(""), None);
        assert_eq!(parse_approval_response("maybe"), None);
    }

    #[test]
    fn renders_action_and_policy_without_tool_parameter_values() {
        let mut parameters = BTreeMap::new();
        parameters.insert(
            "token".into(),
            taskfence_core::RedactedValue::Plain("secret-value".into()),
        );
        let task = task();
        let record = ApprovalRecord {
            id: ApprovalId("approval-1".into()),
            task_id: task.id,
            actor: "alice".into(),
            source: Some("test".into()),
            requested_at: OffsetDateTime::now_utc(),
            resolved_at: None,
            action: Action::ToolCall(taskfence_core::ToolAction {
                protocol: "mcp".into(),
                tool: "github".into(),
                operation: "create_pr".into(),
                parameters,
            }),
            policy_decision: approval_decision(),
            decision: None,
        };

        let rendered = render_approval_request(&record);

        assert!(rendered.contains("TaskFence approval required"));
        assert!(rendered.contains("tool call mcp github.create_pr"));
        assert!(rendered.contains("1 parameter"));
        assert!(rendered.contains("needs review"));
        assert!(!rendered.contains("secret-value"));
    }
}
