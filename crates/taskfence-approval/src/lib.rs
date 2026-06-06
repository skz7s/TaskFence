use std::collections::BTreeMap;
use std::sync::Mutex;

use taskfence_core::{
    Action, ActionDecision, ApprovalDecision, ApprovalEngine, ApprovalId, ApprovalRecord,
    ResolvedTask, TaskFenceError,
};
use time::OffsetDateTime;

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub enum LocalApprovalMode {
    Preconfigured(ApprovalDecision),
    TimedOut,
    #[default]
    FailClosed,
}

#[derive(Debug)]
pub struct LocalApprovalEngine {
    actor: String,
    source: Option<String>,
    mode: LocalApprovalMode,
    records: Mutex<BTreeMap<ApprovalId, ApprovalRecord>>,
}

impl LocalApprovalEngine {
    pub fn fail_closed() -> Self {
        Self {
            actor: "local".into(),
            source: None,
            mode: LocalApprovalMode::FailClosed,
            records: Mutex::new(BTreeMap::new()),
        }
    }

    pub fn preconfigured(decision: ApprovalDecision) -> Self {
        Self {
            actor: "local".into(),
            source: Some("preconfigured".into()),
            mode: LocalApprovalMode::Preconfigured(decision),
            records: Mutex::new(BTreeMap::new()),
        }
    }

    pub fn timed_out() -> Self {
        Self {
            actor: "local".into(),
            source: Some("timeout".into()),
            mode: LocalApprovalMode::TimedOut,
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

    fn configured_decision(&self) -> ApprovalDecision {
        match &self.mode {
            LocalApprovalMode::Preconfigured(decision) => decision.clone(),
            LocalApprovalMode::TimedOut => ApprovalDecision::TimedOut,
            LocalApprovalMode::FailClosed => ApprovalDecision::Denied,
        }
    }
}

impl Default for LocalApprovalEngine {
    fn default() -> Self {
        Self::fail_closed()
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
        let resolved_decision = self.configured_decision();
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
}
