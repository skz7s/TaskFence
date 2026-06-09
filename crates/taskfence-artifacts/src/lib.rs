//! Local artifact layout and contained artifact writes for TaskFence tasks.
//!
//! This crate creates workspace-local `.taskfence/tasks/<task-id>` evidence
//! directories, writes resolved task inputs and logs, prepares gateway spool
//! files, captures Git baselines, and collects diffs without treating rendered
//! reports as source-of-truth state.

use camino::{Utf8Path, Utf8PathBuf};
use std::fs::{self, File};
use std::io::Write;
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};
use taskfence_core::{
    ArtifactRefs, ArtifactStore, LogStream, ResolvedTask, Result, TaskFenceError,
    WorkspaceBaseline, GATEWAY_SPOOL_CONTAINER_PATH, GATEWAY_SPOOL_DIR_NAME,
    GATEWAY_SPOOL_REQUESTS_DIR_NAME, GATEWAY_SPOOL_RESPONSES_DIR_NAME,
    GATEWAY_SPOOL_WRAPPER_FILE_NAME,
};

const TASKS_DIR: &str = "tasks";
const ARTIFACTS_DIR: &str = "artifacts";
const RESOLVED_TASK_FILE: &str = "task.resolved.json";
const EVENTS_FILE: &str = "events.jsonl";
const STDOUT_FILE: &str = "stdout.log";
const STDERR_FILE: &str = "stderr.log";
const DIFF_FILE: &str = "diff.patch";
const REPORT_FILE: &str = "report.md";

#[derive(Clone, Debug, Default)]
pub struct LocalArtifactStore {
    root: Option<Utf8PathBuf>,
}

impl LocalArtifactStore {
    pub fn new(root: impl Into<Utf8PathBuf>) -> Self {
        Self {
            root: Some(root.into()),
        }
    }

    pub fn in_workspace() -> Self {
        Self { root: None }
    }

    pub fn task_dir(&self, task: &ResolvedTask) -> Result<Utf8PathBuf> {
        validate_task_id_component(&task.id.0)?;
        Ok(self.root_for(task).join(TASKS_DIR).join(task.id.0.as_str()))
    }

    fn root_for(&self, task: &ResolvedTask) -> Utf8PathBuf {
        self.root
            .clone()
            .unwrap_or_else(|| task.workspace_host_path.join(".taskfence"))
    }

    fn artifact_refs(&self, task: &ResolvedTask) -> Result<ArtifactRefs> {
        let task_dir = self.task_dir(task)?;
        Ok(ArtifactRefs {
            resolved_task: Some(task_dir.join(RESOLVED_TASK_FILE)),
            events: Some(task_dir.join(EVENTS_FILE)),
            stdout: Some(task_dir.join(STDOUT_FILE)),
            stderr: Some(task_dir.join(STDERR_FILE)),
            diff: Some(task_dir.join(DIFF_FILE)),
            report: Some(task_dir.join(REPORT_FILE)),
            gateway_spool: Some(task_dir.join(GATEWAY_SPOOL_DIR_NAME)),
            task_dir,
        })
    }
}

impl ArtifactStore for LocalArtifactStore {
    fn create_task_dir(&self, task: &ResolvedTask) -> Result<ArtifactRefs> {
        let refs = self.artifact_refs(task)?;
        fs::create_dir_all(refs.task_dir.join(ARTIFACTS_DIR)).map_err(artifact_io_error)?;
        if let Some(spool) = &refs.gateway_spool {
            fs::create_dir_all(spool.join(GATEWAY_SPOOL_REQUESTS_DIR_NAME))
                .map_err(artifact_io_error)?;
            fs::create_dir_all(spool.join(GATEWAY_SPOOL_RESPONSES_DIR_NAME))
                .map_err(artifact_io_error)?;
            write_gateway_wrapper(spool)?;
        }
        Ok(refs)
    }

    fn write_resolved_task(&self, task: &ResolvedTask) -> Result<Utf8PathBuf> {
        let path = self.task_dir(task)?.join(RESOLVED_TASK_FILE);
        let bytes = serde_json::to_vec_pretty(task)
            .map_err(|err| TaskFenceError::Artifact(format!("failed to serialize task: {err}")))?;
        atomic_write(&path, &bytes)?;
        Ok(path)
    }

