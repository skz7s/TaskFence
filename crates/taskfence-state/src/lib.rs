use std::collections::BTreeMap;
use std::fs::{self, File};
use std::io::{BufRead, BufReader};
use std::sync::Mutex;

use camino::{Utf8Path, Utf8PathBuf};
use taskfence_core::{
    AuditEvent, LogStream, ResolvedTask, StateStore, TaskFenceError, TaskId, TaskStatus,
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
        AgentConfig, AgentKind, ApprovalConfig, AuditConfig, LimitConfig, PermissionConfig,
        SandboxConfig, SandboxKind,
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
