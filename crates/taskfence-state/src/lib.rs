use std::collections::BTreeMap;
use std::fs::{self, File};
use std::io::{BufRead, BufReader};
use std::path::Component;
use std::sync::Mutex;

use camino::{Utf8Path, Utf8PathBuf};
use taskfence_core::{
    ApprovalId, ApprovalRecord, AuditEvent, LogStream, ResolvedTask, StateStore, TaskFenceError,
    TaskId, TaskStatus,
};

const TASKFENCE_DIR: &str = ".taskfence";
const TASKS_DIR: &str = "tasks";
const RESOLVED_TASK_FILE: &str = "task.resolved.json";
const EVENTS_FILE: &str = "events.jsonl";
const STDOUT_FILE: &str = "stdout.log";
const STDERR_FILE: &str = "stderr.log";
const DIFF_FILE: &str = "diff.patch";
const REPORT_FILE: &str = "report.md";
const ARTIFACTS_DIR: &str = "artifacts";

#[derive(Debug, Default)]
pub struct InMemoryStateStore {
    statuses: Mutex<BTreeMap<TaskId, TaskStatus>>,
}

impl InMemoryStateStore {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn snapshot(&self) -> taskfence_core::Result<BTreeMap<TaskId, TaskStatus>> {
        self.statuses
            .lock()
            .map(|statuses| statuses.clone())
            .map_err(|_| TaskFenceError::State("state store is poisoned".into()))
    }
}

impl StateStore for InMemoryStateStore {
    fn set_status(&self, task_id: &TaskId, status: TaskStatus) -> taskfence_core::Result<()> {
        self.statuses
            .lock()
            .map_err(|_| TaskFenceError::State("state store is poisoned".into()))?
            .insert(task_id.clone(), status);
        Ok(())
    }