    fn write_log(
        &self,
        task: &ResolvedTask,
        stream: LogStream,
        contents: &str,
    ) -> Result<Utf8PathBuf> {
        let file_name = match stream {
            LogStream::Stdout => STDOUT_FILE,
            LogStream::Stderr => STDERR_FILE,
        };
        let path = self.task_dir(task)?.join(file_name);
        atomic_write(&path, contents.as_bytes())?;
        Ok(path)
    }

    fn capture_baseline(&self, task: &ResolvedTask) -> Result<WorkspaceBaseline> {
        ensure_workspace_dir(&task.workspace_host_path)?;

        match git_status(task, &task.workspace_host_path) {
            GitProbe::Clean => Ok(WorkspaceBaseline {
                dirty_before_run: false,
                summary: "git status clean at baseline".into(),
            }),
            GitProbe::Dirty(status) => Ok(WorkspaceBaseline {
                dirty_before_run: true,
                summary: format!("dirty before run:\n{}", trim_large(&status)),
            }),
            GitProbe::NotGit => Ok(WorkspaceBaseline {
                dirty_before_run: true,
                summary: "workspace is not a git repository; baseline cannot prove cleanliness"
                    .into(),
            }),
            GitProbe::GitUnavailable => Ok(WorkspaceBaseline {
                dirty_before_run: true,
                summary: "git executable is unavailable; baseline cannot prove cleanliness".into(),
            }),
            GitProbe::Error(message) => Err(TaskFenceError::Artifact(message)),
        }
    }

    fn collect_diff(
        &self,
        task: &ResolvedTask,
        baseline: &WorkspaceBaseline,
    ) -> Result<Option<Utf8PathBuf>> {
        ensure_workspace_dir(&task.workspace_host_path)?;
        let diff_path = self.task_dir(task)?.join(DIFF_FILE);

        let mut contents = String::new();
        contents.push_str("TaskFence diff metadata\n");
        contents.push_str(&format!(
            "dirty_before_run: {}\n",
            baseline.dirty_before_run
        ));
        contents.push_str("baseline_summary:\n");
        for line in baseline.summary.lines() {
            contents.push_str("  ");
            contents.push_str(line);
            contents.push('\n');
        }

        if baseline.dirty_before_run {
            contents.push_str(
                "warning: workspace was dirty before the task; final diffs are not attributed exclusively to the agent\n",
            );
        }

        match is_git_repo(&task.workspace_host_path) {
            GitRepoState::Inside => {
                let final_status = match git_status(task, &task.workspace_host_path) {
                    GitProbe::Clean => String::new(),
                    GitProbe::Dirty(status) => status,
                    GitProbe::NotGit => {
                        contents.push_str(
                            "\ndiff_status: unavailable; workspace is no longer a git repository\n",
                        );
                        atomic_write(&diff_path, contents.as_bytes())?;
                        return Ok(Some(diff_path));
                    }
                    GitProbe::GitUnavailable => {
                        contents.push_str(
                            "\ndiff_status: unavailable; git executable is unavailable\n",
                        );
                        atomic_write(&diff_path, contents.as_bytes())?;
                        return Ok(Some(diff_path));
                    }
                    GitProbe::Error(message) => return Err(TaskFenceError::Artifact(message)),
                };

                contents.push_str("\nfinal_status:\n");
                if final_status.trim().is_empty() {
                    contents.push_str("  clean\n");
                } else {
                    for line in trim_large(&final_status).lines() {
                        contents.push_str("  ");
                        contents.push_str(line);
                        contents.push('\n');
                    }
                }

                let unstaged = run_git(
                    &task.workspace_host_path,
                    ["diff", "--binary", "--no-ext-diff"],
                )?;
                let staged = run_git(
                    &task.workspace_host_path,
                    ["diff", "--cached", "--binary", "--no-ext-diff"],
                )?;
                let untracked = run_git(
                    &task.workspace_host_path,
                    ["ls-files", "--others", "--exclude-standard"],
                )?;

                contents.push_str("\nunstaged_diff:\n");
                append_block_or_none(&mut contents, &unstaged);
                contents.push_str("\nstaged_diff:\n");
                append_block_or_none(&mut contents, &staged);
                contents.push_str("\nuntracked_files:\n");
                append_block_or_none(&mut contents, &filter_taskfence_status(task, &untracked));
            }
            GitRepoState::NotGit => {
                contents
                    .push_str("\ndiff_status: unavailable; workspace is not a git repository\n");
            }
            GitRepoState::GitUnavailable => {
                contents.push_str("\ndiff_status: unavailable; git executable is unavailable\n");
            }
            GitRepoState::Error(message) => return Err(TaskFenceError::Artifact(message)),
        }

        atomic_write(&diff_path, contents.as_bytes())?;
        Ok(Some(diff_path))
    }
}

