use camino::Utf8PathBuf;
use clap::{Parser, Subcommand};
use std::process::ExitCode;
use taskfence_agent::GenericAgentAdapter;
use taskfence_approval::{LocalApprovalEngine, LocalApprovalStore, LocalExternalApprovalEngine};
use taskfence_artifacts::LocalArtifactStore;
use taskfence_audit::LocalJsonlAuditLogger;
use taskfence_config::load_task_file;
use taskfence_core::{
    ApprovalDecision, ApprovalEngine, ApprovalId, LogStream, Orchestrator, ResolvedTask, Runner,
    TaskFenceError, TaskId, TaskStatus,
};
use taskfence_policy::BuiltInPolicyEngine;
use taskfence_report::MarkdownReportGenerator;
use taskfence_runner::DockerRunner;
use taskfence_state::{InMemoryStateStore, LocalTaskEvidenceStore, TaskLogs, TaskSummary};

#[derive(Debug, Parser)]
#[command(name = "taskfence")]
#[command(about = "Secure runtime and gateway for AI agent tasks")]
#[command(version)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Debug, Subcommand)]
enum Command {
    /// Create a starter task file.
    Init {
        /// Path where the starter task file should be written.
        #[arg(default_value = "taskfence.yaml")]
        path: Utf8PathBuf,
    },
    /// Load a task file and start the task orchestration boundary.
    Run {
        /// Prompt locally for approval-required actions during this run.
        #[arg(long)]
        interactive_approval: bool,
        /// Wait for taskfence approve/deny to resolve approval-required actions.
        #[arg(long)]
        external_approval: bool,
        /// TaskFence YAML task file.
        task_file: Utf8PathBuf,
    },
    /// Show logs for a task.
    Logs {
        /// Task ID to query.
        task_id: String,
        /// Workspace that owns the .taskfence task evidence directory.
        #[arg(long, default_value = ".")]
        workspace: Utf8PathBuf,
    },
    /// Show the captured diff artifact for a task.
    Diff {
        /// Task ID to query.
        task_id: String,
        /// Workspace that owns the .taskfence task evidence directory.
        #[arg(long, default_value = ".")]
        workspace: Utf8PathBuf,
    },
    /// List locally recorded tasks in a workspace.
    Tasks {
        /// Workspace that owns the .taskfence task evidence directory.
        #[arg(long, default_value = ".")]
        workspace: Utf8PathBuf,
    },
    /// Approve a pending approval request.
    Approve {
        /// Approval ID to approve.
        approval_id: String,
        /// Workspace that owns the .taskfence approval directory.
        #[arg(long, default_value = ".")]
        workspace: Utf8PathBuf,
    },
    /// Deny a pending approval request.
    Deny {
        /// Approval ID to deny.
        approval_id: String,
        /// Workspace that owns the .taskfence approval directory.
        #[arg(long, default_value = ".")]
        workspace: Utf8PathBuf,
    },
    /// Generate or show a task report.
    Report {
        /// Task ID to report on.
        task_id: String,
        /// Workspace that owns the .taskfence task evidence directory.
        #[arg(long, default_value = ".")]
        workspace: Utf8PathBuf,
    },
}

fn main() -> ExitCode {
    match execute(Cli::parse()) {
        Ok(()) => ExitCode::SUCCESS,
        Err(err) => {
            eprintln!("taskfence: {err}");
            ExitCode::FAILURE
        }
    }
}