    fn get_status(&self, task_id: &TaskId) -> taskfence_core::Result<Option<TaskStatus>> {
        self.statuses
            .lock()
            .map(|statuses| statuses.get(task_id).cloned())
            .map_err(|_| TaskFenceError::State("state store is poisoned".into()))
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TaskLogFile {
    pub stream: LogStream,
    pub path: Utf8PathBuf,
    pub contents: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TaskLogs {
    pub task_dir: Utf8PathBuf,
    pub entries: Vec<TaskLogFile>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TaskReport {
    pub path: Utf8PathBuf,
    pub contents: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TaskDiff {
    pub path: Utf8PathBuf,
    pub contents: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TaskEvents {
    pub task_dir: Utf8PathBuf,
    pub path: Utf8PathBuf,
    pub events: Vec<AuditEvent>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TaskInputs {
    pub task_dir: Utf8PathBuf,
    pub path: Utf8PathBuf,
    pub task: ResolvedTask,
    pub contents: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TaskArtifactKind {
    Evidence,
    Artifact,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TaskArtifactFile {
    pub kind: TaskArtifactKind,
    pub relative_path: Utf8PathBuf,
    pub path: Utf8PathBuf,
    pub size_bytes: u64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TaskArtifacts {
    pub task_dir: Utf8PathBuf,
    pub files: Vec<TaskArtifactFile>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TaskSummary {
    pub task_id: TaskId,
    pub task_dir: Utf8PathBuf,
    pub status: Option<TaskStatus>,
    pub goal: Option<String>,
    pub has_report: bool,
    pub has_diff: bool,
    pub has_stdout: bool,
    pub has_stderr: bool,
    pub warnings: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LocalReviewIndex {
    pub workspace: Utf8PathBuf,
    pub tasks: Vec<TaskSummary>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LocalTaskReview {
    pub summary: TaskSummary,
    pub inputs: Option<TaskInputs>,
    pub artifacts: Option<TaskArtifacts>,
    pub events: Option<TaskEvents>,
    pub logs: Option<TaskLogs>,
    pub diff: Option<TaskDiff>,
    pub report: Option<TaskReport>,
    pub replay: ReplayPlan,
    pub warnings: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ReplayPlan {
    pub task_id: TaskId,
    pub source_task_file: Option<Utf8PathBuf>,
    pub resolved_task_path: Option<Utf8PathBuf>,
    pub event_log_path: Option<Utf8PathBuf>,
    pub artifact_dir: Utf8PathBuf,
    pub last_status: Option<TaskStatus>,
    pub can_replay: bool,
    pub deterministic: bool,
    pub blockers: Vec<String>,
    pub limitations: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TeamApiResource {
    TaskList,
    TaskDetail(TaskId),
    TaskEvents(TaskId),
    TaskLogs(TaskId),
    TaskDiff(TaskId),
    TaskReport(TaskId),
    TaskArtifacts(TaskId),
    Approvals,
    ApprovalDetail(ApprovalId),
    ReplayInputs(TaskId),
    AuditExport,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TeamApiMethod {
    Read,
    ResolveApproval,
    EnqueueTask,
    ExportAudit,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TeamApiRequest {
    pub organization: String,
    pub actor: String,
    pub method: TeamApiMethod,
    pub resource: TeamApiResource,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TeamRole {
    Viewer,
    Approver,
    Operator,
    Auditor,
    Admin,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RbacGrant {
    pub actor: String,
    pub role: TeamRole,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct OrganizationPolicy {
    pub organization: String,
    pub grants: Vec<RbacGrant>,
    pub require_approval_owner: bool,
    pub allowed_artifact_roots: Vec<Utf8PathBuf>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TeamAccessDecision {
    Allow { role: TeamRole, reason: String },
    Deny { reason: String },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum WorkerLeaseState {
    Pending,
    Leased { worker_id: String },
    Completed,
    Failed { reason: String },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WorkerLease {
    pub task_id: TaskId,
    pub organization: String,
    pub state: WorkerLeaseState,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PostgresTeamStateConfig {
    pub database_url_env: String,
    pub schema: String,
}

impl PostgresTeamStateConfig {
    pub fn new(
        database_url_env: impl Into<String>,
        schema: impl Into<String>,
    ) -> taskfence_core::Result<Self> {
        let database_url_env = database_url_env.into();
        validate_env_ref("database_url_env", &database_url_env)?;
        let schema = schema.into();
        validate_schema_name(&schema)?;
        Ok(Self {
            database_url_env,
            schema,
        })
    }

    pub fn unsupported_live_state_error(&self) -> TaskFenceError {
        TaskFenceError::Unsupported(
            "Postgres team state is contract-only; no live Postgres backend is implemented".into(),
        )
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum AuditExportSinkKind {
    Siem,
    Webhook,
    ObjectStorage,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AuditExportSinkConfig {
    pub kind: AuditExportSinkKind,
    pub destination_ref: String,
    pub credential_env: String,
}

impl AuditExportSinkConfig {
    pub fn new(
        kind: AuditExportSinkKind,
        destination_ref: impl Into<String>,
        credential_env: impl Into<String>,
    ) -> taskfence_core::Result<Self> {
        let destination_ref = destination_ref.into();
        validate_non_secret_ref("audit export destination_ref", &destination_ref)?;
        let credential_env = credential_env.into();
        validate_env_ref("audit export credential_env", &credential_env)?;
        Ok(Self {
            kind,
            destination_ref,
            credential_env,
        })
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LocalToTeamMigrationPlan {
    pub workspace: Utf8PathBuf,
    pub organization: String,
    pub tasks: Vec<TaskId>,
    pub approval_records_source: Utf8PathBuf,
    pub artifact_roots: Vec<Utf8PathBuf>,
    pub warnings: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TeamServerBoundary {
    pub api_resources: Vec<TeamApiResource>,
    pub worker_model: String,
    pub state_config: PostgresTeamStateConfig,
    pub artifact_roots: Vec<Utf8PathBuf>,
    pub audit_export_sinks: Vec<AuditExportSinkConfig>,
}

impl TeamServerBoundary {
    pub fn new(
        state_config: PostgresTeamStateConfig,
        artifact_roots: Vec<Utf8PathBuf>,
    ) -> taskfence_core::Result<Self> {
        if artifact_roots.is_empty() {
            return Err(TaskFenceError::State(
                "team artifact storage requires at least one allowed root".into(),
            ));
        }
        for root in &artifact_roots {
            validate_team_artifact_root(root)?;
        }
        Ok(Self {
            api_resources: team_api_boundary_resources(),
            worker_model:
                "deterministic in-memory lease contract for local development; live workers are unsupported"
                    .into(),
            state_config,
            artifact_roots,
            audit_export_sinks: Vec::new(),
        })
    }

    pub fn with_audit_export_sinks(
        mut self,
        sinks: Vec<AuditExportSinkConfig>,
    ) -> taskfence_core::Result<Self> {
        if sinks.is_empty() {
            return Err(TaskFenceError::State(
                "team audit export sinks require at least one configured destination".into(),
            ));
        }
        self.audit_export_sinks = sinks;
        Ok(self)
    }

    pub fn unsupported_start_error(&self) -> TaskFenceError {
        TaskFenceError::Unsupported(
            "team API server and workers are contract-only; no persistent server is implemented"
                .into(),
        )
    }

    pub fn unsupported_audit_export_error(&self) -> TaskFenceError {
        TaskFenceError::Unsupported(
            "team audit export is a validated sink contract only; no live export sink is implemented"
                .into(),
        )
    }
}

#[derive(Debug, Default)]
pub struct InMemoryWorkerQueue {
    leases: Mutex<BTreeMap<TaskId, WorkerLease>>,
}

impl InMemoryWorkerQueue {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn enqueue(
        &self,
        organization: impl Into<String>,
        task_id: TaskId,
    ) -> taskfence_core::Result<WorkerLease> {
        validate_task_id_component(&task_id.0)?;
        let organization = normalize_non_empty("organization", organization.into())?;
        let mut leases = self
            .leases
            .lock()
            .map_err(|_| TaskFenceError::State("worker queue is poisoned".into()))?;
        if leases.contains_key(&task_id) {
            return Err(TaskFenceError::State(format!(
                "task {} is already queued for team execution",
                task_id.0
            )));
        }
        let lease = WorkerLease {
            task_id: task_id.clone(),
            organization,
            state: WorkerLeaseState::Pending,
        };
        leases.insert(task_id, lease.clone());
        Ok(lease)
    }

    pub fn lease_next(
        &self,
        organization: impl Into<String>,
        worker_id: impl Into<String>,
    ) -> taskfence_core::Result<Option<WorkerLease>> {
        let organization = normalize_non_empty("organization", organization.into())?;
        let worker_id = normalize_non_empty("worker_id", worker_id.into())?;
        let mut leases = self
            .leases
            .lock()
            .map_err(|_| TaskFenceError::State("worker queue is poisoned".into()))?;
        let Some((_, lease)) = leases.iter_mut().find(|(_, lease)| {
            lease.organization == organization && lease.state == WorkerLeaseState::Pending
        }) else {
            return Ok(None);
        };
        lease.state = WorkerLeaseState::Leased { worker_id };
        Ok(Some(lease.clone()))
    }

    pub fn complete(
        &self,
        task_id: &TaskId,
        worker_id: impl Into<String>,
    ) -> taskfence_core::Result<WorkerLease> {
        self.finish_leased(task_id, worker_id, WorkerLeaseState::Completed)
    }

    pub fn fail(
        &self,
        task_id: &TaskId,
        worker_id: impl Into<String>,
        reason: impl Into<String>,
    ) -> taskfence_core::Result<WorkerLease> {
        let reason = normalize_non_empty("reason", reason.into())?;
        self.finish_leased(task_id, worker_id, WorkerLeaseState::Failed { reason })
    }

    pub fn snapshot(&self) -> taskfence_core::Result<Vec<WorkerLease>> {
        let leases = self
            .leases
            .lock()
            .map_err(|_| TaskFenceError::State("worker queue is poisoned".into()))?;
        Ok(leases.values().cloned().collect())
    }

    fn finish_leased(
        &self,
        task_id: &TaskId,
        worker_id: impl Into<String>,
        next_state: WorkerLeaseState,
    ) -> taskfence_core::Result<WorkerLease> {
        validate_task_id_component(&task_id.0)?;
        let worker_id = normalize_non_empty("worker_id", worker_id.into())?;
        let mut leases = self
            .leases
            .lock()
            .map_err(|_| TaskFenceError::State("worker queue is poisoned".into()))?;
        let lease = leases.get_mut(task_id).ok_or_else(|| {
            TaskFenceError::State(format!(
                "task {} is not queued for team execution",
                task_id.0
            ))
        })?;
        match &lease.state {
            WorkerLeaseState::Leased {
                worker_id: leased_by,
            } if leased_by == &worker_id => {
                lease.state = next_state;
                Ok(lease.clone())
            }
            WorkerLeaseState::Leased {
                worker_id: leased_by,
            } => Err(TaskFenceError::State(format!(
                "task {} is leased by worker {leased_by}, not {worker_id}",
                task_id.0
            ))),
            WorkerLeaseState::Pending => Err(TaskFenceError::State(format!(
                "task {} has not been leased by a worker",
                task_id.0
            ))),
            WorkerLeaseState::Completed => Err(TaskFenceError::State(format!(
                "task {} is already completed",
                task_id.0
            ))),
            WorkerLeaseState::Failed { reason } => Err(TaskFenceError::State(format!(
                "task {} is already failed: {reason}",
                task_id.0
            ))),
        }
    }
}

#[derive(Clone, Debug)]
pub struct LocalTaskEvidenceStore {
    workspace: Utf8PathBuf,
}

impl LocalTaskEvidenceStore {
    pub fn new(workspace: impl Into<Utf8PathBuf>) -> Self {
        Self {
            workspace: workspace.into(),
        }
    }

    pub fn task_dir(&self, task_id: &TaskId) -> taskfence_core::Result<Utf8PathBuf> {
        validate_task_id_component(&task_id.0)?;
        Ok(self.tasks_dir().join(task_id.0.as_str()))
    }

    pub fn list_tasks(&self) -> taskfence_core::Result<Vec<TaskSummary>> {
        let tasks_dir = self.tasks_dir();
        let metadata = match fs::metadata(tasks_dir.as_std_path()) {
            Ok(metadata) => metadata,
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
            Err(err) => {
                return Err(TaskFenceError::State(format!(
                    "failed to access task evidence root {tasks_dir}: {err}"
                )));
            }
        };

        if !metadata.is_dir() {
            return Err(TaskFenceError::State(format!(
                "task evidence root is not a directory: {tasks_dir}"
            )));
        }

        let mut summaries = Vec::new();
        for entry in fs::read_dir(tasks_dir.as_std_path()).map_err(|err| {
            TaskFenceError::State(format!(
                "failed to read task evidence root {tasks_dir}: {err}"
            ))
        })? {
            let entry = entry.map_err(|err| {
                TaskFenceError::State(format!(
                    "failed to read task evidence entry under {tasks_dir}: {err}"
                ))
            })?;
            if !entry
                .file_type()
                .map_err(|err| {
                    TaskFenceError::State(format!(
                        "failed to inspect task evidence entry under {tasks_dir}: {err}"
                    ))
                })?
                .is_dir()
            {
                continue;
            }

            let task_id = entry.file_name().into_string().map_err(|name| {
                TaskFenceError::State(format!(
                    "task evidence directory name is not valid UTF-8: {name:?}"
                ))
            })?;
            validate_task_id_component(&task_id)?;
            let task_id = TaskId(task_id);
            let task_dir = Utf8PathBuf::from_path_buf(entry.path()).map_err(|path| {
                TaskFenceError::State(format!(
                    "task evidence directory path is not valid UTF-8: {path:?}"
                ))
            })?;
            summaries.push(read_task_summary(task_id, task_dir));
        }

        summaries.sort_by(|left, right| left.task_id.cmp(&right.task_id));
        Ok(summaries)
    }

    pub fn review_index(&self) -> taskfence_core::Result<LocalReviewIndex> {
        Ok(LocalReviewIndex {
            workspace: self.workspace.clone(),
            tasks: self.list_tasks()?,
        })
    }

    pub fn read_task_review(&self, task_id: &TaskId) -> taskfence_core::Result<LocalTaskReview> {
        let summary = self.read_task_summary(task_id)?;
        let inputs = optional_evidence("resolved task input", self.read_inputs(task_id));
        let artifacts = optional_evidence("artifact manifest", self.read_artifacts(task_id));
        let events = optional_evidence("event log", self.read_events(task_id));
        let logs = optional_evidence("captured logs", self.read_logs(task_id));
        let diff = optional_evidence("diff artifact", self.read_diff(task_id));
        let report = optional_evidence("report", self.read_report(task_id));
        let mut warnings = summary.warnings.clone();
        push_optional_warning(&mut warnings, &inputs);
        push_optional_warning(&mut warnings, &artifacts);
        push_optional_warning(&mut warnings, &events);
        push_optional_warning(&mut warnings, &logs);
        push_optional_warning(&mut warnings, &diff);
        push_optional_warning(&mut warnings, &report);
        let replay = self.replay_plan(task_id)?;

        Ok(LocalTaskReview {
            summary,
            inputs: inputs.value,
            artifacts: artifacts.value,
            events: events.value,
            logs: logs.value,
            diff: diff.value,
            report: report.value,
            replay,
            warnings,
        })
    }

    pub fn replay_plan(&self, task_id: &TaskId) -> taskfence_core::Result<ReplayPlan> {
        let task_dir = self.task_dir(task_id)?;
        ensure_task_dir(task_id, &task_dir)?;
        let inputs = optional_evidence("resolved task input", self.read_inputs(task_id));
        let events = optional_evidence("event log", self.read_events(task_id));
        let mut blockers = Vec::new();
        let mut limitations = Vec::new();

        let (source_task_file, resolved_task_path) = match &inputs.value {
            Some(inputs) => (
                Some(inputs.task.task_file.clone()),
                Some(inputs.path.clone()),
            ),
            None => {
                blockers.push(
                    inputs
                        .warning
                        .clone()
                        .unwrap_or_else(|| "resolved task input is unavailable".into()),
                );
                (None, None)
            }
        };

        let event_log_path = match &events.value {
            Some(events) => Some(events.path.clone()),
            None => {
                limitations.push(
                    events
                        .warning
                        .clone()
                        .unwrap_or_else(|| "event log is unavailable".into()),
                );
                None
            }
        };

        if let Some(inputs) = &inputs.value {
            if !inputs.task.gateway.tools.is_empty() {
                limitations.push(
                    "gateway calls require fresh mediation, policy, approval, and secret checks"
                        .into(),
                );
            }
            if inputs.task.permissions.network.default != taskfence_core::NetworkDefault::Disabled {
                limitations.push("network outcomes may differ between replay attempts".into());
            }
            if !inputs.task.approval.require_for.is_empty()
                || !inputs
                    .task
                    .permissions
                    .commands
                    .approval_required
                    .is_empty()
                || !inputs.task.permissions.tools.approval_required.is_empty()
            {
                limitations.push("approval decisions must be replayed from captured policy context or re-requested".into());
            }
            if inputs.task.sandbox.image.is_some() {
                limitations.push(
                    "runner image availability and contents are external to the replay input"
                        .into(),
                );
            }
        }

        if limitations.is_empty() {
            limitations.push(
                "file-backed replay can reuse resolved inputs but does not guarantee identical external state"
                    .into(),
            );
        }

        let last_status = read_latest_task_status(task_id, &task_dir, &mut Vec::new());
        Ok(ReplayPlan {
            task_id: task_id.clone(),
            source_task_file,
            resolved_task_path,
            event_log_path,
            artifact_dir: task_dir,
            last_status,
            can_replay: blockers.is_empty(),
            deterministic: false,
            blockers,
            limitations,
        })
    }

    pub fn migration_plan(
        &self,
        organization: impl Into<String>,
    ) -> taskfence_core::Result<LocalToTeamMigrationPlan> {
        let organization = normalize_non_empty("organization", organization.into())?;
        let index = self.review_index()?;
        let mut warnings = Vec::new();
        let mut artifact_roots = Vec::new();
        let mut tasks = Vec::new();
        for task in index.tasks {
            let has_structured_state = task.task_dir.join(RESOLVED_TASK_FILE).is_file()
                || task.task_dir.join(EVENTS_FILE).is_file();
            if has_structured_state {
                tasks.push(task.task_id.clone());
                artifact_roots.push(task.task_dir.clone());
            } else {
                warnings.push(format!(
                    "task {} has no structured task input or event log; rendered reports are ignored",
                    task.task_id.0
                ));
            }
            if !task.warnings.is_empty() {
                warnings.push(format!(
                    "task {} has {} evidence warning(s); migrate structured files only",
                    task.task_id.0,
                    task.warnings.len()
                ));
            }
        }
        warnings.push(
            "rendered Markdown reports are migration artifacts, not source-of-truth state".into(),
        );
        Ok(LocalToTeamMigrationPlan {
            workspace: self.workspace.clone(),
            organization,
            tasks,
            approval_records_source: self.workspace.join(TASKFENCE_DIR).join("approvals"),
            artifact_roots,
            warnings,
        })
    }

    pub fn read_task_summary(&self, task_id: &TaskId) -> taskfence_core::Result<TaskSummary> {
        let task_dir = self.task_dir(task_id)?;
        ensure_task_dir(task_id, &task_dir)?;
        Ok(read_task_summary(task_id.clone(), task_dir))
    }

    pub fn read_inputs(&self, task_id: &TaskId) -> taskfence_core::Result<TaskInputs> {
        let task_dir = self.task_dir(task_id)?;
        ensure_task_dir(task_id, &task_dir)?;
        let path = task_dir.join(RESOLVED_TASK_FILE);
        let contents = fs::read_to_string(path.as_std_path()).map_err(|err| {
            TaskFenceError::State(format!(
                "resolved task input not found for task {} at {path}: {err}",
                task_id.0
            ))
        })?;
        let task = serde_json::from_str::<ResolvedTask>(&contents).map_err(|err| {
            TaskFenceError::State(format!(
                "failed to parse resolved task input for task {} at {path}: {err}",
                task_id.0
            ))
        })?;
        if task.id != *task_id {
            return Err(TaskFenceError::State(format!(
                "{RESOLVED_TASK_FILE} task id {} does not match requested {}",
                task.id.0, task_id.0
            )));
        }
        Ok(TaskInputs {
            task_dir,
            path,
            task,
            contents,
        })
    }

    pub fn read_artifacts(&self, task_id: &TaskId) -> taskfence_core::Result<TaskArtifacts> {
        let task_dir = self.task_dir(task_id)?;
        ensure_task_dir(task_id, &task_dir)?;
        let mut files = Vec::new();

        for file_name in [
            RESOLVED_TASK_FILE,
            EVENTS_FILE,
            STDOUT_FILE,
            STDERR_FILE,
            DIFF_FILE,
            REPORT_FILE,
        ] {
            let relative_path = Utf8PathBuf::from(file_name);
            push_regular_file(
                &mut files,
                TaskArtifactKind::Evidence,
                &relative_path,
                task_dir.join(file_name),
            )?;
        }

        let artifacts_dir = task_dir.join(ARTIFACTS_DIR);
        match fs::symlink_metadata(artifacts_dir.as_std_path()) {
            Ok(metadata) if metadata.is_dir() => {
                read_artifact_dir_files(&mut files, &artifacts_dir)?;
            }
            Ok(_) => {
                return Err(TaskFenceError::State(format!(
                    "task artifact path is not a directory: {artifacts_dir}"
                )));
            }
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => {}
            Err(err) => {
                return Err(TaskFenceError::State(format!(
                    "failed to access task artifact directory {artifacts_dir}: {err}"
                )));
            }
        }

        files.sort_by(|left, right| {
            left.relative_path
                .cmp(&right.relative_path)
                .then_with(|| kind_label(&left.kind).cmp(kind_label(&right.kind)))
        });

        Ok(TaskArtifacts { task_dir, files })
    }

    pub fn read_logs(&self, task_id: &TaskId) -> taskfence_core::Result<TaskLogs> {
        let task_dir = self.task_dir(task_id)?;
        ensure_task_dir(task_id, &task_dir)?;

        let mut entries = Vec::new();
        read_optional_log(&mut entries, &task_dir, LogStream::Stdout, STDOUT_FILE)?;
        read_optional_log(&mut entries, &task_dir, LogStream::Stderr, STDERR_FILE)?;

        if entries.is_empty() {
            return Err(TaskFenceError::State(format!(
                "no captured stdout or stderr logs found for task {} under {task_dir}",
                task_id.0
            )));
        }

        Ok(TaskLogs { task_dir, entries })
    }

    pub fn read_report(&self, task_id: &TaskId) -> taskfence_core::Result<TaskReport> {
        let task_dir = self.task_dir(task_id)?;
        ensure_task_dir(task_id, &task_dir)?;
        let path = task_dir.join(REPORT_FILE);
        let contents = fs::read_to_string(path.as_std_path()).map_err(|err| {
            TaskFenceError::State(format!(
                "report not found for task {} at {path}: {err}",
                task_id.0
            ))
        })?;
        Ok(TaskReport { path, contents })
    }

    pub fn read_diff(&self, task_id: &TaskId) -> taskfence_core::Result<TaskDiff> {
        let task_dir = self.task_dir(task_id)?;
        ensure_task_dir(task_id, &task_dir)?;
        let path = task_dir.join(DIFF_FILE);
        let contents = fs::read_to_string(path.as_std_path()).map_err(|err| {
            TaskFenceError::State(format!(
                "diff artifact not found for task {} at {path}: {err}",
                task_id.0
            ))
        })?;
        Ok(TaskDiff { path, contents })
    }

    pub fn read_events(&self, task_id: &TaskId) -> taskfence_core::Result<TaskEvents> {
        let task_dir = self.task_dir(task_id)?;
        ensure_task_dir(task_id, &task_dir)?;
        let path = task_dir.join(EVENTS_FILE);
        let file = File::open(path.as_std_path()).map_err(|err| {
            TaskFenceError::State(format!(
                "events artifact not found for task {} at {path}: {err}",
                task_id.0
            ))
        })?;
        let events = read_events_file(task_id, &path, file)?;
        Ok(TaskEvents {
            task_dir,
            path,
            events,
        })
    }

    fn tasks_dir(&self) -> Utf8PathBuf {
        self.workspace.join(TASKFENCE_DIR).join(TASKS_DIR)
    }
}

struct OptionalEvidence<T> {
    value: Option<T>,
    warning: Option<String>,
}

fn optional_evidence<T>(label: &str, result: taskfence_core::Result<T>) -> OptionalEvidence<T> {
    match result {
        Ok(value) => OptionalEvidence {
            value: Some(value),
            warning: None,
        },
        Err(err) => OptionalEvidence {
            value: None,
            warning: Some(format!("{label}: {err}")),
        },
    }
}

fn push_optional_warning<T>(warnings: &mut Vec<String>, evidence: &OptionalEvidence<T>) {
    if let Some(warning) = &evidence.warning {
        warnings.push(warning.clone());
    }
}

pub fn evaluate_team_access(
    policy: &OrganizationPolicy,
    request: &TeamApiRequest,
) -> TeamAccessDecision {
    if policy.organization != request.organization {
        return TeamAccessDecision::Deny {
            reason: "request organization does not match policy".into(),
        };
    }

    let Some(role) = policy
        .grants
        .iter()
        .find(|grant| grant.actor == request.actor)
        .map(|grant| grant.role.clone())
    else {
        return TeamAccessDecision::Deny {
            reason: "actor has no organization role".into(),
        };
    };

    if !method_matches_resource(&request.method, &request.resource) {
        return TeamAccessDecision::Deny {
            reason: format!(
                "method {:?} is not valid for resource {:?}",
                request.method, request.resource
            ),
        };
    }

    if role_allows(&role, &request.method, &request.resource) {
        TeamAccessDecision::Allow {
            role,
            reason: "role permits resource action".into(),
        }
    } else {
        TeamAccessDecision::Deny {
            reason: format!("role {role:?} does not permit {:?}", request.method),
        }
    }
}

pub fn evaluate_approval_resolution(
    policy: &OrganizationPolicy,
    request: &TeamApiRequest,
    record: &ApprovalRecord,
) -> TeamAccessDecision {
    if request.method != TeamApiMethod::ResolveApproval {
        return TeamAccessDecision::Deny {
            reason: "request method is not approval resolution".into(),
        };
    }
    if !matches!(
        &request.resource,
        TeamApiResource::ApprovalDetail(approval_id) if approval_id == &record.id
    ) {
        return TeamAccessDecision::Deny {
            reason: "approval request resource does not match approval record".into(),
        };
    }
    let access = evaluate_team_access(policy, request);
    if matches!(access, TeamAccessDecision::Deny { .. }) {
        return access;
    }
    if policy.require_approval_owner && request.actor != record.actor {
        return TeamAccessDecision::Deny {
            reason: format!(
                "approval {} is owned by {}, not {}",
                record.id.0, record.actor, request.actor
            ),
        };
    }
    if record.decision.is_some() {
        return TeamAccessDecision::Deny {
            reason: format!("approval {} is already resolved", record.id.0),
        };
    }
    access
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TeamArtifactDecision {
    Allow { root: Utf8PathBuf },
    Deny { reason: String },
}

pub fn evaluate_artifact_storage_path(
    policy: &OrganizationPolicy,
    requested_path: &Utf8Path,
) -> TeamArtifactDecision {
    if policy.allowed_artifact_roots.is_empty() {
        return TeamArtifactDecision::Deny {
            reason: "organization has no allowed artifact roots".into(),
        };
    }
    if !requested_path.is_absolute() {
        return TeamArtifactDecision::Deny {
            reason: "artifact path must be absolute".into(),
        };
    }
    if path_contains_parent_dir(requested_path) {
        return TeamArtifactDecision::Deny {
            reason: "artifact path must not contain '..'".into(),
        };
    }
    let Some(requested_parent) = requested_path.parent() else {
        return TeamArtifactDecision::Deny {
            reason: "artifact path must have a parent directory".into(),
        };
    };
    let requested_parent = match canonical_utf8(requested_parent) {
        Ok(path) => path,
        Err(err) => {
            return TeamArtifactDecision::Deny {
                reason: format!("artifact parent must exist and be canonicalizable: {err}"),
            };
        }
    };

    for root in &policy.allowed_artifact_roots {
        if let Err(err) = validate_team_artifact_root(root) {
            return TeamArtifactDecision::Deny {
                reason: err.to_string(),
            };
        }
        let root = match canonical_utf8(root) {
            Ok(path) => path,
            Err(err) => {
                return TeamArtifactDecision::Deny {
                    reason: format!("artifact root must exist and be canonicalizable: {err}"),
                };
            }
        };
        if requested_parent == root || requested_parent.starts_with(&root) {
            return TeamArtifactDecision::Allow { root };
        }
    }

    TeamArtifactDecision::Deny {
        reason: "artifact path is outside allowed organization roots".into(),
    }
}

pub fn team_api_boundary_resources() -> Vec<TeamApiResource> {
    let task_id = TaskId("{task_id}".into());
    let approval_id = ApprovalId("{approval_id}".into());
    vec![
        TeamApiResource::TaskList,
        TeamApiResource::TaskDetail(task_id.clone()),
        TeamApiResource::TaskEvents(task_id.clone()),
        TeamApiResource::TaskLogs(task_id.clone()),
        TeamApiResource::TaskDiff(task_id.clone()),
        TeamApiResource::TaskReport(task_id.clone()),
        TeamApiResource::TaskArtifacts(task_id.clone()),
        TeamApiResource::Approvals,
        TeamApiResource::ApprovalDetail(approval_id),
        TeamApiResource::ReplayInputs(task_id),
        TeamApiResource::AuditExport,
    ]
}

fn method_matches_resource(method: &TeamApiMethod, resource: &TeamApiResource) -> bool {
    match method {
        TeamApiMethod::Read => !matches!(resource, TeamApiResource::AuditExport),
        TeamApiMethod::ResolveApproval => matches!(resource, TeamApiResource::ApprovalDetail(_)),
        TeamApiMethod::EnqueueTask => matches!(resource, TeamApiResource::TaskList),
        TeamApiMethod::ExportAudit => matches!(resource, TeamApiResource::AuditExport),
    }
}

fn role_allows(role: &TeamRole, method: &TeamApiMethod, resource: &TeamApiResource) -> bool {
    match role {
        TeamRole::Admin => true,
        TeamRole::Viewer => {
            matches!(method, TeamApiMethod::Read)
                && matches!(
                    resource,
                    TeamApiResource::TaskList
                        | TeamApiResource::TaskDetail(_)
                        | TeamApiResource::TaskEvents(_)
                        | TeamApiResource::TaskLogs(_)
                        | TeamApiResource::TaskDiff(_)
                        | TeamApiResource::TaskReport(_)
                        | TeamApiResource::TaskArtifacts(_)
                        | TeamApiResource::ReplayInputs(_)
                )
        }
        TeamRole::Approver => match method {
            TeamApiMethod::Read => matches!(
                resource,
                TeamApiResource::TaskList
                    | TeamApiResource::TaskDetail(_)
                    | TeamApiResource::TaskEvents(_)
                    | TeamApiResource::TaskLogs(_)
                    | TeamApiResource::TaskDiff(_)
                    | TeamApiResource::TaskReport(_)
                    | TeamApiResource::TaskArtifacts(_)
                    | TeamApiResource::Approvals
                    | TeamApiResource::ApprovalDetail(_)
                    | TeamApiResource::ReplayInputs(_)
            ),
            TeamApiMethod::ResolveApproval => true,
            TeamApiMethod::EnqueueTask | TeamApiMethod::ExportAudit => false,
        },
        TeamRole::Operator => match method {
            TeamApiMethod::Read => !matches!(resource, TeamApiResource::AuditExport),
            TeamApiMethod::EnqueueTask | TeamApiMethod::ResolveApproval => true,
            TeamApiMethod::ExportAudit => false,
        },
        TeamRole::Auditor => match method {
            TeamApiMethod::Read => !matches!(resource, TeamApiResource::ReplayInputs(_)),
            TeamApiMethod::ExportAudit => matches!(resource, TeamApiResource::AuditExport),
            TeamApiMethod::ResolveApproval | TeamApiMethod::EnqueueTask => false,
        },
    }
}

fn validate_team_artifact_root(root: &Utf8Path) -> taskfence_core::Result<()> {
    if !root.is_absolute() {
        return Err(TaskFenceError::State(format!(
            "team artifact root must be absolute: {root}"
        )));
    }
    if path_contains_parent_dir(root) {
        return Err(TaskFenceError::State(format!(
            "team artifact root must not contain '..': {root}"
        )));
    }
    Ok(())
}

fn path_contains_parent_dir(path: &Utf8Path) -> bool {
    path.as_std_path()
        .components()
        .any(|component| matches!(component, Component::ParentDir))
}

fn canonical_utf8(path: &Utf8Path) -> taskfence_core::Result<Utf8PathBuf> {
    let canonical = fs::canonicalize(path.as_std_path())
        .map_err(|err| TaskFenceError::State(format!("failed to canonicalize {path}: {err}")))?;
    Utf8PathBuf::from_path_buf(canonical).map_err(|path| {
        TaskFenceError::State(format!("canonical path is not valid UTF-8: {path:?}"))
    })
}

fn validate_env_ref(field: &str, value: &str) -> taskfence_core::Result<()> {
    if value.is_empty()
        || !value
            .chars()
            .all(|ch| ch.is_ascii_uppercase() || ch.is_ascii_digit() || ch == '_')
    {
        return Err(TaskFenceError::State(format!(
            "{field} must be an uppercase environment variable name"
        )));
    }
    Ok(())
}

fn validate_schema_name(value: &str) -> taskfence_core::Result<()> {
    if value.is_empty()
        || !value
            .chars()
            .all(|ch| ch.is_ascii_lowercase() || ch.is_ascii_digit() || ch == '_')
    {
        return Err(TaskFenceError::State(
            "Postgres schema must contain only lowercase letters, digits, or '_'".into(),
        ));
    }
    Ok(())
}

fn normalize_non_empty(field: &str, value: String) -> taskfence_core::Result<String> {
    let normalized = value.trim();
    if normalized.is_empty() {
        return Err(TaskFenceError::State(format!("{field} must not be empty")));
    }
    Ok(normalized.to_owned())
}

fn validate_non_secret_ref(field: &str, value: &str) -> taskfence_core::Result<()> {
    let normalized = value.trim();
    let lower = normalized.to_ascii_lowercase();
    if normalized.is_empty()
        || normalized.chars().any(char::is_whitespace)
        || normalized.contains('@')
        || normalized.contains('?')
        || normalized.contains('#')
        || lower.contains("token=")
        || lower.contains("password=")
        || lower.contains("secret=")
        || lower.contains("api_key=")
        || lower.contains("authorization=")
        || lower.contains("bearer ")
        || lower.starts_with("http://")
        || lower.starts_with("https://")
        || lower.starts_with("postgres://")
        || lower.starts_with("postgresql://")
    {
        return Err(TaskFenceError::State(format!(
            "{field} must be a non-secret operator-owned reference"
        )));
    }
    Ok(())
}

fn read_task_summary(task_id: TaskId, task_dir: Utf8PathBuf) -> TaskSummary {
    let mut warnings = Vec::new();
    let goal = read_task_goal(&task_id, &task_dir, &mut warnings);
    let status = read_latest_task_status(&task_id, &task_dir, &mut warnings);

    TaskSummary {
        task_id,
        has_report: task_dir.join(REPORT_FILE).is_file(),
        has_diff: task_dir.join(DIFF_FILE).is_file(),
        has_stdout: task_dir.join(STDOUT_FILE).is_file(),
        has_stderr: task_dir.join(STDERR_FILE).is_file(),
        task_dir,
        status,
        goal,
        warnings,
    }
}

fn read_task_goal(
    task_id: &TaskId,
    task_dir: &Utf8Path,
    warnings: &mut Vec<String>,
) -> Option<String> {
    let path = task_dir.join(RESOLVED_TASK_FILE);
    let contents = match fs::read_to_string(path.as_std_path()) {
        Ok(contents) => contents,
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => return None,
        Err(err) => {
            warnings.push(format!("failed to read {RESOLVED_TASK_FILE}: {err}"));
            return None;
        }
    };

    match serde_json::from_str::<ResolvedTask>(&contents) {
        Ok(task) => {
            if task.id != *task_id {
                warnings.push(format!(
                    "{RESOLVED_TASK_FILE} task id {} does not match directory {}",
                    task.id.0, task_id.0
                ));
            }
            Some(task.goal)
        }
        Err(err) => {
            warnings.push(format!("failed to parse {RESOLVED_TASK_FILE}: {err}"));
            None
        }
    }
}

fn read_latest_task_status(
    task_id: &TaskId,
    task_dir: &Utf8Path,
    warnings: &mut Vec<String>,
) -> Option<TaskStatus> {
    let path = task_dir.join(EVENTS_FILE);
    let file = match File::open(path.as_std_path()) {
        Ok(file) => file,
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => return None,
        Err(err) => {
            warnings.push(format!("failed to read {EVENTS_FILE}: {err}"));
            return None;
        }
    };

    let mut latest = None;
    for (index, line) in BufReader::new(file).lines().enumerate() {
        let line_number = index + 1;
        let line = match line {
            Ok(line) => line,
            Err(err) => {
                warnings.push(format!(
                    "failed to read {EVENTS_FILE} line {line_number}: {err}"
                ));
                continue;
            }
        };
        if line.trim().is_empty() {
            continue;
        }

        match serde_json::from_str::<AuditEvent>(&line) {
            Ok(AuditEvent::TaskStatusChanged {
                task_id: event_task_id,
                status,
                ..
            }) if event_task_id == *task_id => {
                latest = Some(status);
            }
            Ok(AuditEvent::TaskStatusChanged {
                task_id: event_task_id,
                ..
            }) => warnings.push(format!(
                "{EVENTS_FILE} line {line_number} belongs to task {}",
                event_task_id.0
            )),
            Ok(_) => {}
            Err(err) => warnings.push(format!(
                "failed to parse {EVENTS_FILE} line {line_number}: {err}"
            )),
        }
    }

    latest
}

fn read_events_file(
    task_id: &TaskId,
    path: &Utf8Path,
    file: File,
) -> taskfence_core::Result<Vec<AuditEvent>> {
    let mut events = Vec::new();
    for (index, line) in BufReader::new(file).lines().enumerate() {
        let line_number = index + 1;
        let line = line.map_err(|err| {
            TaskFenceError::State(format!("failed to read {path} line {line_number}: {err}"))
        })?;
        if line.trim().is_empty() {
            continue;
        }

        let event = serde_json::from_str::<AuditEvent>(&line).map_err(|err| {
            TaskFenceError::State(format!("failed to parse {path} line {line_number}: {err}"))
        })?;
        let event_task_id = audit_event_task_id(&event);
        if event_task_id != task_id {
            return Err(TaskFenceError::State(format!(
                "{path} line {line_number} belongs to task {} instead of {}",
                event_task_id.0, task_id.0
            )));
        }
        events.push(event);
    }
    Ok(events)
}

fn audit_event_task_id(event: &AuditEvent) -> &TaskId {
    match event {
        AuditEvent::TaskCreated { task_id, .. }
        | AuditEvent::TaskStatusChanged { task_id, .. }
        | AuditEvent::PolicyDecision { task_id, .. }
        | AuditEvent::ToolExecutionStarted { task_id, .. }
        | AuditEvent::ToolExecutionFinished { task_id, .. }
        | AuditEvent::BudgetUsageRecorded { task_id, .. }
        | AuditEvent::Log { task_id, .. }
        | AuditEvent::RunnerExit { task_id, .. }
        | AuditEvent::Artifact { task_id, .. }
        | AuditEvent::Error { task_id, .. } => task_id,
        AuditEvent::ApprovalRequested { record } | AuditEvent::ApprovalResolved { record } => {
            &record.task_id
        }
    }
}

fn read_artifact_dir_files(
    files: &mut Vec<TaskArtifactFile>,
    artifacts_dir: &Utf8Path,
) -> taskfence_core::Result<()> {
    for entry in fs::read_dir(artifacts_dir.as_std_path()).map_err(|err| {
        TaskFenceError::State(format!(
            "failed to read task artifact directory {artifacts_dir}: {err}"
        ))
    })? {
        let entry = entry.map_err(|err| {
            TaskFenceError::State(format!(
                "failed to read task artifact entry under {artifacts_dir}: {err}"
            ))
        })?;
        let file_name = entry.file_name().into_string().map_err(|name| {
            TaskFenceError::State(format!(
                "task artifact file name is not valid UTF-8: {name:?}"
            ))
        })?;
        validate_artifact_file_name(&file_name)?;
        let relative_path = Utf8PathBuf::from(ARTIFACTS_DIR).join(&file_name);
        let path = Utf8PathBuf::from_path_buf(entry.path()).map_err(|path| {
            TaskFenceError::State(format!("task artifact path is not valid UTF-8: {path:?}"))
        })?;
        push_regular_file(files, TaskArtifactKind::Artifact, &relative_path, path)?;
    }
    Ok(())
}

fn push_regular_file(
    files: &mut Vec<TaskArtifactFile>,
    kind: TaskArtifactKind,
    relative_path: &Utf8Path,
    path: Utf8PathBuf,
) -> taskfence_core::Result<()> {
    match fs::symlink_metadata(path.as_std_path()) {
        Ok(metadata) if metadata.is_file() => {
            files.push(TaskArtifactFile {
                kind,
                relative_path: relative_path.to_path_buf(),
                path,
                size_bytes: metadata.len(),
            });
            Ok(())
        }
        Ok(_) => Ok(()),
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(err) => Err(TaskFenceError::State(format!(
            "failed to inspect task artifact {path}: {err}"
        ))),
    }
}

fn validate_artifact_file_name(value: &str) -> taskfence_core::Result<()> {
    if value.is_empty()
        || value == "."
        || value == ".."
        || value.contains('/')
        || value.contains('\\')
        || value.chars().any(char::is_control)
    {
        return Err(TaskFenceError::State(format!(
            "artifact file name is not a safe path component: {value:?}"
        )));
    }
    Ok(())
}

fn kind_label(kind: &TaskArtifactKind) -> &'static str {
    match kind {
        TaskArtifactKind::Evidence => "evidence",
        TaskArtifactKind::Artifact => "artifact",
    }
}

fn read_optional_log(
    entries: &mut Vec<TaskLogFile>,
    task_dir: &Utf8PathBuf,
    stream: LogStream,
    file_name: &str,
) -> taskfence_core::Result<()> {
    let path = task_dir.join(file_name);
    match fs::read_to_string(path.as_std_path()) {
        Ok(contents) => {
            entries.push(TaskLogFile {
                stream,
                path,
                contents,
            });
            Ok(())
        }
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(err) => Err(TaskFenceError::State(format!(
            "failed to read task log {path}: {err}"
        ))),
    }
}

fn ensure_task_dir(task_id: &TaskId, task_dir: &Utf8PathBuf) -> taskfence_core::Result<()> {
    match fs::metadata(task_dir.as_std_path()) {
        Ok(metadata) if metadata.is_dir() => Ok(()),
        Ok(_) => Err(TaskFenceError::State(format!(
            "task evidence path is not a directory for task {}: {task_dir}",
            task_id.0
        ))),
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => {
            Err(TaskFenceError::State(format!(
                "task evidence directory not found for task {}: {task_dir}",
                task_id.0
            )))
        }
        Err(err) => Err(TaskFenceError::State(format!(
            "failed to access task evidence directory for task {} at {task_dir}: {err}",
            task_id.0
        ))),
    }
}

fn validate_task_id_component(value: &str) -> taskfence_core::Result<()> {
    if value.is_empty()
        || value == "."
        || value == ".."
        || value.contains('/')
        || value.contains('\\')
        || value.chars().any(char::is_control)
    {
        return Err(TaskFenceError::State(format!(
            "task id is not a safe artifact path component: {value:?}"
        )));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use camino::Utf8PathBuf;
    use std::fs;
    use taskfence_core::{
        Action, ActionDecision, AgentConfig, AgentKind, ApprovalConfig, ApprovalDecision,
        AuditConfig, LimitConfig, PermissionConfig, RiskLevel, SandboxConfig, SandboxKind,
    };
    use time::macros::datetime;

    #[test]
    fn missing_task_status_returns_none() {
        let store = InMemoryStateStore::new();

        assert_eq!(store.get_status(&TaskId("missing".into())).unwrap(), None);
    }

    #[test]
    fn set_and_get_status() {
        let store = InMemoryStateStore::new();
        let task_id = TaskId("task-1".into());

        store.set_status(&task_id, TaskStatus::Created).unwrap();
        store.set_status(&task_id, TaskStatus::Running).unwrap();

        assert_eq!(
            store.get_status(&task_id).unwrap(),
            Some(TaskStatus::Running)
        );
    }

    #[test]
    fn snapshot_returns_all_statuses() {
        let store = InMemoryStateStore::new();
        store
            .set_status(&TaskId("task-1".into()), TaskStatus::Succeeded)
            .unwrap();
        store
            .set_status(&TaskId("task-2".into()), TaskStatus::Denied)
            .unwrap();

        let snapshot = store.snapshot().unwrap();

        assert_eq!(snapshot.len(), 2);
        assert_eq!(
            snapshot.get(&TaskId("task-1".into())),
            Some(&TaskStatus::Succeeded)
        );
    }

    #[test]
    fn team_rbac_allows_and_denies_by_role_and_resource() {
        let policy = team_policy();

        assert_eq!(
            evaluate_team_access(
                &policy,
                &team_request("viewer", TeamApiMethod::Read, TeamApiResource::TaskList)
            ),
            TeamAccessDecision::Allow {
                role: TeamRole::Viewer,
                reason: "role permits resource action".into(),
            }
        );
        assert!(matches!(
            evaluate_team_access(
                &policy,
                &team_request(
                    "viewer",
                    TeamApiMethod::ExportAudit,
                    TeamApiResource::AuditExport
                )
            ),
            TeamAccessDecision::Deny { reason } if reason.contains("Viewer")
        ));
        assert!(matches!(
            evaluate_team_access(
                &policy,
                &team_request(
                    "auditor",
                    TeamApiMethod::ExportAudit,
                    TeamApiResource::AuditExport
                )
            ),
            TeamAccessDecision::Allow {
                role: TeamRole::Auditor,
                ..
            }
        ));
        assert!(matches!(
            evaluate_team_access(
                &policy,
                &team_request(
                    "auditor",
                    TeamApiMethod::ResolveApproval,
                    TeamApiResource::ApprovalDetail(ApprovalId("approval-1".into()))
                )
            ),
            TeamAccessDecision::Deny { reason } if reason.contains("Auditor")
        ));
        assert!(matches!(
            evaluate_team_access(
                &policy,
                &team_request(
                    "operator",
                    TeamApiMethod::EnqueueTask,
                    TeamApiResource::TaskList
                )
            ),
            TeamAccessDecision::Allow {
                role: TeamRole::Operator,
                ..
            }
        ));
        assert!(matches!(
            evaluate_team_access(
                &policy,
                &TeamApiRequest {
                    organization: "other-org".into(),
                    actor: "admin".into(),
                    method: TeamApiMethod::Read,
                    resource: TeamApiResource::TaskList,
                }
            ),
            TeamAccessDecision::Deny { reason } if reason.contains("organization")
        ));
        assert!(matches!(
            evaluate_team_access(
                &policy,
                &team_request(
                    "admin",
                    TeamApiMethod::EnqueueTask,
                    TeamApiResource::ApprovalDetail(ApprovalId("approval-1".into()))
                )
            ),
            TeamAccessDecision::Deny { reason } if reason.contains("not valid")
        ));
    }

    #[test]
    fn approval_owner_policy_blocks_non_owner_resolution() {
        let mut policy = team_policy();
        policy.require_approval_owner = true;
        let record = approval_record("approval-1", "approver");

        let non_owner = evaluate_approval_resolution(
            &policy,
            &team_request(
                "operator",
                TeamApiMethod::ResolveApproval,
                TeamApiResource::ApprovalDetail(record.id.clone()),
            ),
            &record,
        );

        assert!(
            matches!(non_owner, TeamAccessDecision::Deny { reason } if reason.contains("owned by approver"))
        );

        let owner = evaluate_approval_resolution(
            &policy,
            &team_request(
                "approver",
                TeamApiMethod::ResolveApproval,
                TeamApiResource::ApprovalDetail(record.id.clone()),
            ),
            &record,
        );

        assert!(matches!(
            owner,
            TeamAccessDecision::Allow {
                role: TeamRole::Approver,
                ..
            }
        ));
    }

    #[test]
    fn approval_resolution_rejects_mismatch_and_already_resolved_records() {
        let policy = team_policy();
        let mut record = approval_record("approval-1", "approver");

        let mismatch = evaluate_approval_resolution(
            &policy,
            &team_request(
                "approver",
                TeamApiMethod::ResolveApproval,
                TeamApiResource::ApprovalDetail(ApprovalId("approval-2".into())),
            ),
            &record,
        );
        assert!(
            matches!(mismatch, TeamAccessDecision::Deny { reason } if reason.contains("does not match"))
        );

        record.decision = Some(ApprovalDecision::Approved);
        let resolved = evaluate_approval_resolution(
            &policy,
            &team_request(
                "approver",
                TeamApiMethod::ResolveApproval,
                TeamApiResource::ApprovalDetail(record.id.clone()),
            ),
            &record,
        );
        assert!(
            matches!(resolved, TeamAccessDecision::Deny { reason } if reason.contains("already resolved"))
        );
    }

    #[test]
    fn in_memory_worker_queue_leases_completes_and_fails_closed() {
        let queue = InMemoryWorkerQueue::new();
        let task_a = TaskId("task-a".into());
        let task_b = TaskId("task-b".into());

        queue.enqueue("acme", task_b.clone()).unwrap();
        queue.enqueue("acme", task_a.clone()).unwrap();
        assert!(
            matches!(queue.enqueue("acme", task_a.clone()), Err(TaskFenceError::State(message)) if message.contains("already queued"))
        );

        let leased = queue.lease_next("acme", "worker-1").unwrap().unwrap();
        assert_eq!(leased.task_id, task_a);
        assert_eq!(
            leased.state,
            WorkerLeaseState::Leased {
                worker_id: "worker-1".into()
            }
        );
        assert!(
            matches!(queue.complete(&task_a, "worker-2"), Err(TaskFenceError::State(message)) if message.contains("worker-1"))
        );
        assert_eq!(
            queue.complete(&task_a, "worker-1").unwrap().state,
            WorkerLeaseState::Completed
        );
        assert!(
            matches!(queue.fail(&task_a, "worker-1", "late fail"), Err(TaskFenceError::State(message)) if message.contains("already completed"))
        );
        let failed = queue.lease_next("acme", "worker-2").unwrap().unwrap();
        assert_eq!(failed.task_id, task_b);
        assert_eq!(
            queue
                .fail(&task_b, "worker-2", "runner unavailable")
                .unwrap()
                .state,
            WorkerLeaseState::Failed {
                reason: "runner unavailable".into()
            }
        );
        assert_eq!(queue.snapshot().unwrap().len(), 2);
    }

    #[test]
    fn artifact_storage_path_must_stay_under_allowed_roots() {
        let temp = tempfile::tempdir().unwrap();
        let root = Utf8PathBuf::from_path_buf(temp.path().join("artifacts")).unwrap();
        let outside = Utf8PathBuf::from_path_buf(temp.path().join("outside")).unwrap();
        fs::create_dir_all(root.join("task-1")).unwrap();
        fs::create_dir_all(&outside).unwrap();
        let mut policy = team_policy();
        policy.allowed_artifact_roots = vec![root.clone()];

        let canonical_root = canonical_utf8(&root).unwrap();
        let allowed = evaluate_artifact_storage_path(&policy, &root.join("task-1/output.json"));
        assert!(
            matches!(allowed, TeamArtifactDecision::Allow { root: allowed_root } if allowed_root == canonical_root)
        );

        let denied = evaluate_artifact_storage_path(&policy, &outside.join("output.json"));
        assert!(
            matches!(denied, TeamArtifactDecision::Deny { reason } if reason.contains("outside allowed"))
        );

        let relative =
            evaluate_artifact_storage_path(&policy, Utf8Path::new("relative/output.json"));
        assert!(
            matches!(relative, TeamArtifactDecision::Deny { reason } if reason.contains("absolute"))
        );

        let escape = evaluate_artifact_storage_path(&policy, &root.join("../outside/output.json"));
        assert!(matches!(escape, TeamArtifactDecision::Deny { reason } if reason.contains("'..'")));
    }

    #[test]
    fn postgres_config_validates_contract_and_live_backend_is_unsupported() {
        let config =
            PostgresTeamStateConfig::new("TASKFENCE_DATABASE_URL", "taskfence_team").unwrap();
        assert_eq!(config.database_url_env, "TASKFENCE_DATABASE_URL");
        assert_eq!(config.schema, "taskfence_team");
        assert!(matches!(
            config.unsupported_live_state_error(),
            TaskFenceError::Unsupported(message) if message.contains("no live Postgres backend")
        ));

        assert!(matches!(
            PostgresTeamStateConfig::new("taskfence_database_url", "taskfence_team"),
            Err(TaskFenceError::State(message)) if message.contains("uppercase")
        ));
        assert!(matches!(
            PostgresTeamStateConfig::new("TASKFENCE_DATABASE_URL", "TaskFence"),
            Err(TaskFenceError::State(message)) if message.contains("lowercase")
        ));
    }

    #[test]
    fn team_boundary_lists_resources_and_rejects_live_server_start() {
        let temp = tempfile::tempdir().unwrap();
        let root = Utf8PathBuf::from_path_buf(temp.path().join("artifacts")).unwrap();
        fs::create_dir_all(&root).unwrap();
        let boundary = TeamServerBoundary::new(
            PostgresTeamStateConfig::new("TASKFENCE_DATABASE_URL", "taskfence_team").unwrap(),
            vec![root],
        )
        .unwrap()
        .with_audit_export_sinks(vec![AuditExportSinkConfig::new(
            AuditExportSinkKind::Siem,
            "soc-pipeline",
            "TASKFENCE_AUDIT_EXPORT_TOKEN",
        )
        .unwrap()])
        .unwrap();

        assert!(boundary
            .api_resources
            .iter()
            .any(|resource| matches!(resource, TeamApiResource::AuditExport)));
        assert_eq!(boundary.audit_export_sinks.len(), 1);
        assert_eq!(
            boundary.audit_export_sinks[0].credential_env,
            "TASKFENCE_AUDIT_EXPORT_TOKEN"
        );
        assert!(boundary
            .worker_model
            .contains("deterministic in-memory lease"));
        assert!(matches!(
            boundary.unsupported_start_error(),
            TaskFenceError::Unsupported(message) if message.contains("contract-only")
        ));
        assert!(matches!(
            boundary.unsupported_audit_export_error(),
            TaskFenceError::Unsupported(message) if message.contains("no live export sink")
        ));
        assert!(matches!(
            AuditExportSinkConfig::new(
                AuditExportSinkKind::Webhook,
                "https://token=secret@example.invalid",
                "TASKFENCE_WEBHOOK_TOKEN",
            ),
            Err(TaskFenceError::State(message)) if message.contains("non-secret")
        ));
        assert!(matches!(
            AuditExportSinkConfig::new(
                AuditExportSinkKind::ObjectStorage,
                "archive-bucket",
                "taskfence_audit_token",
            ),
            Err(TaskFenceError::State(message)) if message.contains("uppercase")
        ));
        assert!(matches!(
            TeamServerBoundary::new(
                PostgresTeamStateConfig::new("TASKFENCE_DATABASE_URL", "taskfence_team").unwrap(),
                Vec::new(),
            ),
            Err(TaskFenceError::State(message)) if message.contains("at least one")
        ));
    }

    #[test]
    fn reads_logs_from_workspace_task_directory() {
        let temp = tempfile::tempdir().unwrap();
        let workspace = Utf8PathBuf::from_path_buf(temp.path().join("repo")).unwrap();
        let task_dir = workspace.join(".taskfence/tasks/task-1");
        fs::create_dir_all(&task_dir).unwrap();
        fs::write(task_dir.join("stdout.log"), "hello\n").unwrap();
        fs::write(task_dir.join("stderr.log"), "warning\n").unwrap();
        let store = LocalTaskEvidenceStore::new(workspace);

        let logs = store.read_logs(&TaskId("task-1".into())).unwrap();

        assert_eq!(logs.entries.len(), 2);
        assert_eq!(logs.entries[0].stream, LogStream::Stdout);
        assert_eq!(logs.entries[0].contents, "hello\n");
        assert_eq!(logs.entries[1].stream, LogStream::Stderr);
        assert_eq!(logs.entries[1].contents, "warning\n");
    }

    #[test]
    fn reads_report_from_workspace_task_directory() {
        let temp = tempfile::tempdir().unwrap();
        let workspace = Utf8PathBuf::from_path_buf(temp.path().join("repo")).unwrap();
        let task_dir = workspace.join(".taskfence/tasks/task-1");
        fs::create_dir_all(&task_dir).unwrap();
        fs::write(task_dir.join("report.md"), "# Report\n").unwrap();
        let store = LocalTaskEvidenceStore::new(workspace);

        let report = store.read_report(&TaskId("task-1".into())).unwrap();

        assert_eq!(report.contents, "# Report\n");
        assert!(report.path.ends_with("report.md"));
    }

    #[test]
    fn reads_diff_from_workspace_task_directory() {
        let temp = tempfile::tempdir().unwrap();
        let workspace = Utf8PathBuf::from_path_buf(temp.path().join("repo")).unwrap();
        let task_dir = workspace.join(".taskfence/tasks/task-1");
        fs::create_dir_all(&task_dir).unwrap();
        fs::write(task_dir.join("diff.patch"), "TaskFence diff metadata\n").unwrap();
        let store = LocalTaskEvidenceStore::new(workspace);

        let diff = store.read_diff(&TaskId("task-1".into())).unwrap();

        assert_eq!(diff.contents, "TaskFence diff metadata\n");
        assert!(diff.path.ends_with("diff.patch"));
    }

    #[test]
    fn reads_events_from_workspace_task_directory() {
        let temp = tempfile::tempdir().unwrap();
        let workspace = Utf8PathBuf::from_path_buf(temp.path().join("repo")).unwrap();
        write_events(
            &workspace,
            "task-1",
            &[
                AuditEvent::TaskCreated {
                    task_id: TaskId("task-1".into()),
                    at: datetime!(2024-01-01 00:00 UTC),
                    goal: "inspect events".into(),
                },
                AuditEvent::TaskStatusChanged {
                    task_id: TaskId("task-1".into()),
                    at: datetime!(2024-01-01 00:01 UTC),
                    status: TaskStatus::Succeeded,
                },
            ],
        );
        let store = LocalTaskEvidenceStore::new(workspace);

        let events = store.read_events(&TaskId("task-1".into())).unwrap();

        assert_eq!(events.events.len(), 2);
        assert!(events.path.ends_with("events.jsonl"));
        assert!(events.task_dir.ends_with(".taskfence/tasks/task-1"));
        assert!(matches!(
            &events.events[0],
            AuditEvent::TaskCreated { goal, .. } if goal == "inspect events"
        ));
        assert!(matches!(
            &events.events[1],
            AuditEvent::TaskStatusChanged {
                status: TaskStatus::Succeeded,
                ..
            }
        ));
    }

    #[test]
    fn read_events_rejects_missing_event_file() {
        let temp = tempfile::tempdir().unwrap();
        let workspace = Utf8PathBuf::from_path_buf(temp.path().join("repo")).unwrap();
        fs::create_dir_all(workspace.join(".taskfence/tasks/task-1")).unwrap();
        let store = LocalTaskEvidenceStore::new(workspace);

        let err = store.read_events(&TaskId("task-1".into())).unwrap_err();

        assert!(
            matches!(err, TaskFenceError::State(message) if message.contains("events artifact not found"))
        );
    }

    #[test]
    fn read_events_rejects_malformed_jsonl() {
        let temp = tempfile::tempdir().unwrap();
        let workspace = Utf8PathBuf::from_path_buf(temp.path().join("repo")).unwrap();
        let task_dir = workspace.join(".taskfence/tasks/task-1");
        fs::create_dir_all(&task_dir).unwrap();
        fs::write(task_dir.join("events.jsonl"), "{not-json\n").unwrap();
        let store = LocalTaskEvidenceStore::new(workspace);

        let err = store.read_events(&TaskId("task-1".into())).unwrap_err();

        assert!(
            matches!(err, TaskFenceError::State(message) if message.contains("failed to parse") && message.contains("line 1"))
        );
    }

    #[test]
    fn read_events_rejects_mismatched_task_ids() {
        let temp = tempfile::tempdir().unwrap();
        let workspace = Utf8PathBuf::from_path_buf(temp.path().join("repo")).unwrap();
        write_events(
            &workspace,
            "task-1",
            &[AuditEvent::TaskStatusChanged {
                task_id: TaskId("other-task".into()),
                at: datetime!(2024-01-01 00:00 UTC),
                status: TaskStatus::Succeeded,
            }],
        );
        let store = LocalTaskEvidenceStore::new(workspace);

        let err = store.read_events(&TaskId("task-1".into())).unwrap_err();

        assert!(
            matches!(err, TaskFenceError::State(message) if message.contains("belongs to task other-task instead of task-1"))
        );
    }

    #[test]
    fn missing_task_directory_returns_state_error() {
        let temp = tempfile::tempdir().unwrap();
        let workspace = Utf8PathBuf::from_path_buf(temp.path().join("repo")).unwrap();
        let store = LocalTaskEvidenceStore::new(workspace);

        let err = store.read_report(&TaskId("missing".into())).unwrap_err();

        assert!(
            matches!(err, TaskFenceError::State(message) if message.contains("directory not found"))
        );
    }

    #[test]
    fn missing_logs_returns_state_error() {
        let temp = tempfile::tempdir().unwrap();
        let workspace = Utf8PathBuf::from_path_buf(temp.path().join("repo")).unwrap();
        fs::create_dir_all(workspace.join(".taskfence/tasks/task-1")).unwrap();
        let store = LocalTaskEvidenceStore::new(workspace);

        let err = store.read_logs(&TaskId("task-1".into())).unwrap_err();

        assert!(
            matches!(err, TaskFenceError::State(message) if message.contains("no captured stdout or stderr logs"))
        );
    }

    #[test]
    fn missing_diff_returns_state_error() {
        let temp = tempfile::tempdir().unwrap();
        let workspace = Utf8PathBuf::from_path_buf(temp.path().join("repo")).unwrap();
        fs::create_dir_all(workspace.join(".taskfence/tasks/task-1")).unwrap();
        let store = LocalTaskEvidenceStore::new(workspace);

        let err = store.read_diff(&TaskId("task-1".into())).unwrap_err();

        assert!(
            matches!(err, TaskFenceError::State(message) if message.contains("diff artifact not found"))
        );
    }

    #[test]
    fn rejects_unsafe_task_ids_before_reading() {
        let store = LocalTaskEvidenceStore::new("/tmp/taskfence-state-test");

        let err = store.read_logs(&TaskId("../escape".into())).unwrap_err();

        assert!(
            matches!(err, TaskFenceError::State(message) if message.contains("safe artifact path component"))
        );
    }

    #[test]
    fn list_tasks_returns_empty_when_workspace_has_no_task_root() {
        let temp = tempfile::tempdir().unwrap();
        let workspace = Utf8PathBuf::from_path_buf(temp.path().join("repo")).unwrap();
        let store = LocalTaskEvidenceStore::new(workspace);

        let tasks = store.list_tasks().unwrap();

        assert!(tasks.is_empty());
    }

    #[test]
    fn list_tasks_reads_structured_task_summaries() {
        let temp = tempfile::tempdir().unwrap();
        let workspace = Utf8PathBuf::from_path_buf(temp.path().join("repo")).unwrap();
        write_task_evidence(
            &workspace,
            "task-b",
            "second task",
            &[TaskStatus::Running, TaskStatus::Succeeded],
            &["report.md", "diff.patch"],
        );
        write_task_evidence(
            &workspace,
            "task-a",
            "first task",
            &[TaskStatus::Denied],
            &["stdout.log", "stderr.log"],
        );
        fs::write(workspace.join(".taskfence/tasks/ignore.txt"), "not a task").unwrap();
        let store = LocalTaskEvidenceStore::new(workspace);

        let tasks = store.list_tasks().unwrap();

        assert_eq!(tasks.len(), 2);
        assert_eq!(tasks[0].task_id, TaskId("task-a".into()));
        assert_eq!(tasks[0].goal.as_deref(), Some("first task"));
        assert_eq!(tasks[0].status, Some(TaskStatus::Denied));
        assert!(tasks[0].has_stdout);
        assert!(tasks[0].has_stderr);
        assert!(!tasks[0].has_report);
        assert_eq!(tasks[1].task_id, TaskId("task-b".into()));
        assert_eq!(tasks[1].goal.as_deref(), Some("second task"));
        assert_eq!(tasks[1].status, Some(TaskStatus::Succeeded));
        assert!(tasks[1].has_report);
        assert!(tasks[1].has_diff);
        assert!(tasks[1].warnings.is_empty());
    }

    #[test]
    fn review_index_uses_file_backed_task_summaries() {
        let temp = tempfile::tempdir().unwrap();
        let workspace = Utf8PathBuf::from_path_buf(temp.path().join("repo")).unwrap();
        write_task_evidence(
            &workspace,
            "review-index",
            "review task",
            &[TaskStatus::Succeeded],
            &["report.md"],
        );
        let store = LocalTaskEvidenceStore::new(workspace.clone());

        let index = store.review_index().unwrap();

        assert_eq!(index.workspace, workspace);
        assert_eq!(index.tasks.len(), 1);
        assert_eq!(index.tasks[0].task_id, TaskId("review-index".into()));
        assert_eq!(index.tasks[0].status, Some(TaskStatus::Succeeded));
    }

    #[test]
    fn task_review_collects_available_evidence_without_report_scraping() {
        let temp = tempfile::tempdir().unwrap();
        let workspace = Utf8PathBuf::from_path_buf(temp.path().join("repo")).unwrap();
        write_task_evidence(
            &workspace,
            "review-detail",
            "inspect review",
            &[TaskStatus::Running, TaskStatus::Succeeded],
            &["report.md", "diff.patch", "stdout.log", "stderr.log"],
        );
        let store = LocalTaskEvidenceStore::new(workspace);

        let review = store
            .read_task_review(&TaskId("review-detail".into()))
            .unwrap();

        assert_eq!(review.summary.task_id, TaskId("review-detail".into()));
        assert!(review.inputs.is_some());
        assert!(review.artifacts.is_some());
        assert!(review.events.is_some());
        assert!(review.logs.is_some());
        assert!(review.diff.is_some());
        assert!(review.report.is_some());
        assert!(review.replay.can_replay);
        assert_eq!(review.replay.last_status, Some(TaskStatus::Succeeded));
        assert!(review
            .replay
            .resolved_task_path
            .as_ref()
            .is_some_and(|path| path.ends_with("task.resolved.json")));
        assert!(review.warnings.is_empty());
    }

    #[test]
    fn task_review_keeps_missing_optional_evidence_as_warnings() {
        let temp = tempfile::tempdir().unwrap();
        let workspace = Utf8PathBuf::from_path_buf(temp.path().join("repo")).unwrap();
        write_task_evidence(
            &workspace,
            "review-partial",
            "partial review",
            &[TaskStatus::Denied],
            &[],
        );
        let store = LocalTaskEvidenceStore::new(workspace);

        let review = store
            .read_task_review(&TaskId("review-partial".into()))
            .unwrap();

        assert!(review.inputs.is_some());
        assert!(review.events.is_some());
        assert!(review.report.is_none());
        assert!(review.diff.is_none());
        assert!(review.logs.is_none());
        assert!(review
            .warnings
            .iter()
            .any(|warning| warning.contains("report not found")));
        assert!(review
            .warnings
            .iter()
            .any(|warning| warning.contains("diff artifact not found")));
        assert!(review
            .warnings
            .iter()
            .any(|warning| warning.contains("no captured stdout or stderr logs")));
    }

    #[test]
    fn replay_plan_blocks_when_resolved_inputs_are_missing() {
        let temp = tempfile::tempdir().unwrap();
        let workspace = Utf8PathBuf::from_path_buf(temp.path().join("repo")).unwrap();
        let task_dir = workspace.join(".taskfence/tasks/replay-missing");
        fs::create_dir_all(&task_dir).unwrap();
        write_events(
            &workspace,
            "replay-missing",
            &[AuditEvent::TaskStatusChanged {
                task_id: TaskId("replay-missing".into()),
                at: datetime!(2024-01-01 00:00 UTC),
                status: TaskStatus::Failed,
            }],
        );
        let store = LocalTaskEvidenceStore::new(workspace);

        let plan = store.replay_plan(&TaskId("replay-missing".into())).unwrap();

        assert!(!plan.can_replay);
        assert!(!plan.deterministic);
        assert_eq!(plan.last_status, Some(TaskStatus::Failed));
        assert!(plan
            .blockers
            .iter()
            .any(|blocker| blocker.contains("resolved task input not found")));
        assert!(plan.event_log_path.is_some());
    }

    #[test]
    fn replay_plan_records_gateway_and_runner_limitations() {
        let temp = tempfile::tempdir().unwrap();
        let workspace = Utf8PathBuf::from_path_buf(temp.path().join("repo")).unwrap();
        let task_dir = workspace.join(".taskfence/tasks/replay-gateway");
        fs::create_dir_all(&task_dir).unwrap();
        let mut task = test_task("replay-gateway", &workspace, "replay gateway");
        task.gateway.tools = vec![taskfence_core::GatewayToolConfig {
            protocol: "mcp".into(),
            tool: "github".into(),
            operation: "read_issue".into(),
            connector: taskfence_core::GatewayConnectorConfig::Unsupported {
                kind: "test".into(),
            },
            secret_refs: Vec::new(),
        }];
        fs::write(
            task_dir.join("task.resolved.json"),
            serde_json::to_string_pretty(&task).unwrap(),
        )
        .unwrap();
        write_events(
            &workspace,
            "replay-gateway",
            &[AuditEvent::TaskStatusChanged {
                task_id: TaskId("replay-gateway".into()),
                at: datetime!(2024-01-01 00:00 UTC),
                status: TaskStatus::Succeeded,
            }],
        );
        let store = LocalTaskEvidenceStore::new(workspace);

        let plan = store.replay_plan(&TaskId("replay-gateway".into())).unwrap();

        assert!(plan.can_replay);
        assert!(!plan.deterministic);
        assert!(plan
            .limitations
            .iter()
            .any(|limitation| limitation.contains("gateway calls require fresh mediation")));
        assert!(plan
            .limitations
            .iter()
            .any(|limitation| limitation.contains("runner image availability")));
    }

    #[test]
    fn replay_plan_preserves_denied_and_timeout_statuses() {
        let temp = tempfile::tempdir().unwrap();
        let workspace = Utf8PathBuf::from_path_buf(temp.path().join("repo")).unwrap();
        write_task_evidence(
            &workspace,
            "replay-denied",
            "denied replay",
            &[TaskStatus::Denied],
            &[],
        );
        write_task_evidence(
            &workspace,
            "replay-timeout",
            "timeout replay",
            &[TaskStatus::TimedOut],
            &[],
        );
        let store = LocalTaskEvidenceStore::new(workspace);

        let denied = store.replay_plan(&TaskId("replay-denied".into())).unwrap();
        let timeout = store.replay_plan(&TaskId("replay-timeout".into())).unwrap();

        assert_eq!(denied.last_status, Some(TaskStatus::Denied));
        assert!(denied.can_replay);
        assert_eq!(timeout.last_status, Some(TaskStatus::TimedOut));
        assert!(timeout.can_replay);
    }

    #[test]
    fn replay_plan_notes_approval_paths() {
        let temp = tempfile::tempdir().unwrap();
        let workspace = Utf8PathBuf::from_path_buf(temp.path().join("repo")).unwrap();
        let task_dir = workspace.join(".taskfence/tasks/replay-approval");
        fs::create_dir_all(&task_dir).unwrap();
        let mut task = test_task("replay-approval", &workspace, "approval replay");
        task.permissions.commands.approval_required = vec!["echo".into()];
        fs::write(
            task_dir.join("task.resolved.json"),
            serde_json::to_string_pretty(&task).unwrap(),
        )
        .unwrap();
        write_events(
            &workspace,
            "replay-approval",
            &[AuditEvent::TaskStatusChanged {
                task_id: TaskId("replay-approval".into()),
                at: datetime!(2024-01-01 00:00 UTC),
                status: TaskStatus::Denied,
            }],
        );
        let store = LocalTaskEvidenceStore::new(workspace);

        let plan = store
            .replay_plan(&TaskId("replay-approval".into()))
            .unwrap();

        assert!(plan.can_replay);
        assert!(plan
            .limitations
            .iter()
            .any(|limitation| limitation.contains("approval decisions must be replayed")));
    }

    #[test]
    fn migration_plan_uses_structured_local_state_not_rendered_reports() {
        let temp = tempfile::tempdir().unwrap();
        let workspace = Utf8PathBuf::from_path_buf(temp.path().join("repo")).unwrap();
        write_task_evidence(
            &workspace,
            "structured-task",
            "structured migration",
            &[TaskStatus::Succeeded],
            &["report.md"],
        );
        let report_only_dir = workspace.join(".taskfence/tasks/report-only");
        fs::create_dir_all(&report_only_dir).unwrap();
        fs::write(report_only_dir.join("report.md"), "# Rendered only\n").unwrap();
        let store = LocalTaskEvidenceStore::new(workspace.clone());

        let plan = store.migration_plan(" acme ").unwrap();

        assert_eq!(plan.workspace, workspace);
        assert_eq!(plan.organization, "acme");
        assert_eq!(plan.tasks, vec![TaskId("structured-task".into())]);
        assert!(plan
            .approval_records_source
            .ends_with(".taskfence/approvals"));
        assert_eq!(
            plan.artifact_roots,
            vec![report_only_dir.with_file_name("structured-task")]
        );
        assert!(plan
            .warnings
            .iter()
            .any(|warning| warning.contains("report-only") && warning.contains("ignored")));
        assert!(plan
            .warnings
            .iter()
            .any(|warning| warning.contains("rendered Markdown reports")));
    }

    #[test]
    fn reads_single_structured_task_summary() {
        let temp = tempfile::tempdir().unwrap();
        let workspace = Utf8PathBuf::from_path_buf(temp.path().join("repo")).unwrap();
        write_task_evidence(
            &workspace,
            "task-detail",
            "inspect task",
            &[TaskStatus::Running, TaskStatus::Succeeded],
            &["report.md", "diff.patch", "stdout.log"],
        );
        let store = LocalTaskEvidenceStore::new(workspace);

        let task = store
            .read_task_summary(&TaskId("task-detail".into()))
            .unwrap();

        assert_eq!(task.task_id, TaskId("task-detail".into()));
        assert_eq!(task.goal.as_deref(), Some("inspect task"));
        assert_eq!(task.status, Some(TaskStatus::Succeeded));
        assert!(task.has_report);
        assert!(task.has_diff);
        assert!(task.has_stdout);
        assert!(!task.has_stderr);
        assert!(task.task_dir.ends_with(".taskfence/tasks/task-detail"));
        assert!(task.warnings.is_empty());
    }

    #[test]
    fn reads_resolved_task_inputs_from_workspace_task_directory() {
        let temp = tempfile::tempdir().unwrap();
        let workspace = Utf8PathBuf::from_path_buf(temp.path().join("repo")).unwrap();
        write_task_evidence(
            &workspace,
            "task-inputs",
            "inspect inputs",
            &[TaskStatus::Succeeded],
            &[],
        );
        let store = LocalTaskEvidenceStore::new(workspace);

        let inputs = store.read_inputs(&TaskId("task-inputs".into())).unwrap();

        assert_eq!(inputs.task.id, TaskId("task-inputs".into()));
        assert_eq!(inputs.task.goal, "inspect inputs");
        assert!(inputs.path.ends_with("task.resolved.json"));
        assert!(inputs.task_dir.ends_with(".taskfence/tasks/task-inputs"));
        assert!(inputs.contents.contains("\"id\""));
        assert!(inputs.contents.contains("task-inputs"));
    }

    #[test]
    fn reads_task_artifact_manifest_from_workspace_task_directory() {
        let temp = tempfile::tempdir().unwrap();
        let workspace = Utf8PathBuf::from_path_buf(temp.path().join("repo")).unwrap();
        write_task_evidence(
            &workspace,
            "task-artifacts",
            "inspect artifacts",
            &[TaskStatus::Succeeded],
            &["report.md", "diff.patch", "stdout.log"],
        );
        let task_dir = workspace.join(".taskfence/tasks/task-artifacts");
        let artifacts_dir = task_dir.join("artifacts");
        fs::create_dir_all(&artifacts_dir).unwrap();
        fs::write(artifacts_dir.join("metadata.json"), "{}\n").unwrap();
        fs::create_dir(artifacts_dir.join("nested")).unwrap();
        fs::write(task_dir.join("notes.tmp"), "ignored\n").unwrap();
        let store = LocalTaskEvidenceStore::new(workspace);

        let artifacts = store
            .read_artifacts(&TaskId("task-artifacts".into()))
            .unwrap();
        let paths = artifacts
            .files
            .iter()
            .map(|file| file.relative_path.as_str())
            .collect::<Vec<_>>();

        assert!(artifacts
            .task_dir
            .ends_with(".taskfence/tasks/task-artifacts"));
        assert_eq!(
            paths,
            vec![
                "artifacts/metadata.json",
                "diff.patch",
                "events.jsonl",
                "report.md",
                "stdout.log",
                "task.resolved.json",
            ]
        );
        let custom = artifacts
            .files
            .iter()
            .find(|file| file.relative_path == "artifacts/metadata.json")
            .unwrap();
        assert_eq!(custom.kind, TaskArtifactKind::Artifact);
        assert_eq!(custom.size_bytes, 3);
        let evidence = artifacts
            .files
            .iter()
            .find(|file| file.relative_path == "task.resolved.json")
            .unwrap();
        assert_eq!(evidence.kind, TaskArtifactKind::Evidence);
        assert!(evidence.size_bytes > 0);
        assert!(!paths.contains(&"notes.tmp"));
        assert!(!paths.contains(&"artifacts/nested"));
    }

    #[test]
    fn read_artifacts_rejects_missing_task_directory() {
        let temp = tempfile::tempdir().unwrap();
        let workspace = Utf8PathBuf::from_path_buf(temp.path().join("repo")).unwrap();
        let store = LocalTaskEvidenceStore::new(workspace);

        let err = store.read_artifacts(&TaskId("missing".into())).unwrap_err();

        assert!(
            matches!(err, TaskFenceError::State(message) if message.contains("directory not found"))
        );
    }

    #[test]
    fn read_artifacts_rejects_unsafe_task_ids_before_reading() {
        let store = LocalTaskEvidenceStore::new("/tmp/taskfence-state-test");

        let err = store
            .read_artifacts(&TaskId("../escape".into()))
            .unwrap_err();

        assert!(
            matches!(err, TaskFenceError::State(message) if message.contains("safe artifact path component"))
        );
    }

    #[test]
    fn read_artifacts_rejects_unsafe_custom_artifact_names() {
        let temp = tempfile::tempdir().unwrap();
        let workspace = Utf8PathBuf::from_path_buf(temp.path().join("repo")).unwrap();
        let artifacts_dir = workspace.join(".taskfence/tasks/task-1/artifacts");
        fs::create_dir_all(&artifacts_dir).unwrap();
        fs::write(artifacts_dir.join("bad\nname"), "bad\n").unwrap();
        let store = LocalTaskEvidenceStore::new(workspace);

        let err = store.read_artifacts(&TaskId("task-1".into())).unwrap_err();

        assert!(
            matches!(err, TaskFenceError::State(message) if message.contains("safe path component"))
        );
    }

    #[test]
    fn read_artifacts_rejects_non_directory_artifacts_path() {
        let temp = tempfile::tempdir().unwrap();
        let workspace = Utf8PathBuf::from_path_buf(temp.path().join("repo")).unwrap();
        let task_dir = workspace.join(".taskfence/tasks/task-1");
        fs::create_dir_all(&task_dir).unwrap();
        fs::write(task_dir.join("artifacts"), "not a directory\n").unwrap();
        let store = LocalTaskEvidenceStore::new(workspace);

        let err = store.read_artifacts(&TaskId("task-1".into())).unwrap_err();

        assert!(
            matches!(err, TaskFenceError::State(message) if message.contains("not a directory"))
        );
    }

    #[cfg(unix)]
    #[test]
    fn read_artifacts_does_not_follow_symlinked_custom_entries() {
        let temp = tempfile::tempdir().unwrap();
        let workspace = Utf8PathBuf::from_path_buf(temp.path().join("repo")).unwrap();
        let artifacts_dir = workspace.join(".taskfence/tasks/task-1/artifacts");
        fs::create_dir_all(&artifacts_dir).unwrap();
        fs::write(artifacts_dir.join("actual.txt"), "ok\n").unwrap();
        std::os::unix::fs::symlink("actual.txt", artifacts_dir.join("linked.txt")).unwrap();
        let store = LocalTaskEvidenceStore::new(workspace);

        let artifacts = store.read_artifacts(&TaskId("task-1".into())).unwrap();
        let paths = artifacts
            .files
            .iter()
            .map(|file| file.relative_path.as_str())
            .collect::<Vec<_>>();

        assert!(paths.contains(&"artifacts/actual.txt"));
        assert!(!paths.contains(&"artifacts/linked.txt"));
    }

    #[test]
    fn read_inputs_rejects_missing_resolved_task_file() {
        let temp = tempfile::tempdir().unwrap();
        let workspace = Utf8PathBuf::from_path_buf(temp.path().join("repo")).unwrap();
        fs::create_dir_all(workspace.join(".taskfence/tasks/task-1")).unwrap();
        let store = LocalTaskEvidenceStore::new(workspace);

        let err = store.read_inputs(&TaskId("task-1".into())).unwrap_err();

        assert!(
            matches!(err, TaskFenceError::State(message) if message.contains("resolved task input not found"))
        );
    }

    #[test]
    fn read_inputs_rejects_malformed_resolved_task_file() {
        let temp = tempfile::tempdir().unwrap();
        let workspace = Utf8PathBuf::from_path_buf(temp.path().join("repo")).unwrap();
        let task_dir = workspace.join(".taskfence/tasks/task-1");
        fs::create_dir_all(&task_dir).unwrap();
        fs::write(task_dir.join("task.resolved.json"), "{not-json").unwrap();
        let store = LocalTaskEvidenceStore::new(workspace);

        let err = store.read_inputs(&TaskId("task-1".into())).unwrap_err();

        assert!(
            matches!(err, TaskFenceError::State(message) if message.contains("failed to parse resolved task input"))
        );
    }

    #[test]
    fn read_inputs_rejects_mismatched_task_id() {
        let temp = tempfile::tempdir().unwrap();
        let workspace = Utf8PathBuf::from_path_buf(temp.path().join("repo")).unwrap();
        let task_dir = workspace.join(".taskfence/tasks/task-1");
        fs::create_dir_all(&task_dir).unwrap();
        let task = test_task("other-task", &workspace, "wrong inputs");
        fs::write(
            task_dir.join("task.resolved.json"),
            serde_json::to_string_pretty(&task).unwrap(),
        )
        .unwrap();
        let store = LocalTaskEvidenceStore::new(workspace);

        let err = store.read_inputs(&TaskId("task-1".into())).unwrap_err();

        assert!(
            matches!(err, TaskFenceError::State(message) if message.contains("other-task") && message.contains("requested task-1"))
        );
    }

    #[test]
    fn read_inputs_rejects_unsafe_task_ids_before_reading() {
        let store = LocalTaskEvidenceStore::new("/tmp/taskfence-state-test");

        let err = store.read_inputs(&TaskId("../escape".into())).unwrap_err();

        assert!(
            matches!(err, TaskFenceError::State(message) if message.contains("safe artifact path component"))
        );
    }

    #[test]
    fn read_single_task_summary_keeps_malformed_evidence_warnings() {
        let temp = tempfile::tempdir().unwrap();
        let workspace = Utf8PathBuf::from_path_buf(temp.path().join("repo")).unwrap();
        let task_dir = workspace.join(".taskfence/tasks/task-broken");
        fs::create_dir_all(&task_dir).unwrap();
        fs::write(task_dir.join("task.resolved.json"), "{not-json").unwrap();
        fs::write(task_dir.join("events.jsonl"), "also-not-json\n").unwrap();
        let store = LocalTaskEvidenceStore::new(workspace);

        let task = store
            .read_task_summary(&TaskId("task-broken".into()))
            .unwrap();

        assert_eq!(task.task_id, TaskId("task-broken".into()));
        assert_eq!(task.goal, None);
        assert_eq!(task.status, None);
        assert!(task
            .warnings
            .iter()
            .any(|warning| warning.contains("task.resolved.json")));
        assert!(task
            .warnings
            .iter()
            .any(|warning| warning.contains("events.jsonl line 1")));
    }

    #[test]
    fn read_single_task_summary_rejects_missing_task_directory() {
        let temp = tempfile::tempdir().unwrap();
        let workspace = Utf8PathBuf::from_path_buf(temp.path().join("repo")).unwrap();
        let store = LocalTaskEvidenceStore::new(workspace);

        let err = store
            .read_task_summary(&TaskId("missing".into()))
            .unwrap_err();

        assert!(
            matches!(err, TaskFenceError::State(message) if message.contains("directory not found"))
        );
    }

    #[test]
    fn list_tasks_keeps_summary_with_malformed_evidence_warnings() {
        let temp = tempfile::tempdir().unwrap();
        let workspace = Utf8PathBuf::from_path_buf(temp.path().join("repo")).unwrap();
        let task_dir = workspace.join(".taskfence/tasks/task-broken");
        fs::create_dir_all(&task_dir).unwrap();
        fs::write(task_dir.join("task.resolved.json"), "{not-json").unwrap();
        fs::write(task_dir.join("events.jsonl"), "also-not-json\n").unwrap();
        let store = LocalTaskEvidenceStore::new(workspace);

        let tasks = store.list_tasks().unwrap();

        assert_eq!(tasks.len(), 1);
        assert_eq!(tasks[0].task_id, TaskId("task-broken".into()));
        assert_eq!(tasks[0].goal, None);
        assert_eq!(tasks[0].status, None);
        assert!(tasks[0]
            .warnings
            .iter()
            .any(|warning| warning.contains("task.resolved.json")));
        assert!(tasks[0]
            .warnings
            .iter()
            .any(|warning| warning.contains("events.jsonl line 1")));
    }

    #[test]
    fn list_tasks_rejects_unsafe_task_directory_names() {
        let temp = tempfile::tempdir().unwrap();
        let workspace = Utf8PathBuf::from_path_buf(temp.path().join("repo")).unwrap();
        fs::create_dir_all(workspace.join(".taskfence/tasks/bad\nid")).unwrap();
        let store = LocalTaskEvidenceStore::new(workspace);

        let err = store.list_tasks().unwrap_err();

        assert!(
            matches!(err, TaskFenceError::State(message) if message.contains("safe artifact path component"))
        );
    }

    fn write_task_evidence(
        workspace: &Utf8PathBuf,
        task_id: &str,
        goal: &str,
        statuses: &[TaskStatus],
        files: &[&str],
    ) {
        let task_dir = workspace.join(".taskfence/tasks").join(task_id);
        fs::create_dir_all(&task_dir).unwrap();
        let task = test_task(task_id, workspace, goal);
        fs::write(
            task_dir.join("task.resolved.json"),
            serde_json::to_string_pretty(&task).unwrap(),
        )
        .unwrap();
        let mut events = String::new();
        for status in statuses {
            let event = AuditEvent::TaskStatusChanged {
                task_id: TaskId(task_id.into()),
                at: datetime!(2024-01-01 00:00 UTC),
                status: status.clone(),
            };
            events.push_str(&serde_json::to_string(&event).unwrap());
            events.push('\n');
        }
        fs::write(task_dir.join("events.jsonl"), events).unwrap();
        for file in files {
            fs::write(task_dir.join(file), "artifact\n").unwrap();
        }
    }

    fn write_events(workspace: &Utf8PathBuf, task_id: &str, events: &[AuditEvent]) {
        let task_dir = workspace.join(".taskfence/tasks").join(task_id);
        fs::create_dir_all(&task_dir).unwrap();
        let mut contents = String::new();
        for event in events {
            contents.push_str(&serde_json::to_string(event).unwrap());
            contents.push('\n');
        }
        fs::write(task_dir.join("events.jsonl"), contents).unwrap();
    }

    fn team_policy() -> OrganizationPolicy {
        OrganizationPolicy {
            organization: "acme".into(),
            grants: vec![
                RbacGrant {
                    actor: "viewer".into(),
                    role: TeamRole::Viewer,
                },
                RbacGrant {
                    actor: "approver".into(),
                    role: TeamRole::Approver,
                },
                RbacGrant {
                    actor: "operator".into(),
                    role: TeamRole::Operator,
                },
                RbacGrant {
                    actor: "auditor".into(),
                    role: TeamRole::Auditor,
                },
                RbacGrant {
                    actor: "admin".into(),
                    role: TeamRole::Admin,
                },
            ],
            require_approval_owner: false,
            allowed_artifact_roots: Vec::new(),
        }
    }

    fn team_request(
        actor: &str,
        method: TeamApiMethod,
        resource: TeamApiResource,
    ) -> TeamApiRequest {
        TeamApiRequest {
            organization: "acme".into(),
            actor: actor.into(),
            method,
            resource,
        }
    }

    fn approval_record(id: &str, actor: &str) -> ApprovalRecord {
        ApprovalRecord {
            id: ApprovalId(id.into()),
            task_id: TaskId("task-approval".into()),
            actor: actor.into(),
            source: Some("team-api".into()),
            requested_at: datetime!(2024-01-01 00:00 UTC),
            resolved_at: None,
            action: Action::Budget {
                kind: "gateway_calls".into(),
                amount: 1,
            },
            policy_decision: ActionDecision::RequireApproval {
                approval_kind: "budget".into(),
                rule_id: Some("approval-owner-test".into()),
                reason: "test approval ownership".into(),
                risk: RiskLevel::Medium,
            },
            decision: None,
        }
    }

    fn test_task(task_id: &str, workspace: &Utf8PathBuf, goal: &str) -> ResolvedTask {
        ResolvedTask {
            id: TaskId(task_id.into()),
            task_file: workspace.join("task.yaml"),
            goal: goal.into(),
            workspace_host_path: workspace.clone(),
            workspace_container_path: "/workspace".into(),
            agent: AgentConfig {
                kind: AgentKind::Generic,
                command: "echo".into(),
                args: vec!["ok".into()],
            },
            sandbox: SandboxConfig {
                kind: SandboxKind::Docker,
                image: Some("taskfence/runner:latest".into()),
                limits: LimitConfig::default(),
            },
            permissions: PermissionConfig::default(),
            secrets: Default::default(),
            approval: ApprovalConfig::default(),
            gateway: Default::default(),
            audit: AuditConfig::default(),
        }
    }
}
