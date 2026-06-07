use camino::{Utf8Path, Utf8PathBuf};
use std::collections::BTreeMap;
use std::fs;
use std::io::{self, Write};
use std::sync::Arc;
use std::sync::Mutex;
use std::thread;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use taskfence_core::{
    Action, ActionDecision, ApprovalDecision, ApprovalEngine, ApprovalId, ApprovalRecord,
    ResolvedTask, RiskLevel, TaskFenceError,
};
use time::OffsetDateTime;

const MAX_INTERACTIVE_ATTEMPTS: usize = 3;
const DEFAULT_EXTERNAL_POLL_INTERVAL: Duration = Duration::from_millis(250);
const DEFAULT_EXTERNAL_TIMEOUT: Duration = Duration::from_secs(60 * 60);
const TASKFENCE_DIR: &str = ".taskfence";
const APPROVALS_DIR: &str = "approvals";

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

#[derive(Clone, Debug)]
pub struct LocalApprovalStore {
    workspace: Utf8PathBuf,
}

impl LocalApprovalStore {
    pub fn new(workspace: impl Into<Utf8PathBuf>) -> Self {
        Self {
            workspace: workspace.into(),
        }
    }

    pub fn approvals_dir(&self) -> Utf8PathBuf {
        self.workspace.join(TASKFENCE_DIR).join(APPROVALS_DIR)
    }

    pub fn approval_path(&self, approval_id: &ApprovalId) -> taskfence_core::Result<Utf8PathBuf> {
        validate_approval_id_component(&approval_id.0)?;
        Ok(self.approvals_dir().join(format!("{}.json", approval_id.0)))
    }

    pub fn write_pending(&self, record: &ApprovalRecord) -> taskfence_core::Result<Utf8PathBuf> {
        if record.decision.is_some() {
            return Err(TaskFenceError::Approval(format!(
                "approval {} is already resolved",
                record.id.0
            )));
        }
        self.write_record(record)
    }

    pub fn read(&self, approval_id: &ApprovalId) -> taskfence_core::Result<ApprovalRecord> {
        let path = self.approval_path(approval_id)?;
        read_record_at(&path, approval_id)
    }

    pub fn list(&self) -> taskfence_core::Result<Vec<ApprovalRecord>> {
        let approvals_dir = self.approvals_dir();
        let metadata = match fs::metadata(approvals_dir.as_std_path()) {
            Ok(metadata) => metadata,
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
            Err(err) => {
                return Err(TaskFenceError::Approval(format!(
                    "failed to access approval queue {approvals_dir}: {err}"
                )));
            }
        };

        if !metadata.is_dir() {
            return Err(TaskFenceError::Approval(format!(
                "approval queue is not a directory: {approvals_dir}"
            )));
        }

        let mut records = Vec::new();
        for entry in fs::read_dir(approvals_dir.as_std_path()).map_err(|err| {
            TaskFenceError::Approval(format!(
                "failed to read approval queue {approvals_dir}: {err}"
            ))
        })? {
            let entry = entry.map_err(|err| {
                TaskFenceError::Approval(format!(
                    "failed to read approval queue entry under {approvals_dir}: {err}"
                ))
            })?;
            if !entry
                .file_type()
                .map_err(|err| {
                    TaskFenceError::Approval(format!(
                        "failed to inspect approval queue entry under {approvals_dir}: {err}"
                    ))
                })?
                .is_file()
            {
                continue;
            }

            let path = Utf8PathBuf::from_path_buf(entry.path()).map_err(|path| {
                TaskFenceError::Approval(format!(
                    "approval record path is not valid UTF-8: {path:?}"
                ))
            })?;
            if path.extension() != Some("json") {
                continue;
            }
            let approval_id = path
                .file_stem()
                .ok_or_else(|| {
                    TaskFenceError::Approval(format!(
                        "approval record has no file stem under {approvals_dir}: {path}"
                    ))
                })?
                .to_owned();
            validate_approval_id_component(&approval_id)?;
            records.push(read_record_at(&path, &ApprovalId(approval_id))?);
        }

        records.sort_by(|left, right| left.id.cmp(&right.id));
        Ok(records)
    }

    pub fn resolve(
        &self,
        approval_id: &ApprovalId,
        decision: ApprovalDecision,
    ) -> taskfence_core::Result<ApprovalRecord> {
        self.resolve_with_actor(approval_id, decision, "local-cli", Some("cli".into()))
    }

