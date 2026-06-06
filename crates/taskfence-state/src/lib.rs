use std::collections::BTreeMap;
use std::fs;
use std::sync::Mutex;

use camino::Utf8PathBuf;
use taskfence_core::{LogStream, StateStore, TaskFenceError, TaskId, TaskStatus};

const TASKFENCE_DIR: &str = ".taskfence";
const TASKS_DIR: &str = "tasks";
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
        Ok(self
            .workspace
            .join(TASKFENCE_DIR)
            .join(TASKS_DIR)
            .join(task_id.0.as_str()))
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
}
