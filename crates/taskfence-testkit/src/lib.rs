use camino::Utf8PathBuf;
use std::cell::RefCell;
use std::collections::BTreeMap;
use taskfence_core::{
    Action, ActionDecision, AgentAdapter, AgentConfig, AgentInvocation, AgentKind, ApprovalConfig,
    ApprovalDecision, ApprovalEngine, ApprovalId, ApprovalRecord, ArtifactRefs, ArtifactStore,
    AuditConfig, AuditEvent, AuditLogger, CommandAction, EnvPermissions, ExitStatus, LimitConfig,
    LogStream, MountPlan, NetworkPermissions, PathPermissions, PermissionConfig, PolicyEngine,
    PreparedRun, ReportGenerator, ResolvedTask, Result, RunOutput, RunningTask, SandboxConfig,
    SandboxKind, SecretConfig, StateStore, TaskId, TaskStatus, WorkspaceBaseline,
};
use taskfence_runner::{RunnerCapabilityReport, RunnerKind};
use time::OffsetDateTime;

pub fn sample_task() -> ResolvedTask {
    ResolvedTask {
        id: TaskId("test-task".into()),
        task_file: Utf8PathBuf::from("/tmp/task.yaml"),
        goal: "Run test task".into(),
        workspace_host_path: Utf8PathBuf::from("/tmp/repo"),
        workspace_container_path: Utf8PathBuf::from("/workspace"),
        agent: AgentConfig {
            kind: AgentKind::Generic,
            command: "npm".into(),
            args: vec!["test".into()],
        },
        sandbox: SandboxConfig {
            kind: SandboxKind::Docker,
            image: Some("taskfence/runner:latest".into()),
            limits: LimitConfig::default(),
        },
        permissions: PermissionConfig {
            paths: PathPermissions {
                read: vec![Utf8PathBuf::from("/tmp/repo")],
                write: vec![Utf8PathBuf::from("/tmp/repo/src")],
            },
            commands: Default::default(),
            network: NetworkPermissions::default(),
            env: EnvPermissions::default(),
            tools: Default::default(),
            budget: Default::default(),
        },
        secrets: SecretConfig::default(),
        approval: ApprovalConfig::default(),
        gateway: Default::default(),
        audit: AuditConfig::default(),
    }
}

#[derive(Debug)]
pub struct StaticPolicy {
    pub decision: ActionDecision,
}

impl PolicyEngine for StaticPolicy {
    fn evaluate(&self, _task: &ResolvedTask, _action: &Action) -> Result<ActionDecision> {
        Ok(self.decision.clone())
    }
}

#[derive(Debug)]
pub struct StaticApproval {
    pub decision: ApprovalDecision,
}

impl ApprovalEngine for StaticApproval {
    fn request(
        &self,
        task: &ResolvedTask,
        action: Action,
        policy_decision: ActionDecision,
    ) -> Result<ApprovalRecord> {
        Ok(ApprovalRecord {
            id: ApprovalId::new(),
            task_id: task.id.clone(),
            actor: "test".into(),
            source: Some("testkit".into()),
            requested_at: OffsetDateTime::now_utc(),
            resolved_at: None,
            action,
            policy_decision,
            decision: None,
        })
    }

    fn wait(&self, approval_id: &ApprovalId) -> Result<ApprovalRecord> {
        Ok(ApprovalRecord {
            id: approval_id.clone(),
            task_id: TaskId("test-task".into()),
            actor: "test".into(),
            source: Some("testkit".into()),
            requested_at: OffsetDateTime::now_utc(),
            resolved_at: Some(OffsetDateTime::now_utc()),
            action: Action::Command(CommandAction::parse("npm test")),
            policy_decision: ActionDecision::Allow {
                rule_id: None,
                reason: "test".into(),
            },
            decision: Some(self.decision.clone()),
        })
    }
}

#[derive(Debug, Default)]
pub struct MemoryAudit {
    pub events: RefCell<Vec<AuditEvent>>,
}

impl AuditLogger for MemoryAudit {
    fn record(&self, event: AuditEvent) -> Result<()> {
        self.events.borrow_mut().push(event);
        Ok(())
    }
}

#[derive(Debug)]
pub struct MemoryArtifacts {
    pub root: Utf8PathBuf,
}

impl Default for MemoryArtifacts {
    fn default() -> Self {
        Self {
            root: Utf8PathBuf::from("/tmp/taskfence-testkit"),
        }
    }
}

impl ArtifactStore for MemoryArtifacts {
    fn create_task_dir(&self, task: &ResolvedTask) -> Result<ArtifactRefs> {
        Ok(ArtifactRefs {
            task_dir: self.root.join(&task.id.0),
            resolved_task: None,
            events: None,
            stdout: None,
            stderr: None,
            diff: None,
            report: None,
            gateway_spool: None,
        })
    }