    pub fn resolve_with_actor(
        &self,
        approval_id: &ApprovalId,
        decision: ApprovalDecision,
        actor: impl Into<String>,
        source: Option<String>,
    ) -> taskfence_core::Result<ApprovalRecord> {
        let mut record = self.read(approval_id)?;
        if record.decision.is_some() {
            return Err(TaskFenceError::Approval(format!(
                "approval {} is already resolved",
                approval_id.0
            )));
        }

        record.actor = actor.into();
        record.source = source;
        record.decision = Some(decision);
        record.resolved_at = Some(OffsetDateTime::now_utc());
        self.write_record(&record)?;
        Ok(record)
    }

    fn write_record(&self, record: &ApprovalRecord) -> taskfence_core::Result<Utf8PathBuf> {
        let path = self.approval_path(&record.id)?;
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent.as_std_path()).map_err(approval_store_io_error)?;
        }
        let bytes = serde_json::to_vec_pretty(record).map_err(|err| {
            TaskFenceError::Approval(format!(
                "failed to serialize approval record {}: {err}",
                record.id.0
            ))
        })?;
        atomic_write(&path, &bytes)?;
        Ok(path)
    }
}

pub struct LocalExternalApprovalEngine {
    actor: String,
    source: Option<String>,
    store: LocalApprovalStore,
    timeout_override: Option<Duration>,
    poll_interval: Duration,
    announce: bool,
    timeouts: Mutex<BTreeMap<ApprovalId, Duration>>,
}

impl std::fmt::Debug for LocalExternalApprovalEngine {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("LocalExternalApprovalEngine")
            .field("actor", &self.actor)
            .field("source", &self.source)
            .field("store", &self.store)
            .field("timeout_override", &self.timeout_override)
            .field("poll_interval", &self.poll_interval)
            .field("announce", &self.announce)
            .finish_non_exhaustive()
    }
}

impl LocalExternalApprovalEngine {
    pub fn new(workspace: impl Into<Utf8PathBuf>) -> Self {
        Self {
            actor: "local".into(),
            source: Some("external".into()),
            store: LocalApprovalStore::new(workspace),
            timeout_override: None,
            poll_interval: DEFAULT_EXTERNAL_POLL_INTERVAL,
            announce: true,
            timeouts: Mutex::new(BTreeMap::new()),
        }
    }

    pub fn with_actor(mut self, actor: impl Into<String>) -> Self {
        self.actor = actor.into();
        self
    }

    pub fn with_timeout(mut self, timeout: Duration) -> Self {
        self.timeout_override = Some(timeout);
        self
    }

    pub fn with_poll_interval(mut self, poll_interval: Duration) -> Self {
        self.poll_interval = poll_interval;
        self
    }

    pub fn without_announcement(mut self) -> Self {
        self.announce = false;
        self
    }

    pub fn store(&self) -> &LocalApprovalStore {
        &self.store
    }

    fn timeout_for(&self, task: &ResolvedTask) -> Duration {
        self.timeout_override.unwrap_or_else(|| {
            task.approval
                .timeout_minutes
                .map(|minutes| Duration::from_secs(minutes.saturating_mul(60)))
                .unwrap_or(DEFAULT_EXTERNAL_TIMEOUT)
        })
    }

    fn remember_timeout(
        &self,
        approval_id: ApprovalId,
        timeout: Duration,
    ) -> taskfence_core::Result<()> {
        self.timeouts
            .lock()
            .map_err(|_| {
                TaskFenceError::Approval("external approval timeout store is poisoned".into())
            })?
            .insert(approval_id, timeout);
        Ok(())
    }

    fn timeout_for_approval(&self, approval_id: &ApprovalId) -> taskfence_core::Result<Duration> {
        self.timeouts
            .lock()
            .map_err(|_| {
                TaskFenceError::Approval("external approval timeout store is poisoned".into())
            })
            .map(|timeouts| {
                timeouts
                    .get(approval_id)
                    .copied()
                    .unwrap_or(DEFAULT_EXTERNAL_TIMEOUT)
            })
    }