#[derive(Debug, PartialEq, Eq)]
enum GitProbe {
    Clean,
    Dirty(String),
    NotGit,
    GitUnavailable,
    Error(String),
}

#[derive(Debug, PartialEq, Eq)]
enum GitRepoState {
    Inside,
    NotGit,
    GitUnavailable,
    Error(String),
}

fn ensure_workspace_dir(path: &Utf8Path) -> Result<()> {
    match fs::metadata(path.as_std_path()) {
        Ok(metadata) if metadata.is_dir() => Ok(()),
        Ok(_) => Err(TaskFenceError::Artifact(format!(
            "workspace is not a directory: {path}"
        ))),
        Err(err) => Err(TaskFenceError::Artifact(format!(
            "failed to access workspace {path}: {err}"
        ))),
    }
}

fn git_status(task: &ResolvedTask, workspace: &Utf8Path) -> GitProbe {
    match is_git_repo(workspace) {
        GitRepoState::Inside => {}
        GitRepoState::NotGit => return GitProbe::NotGit,
        GitRepoState::GitUnavailable => return GitProbe::GitUnavailable,
        GitRepoState::Error(message) => return GitProbe::Error(message),
    }

    match run_git_status(workspace) {
        Ok(status) => {
            let filtered = filter_taskfence_status(task, &status);
            if filtered.trim().is_empty() {
                GitProbe::Clean
            } else {
                GitProbe::Dirty(filtered)
            }
        }
        Err(GitRunError::Unavailable) => GitProbe::GitUnavailable,
        Err(GitRunError::Failed(message)) => GitProbe::Error(message),
    }
}

fn is_git_repo(workspace: &Utf8Path) -> GitRepoState {
    let output = Command::new("git")
        .arg("-C")
        .arg(workspace.as_std_path())
        .args(["rev-parse", "--is-inside-work-tree"])
        .output();

    match output {
        Ok(output) if output.status.success() => GitRepoState::Inside,
        Ok(output) => {
            let stderr = String::from_utf8_lossy(&output.stderr);
            if stderr.contains("not a git repository") {
                GitRepoState::NotGit
            } else {
                GitRepoState::Error(trim_large(&stderr))
            }
        }
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => GitRepoState::GitUnavailable,
        Err(err) => GitRepoState::Error(err.to_string()),
    }
}

#[derive(Debug, PartialEq, Eq)]
enum GitRunError {
    Unavailable,
    Failed(String),
}

fn run_git_status(workspace: &Utf8Path) -> std::result::Result<String, GitRunError> {
    run_git_raw(
        workspace,
        ["status", "--porcelain=v1", "--untracked-files=all"],
    )
}

fn run_git<const N: usize>(workspace: &Utf8Path, args: [&str; N]) -> Result<String> {
    run_git_raw(workspace, args).map_err(|err| match err {
        GitRunError::Unavailable => {
            TaskFenceError::Artifact("git executable is unavailable".into())
        }
        GitRunError::Failed(message) => TaskFenceError::Artifact(message),
    })
}

fn run_git_raw<const N: usize>(
    workspace: &Utf8Path,
    args: [&str; N],
) -> std::result::Result<String, GitRunError> {
    let output = Command::new("git")
        .arg("-C")
        .arg(workspace.as_std_path())
        .args(args)
        .output();

    match output {
        Ok(output) if output.status.success() => {
            Ok(String::from_utf8_lossy(&output.stdout).into_owned())
        }
        Ok(output) => Err(GitRunError::Failed(trim_large(&String::from_utf8_lossy(
            &output.stderr,
        )))),
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => Err(GitRunError::Unavailable),
        Err(err) => Err(GitRunError::Failed(err.to_string())),
    }
}