fn execute(cli: Cli) -> taskfence_core::Result<()> {
    match cli.command {
        Command::Init { path } => unsupported(format!(
            "init command is parsed but task-file scaffolding is not implemented yet for {path}"
        )),
        Command::Run {
            task_file,
            interactive_approval,
            external_approval,
        } => {
            if interactive_approval && external_approval {
                return Err(TaskFenceError::Config(
                    "--interactive-approval and --external-approval cannot be used together".into(),
                ));
            }
            let approval_mode = if interactive_approval {
                RunApprovalMode::Interactive
            } else if external_approval {
                RunApprovalMode::External
            } else {
                RunApprovalMode::FailClosed
            };
            let runner = DockerRunner::new();
            run_task_with_runner(task_file, &runner, approval_mode)
        }
        Command::Logs { task_id, workspace } => show_logs(workspace, task_id),
        Command::Diff { task_id, workspace } => show_diff(workspace, task_id),
        Command::Tasks { workspace } => show_tasks(workspace),
        Command::Approve {
            approval_id,
            workspace,
        } => resolve_approval(workspace, approval_id, ApprovalDecision::Approved),
        Command::Deny {
            approval_id,
            workspace,
        } => resolve_approval(workspace, approval_id, ApprovalDecision::Denied),
        Command::Report { task_id, workspace } => show_report(workspace, task_id),
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum RunApprovalMode {
    FailClosed,
    Interactive,
    External,
}

fn show_logs(workspace: Utf8PathBuf, task_id: String) -> taskfence_core::Result<()> {
    let text = logs_text(workspace, &TaskId(task_id))?;
    print!("{text}");
    Ok(())
}

fn show_report(workspace: Utf8PathBuf, task_id: String) -> taskfence_core::Result<()> {
    let text = report_text(workspace, &TaskId(task_id))?;
    print!("{text}");
    Ok(())
}

fn show_diff(workspace: Utf8PathBuf, task_id: String) -> taskfence_core::Result<()> {
    let text = diff_text(workspace, &TaskId(task_id))?;
    print!("{text}");
    Ok(())
}

fn show_tasks(workspace: Utf8PathBuf) -> taskfence_core::Result<()> {
    let text = tasks_text(workspace)?;
    print!("{text}");
    Ok(())
}

fn resolve_approval(
    workspace: Utf8PathBuf,
    approval_id: String,
    decision: ApprovalDecision,
) -> taskfence_core::Result<()> {
    let resolved = resolve_approval_record(workspace, &ApprovalId(approval_id), decision)?;
    println!("Approval resolved");
    println!("  approval: {}", resolved.id.0);
    println!("  task: {}", resolved.task_id.0);
    println!("  decision: {:?}", resolved.decision);
    Ok(())
}

fn resolve_approval_record(
    workspace: Utf8PathBuf,
    approval_id: &ApprovalId,
    decision: ApprovalDecision,
) -> taskfence_core::Result<taskfence_core::ApprovalRecord> {
    let store = LocalApprovalStore::new(workspace);
    store.resolve(approval_id, decision)
}

fn logs_text(workspace: Utf8PathBuf, task_id: &TaskId) -> taskfence_core::Result<String> {
    let store = LocalTaskEvidenceStore::new(workspace);
    let logs = store.read_logs(task_id)?;
    Ok(render_logs(&logs))
}

fn report_text(workspace: Utf8PathBuf, task_id: &TaskId) -> taskfence_core::Result<String> {
    let store = LocalTaskEvidenceStore::new(workspace);
    Ok(store.read_report(task_id)?.contents)
}

fn diff_text(workspace: Utf8PathBuf, task_id: &TaskId) -> taskfence_core::Result<String> {
    let store = LocalTaskEvidenceStore::new(workspace);
    Ok(store.read_diff(task_id)?.contents)
}

fn tasks_text(workspace: Utf8PathBuf) -> taskfence_core::Result<String> {
    let store = LocalTaskEvidenceStore::new(workspace);
    let tasks = store.list_tasks()?;
    Ok(render_task_summaries(&tasks))
}

fn render_logs(logs: &TaskLogs) -> String {
    let mut rendered = String::new();
    for entry in &logs.entries {
        let stream = match entry.stream {
            LogStream::Stdout => "stdout",
            LogStream::Stderr => "stderr",
        };
        rendered.push_str("== ");
        rendered.push_str(stream);
        rendered.push_str(": ");
        rendered.push_str(entry.path.as_str());
        rendered.push_str(" ==\n");
        rendered.push_str(&entry.contents);
        if !entry.contents.ends_with('\n') {
            rendered.push('\n');
        }
    }
    rendered
}

fn render_task_summaries(tasks: &[TaskSummary]) -> String {
    let mut rendered = String::from("TASK ID\tSTATUS\tARTIFACTS\tWARNINGS\tGOAL\n");
    for task in tasks {
        rendered.push_str(&compact_cell(&task.task_id.0));
        rendered.push('\t');
        rendered.push_str(
            task.status
                .as_ref()
                .map(|status| format!("{status:?}"))
                .as_deref()
                .unwrap_or("-"),
        );
        rendered.push('\t');
        rendered.push_str(&artifact_flags(task));
        rendered.push('\t');
        if task.warnings.is_empty() {
            rendered.push('-');
        } else {
            rendered.push_str("warnings:");
            rendered.push_str(&task.warnings.len().to_string());
        }
        rendered.push('\t');
        rendered.push_str(
            task.goal
                .as_deref()
                .map(compact_cell)
                .as_deref()
                .unwrap_or("-"),
        );
        rendered.push('\n');
    }
    rendered
}

fn artifact_flags(task: &TaskSummary) -> String {
    let mut flags = Vec::new();
    if task.has_report {
        flags.push("report");
    }
    if task.has_diff {
        flags.push("diff");
    }
    if task.has_stdout {
        flags.push("stdout");
    }
    if task.has_stderr {
        flags.push("stderr");
    }
    if flags.is_empty() {
        "-".into()
    } else {
        flags.join(",")
    }
}

fn compact_cell(value: &str) -> String {
    value.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn run_task_with_runner(
    task_file: Utf8PathBuf,
    runner: &dyn Runner,
    approval_mode: RunApprovalMode,
) -> taskfence_core::Result<()> {
    let task = load_task_file(&task_file)?;
    match approval_mode {
        RunApprovalMode::FailClosed => {
            let approval = LocalApprovalEngine::fail_closed();
            run_resolved_task_with_runner_and_approval(task, runner, &approval)
        }
        RunApprovalMode::Interactive => {
            let approval = LocalApprovalEngine::interactive();
            run_resolved_task_with_runner_and_approval(task, runner, &approval)
        }
        RunApprovalMode::External => {
            let approval = LocalExternalApprovalEngine::new(task.workspace_host_path.clone());
            run_resolved_task_with_runner_and_approval(task, runner, &approval)
        }
    }
}

fn unsupported(message: String) -> taskfence_core::Result<()> {
    Err(TaskFenceError::Unsupported(message))
}

#[cfg(test)]
fn run_task_with_runner_and_approval(
    task_file: Utf8PathBuf,
    runner: &dyn Runner,
    approval: &dyn ApprovalEngine,
) -> taskfence_core::Result<()> {
    let task = load_task_file(&task_file)?;
    run_resolved_task_with_runner_and_approval(task, runner, approval)
}

fn run_resolved_task_with_runner_and_approval(
    task: ResolvedTask,
    runner: &dyn Runner,
    approval: &dyn ApprovalEngine,
) -> taskfence_core::Result<()> {
    let artifacts = LocalArtifactStore::in_workspace();
    let events_path = artifacts.task_dir(&task)?.join("events.jsonl");
    let audit = LocalJsonlAuditLogger::new(events_path)?;
    let policy = BuiltInPolicyEngine;
    let adapter = GenericAgentAdapter;
    let report = MarkdownReportGenerator::new();
    let state = InMemoryStateStore::new();
    let orchestrator = Orchestrator {
        policy: &policy,
        approval,
        audit: &audit,
        artifacts: &artifacts,
        adapter: &adapter,
        runner,
        report: &report,
        state: &state,
    };

    println!("Task starting");
    println!("  id: {}", task.id.0);
    println!("  goal: {}", task.goal);
    println!("  workspace: {}", task.workspace_host_path);

    let result = orchestrator.run(task)?;

    println!("Task finished");
    println!("  id: {}", result.task_id.0);
    println!("  status: {:?}", result.status);
    println!("  artifacts: {}", result.artifacts.task_dir);
    if let Some(report) = &result.artifacts.report {
        println!("  report: {report}");
    }

    match result.status {
        TaskStatus::Succeeded => Ok(()),
        status => {
            let detail = result
                .message
                .as_deref()
                .unwrap_or("task did not finish successfully");
            Err(TaskFenceError::Runner(format!(
                "task {} finished with status {status:?}: {detail}",
                result.task_id.0
            )))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::CommandFactory;
    use std::fs;
    use std::thread;
    use std::time::{Duration, Instant};
    use taskfence_core::{Action, ActionDecision, ApprovalDecision, RiskLevel};
    use taskfence_runner::FakeRunner;

    #[test]
    fn parses_init_with_default_path() {
        let cli = Cli::try_parse_from(["taskfence", "init"]).unwrap();

        match cli.command {
            Command::Init { path } => assert_eq!(path, Utf8PathBuf::from("taskfence.yaml")),
            other => panic!("expected init command, got {other:?}"),
        }
    }

    #[test]
    fn parses_init_with_custom_path() {
        let cli = Cli::try_parse_from(["taskfence", "init", "tasks/fix.yaml"]).unwrap();

        match cli.command {
            Command::Init { path } => assert_eq!(path, Utf8PathBuf::from("tasks/fix.yaml")),
            other => panic!("expected init command, got {other:?}"),
        }
    }

    #[test]
    fn parses_run_task_file() {
        let cli = Cli::try_parse_from(["taskfence", "run", "task.yaml"]).unwrap();

        match cli.command {
            Command::Run {
                task_file,
                interactive_approval,
                external_approval,
            } => {
                assert_eq!(task_file, Utf8PathBuf::from("task.yaml"));
                assert!(!interactive_approval);
                assert!(!external_approval);
            }
            other => panic!("expected run command, got {other:?}"),
        }
    }

    #[test]
    fn parses_interactive_approval_flag() {
        let cli = Cli::try_parse_from(["taskfence", "run", "--interactive-approval", "task.yaml"])
            .unwrap();

        match cli.command {
            Command::Run {
                task_file,
                interactive_approval,
                external_approval,
            } => {
                assert_eq!(task_file, Utf8PathBuf::from("task.yaml"));
                assert!(interactive_approval);
                assert!(!external_approval);
            }
            other => panic!("expected run command, got {other:?}"),
        }
    }

    #[test]
    fn parses_external_approval_flag() {
        let cli =
            Cli::try_parse_from(["taskfence", "run", "--external-approval", "task.yaml"]).unwrap();

        match cli.command {
            Command::Run {
                task_file,
                interactive_approval,
                external_approval,
            } => {
                assert_eq!(task_file, Utf8PathBuf::from("task.yaml"));
                assert!(!interactive_approval);
                assert!(external_approval);
            }
            other => panic!("expected run command, got {other:?}"),
        }
    }

    #[test]
    fn parses_logs_task_id() {
        let cli = Cli::try_parse_from(["taskfence", "logs", "task-123"]).unwrap();

        match cli.command {
            Command::Logs { task_id, workspace } => {
                assert_eq!(task_id, "task-123");
                assert_eq!(workspace, Utf8PathBuf::from("."));
            }
            other => panic!("expected logs command, got {other:?}"),
        }
    }

    #[test]
    fn parses_logs_workspace() {
        let cli =
            Cli::try_parse_from(["taskfence", "logs", "task-123", "--workspace", "repo"]).unwrap();

        match cli.command {
            Command::Logs { task_id, workspace } => {
                assert_eq!(task_id, "task-123");
                assert_eq!(workspace, Utf8PathBuf::from("repo"));
            }
            other => panic!("expected logs command, got {other:?}"),
        }
    }

    #[test]
    fn parses_diff_task_id() {
        let cli = Cli::try_parse_from(["taskfence", "diff", "task-123"]).unwrap();

        match cli.command {
            Command::Diff { task_id, workspace } => {
                assert_eq!(task_id, "task-123");
                assert_eq!(workspace, Utf8PathBuf::from("."));
            }
            other => panic!("expected diff command, got {other:?}"),
        }
    }

    #[test]
    fn parses_diff_workspace() {
        let cli =
            Cli::try_parse_from(["taskfence", "diff", "task-123", "--workspace", "repo"]).unwrap();

        match cli.command {
            Command::Diff { task_id, workspace } => {
                assert_eq!(task_id, "task-123");
                assert_eq!(workspace, Utf8PathBuf::from("repo"));
            }
            other => panic!("expected diff command, got {other:?}"),
        }
    }

    #[test]
    fn parses_tasks_default_workspace() {
        let cli = Cli::try_parse_from(["taskfence", "tasks"]).unwrap();

        match cli.command {
            Command::Tasks { workspace } => assert_eq!(workspace, Utf8PathBuf::from(".")),
            other => panic!("expected tasks command, got {other:?}"),
        }
    }

    #[test]
    fn parses_tasks_workspace() {
        let cli = Cli::try_parse_from(["taskfence", "tasks", "--workspace", "repo"]).unwrap();

        match cli.command {
            Command::Tasks { workspace } => assert_eq!(workspace, Utf8PathBuf::from("repo")),
            other => panic!("expected tasks command, got {other:?}"),
        }
    }

    #[test]
    fn parses_approve_approval_id() {
        let cli = Cli::try_parse_from(["taskfence", "approve", "approval-123"]).unwrap();

        match cli.command {
            Command::Approve {
                approval_id,
                workspace,
            } => {
                assert_eq!(approval_id, "approval-123");
                assert_eq!(workspace, Utf8PathBuf::from("."));
            }
            other => panic!("expected approve command, got {other:?}"),
        }
    }

    #[test]
    fn parses_approve_workspace() {
        let cli = Cli::try_parse_from([
            "taskfence",
            "approve",
            "approval-123",
            "--workspace",
            "repo",
        ])
        .unwrap();

        match cli.command {
            Command::Approve {
                approval_id,
                workspace,
            } => {
                assert_eq!(approval_id, "approval-123");
                assert_eq!(workspace, Utf8PathBuf::from("repo"));
            }
            other => panic!("expected approve command, got {other:?}"),
        }
    }

    #[test]
    fn parses_deny_approval_id() {
        let cli = Cli::try_parse_from(["taskfence", "deny", "approval-123"]).unwrap();

        match cli.command {
            Command::Deny {
                approval_id,
                workspace,
            } => {
                assert_eq!(approval_id, "approval-123");
                assert_eq!(workspace, Utf8PathBuf::from("."));
            }
            other => panic!("expected deny command, got {other:?}"),
        }
    }

    #[test]
    fn parses_report_task_id() {
        let cli = Cli::try_parse_from(["taskfence", "report", "task-123"]).unwrap();

        match cli.command {
            Command::Report { task_id, workspace } => {
                assert_eq!(task_id, "task-123");
                assert_eq!(workspace, Utf8PathBuf::from("."));
            }
            other => panic!("expected report command, got {other:?}"),
        }
    }

    #[test]
    fn rejects_missing_run_task_file() {
        let err = Cli::try_parse_from(["taskfence", "run"]).unwrap_err();

        assert!(err.to_string().contains("<TASK_FILE>"));
    }

    #[test]
    fn clap_debug_asserts() {
        Cli::command().debug_assert();
    }

    #[test]
    fn init_remains_explicitly_unsupported() {
        let err = execute(Cli {
            command: Command::Init {
                path: Utf8PathBuf::from("taskfence.yaml"),
            },
        })
        .unwrap_err();

        assert!(
            matches!(err, TaskFenceError::Unsupported(message) if message.contains("init command"))
        );
    }

    #[test]
    fn rejects_combined_approval_modes_before_loading_task() {
        let err = execute(Cli {
            command: Command::Run {
                interactive_approval: true,
                external_approval: true,
                task_file: Utf8PathBuf::from("/tmp/taskfence-missing-task.yaml"),
            },
        })
        .unwrap_err();

        assert!(
            matches!(err, TaskFenceError::Config(message) if message.contains("cannot be used together"))
        );
    }

    #[test]
    fn logs_command_reads_local_task_logs() {
        let temp = tempfile::tempdir().unwrap();
        let workspace = Utf8PathBuf::from_path_buf(temp.path().join("repo")).unwrap();
        let task_dir = workspace.join(".taskfence/tasks/task-123");
        fs::create_dir_all(&task_dir).unwrap();
        fs::write(task_dir.join("stdout.log"), "hello\n").unwrap();
        fs::write(task_dir.join("stderr.log"), "warning\n").unwrap();

        let text = logs_text(workspace, &TaskId("task-123".into())).unwrap();

        assert!(text.contains("== stdout:"));
        assert!(text.contains("hello\n"));
        assert!(text.contains("== stderr:"));
        assert!(text.contains("warning\n"));
    }

    #[test]
    fn report_command_reads_local_task_report() {
        let temp = tempfile::tempdir().unwrap();
        let workspace = Utf8PathBuf::from_path_buf(temp.path().join("repo")).unwrap();
        let task_dir = workspace.join(".taskfence/tasks/task-123");
        fs::create_dir_all(&task_dir).unwrap();
        fs::write(task_dir.join("report.md"), "# Task Report\n").unwrap();

        let text = report_text(workspace, &TaskId("task-123".into())).unwrap();

        assert_eq!(text, "# Task Report\n");
    }

    #[test]
    fn diff_command_reads_local_task_diff() {
        let temp = tempfile::tempdir().unwrap();
        let workspace = Utf8PathBuf::from_path_buf(temp.path().join("repo")).unwrap();
        let task_dir = workspace.join(".taskfence/tasks/task-123");
        fs::create_dir_all(&task_dir).unwrap();
        fs::write(task_dir.join("diff.patch"), "TaskFence diff metadata\n").unwrap();

        let text = diff_text(workspace, &TaskId("task-123".into())).unwrap();

        assert_eq!(text, "TaskFence diff metadata\n");
    }

    #[test]
    fn tasks_command_reads_local_task_list() {
        let temp = tempfile::tempdir().unwrap();
        let workspace = Utf8PathBuf::from_path_buf(temp.path().join("repo")).unwrap();
        fs::create_dir(&workspace).unwrap();
        let task_file = Utf8PathBuf::from_path_buf(temp.path().join("task.yaml")).unwrap();
        fs::write(&task_file, task_yaml("cli-list", &workspace, "echo ok")).unwrap();

        run_task_with_runner(
            task_file,
            &FakeRunner::succeeding(),
            RunApprovalMode::FailClosed,
        )
        .unwrap();

        let text = tasks_text(workspace).unwrap();

        assert!(text.contains("TASK ID\tSTATUS\tARTIFACTS\tWARNINGS\tGOAL\n"));
        assert!(text.contains("cli-list\tSucceeded\treport,diff\t-\tCLI test\n"));
    }

    #[test]
    fn logs_command_surfaces_missing_artifact_errors() {
        let temp = tempfile::tempdir().unwrap();
        let workspace = Utf8PathBuf::from_path_buf(temp.path().join("repo")).unwrap();

        let err = logs_text(workspace, &TaskId("missing".into())).unwrap_err();

        assert!(
            matches!(err, TaskFenceError::State(message) if message.contains("directory not found"))
        );
    }

    #[test]
    fn run_writes_artifacts_with_successful_runner() {
        let temp = tempfile::tempdir().unwrap();
        let workspace = Utf8PathBuf::from_path_buf(temp.path().join("repo")).unwrap();
        fs::create_dir(&workspace).unwrap();
        let task_file = Utf8PathBuf::from_path_buf(temp.path().join("task.yaml")).unwrap();
        fs::write(&task_file, task_yaml("cli-success", &workspace, "echo ok")).unwrap();

        run_task_with_runner(
            task_file,
            &FakeRunner::succeeding(),
            RunApprovalMode::FailClosed,
        )
        .unwrap();

        assert!(workspace
            .join(".taskfence/tasks/cli-success/task.resolved.json")
            .is_file());
        assert!(workspace
            .join(".taskfence/tasks/cli-success/report.md")
            .is_file());
    }

    #[test]
    fn run_surfaces_config_errors() {
        let err = run_task_with_runner(
            Utf8PathBuf::from("/tmp/taskfence-missing-task.yaml"),
            &FakeRunner::succeeding(),
            RunApprovalMode::FailClosed,
        )
        .unwrap_err();

        assert!(
            matches!(err, TaskFenceError::Config(message) if message.contains("failed to read"))
        );
    }

    #[test]
    fn run_surfaces_orchestrator_failures() {
        let temp = tempfile::tempdir().unwrap();
        let workspace = Utf8PathBuf::from_path_buf(temp.path().join("repo")).unwrap();
        fs::create_dir(&workspace).unwrap();
        let task_file = Utf8PathBuf::from_path_buf(temp.path().join("task.yaml")).unwrap();
        fs::write(&task_file, task_yaml("cli-fail", &workspace, "echo ok")).unwrap();

        let err = run_task_with_runner(
            task_file,
            &FakeRunner::succeeding().with_start_error("boom"),
            RunApprovalMode::FailClosed,
        )
        .unwrap_err();

        assert!(
            matches!(err, TaskFenceError::Runner(message) if message.contains("status Failed") && message.contains("boom"))
        );
    }

    #[test]
    fn default_run_fails_closed_for_approval_required_commands() {
        let temp = tempfile::tempdir().unwrap();
        let workspace = Utf8PathBuf::from_path_buf(temp.path().join("repo")).unwrap();
        fs::create_dir(&workspace).unwrap();
        let task_file = Utf8PathBuf::from_path_buf(temp.path().join("task.yaml")).unwrap();
        fs::write(
            &task_file,
            task_yaml_with_command_policy("approval-default", &workspace, &[], &["echo"], &[]),
        )
        .unwrap();

        let err = run_task_with_runner(
            task_file.clone(),
            &FakeRunner::succeeding(),
            RunApprovalMode::FailClosed,
        )
        .unwrap_err();

        assert!(
            matches!(err, TaskFenceError::Runner(message) if message.contains("status Denied") && message.contains("denied or timed out"))
        );
        assert!(workspace
            .join(".taskfence/tasks/approval-default/report.md")
            .is_file());
    }

    #[test]
    fn approved_run_continues_after_approval_required_command() {
        let temp = tempfile::tempdir().unwrap();
        let workspace = Utf8PathBuf::from_path_buf(temp.path().join("repo")).unwrap();
        fs::create_dir(&workspace).unwrap();
        let task_file = Utf8PathBuf::from_path_buf(temp.path().join("task.yaml")).unwrap();
        fs::write(
            &task_file,
            task_yaml_with_command_policy("approval-ok", &workspace, &[], &["echo"], &[]),
        )
        .unwrap();
        let approval = LocalApprovalEngine::preconfigured(ApprovalDecision::Approved);

        run_task_with_runner_and_approval(task_file, &FakeRunner::succeeding(), &approval).unwrap();

        assert!(workspace
            .join(".taskfence/tasks/approval-ok/report.md")
            .is_file());
    }

    #[test]
    fn approve_command_resolves_pending_local_approval() {
        let temp = tempfile::tempdir().unwrap();
        let workspace = Utf8PathBuf::from_path_buf(temp.path().join("repo")).unwrap();
        fs::create_dir(&workspace).unwrap();
        let approval_id = create_pending_approval(&workspace, "approval-cli-approve");

        execute(Cli {
            command: Command::Approve {
                approval_id: approval_id.0.clone(),
                workspace: workspace.clone(),
            },
        })
        .unwrap();

        let record = LocalApprovalStore::new(workspace)
            .read(&approval_id)
            .unwrap();
        assert_eq!(record.decision, Some(ApprovalDecision::Approved));
    }

    #[test]
    fn deny_command_resolves_pending_local_approval() {
        let temp = tempfile::tempdir().unwrap();
        let workspace = Utf8PathBuf::from_path_buf(temp.path().join("repo")).unwrap();
        fs::create_dir(&workspace).unwrap();
        let approval_id = create_pending_approval(&workspace, "approval-cli-deny");

        execute(Cli {
            command: Command::Deny {
                approval_id: approval_id.0.clone(),
                workspace: workspace.clone(),
            },
        })
        .unwrap();

        let record = LocalApprovalStore::new(workspace)
            .read(&approval_id)
            .unwrap();
        assert_eq!(record.decision, Some(ApprovalDecision::Denied));
    }

    #[test]
    fn external_approval_run_continues_after_cli_approval() {
        let temp = tempfile::tempdir().unwrap();
        let workspace = Utf8PathBuf::from_path_buf(temp.path().join("repo")).unwrap();
        fs::create_dir(&workspace).unwrap();
        let task_file = Utf8PathBuf::from_path_buf(temp.path().join("task.yaml")).unwrap();
        fs::write(
            &task_file,
            task_yaml_with_command_policy("approval-external-ok", &workspace, &[], &["echo"], &[]),
        )
        .unwrap();
        let workspace_for_resolution = workspace.clone();
        let task_file_for_thread = task_file.clone();
        let handle = thread::spawn(move || {
            let runner = FakeRunner::succeeding();
            run_task_with_runner(task_file_for_thread, &runner, RunApprovalMode::External)
        });

        let approval_id = wait_for_pending_approval(&workspace_for_resolution);
        resolve_approval_record(
            workspace_for_resolution.clone(),
            &approval_id,
            ApprovalDecision::Approved,
        )
        .unwrap();
        handle.join().unwrap().unwrap();

        assert!(workspace
            .join(".taskfence/tasks/approval-external-ok/report.md")
            .is_file());
        assert_eq!(
            LocalApprovalStore::new(workspace)
                .read(&approval_id)
                .unwrap()
                .decision,
            Some(ApprovalDecision::Approved)
        );
    }

    #[test]
    fn external_approval_run_stops_after_cli_denial() {
        let temp = tempfile::tempdir().unwrap();
        let workspace = Utf8PathBuf::from_path_buf(temp.path().join("repo")).unwrap();
        fs::create_dir(&workspace).unwrap();
        let task_file = Utf8PathBuf::from_path_buf(temp.path().join("task.yaml")).unwrap();
        fs::write(
            &task_file,
            task_yaml_with_command_policy(
                "approval-external-denied",
                &workspace,
                &[],
                &["echo"],
                &[],
            ),
        )
        .unwrap();
        let workspace_for_resolution = workspace.clone();
        let task_file_for_thread = task_file.clone();
        let handle = thread::spawn(move || {
            let runner = FakeRunner::succeeding();
            run_task_with_runner(task_file_for_thread, &runner, RunApprovalMode::External)
        });

        let approval_id = wait_for_pending_approval(&workspace_for_resolution);
        resolve_approval_record(
            workspace_for_resolution.clone(),
            &approval_id,
            ApprovalDecision::Denied,
        )
        .unwrap();
        let err = handle.join().unwrap().unwrap_err();

        assert!(
            matches!(err, TaskFenceError::Runner(message) if message.contains("status Denied"))
        );
        assert!(workspace
            .join(".taskfence/tasks/approval-external-denied/report.md")
            .is_file());
    }

    fn task_yaml(id: &str, workspace: &Utf8PathBuf, allowed_command: &str) -> String {
        format!(
            r#"id: "{id}"
goal: "CLI test"
workspace: "{workspace}"
agent:
  type: "generic"
  command: "echo"
  args:
    - "ok"
sandbox:
  type: "docker"
  image: "taskfence/runner:latest"
permissions:
  paths:
    read:
      - "{workspace}"
    write:
      - "{workspace}"
  commands:
    allow:
      - "{allowed_command}"
  network:
    default: "disabled"
"#
        )
    }

    fn task_yaml_with_command_policy(
        id: &str,
        workspace: &Utf8PathBuf,
        allow: &[&str],
        approval_required: &[&str],
        deny: &[&str],
    ) -> String {
        let mut command_rules = String::new();
        append_command_rules(&mut command_rules, "allow", allow);
        append_command_rules(&mut command_rules, "approval_required", approval_required);
        append_command_rules(&mut command_rules, "deny", deny);

        format!(
            r#"id: "{id}"
goal: "CLI test"
workspace: "{workspace}"
agent:
  type: "generic"
  command: "echo"
  args:
    - "ok"
sandbox:
  type: "docker"
  image: "taskfence/runner:latest"
permissions:
  paths:
    read:
      - "{workspace}"
    write:
      - "{workspace}"
  commands:
{command_rules}  network:
    default: "disabled"
"#
        )
    }

    fn append_command_rules(output: &mut String, key: &str, values: &[&str]) {
        if values.is_empty() {
            return;
        }

        output.push_str("    ");
        output.push_str(key);
        output.push_str(":\n");
        for value in values {
            output.push_str("      - \"");
            output.push_str(value);
            output.push_str("\"\n");
        }
    }

    fn create_pending_approval(workspace: &Utf8PathBuf, approval_id: &str) -> ApprovalId {
        let task_file = workspace.join("task.yaml");
        let task = taskfence_config::parse_task_file(
            &task_file,
            &task_yaml_with_command_policy("pending-approval", workspace, &[], &["echo"], &[]),
        )
        .unwrap();
        let record = taskfence_core::ApprovalRecord {
            id: ApprovalId(approval_id.into()),
            task_id: task.id,
            actor: "local".into(),
            source: Some("external".into()),
            requested_at: time::OffsetDateTime::now_utc(),
            resolved_at: None,
            action: Action::Command(taskfence_core::CommandAction::parse("echo ok")),
            policy_decision: ActionDecision::RequireApproval {
                approval_kind: "command".into(),
                rule_id: Some("test".into()),
                reason: "test approval".into(),
                risk: RiskLevel::High,
            },
            decision: None,
        };
        LocalApprovalStore::new(workspace.clone())
            .write_pending(&record)
            .unwrap();
        record.id
    }

    fn wait_for_pending_approval(workspace: &Utf8PathBuf) -> ApprovalId {
        let approvals_dir = workspace.join(".taskfence/approvals");
        let deadline = Instant::now() + Duration::from_secs(5);
        loop {
            if approvals_dir.is_dir() {
                for entry in fs::read_dir(approvals_dir.as_std_path()).unwrap() {
                    let entry = entry.unwrap();
                    let path = Utf8PathBuf::from_path_buf(entry.path()).unwrap();
                    if path.extension() == Some("json") {
                        let id = path.file_stem().unwrap().to_owned();
                        return ApprovalId(id);
                    }
                }
            }

            assert!(
                Instant::now() < deadline,
                "timed out waiting for pending approval file"
            );
            thread::sleep(Duration::from_millis(10));
        }
    }
}
