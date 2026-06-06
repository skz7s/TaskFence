use camino::Utf8PathBuf;
use clap::{Parser, Subcommand};
use std::process::ExitCode;
use taskfence_agent::GenericAgentAdapter;
use taskfence_approval::LocalApprovalEngine;
use taskfence_artifacts::LocalArtifactStore;
use taskfence_audit::LocalJsonlAuditLogger;
use taskfence_config::load_task_file;
use taskfence_core::{
    ApprovalEngine, LogStream, Orchestrator, Runner, TaskFenceError, TaskId, TaskStatus,
};
use taskfence_policy::BuiltInPolicyEngine;
use taskfence_report::MarkdownReportGenerator;
use taskfence_runner::DockerRunner;
use taskfence_state::{InMemoryStateStore, LocalTaskEvidenceStore, TaskLogs};

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
    /// Approve a pending approval request.
    Approve {
        /// Approval ID to approve.
        approval_id: String,
    },
    /// Deny a pending approval request.
    Deny {
        /// Approval ID to deny.
        approval_id: String,
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
        } => {
            let runner = DockerRunner::new();
            run_task_with_runner(task_file, &runner, interactive_approval)
        }
        Command::Logs { task_id, workspace } => show_logs(workspace, task_id),
        Command::Approve { approval_id } => unsupported(format!(
            "approve command is parsed but approval storage is not implemented yet for {approval_id}"
        )),
        Command::Deny { approval_id } => unsupported(format!(
            "deny command is parsed but approval storage is not implemented yet for {approval_id}"
        )),
        Command::Report { task_id, workspace } => show_report(workspace, task_id),
    }
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

fn logs_text(workspace: Utf8PathBuf, task_id: &TaskId) -> taskfence_core::Result<String> {
    let store = LocalTaskEvidenceStore::new(workspace);
    let logs = store.read_logs(task_id)?;
    Ok(render_logs(&logs))
}

fn report_text(workspace: Utf8PathBuf, task_id: &TaskId) -> taskfence_core::Result<String> {
    let store = LocalTaskEvidenceStore::new(workspace);
    Ok(store.read_report(task_id)?.contents)
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

fn run_task_with_runner(
    task_file: Utf8PathBuf,
    runner: &dyn Runner,
    interactive_approval: bool,
) -> taskfence_core::Result<()> {
    let approval = if interactive_approval {
        LocalApprovalEngine::interactive()
    } else {
        LocalApprovalEngine::fail_closed()
    };
    run_task_with_runner_and_approval(task_file, runner, &approval)
}

fn run_task_with_runner_and_approval(
    task_file: Utf8PathBuf,
    runner: &dyn Runner,
    approval: &dyn ApprovalEngine,
) -> taskfence_core::Result<()> {
    let task = load_task_file(&task_file)?;
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

fn unsupported(message: String) -> taskfence_core::Result<()> {
    Err(TaskFenceError::Unsupported(message))
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::CommandFactory;
    use std::fs;
    use taskfence_core::ApprovalDecision;
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
            } => {
                assert_eq!(task_file, Utf8PathBuf::from("task.yaml"));
                assert!(!interactive_approval);
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
            } => {
                assert_eq!(task_file, Utf8PathBuf::from("task.yaml"));
                assert!(interactive_approval);
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
    fn parses_approve_approval_id() {
        let cli = Cli::try_parse_from(["taskfence", "approve", "approval-123"]).unwrap();

        match cli.command {
            Command::Approve { approval_id } => assert_eq!(approval_id, "approval-123"),
            other => panic!("expected approve command, got {other:?}"),
        }
    }

    #[test]
    fn parses_deny_approval_id() {
        let cli = Cli::try_parse_from(["taskfence", "deny", "approval-123"]).unwrap();

        match cli.command {
            Command::Deny { approval_id } => assert_eq!(approval_id, "approval-123"),
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
    fn unsupported_non_run_commands_remain_explicit() {
        let err = execute(Cli {
            command: Command::Approve {
                approval_id: "approval-123".into(),
            },
        })
        .unwrap_err();

        assert!(
            matches!(err, TaskFenceError::Unsupported(message) if message.contains("approve command"))
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

        run_task_with_runner(task_file, &FakeRunner::succeeding(), false).unwrap();

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
            false,
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
            false,
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

        let err =
            run_task_with_runner(task_file.clone(), &FakeRunner::succeeding(), false).unwrap_err();

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
}