fn filter_taskfence_status(task: &ResolvedTask, status: &str) -> String {
    let root = ".taskfence/";
    let task_fragment = format!(".taskfence/tasks/{}/", task.id.0);
    status
        .lines()
        .filter(|line| {
            let trimmed = line.trim_start();
            !(trimmed.ends_with(root)
                || trimmed.contains(root)
                || trimmed.ends_with(&task_fragment)
                || trimmed.contains(&task_fragment))
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn append_block_or_none(contents: &mut String, block: &str) {
    let block = trim_large(block);
    if block.trim().is_empty() {
        contents.push_str("  none\n");
        return;
    }
    for line in block.lines() {
        contents.push_str("  ");
        contents.push_str(line);
        contents.push('\n');
    }
}

fn trim_large(input: &str) -> String {
    const MAX_BYTES: usize = 256 * 1024;
    if input.len() <= MAX_BYTES {
        return input.to_owned();
    }

    let mut end = MAX_BYTES;
    while !input.is_char_boundary(end) {
        end -= 1;
    }
    format!("{}[taskfence truncated]", &input[..end])
}

fn validate_task_id_component(value: &str) -> Result<()> {
    if value.is_empty()
        || value == "."
        || value == ".."
        || value.contains('/')
        || value.contains('\\')
        || value.chars().any(char::is_control)
    {
        return Err(TaskFenceError::Artifact(format!(
            "task id is not a safe artifact path component: {value:?}"
        )));
    }
    Ok(())
}

fn write_gateway_wrapper(spool: &Utf8Path) -> Result<()> {
    let wrapper_path = spool.join(GATEWAY_SPOOL_WRAPPER_FILE_NAME);
    let contents = format!(
        r#"#!/bin/sh
set -eu
if [ "$#" -ne 1 ]; then
  echo "usage: {wrapper} REQUEST_JSON" >&2
  exit 64
fi
request_id="$(date +%s)-$$"
request_path="{requests}/$request_id.json"
response_path="{responses}/$request_id.json"
printf '%s\n' "$1" > "$request_path"
echo "$response_path"
"#,
        wrapper = GATEWAY_SPOOL_WRAPPER_FILE_NAME,
        requests =
            Utf8PathBuf::from(GATEWAY_SPOOL_CONTAINER_PATH).join(GATEWAY_SPOOL_REQUESTS_DIR_NAME),
        responses =
            Utf8PathBuf::from(GATEWAY_SPOOL_CONTAINER_PATH).join(GATEWAY_SPOOL_RESPONSES_DIR_NAME),
    );
    atomic_write(&wrapper_path, contents.as_bytes())?;
    make_executable(&wrapper_path)
}

#[cfg(unix)]
fn make_executable(path: &Utf8Path) -> Result<()> {
    use std::os::unix::fs::PermissionsExt;

    let mut permissions = fs::metadata(path.as_std_path())
        .map_err(artifact_io_error)?
        .permissions();
    permissions.set_mode(0o755);
    fs::set_permissions(path.as_std_path(), permissions).map_err(artifact_io_error)
}

#[cfg(not(unix))]
fn make_executable(_path: &Utf8Path) -> Result<()> {
    Ok(())
}

fn atomic_write(path: &Utf8Path, bytes: &[u8]) -> Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(artifact_io_error)?;
    }

    let tmp_path = path.with_extension(format!(
        "tmp.{}.{}",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(|err| TaskFenceError::Artifact(err.to_string()))?
            .as_nanos()
    ));
    {
        let mut file = File::create(tmp_path.as_std_path()).map_err(artifact_io_error)?;
        file.write_all(bytes).map_err(artifact_io_error)?;
        file.sync_all().map_err(artifact_io_error)?;
    }
    fs::rename(tmp_path.as_std_path(), path.as_std_path()).map_err(artifact_io_error)
}

fn artifact_io_error(err: std::io::Error) -> TaskFenceError {
    TaskFenceError::Artifact(err.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use taskfence_core::{ArtifactStore, TaskId};
    use taskfence_testkit::sample_task;

    #[test]
    fn creates_expected_task_layout() {
        let temp = tempfile::tempdir().unwrap();
        let root = Utf8PathBuf::from_path_buf(temp.path().join(".taskfence")).unwrap();
        let mut task = sample_task();
        task.id = TaskId("layout-task".into());
        let store = LocalArtifactStore::new(root.clone());

        let refs = store.create_task_dir(&task).unwrap();

        assert_eq!(refs.task_dir, root.join("tasks/layout-task"));
        assert_eq!(
            refs.resolved_task,
            Some(root.join("tasks/layout-task/task.resolved.json"))
        );
        assert!(refs.task_dir.join("artifacts").is_dir());
    }

    #[test]
    fn writes_resolved_task_atomically() {
        let temp = tempfile::tempdir().unwrap();
        let root = Utf8PathBuf::from_path_buf(temp.path().join(".taskfence")).unwrap();
        let task = sample_task();
        let store = LocalArtifactStore::new(root);

        let path = store.write_resolved_task(&task).unwrap();

        let value: serde_json::Value =
            serde_json::from_str(&fs::read_to_string(path).unwrap()).unwrap();
        assert_eq!(value["goal"], "Run test task");
    }

    #[test]
    fn writes_stdout_and_stderr_logs() {
        let temp = tempfile::tempdir().unwrap();
        let root = Utf8PathBuf::from_path_buf(temp.path().join(".taskfence")).unwrap();
        let task = sample_task();
        let store = LocalArtifactStore::new(root);

        let stdout = store
            .write_log(&task, LogStream::Stdout, "hello\n")
            .unwrap();
        let stderr = store
            .write_log(&task, LogStream::Stderr, "warning\n")
            .unwrap();

        assert_eq!(fs::read_to_string(stdout).unwrap(), "hello\n");
        assert_eq!(fs::read_to_string(stderr).unwrap(), "warning\n");
    }

    #[test]
    fn rejects_unsafe_task_id_path_component() {
        let temp = tempfile::tempdir().unwrap();
        let root = Utf8PathBuf::from_path_buf(temp.path().join(".taskfence")).unwrap();
        let mut task = sample_task();
        task.id = TaskId("../escape".into());
        let store = LocalArtifactStore::new(root);

        let err = store.create_task_dir(&task).unwrap_err();
        assert!(err.to_string().contains("safe artifact path component"));
    }

    #[test]
    fn non_git_baseline_is_dirty_and_diff_is_explicitly_unavailable() {
        let temp = tempfile::tempdir().unwrap();
        let workspace = Utf8PathBuf::from_path_buf(temp.path().join("repo")).unwrap();
        fs::create_dir(&workspace).unwrap();
        let root = Utf8PathBuf::from_path_buf(temp.path().join(".taskfence")).unwrap();
        let mut task = sample_task();
        task.workspace_host_path = workspace;
        let store = LocalArtifactStore::new(root);

        let baseline = store.capture_baseline(&task).unwrap();
        let diff = store.collect_diff(&task, &baseline).unwrap().unwrap();
        let contents = fs::read_to_string(diff).unwrap();

        assert!(baseline.dirty_before_run);
        assert!(contents.contains("workspace is not a git repository"));
    }

    #[test]
    fn clean_git_baseline_and_modified_file_diff_are_captured() {
        let temp = tempfile::tempdir().unwrap();
        let workspace = Utf8PathBuf::from_path_buf(temp.path().join("repo")).unwrap();
        fs::create_dir(&workspace).unwrap();
        run_git_raw(&workspace, ["init"]).unwrap();
        run_git_raw(
            &workspace,
            ["config", "user.email", "taskfence@example.test"],
        )
        .unwrap();
        run_git_raw(&workspace, ["config", "user.name", "TaskFence Test"]).unwrap();
        fs::write(workspace.join(".gitignore"), ".taskfence/\n").unwrap();
        fs::write(workspace.join("file.txt"), "before\n").unwrap();
        run_git_raw(&workspace, ["add", "."]).unwrap();
        run_git_raw(&workspace, ["commit", "-m", "initial"]).unwrap();

        let mut task = sample_task();
        task.workspace_host_path = workspace.clone();
        let store = LocalArtifactStore::in_workspace();
        let baseline = store.capture_baseline(&task).unwrap();
        fs::write(workspace.join("file.txt"), "after\n").unwrap();
        let diff = store.collect_diff(&task, &baseline).unwrap().unwrap();
        let contents = fs::read_to_string(diff).unwrap();

        assert!(!baseline.dirty_before_run);
        assert!(contents.contains("dirty_before_run: false"));
        assert!(contents.contains("-before"));
        assert!(contents.contains("+after"));
    }
}