    fn write_resolved_task(&self, task: &ResolvedTask) -> Result<Utf8PathBuf> {
        Ok(self.root.join(&task.id.0).join("task.resolved.json"))
    }

    fn write_log(
        &self,
        task: &ResolvedTask,
        stream: LogStream,
        _contents: &str,
    ) -> Result<Utf8PathBuf> {
        let file = match stream {
            LogStream::Stdout => "stdout.log",
            LogStream::Stderr => "stderr.log",
        };
        Ok(self.root.join(&task.id.0).join(file))
    }

    fn capture_baseline(&self, _task: &ResolvedTask) -> Result<WorkspaceBaseline> {
        Ok(WorkspaceBaseline {
            dirty_before_run: false,
            summary: "clean".into(),
        })
    }

    fn collect_diff(
        &self,
        task: &ResolvedTask,
        _baseline: &WorkspaceBaseline,
    ) -> Result<Option<Utf8PathBuf>> {
        Ok(Some(self.root.join(&task.id.0).join("diff.patch")))
    }
}

#[derive(Debug, Default)]
pub struct StaticAgentAdapter;

impl AgentAdapter for StaticAgentAdapter {
    fn build_invocation(&self, task: &ResolvedTask) -> Result<AgentInvocation> {
        Ok(AgentInvocation {
            executable: task.agent.command.clone(),
            args: task.agent.args.clone(),
            env: BTreeMap::new(),
            working_dir: task.workspace_container_path.clone(),
        })
    }
}

#[derive(Debug)]
pub struct StaticRunner {
    pub exit_status: ExitStatus,
}

impl Default for StaticRunner {
    fn default() -> Self {
        Self {
            exit_status: ExitStatus {
                code: Some(0),
                timed_out: false,
                signal: None,
            },
        }
    }
}

impl taskfence_core::Runner for StaticRunner {
    fn prepare(&self, task: &ResolvedTask) -> Result<PreparedRun> {
        Ok(PreparedRun {
            task_id: task.id.clone(),
            image: task.sandbox.image.clone(),
            mounts: Vec::<MountPlan>::new(),
            env: BTreeMap::new(),
            network: task.permissions.network.clone(),
            limits: task.sandbox.limits.clone(),
        })
    }

    fn start(&self, prepared: PreparedRun, _invocation: AgentInvocation) -> Result<RunningTask> {
        Ok(RunningTask {
            task_id: prepared.task_id,
            runner_ref: "test-runner".into(),
        })
    }

    fn stop(&self, _running: &RunningTask) -> Result<()> {
        Ok(())
    }

    fn collect_exit(&self, _running: &RunningTask) -> Result<RunOutput> {
        Ok(RunOutput {
            exit_status: self.exit_status.clone(),
            stdout: String::new(),
            stderr: String::new(),
        })
    }
}

#[derive(Clone, Debug)]
pub struct RunnerCapabilityFixture {
    pub report: RunnerCapabilityReport,
}

impl RunnerCapabilityFixture {
    pub fn available_docker() -> Self {
        Self {
            report: RunnerCapabilityReport {
                kind: RunnerKind::Docker,
                available: true,
                can_isolate_filesystem: true,
                can_isolate_secrets: true,
                can_disable_network: true,
                can_enforce_default_deny_network: true,
                can_enforce_domain_allowlist: false,
                can_enforce_limits: true,
                can_capture_output: true,
                missing: Vec::new(),
            },
        }
    }

    pub fn unavailable(kind: RunnerKind, missing: impl Into<String>) -> Self {
        Self {
            report: RunnerCapabilityReport::unavailable(kind, vec![missing.into()]),
        }
    }

    pub fn ensure_sufficient_for(&self, task: &ResolvedTask) -> Result<()> {
        self.report.ensure_sufficient_for_task(task)
    }
}

#[derive(Debug)]
pub struct StaticReport {
    pub path: Utf8PathBuf,
}

impl Default for StaticReport {
    fn default() -> Self {
        Self {
            path: Utf8PathBuf::from("/tmp/taskfence-testkit/report.md"),
        }
    }
}

impl ReportGenerator for StaticReport {
    fn generate(
        &self,
        _task: &ResolvedTask,
        _artifacts: &ArtifactRefs,
        _events: &[AuditEvent],
    ) -> Result<Utf8PathBuf> {
        Ok(self.path.clone())
    }
}

#[derive(Debug, Default)]
pub struct MemoryState {
    statuses: RefCell<BTreeMap<TaskId, TaskStatus>>,
}

impl StateStore for MemoryState {
    fn set_status(&self, task_id: &TaskId, status: TaskStatus) -> Result<()> {
        self.statuses.borrow_mut().insert(task_id.clone(), status);
        Ok(())
    }

    fn get_status(&self, task_id: &TaskId) -> Result<Option<TaskStatus>> {
        Ok(self.statuses.borrow().get(task_id).cloned())
    }
}