    fn announce_request(&self, record: &ApprovalRecord) -> taskfence_core::Result<()> {
        if !self.announce {
            return Ok(());
        }

        let mut stderr = io::stderr();
        stderr
            .write_all(render_approval_request(record).as_bytes())
            .map_err(approval_io_error)?;
        stderr
            .write_all(render_external_approval_commands(record, &self.store.workspace).as_bytes())
            .map_err(approval_io_error)?;
        stderr.flush().map_err(approval_io_error)
    }
}

impl ApprovalEngine for LocalExternalApprovalEngine {
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

        self.store.write_pending(&record)?;
        self.remember_timeout(record.id.clone(), self.timeout_for(task))?;
        self.announce_request(&record)?;
        Ok(record)
    }

    fn wait(&self, approval_id: &ApprovalId) -> taskfence_core::Result<ApprovalRecord> {
        let timeout = self.timeout_for_approval(approval_id)?;
        let started = std::time::Instant::now();
        loop {
            let record = self.store.read(approval_id)?;
            if record.decision.is_some() {
                return Ok(record);
            }

            if started.elapsed() >= timeout {
                return match self.store.resolve_with_actor(
                    approval_id,
                    ApprovalDecision::TimedOut,
                    self.actor.clone(),
                    Some("external-timeout".into()),
                ) {
                    Ok(record) => Ok(record),
                    Err(err) => {
                        let current = self.store.read(approval_id)?;
                        if current.decision.is_some() {
                            Ok(current)
                        } else {
                            Err(err)
                        }
                    }
                };
            }

            thread::sleep(self.poll_interval);
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

pub fn render_external_approval_commands(record: &ApprovalRecord, workspace: &Utf8Path) -> String {
    let workspace = workspace.as_str();
    format!(
        "Resolve from another terminal:\n  taskfence approve {} --workspace \"{}\"\n  taskfence deny {} --workspace \"{}\"\n\n",
        record.id.0, workspace, record.id.0, workspace
    )
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

fn approval_store_io_error(err: std::io::Error) -> TaskFenceError {
    TaskFenceError::Approval(format!("approval store IO failed: {err}"))
}

fn read_record_at(
    path: &Utf8Path,
    approval_id: &ApprovalId,
) -> taskfence_core::Result<ApprovalRecord> {
    let contents = fs::read_to_string(path.as_std_path()).map_err(|err| {
        TaskFenceError::Approval(format!(
            "approval record not found for {} at {path}: {err}",
            approval_id.0
        ))
    })?;
    let record: ApprovalRecord = serde_json::from_str(&contents).map_err(|err| {
        TaskFenceError::Approval(format!(
            "approval record is not valid JSON for {} at {path}: {err}",
            approval_id.0
        ))
    })?;
    validate_approval_id_component(&record.id.0)?;
    if record.id != *approval_id {
        return Err(TaskFenceError::Approval(format!(
            "approval record id {} does not match path id {} at {path}",
            record.id.0, approval_id.0
        )));
    }
    Ok(record)
}

fn atomic_write(path: &Utf8Path, bytes: &[u8]) -> taskfence_core::Result<()> {
    let parent = path.parent().ok_or_else(|| {
        TaskFenceError::Approval(format!("approval path has no parent directory: {path}"))
    })?;
    fs::create_dir_all(parent.as_std_path()).map_err(approval_store_io_error)?;
    let file_name = path.file_name().ok_or_else(|| {
        TaskFenceError::Approval(format!("approval path has no file name: {path}"))
    })?;
    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|err| TaskFenceError::Approval(format!("system clock error: {err}")))?
        .as_nanos();
    let tmp_path = parent.join(format!(
        ".{file_name}.{}.{}.tmp",
        std::process::id(),
        timestamp
    ));

    fs::write(tmp_path.as_std_path(), bytes).map_err(approval_store_io_error)?;
    if let Err(err) = fs::rename(tmp_path.as_std_path(), path.as_std_path()) {
        let _ = fs::remove_file(tmp_path.as_std_path());
        return Err(approval_store_io_error(err));
    }
    Ok(())
}

fn validate_approval_id_component(value: &str) -> taskfence_core::Result<()> {
    if value.is_empty()
        || value == "."
        || value == ".."
        || value.contains('/')
        || value.contains('\\')
        || value.chars().any(char::is_control)
    {
        return Err(TaskFenceError::Approval(format!(
            "approval id is not a safe path component: {value:?}"
        )));
    }
    Ok(())
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
            gateway: Default::default(),
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

    #[test]
    fn local_store_writes_and_resolves_pending_approval() {
        let temp = tempfile::tempdir().unwrap();
        let workspace = Utf8PathBuf::from_path_buf(temp.path().join("repo")).unwrap();
        fs::create_dir(&workspace).unwrap();
        let store = LocalApprovalStore::new(workspace.clone());
        let record = pending_record("approval-1");

        let path = store.write_pending(&record).unwrap();

        assert_eq!(path, workspace.join(".taskfence/approvals/approval-1.json"));
        assert_eq!(store.read(&record.id).unwrap().decision, None);

        let resolved = store
            .resolve(&record.id, ApprovalDecision::Approved)
            .unwrap();

        assert_eq!(resolved.decision, Some(ApprovalDecision::Approved));
        assert_eq!(resolved.actor, "local-cli");
        assert_eq!(resolved.source.as_deref(), Some("cli"));
        assert!(resolved.resolved_at.is_some());
        assert_eq!(
            store.read(&record.id).unwrap().decision,
            Some(ApprovalDecision::Approved)
        );
    }

    #[test]
    fn local_store_lists_approval_records_sorted_by_id() {
        let temp = tempfile::tempdir().unwrap();
        let workspace = Utf8PathBuf::from_path_buf(temp.path().join("repo")).unwrap();
        fs::create_dir(&workspace).unwrap();
        let store = LocalApprovalStore::new(workspace.clone());
        store.write_pending(&pending_record("approval-b")).unwrap();
        store.write_pending(&pending_record("approval-a")).unwrap();
        store
            .resolve(&ApprovalId("approval-b".into()), ApprovalDecision::Denied)
            .unwrap();

        let records = store.list().unwrap();

        assert_eq!(
            records
                .iter()
                .map(|record| &record.id.0)
                .collect::<Vec<_>>(),
            vec!["approval-a", "approval-b"]
        );
        assert_eq!(records[0].decision, None);
        assert_eq!(records[1].decision, Some(ApprovalDecision::Denied));
    }

    #[test]
    fn local_store_list_returns_empty_when_queue_is_missing() {
        let temp = tempfile::tempdir().unwrap();
        let workspace = Utf8PathBuf::from_path_buf(temp.path().join("repo")).unwrap();
        fs::create_dir(&workspace).unwrap();
        let store = LocalApprovalStore::new(workspace);

        assert!(store.list().unwrap().is_empty());
    }

    #[test]
    fn local_store_list_ignores_non_json_files() {
        let temp = tempfile::tempdir().unwrap();
        let workspace = Utf8PathBuf::from_path_buf(temp.path().join("repo")).unwrap();
        fs::create_dir(&workspace).unwrap();
        let store = LocalApprovalStore::new(workspace);
        fs::create_dir_all(store.approvals_dir()).unwrap();
        fs::write(
            store.approvals_dir().join("scratch.tmp"),
            "not approval json",
        )
        .unwrap();

        assert!(store.list().unwrap().is_empty());
    }

    #[test]
    fn local_store_list_rejects_malformed_approval_records() {
        let temp = tempfile::tempdir().unwrap();
        let workspace = Utf8PathBuf::from_path_buf(temp.path().join("repo")).unwrap();
        fs::create_dir(&workspace).unwrap();
        let store = LocalApprovalStore::new(workspace);
        fs::create_dir_all(store.approvals_dir()).unwrap();
        fs::write(store.approvals_dir().join("approval-bad.json"), "not json").unwrap();

        let err = store.list().unwrap_err();

        assert!(
            matches!(err, TaskFenceError::Approval(message) if message.contains("not valid JSON"))
        );
    }

    #[test]
    fn local_store_list_rejects_mismatched_record_ids() {
        let temp = tempfile::tempdir().unwrap();
        let workspace = Utf8PathBuf::from_path_buf(temp.path().join("repo")).unwrap();
        fs::create_dir(&workspace).unwrap();
        let store = LocalApprovalStore::new(workspace);
        let record = pending_record("approval-inner");
        fs::create_dir_all(store.approvals_dir()).unwrap();
        fs::write(
            store.approvals_dir().join("approval-outer.json"),
            serde_json::to_vec_pretty(&record).unwrap(),
        )
        .unwrap();

        let err = store.list().unwrap_err();

        assert!(
            matches!(err, TaskFenceError::Approval(message) if message.contains("does not match path id"))
        );
    }

    #[test]
    fn local_store_list_rejects_unsafe_approval_record_names() {
        let temp = tempfile::tempdir().unwrap();
        let workspace = Utf8PathBuf::from_path_buf(temp.path().join("repo")).unwrap();
        fs::create_dir(&workspace).unwrap();
        let store = LocalApprovalStore::new(workspace);
        fs::create_dir_all(store.approvals_dir()).unwrap();
        fs::write(store.approvals_dir().join("..json"), "{}").unwrap();

        let err = store.list().unwrap_err();

        assert!(
            matches!(err, TaskFenceError::Approval(message) if message.contains("safe path component"))
        );
    }

    #[test]
    fn local_store_rejects_unknown_and_unsafe_approval_ids() {
        let temp = tempfile::tempdir().unwrap();
        let workspace = Utf8PathBuf::from_path_buf(temp.path().join("repo")).unwrap();
        fs::create_dir(&workspace).unwrap();
        let store = LocalApprovalStore::new(workspace);

        let missing = store
            .resolve(&ApprovalId("missing".into()), ApprovalDecision::Approved)
            .unwrap_err();
        assert!(
            matches!(missing, TaskFenceError::Approval(message) if message.contains("not found"))
        );

        let unsafe_id = store
            .approval_path(&ApprovalId("../escape".into()))
            .unwrap_err();
        assert!(
            matches!(unsafe_id, TaskFenceError::Approval(message) if message.contains("safe path component"))
        );
    }

    #[test]
    fn local_store_rejects_double_resolution() {
        let temp = tempfile::tempdir().unwrap();
        let workspace = Utf8PathBuf::from_path_buf(temp.path().join("repo")).unwrap();
        fs::create_dir(&workspace).unwrap();
        let store = LocalApprovalStore::new(workspace);
        let record = pending_record("approval-double");
        store.write_pending(&record).unwrap();

        store.resolve(&record.id, ApprovalDecision::Denied).unwrap();
        let err = store
            .resolve(&record.id, ApprovalDecision::Approved)
            .unwrap_err();

        assert!(
            matches!(err, TaskFenceError::Approval(message) if message.contains("already resolved"))
        );
    }

    #[test]
    fn external_engine_wait_observes_file_resolution() {
        let temp = tempfile::tempdir().unwrap();
        let workspace = Utf8PathBuf::from_path_buf(temp.path().join("repo")).unwrap();
        fs::create_dir(&workspace).unwrap();
        let engine = LocalExternalApprovalEngine::new(workspace)
            .with_timeout(Duration::from_secs(1))
            .with_poll_interval(Duration::from_millis(10))
            .without_announcement();
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
        let store = engine.store().clone();
        let approval_id = requested.id.clone();

        let resolver = thread::spawn(move || {
            thread::sleep(Duration::from_millis(30));
            store
                .resolve(&approval_id, ApprovalDecision::Approved)
                .unwrap();
        });
        let resolved = engine.wait(&requested.id).unwrap();
        resolver.join().unwrap();

        assert_eq!(resolved.decision, Some(ApprovalDecision::Approved));
        assert_eq!(resolved.source.as_deref(), Some("cli"));
    }

    #[test]
    fn external_engine_timeout_resolves_fail_closed_record() {
        let temp = tempfile::tempdir().unwrap();
        let workspace = Utf8PathBuf::from_path_buf(temp.path().join("repo")).unwrap();
        fs::create_dir(&workspace).unwrap();
        let engine = LocalExternalApprovalEngine::new(workspace)
            .with_timeout(Duration::from_millis(1))
            .with_poll_interval(Duration::from_millis(1))
            .without_announcement();
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
        assert_eq!(resolved.source.as_deref(), Some("external-timeout"));
        assert_eq!(
            engine.store().read(&requested.id).unwrap().decision,
            Some(ApprovalDecision::TimedOut)
        );
    }

    fn pending_record(id: &str) -> ApprovalRecord {
        let task = task();
        ApprovalRecord {
            id: ApprovalId(id.into()),
            task_id: task.id,
            actor: "local".into(),
            source: Some("external".into()),
            requested_at: OffsetDateTime::now_utc(),
            resolved_at: None,
            action: Action::Budget {
                kind: "tokens".into(),
                amount: 10,
            },
            policy_decision: approval_decision(),
            decision: None,
        }
    }
}
