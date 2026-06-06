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
const REPORT_FILE: &str = "report.md";

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
pub struct TaskSummary {
    pub task_id: TaskId,
    pub task_dir: Utf8PathBuf,
    pub status: Option<TaskStatus>,
    pub goal: Option<String>,
    pub has_report: bool,
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
            &["report.md"],
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
        assert!(tasks[1].warnings.is_empty());
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
            audit: AuditConfig::default(),
        }
    }
}
