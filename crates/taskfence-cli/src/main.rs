use camino::Utf8PathBuf;
use clap::{Parser, Subcommand};
use std::fs::{self, OpenOptions};
use std::io::{ErrorKind, Write};
use std::process::ExitCode;
use taskfence_agent::GenericAgentAdapter;
use taskfence_approval::{LocalApprovalEngine, LocalApprovalStore, LocalExternalApprovalEngine};
use taskfence_artifacts::LocalArtifactStore;
use taskfence_audit::LocalJsonlAuditLogger;
use taskfence_config::load_task_file;
use taskfence_core::{
    validate_task_for_run, Action, ActionDecision, ApprovalDecision, ApprovalEngine, ApprovalId,
    ApprovalRecord, ArtifactRefs, ArtifactStore, AuditEvent, AuditLogger, ExitStatus, LogStream,
    NetworkDefault, Orchestrator, RedactedValue, ReportGenerator, ResolvedTask, RiskLevel, Runner,
    StateStore, TaskFenceError, TaskId, TaskStatus, TaskValidation, ToolAction, ToolExecution,
    ToolExecutionContext, ToolExecutionErrorKind,
};
use taskfence_gateway::{
    gateway_spool_request_id_from_path, normalize_tool_action, read_gateway_spool_request,
    write_gateway_spool_response, GatewayExecutor, GatewaySpoolPaths, GatewaySpoolResponse,
    GatewaySpoolResponseState, InMemoryToolRegistry, LocalFixtureToolAdapter,
    LocalRedactedSecretBroker, RegisteredTool, ToolAdapter, UnsupportedGatewayAdapter,
};
use taskfence_policy::BuiltInPolicyEngine;
use taskfence_report::MarkdownReportGenerator;
use taskfence_runner::DockerRunner;
use taskfence_state::{
    InMemoryStateStore, LocalTaskEvidenceStore, TaskArtifactKind, TaskArtifacts, TaskEvents,
    TaskLogs, TaskSummary,
};

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
    /// Validate a task file without starting the runner.
    Validate {
        /// TaskFence YAML task file.
        task_file: Utf8PathBuf,
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
    /// Execute a configured local gateway fixture call.
    Gateway {
        #[command(subcommand)]
        command: GatewayCommand,
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
    /// Show a locally recorded task summary.
    Task {
        /// Task ID to query.
        task_id: String,
        /// Workspace that owns the .taskfence task evidence directory.
        #[arg(long, default_value = ".")]
        workspace: Utf8PathBuf,
    },
    /// Show the resolved task input saved for a local run.
    Inputs {
        /// Task ID to query.
        task_id: String,
        /// Workspace that owns the .taskfence task evidence directory.
        #[arg(long, default_value = ".")]
        workspace: Utf8PathBuf,
    },
    /// List saved evidence and artifact files for a task.
    Artifacts {
        /// Task ID to query.
        task_id: String,
        /// Workspace that owns the .taskfence task evidence directory.
        #[arg(long, default_value = ".")]
        workspace: Utf8PathBuf,
    },
    /// Compare two locally recorded task summaries.
    Compare {
        /// Left task ID to compare.
        left_task_id: String,
        /// Right task ID to compare.
        right_task_id: String,
        /// Workspace that owns the .taskfence task evidence directory.
        #[arg(long, default_value = ".")]
        workspace: Utf8PathBuf,
    },
    /// Show the latest locally recorded task status.
    Status {
        /// Task ID to query.
        task_id: String,
        /// Workspace that owns the .taskfence task evidence directory.
        #[arg(long, default_value = ".")]
        workspace: Utf8PathBuf,
    },
    /// Show the structured event timeline for a task.
    Events {
        /// Task ID to query.
        task_id: String,
        /// Workspace that owns the .taskfence task evidence directory.
        #[arg(long, default_value = ".")]
        workspace: Utf8PathBuf,
    },
    /// List locally recorded approval requests in a workspace.
    Approvals {
        /// Workspace that owns the .taskfence approval directory.
        #[arg(long, default_value = ".")]
        workspace: Utf8PathBuf,
    },
    /// Show a locally recorded approval request.
    Approval {
        /// Approval ID to query.
        approval_id: String,
        /// Workspace that owns the .taskfence approval directory.
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

#[derive(Debug, Subcommand)]
enum GatewayCommand {
    /// Mediate and execute one configured local gateway tool call.
    Call {
        /// TaskFence YAML task file.
        task_file: Utf8PathBuf,
        /// Tool name, for example github.
        tool: String,
        /// Operation name, for example read_issue.
        operation: String,
        /// Gateway protocol shape.
        #[arg(long, default_value = "mcp")]
        protocol: String,
        /// Plain parameter in KEY=VALUE form. Values are redacted in summaries.
        #[arg(long = "param", value_name = "KEY=VALUE")]
        params: Vec<String>,
        /// Resolve approval-required fixture calls with a local approved decision.
        #[arg(long)]
        approve: bool,
        /// Wait for taskfence approve/deny to resolve approval-required fixture calls.
        #[arg(long)]
        external_approval: bool,
    },
    /// Process agent-facing gateway spool requests.
    Spool {
        #[command(subcommand)]
        command: GatewaySpoolCommand,
    },
}

#[derive(Debug, Subcommand)]
enum GatewaySpoolCommand {
    /// Process one request file from the task gateway spool.
    Process {
        /// TaskFence YAML task file.
        task_file: Utf8PathBuf,
        /// Request JSON file under the task gateway spool requests directory.
        request_file: Utf8PathBuf,
        /// Resolve approval-required spool calls with a local approved decision.
        #[arg(long)]
        approve: bool,
        /// Wait for taskfence approve/deny to resolve approval-required spool calls.
        #[arg(long)]
        external_approval: bool,
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
        Command::Init { path } => init_task_file(path),
        Command::Validate { task_file } => validate_task_file(task_file),
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
        Command::Gateway { command } => match command {
            GatewayCommand::Call {
                task_file,
                protocol,
                tool,
                operation,
                params,
                approve,
                external_approval,
            } => {
                if approve && external_approval {
                    return Err(TaskFenceError::Config(
                        "--approve and --external-approval cannot be used together".into(),
                    ));
                }
                let approval_mode = if approve {
                    GatewayApprovalMode::Approved
                } else if external_approval {
                    GatewayApprovalMode::External
                } else {
                    GatewayApprovalMode::FailClosed
                };
                run_gateway_call(task_file, protocol, tool, operation, params, approval_mode)
            }
            GatewayCommand::Spool {
                command:
                    GatewaySpoolCommand::Process {
                        task_file,
                        request_file,
                        approve,
                        external_approval,
                    },
            } => {
                if approve && external_approval {
                    return Err(TaskFenceError::Config(
                        "--approve and --external-approval cannot be used together".into(),
                    ));
                }
                let approval_mode = if approve {
                    GatewayApprovalMode::Approved
                } else if external_approval {
                    GatewayApprovalMode::External
                } else {
                    GatewayApprovalMode::FailClosed
                };
                run_gateway_spool_process(task_file, request_file, approval_mode)
            }
        },
        Command::Logs { task_id, workspace } => show_logs(workspace, task_id),
        Command::Diff { task_id, workspace } => show_diff(workspace, task_id),
        Command::Tasks { workspace } => show_tasks(workspace),
        Command::Task { task_id, workspace } => show_task(workspace, task_id),
        Command::Inputs { task_id, workspace } => show_inputs(workspace, task_id),
        Command::Artifacts { task_id, workspace } => show_artifacts(workspace, task_id),
        Command::Compare {
            left_task_id,
            right_task_id,
            workspace,
        } => show_compare(workspace, left_task_id, right_task_id),
        Command::Status { task_id, workspace } => show_status(workspace, task_id),
        Command::Events { task_id, workspace } => show_events(workspace, task_id),
        Command::Approvals { workspace } => show_approvals(workspace),
        Command::Approval {
            approval_id,
            workspace,
        } => show_approval(workspace, approval_id),
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

const STARTER_TASK_FILE: &str = r#"id: "local-demo"
goal: "Describe the task goal here"
workspace: "."

agent:
  type: "generic"
  command: "echo"
  args:
    - "hello from TaskFence"

sandbox:
  type: "docker"
  image: "debian:bookworm-slim"
  limits:
    timeout_minutes: 5

permissions:
  paths:
    read:
      - "."
    write:
      - "."
  commands:
    allow:
      - "echo"
  network:
    default: "disabled"

audit:
  report:
    format: "markdown"
  capture:
    stdout: true
    stderr: true
    file_diff: true
    network_destinations: true
    approvals: true
"#;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum RunApprovalMode {
    FailClosed,
    Interactive,
    External,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum GatewayApprovalMode {
    FailClosed,
    Approved,
    External,
}

#[derive(Debug, PartialEq, Eq)]
struct ValidationSummary {
    task_id: String,
    workspace: Utf8PathBuf,
    agent_command: String,
    command_policy: String,
    sandbox_image: String,
    network_mode: String,
    mount_count: usize,
}

fn init_task_file(path: Utf8PathBuf) -> taskfence_core::Result<()> {
    write_starter_task_file(&path)?;
    println!("Task file created");
    println!("  path: {path}");
    Ok(())
}

fn write_starter_task_file(path: &Utf8PathBuf) -> taskfence_core::Result<()> {
    if let Some(parent) = path
        .parent()
        .filter(|parent| !parent.as_str().is_empty() && parent.as_str() != ".")
    {
        fs::create_dir_all(parent).map_err(|err| {
            TaskFenceError::Config(format!("failed to create parent directory {parent}: {err}"))
        })?;
    }

    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)
        .map_err(|err| init_write_error(path, err))?;
    file.write_all(STARTER_TASK_FILE.as_bytes())
        .map_err(|err| TaskFenceError::Config(format!("failed to write task file {path}: {err}")))
}

fn init_write_error(path: &Utf8PathBuf, err: std::io::Error) -> TaskFenceError {
    if err.kind() == ErrorKind::AlreadyExists {
        TaskFenceError::Config(format!("task file already exists: {path}"))
    } else {
        TaskFenceError::Config(format!("failed to create task file {path}: {err}"))
    }
}

fn validate_task_file(task_file: Utf8PathBuf) -> taskfence_core::Result<()> {
    let summary = validate_task_file_summary(task_file)?;
    print_validation_summary(&summary);
    Ok(())
}

fn validate_task_file_summary(task_file: Utf8PathBuf) -> taskfence_core::Result<ValidationSummary> {
    let task = load_task_file(&task_file)?;
    let adapter = GenericAgentAdapter;
    let policy = BuiltInPolicyEngine;
    let runner = DockerRunner::new();
    Ok(validation_summary(validate_task_for_run(
        &task, &adapter, &policy, &runner,
    )?))
}

fn validation_summary(validation: TaskValidation) -> ValidationSummary {
    let network_mode = match validation.prepared.network.default {
        NetworkDefault::Disabled | NetworkDefault::Deny => "none",
        NetworkDefault::Allow => "bridge",
    };

    ValidationSummary {
        task_id: validation.task_id.0,
        workspace: validation.workspace_host_path,
        agent_command: validation.command_action.raw,
        command_policy: approval_policy_summary(&validation.command_decision),
        sandbox_image: validation.prepared.image.unwrap_or_else(|| "-".into()),
        network_mode: network_mode.into(),
        mount_count: validation.prepared.mounts.len(),
    }
}

fn print_validation_summary(summary: &ValidationSummary) {
    println!("Task file valid");
    println!("  id: {}", summary.task_id);
    println!("  workspace: {}", summary.workspace);
    println!("  command: {}", summary.agent_command);
    println!("  command policy: {}", summary.command_policy);
    println!("  image: {}", summary.sandbox_image);
    println!("  network: {}", summary.network_mode);
    println!("  mounts: {}", summary.mount_count);
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

fn show_task(workspace: Utf8PathBuf, task_id: String) -> taskfence_core::Result<()> {
    let text = task_text(workspace, &TaskId(task_id))?;
    print!("{text}");
    Ok(())
}

fn show_inputs(workspace: Utf8PathBuf, task_id: String) -> taskfence_core::Result<()> {
    let text = inputs_text(workspace, &TaskId(task_id))?;
    print!("{text}");
    Ok(())
}

fn show_artifacts(workspace: Utf8PathBuf, task_id: String) -> taskfence_core::Result<()> {
    let text = artifacts_text(workspace, &TaskId(task_id))?;
    print!("{text}");
    Ok(())
}

fn show_compare(
    workspace: Utf8PathBuf,
    left_task_id: String,
    right_task_id: String,
) -> taskfence_core::Result<()> {
    let text = compare_text(workspace, &TaskId(left_task_id), &TaskId(right_task_id))?;
    print!("{text}");
    Ok(())
}

fn show_status(workspace: Utf8PathBuf, task_id: String) -> taskfence_core::Result<()> {
    let text = status_text(workspace, &TaskId(task_id))?;
    print!("{text}");
    Ok(())
}

fn show_events(workspace: Utf8PathBuf, task_id: String) -> taskfence_core::Result<()> {
    let task_id = TaskId(task_id);
    let text = events_text(workspace, &task_id)?;
    print!("{text}");
    Ok(())
}

fn show_approvals(workspace: Utf8PathBuf) -> taskfence_core::Result<()> {
    let text = approvals_text(workspace)?;
    print!("{text}");
    Ok(())
}

fn show_approval(workspace: Utf8PathBuf, approval_id: String) -> taskfence_core::Result<()> {
    let text = approval_text(workspace, &ApprovalId(approval_id))?;
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

fn task_text(workspace: Utf8PathBuf, task_id: &TaskId) -> taskfence_core::Result<String> {
    let store = LocalTaskEvidenceStore::new(workspace);
    let task = store.read_task_summary(task_id)?;
    Ok(render_task_summary(&task))
}

fn inputs_text(workspace: Utf8PathBuf, task_id: &TaskId) -> taskfence_core::Result<String> {
    let store = LocalTaskEvidenceStore::new(workspace);
    Ok(render_task_inputs(&store.read_inputs(task_id)?.contents))
}

fn artifacts_text(workspace: Utf8PathBuf, task_id: &TaskId) -> taskfence_core::Result<String> {
    let store = LocalTaskEvidenceStore::new(workspace);
    Ok(render_task_artifacts(
        task_id,
        &store.read_artifacts(task_id)?,
    ))
}

fn compare_text(
    workspace: Utf8PathBuf,
    left_task_id: &TaskId,
    right_task_id: &TaskId,
) -> taskfence_core::Result<String> {
    let store = LocalTaskEvidenceStore::new(workspace);
    let left = store.read_task_summary(left_task_id)?;
    let right = store.read_task_summary(right_task_id)?;
    Ok(render_task_comparison(&left, &right))
}

fn status_text(workspace: Utf8PathBuf, task_id: &TaskId) -> taskfence_core::Result<String> {
    let store = LocalTaskEvidenceStore::new(workspace);
    let task = store.read_task_summary(task_id)?;
    Ok(render_task_status(&task))
}

fn events_text(workspace: Utf8PathBuf, task_id: &TaskId) -> taskfence_core::Result<String> {
    let store = LocalTaskEvidenceStore::new(workspace);
    let events = store.read_events(task_id)?;
    Ok(render_task_events(task_id, &events))
}

fn approvals_text(workspace: Utf8PathBuf) -> taskfence_core::Result<String> {
    let store = LocalApprovalStore::new(workspace);
    let approvals = store.list()?;
    Ok(render_approval_records(&approvals))
}

fn approval_text(
    workspace: Utf8PathBuf,
    approval_id: &ApprovalId,
) -> taskfence_core::Result<String> {
    let store = LocalApprovalStore::new(workspace);
    let approval = store.read(approval_id)?;
    Ok(render_approval_record(&approval))
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

fn render_task_events(task_id: &TaskId, events: &TaskEvents) -> String {
    let mut rendered = String::new();
    rendered.push_str("Task events\n");
    rendered.push_str(&format!("  task: {}\n", task_id.0));
    rendered.push_str(&format!("  evidence: {}\n", events.path));
    rendered.push_str("KIND\tAT\tSUMMARY\n");
    for event in &events.events {
        rendered.push_str(event_kind(event));
        rendered.push('\t');
        rendered.push_str(&event_time(event));
        rendered.push('\t');
        rendered.push_str(&compact_cell(&event_summary(event)));
        rendered.push('\n');
    }
    rendered
}

fn event_kind(event: &AuditEvent) -> &'static str {
    match event {
        AuditEvent::TaskCreated { .. } => "task-created",
        AuditEvent::TaskStatusChanged { .. } => "status",
        AuditEvent::PolicyDecision { .. } => "policy",
        AuditEvent::ToolExecutionStarted { .. } => "tool-started",
        AuditEvent::ToolExecutionFinished { .. } => "tool-finished",
        AuditEvent::ApprovalRequested { .. } => "approval-requested",
        AuditEvent::ApprovalResolved { .. } => "approval-resolved",
        AuditEvent::Log { .. } => "log",
        AuditEvent::RunnerExit { .. } => "runner-exit",
        AuditEvent::Artifact { .. } => "artifact",
        AuditEvent::Error { .. } => "error",
    }
}

fn event_time(event: &AuditEvent) -> String {
    match event {
        AuditEvent::TaskCreated { at, .. }
        | AuditEvent::TaskStatusChanged { at, .. }
        | AuditEvent::PolicyDecision { at, .. }
        | AuditEvent::ToolExecutionStarted { at, .. }
        | AuditEvent::ToolExecutionFinished { at, .. }
        | AuditEvent::RunnerExit { at, .. }
        | AuditEvent::Artifact { at, .. }
        | AuditEvent::Error { at, .. } => at.to_string(),
        AuditEvent::Log { chunk, .. } => chunk.timestamp.to_string(),
        AuditEvent::ApprovalRequested { record } => record.requested_at.to_string(),
        AuditEvent::ApprovalResolved { record } => record
            .resolved_at
            .as_ref()
            .unwrap_or(&record.requested_at)
            .to_string(),
    }
}

fn event_summary(event: &AuditEvent) -> String {
    match event {
        AuditEvent::TaskCreated { goal, .. } => format!("goal: {}", compact_cell(goal)),
        AuditEvent::TaskStatusChanged { status, .. } => format!("status {status:?}"),
        AuditEvent::PolicyDecision {
            action, decision, ..
        } => format!(
            "{} => {}",
            approval_action_summary(action),
            approval_policy_summary(decision)
        ),
        AuditEvent::ToolExecutionStarted { request, .. } => format!(
            "tool execution started for {}",
            tool_action_summary(&request.action)
        ),
        AuditEvent::ToolExecutionFinished { execution, .. } => {
            let action = &execution.request.action;
            match (&execution.result, &execution.error) {
                (Some(result), None) => format!(
                    "tool execution succeeded for {}; {}",
                    tool_action_summary(action),
                    compact_cell(&result.summary)
                ),
                (_, Some(error)) => format!(
                    "tool execution failed for {}; {:?}: {}",
                    tool_action_summary(action),
                    error.kind,
                    compact_cell(&error.message)
                ),
                (None, None) => format!(
                    "tool execution finished for {}; no result or error recorded",
                    tool_action_summary(action)
                ),
            }
        }
        AuditEvent::ApprovalRequested { record } => format!(
            "approval {} requested for {}; {}",
            record.id.0,
            approval_action_summary(&record.action),
            approval_policy_summary(&record.policy_decision)
        ),
        AuditEvent::ApprovalResolved { record } => format!(
            "approval {} {}; {}",
            record.id.0,
            approval_status(record),
            approval_action_summary(&record.action)
        ),
        AuditEvent::Log { chunk, .. } => {
            let stream = match chunk.stream {
                LogStream::Stdout => "stdout",
                LogStream::Stderr => "stderr",
            };
            format!("{stream} log {} byte(s)", chunk.text.len())
        }
        AuditEvent::RunnerExit { exit_status, .. } => exit_status_summary(exit_status),
        AuditEvent::Artifact { kind, path, .. } => {
            format!(
                "{} artifact {}",
                compact_cell(kind),
                compact_cell(path.as_str())
            )
        }
        AuditEvent::Error { message, .. } => format!("error: {}", compact_cell(message)),
    }
}

fn exit_status_summary(exit_status: &ExitStatus) -> String {
    let code = exit_status
        .code
        .map(|code| code.to_string())
        .unwrap_or_else(|| "-".into());
    let signal = exit_status.signal.as_deref().unwrap_or("-");
    format!(
        "exit code {code}; timed_out {}; signal {signal}",
        exit_status.timed_out
    )
}

fn render_approval_records(records: &[ApprovalRecord]) -> String {
    let mut rendered = String::from("APPROVAL ID\tTASK ID\tSTATUS\tREQUESTED\tACTION\n");
    for record in records {
        rendered.push_str(&compact_cell(&record.id.0));
        rendered.push('\t');
        rendered.push_str(&compact_cell(&record.task_id.0));
        rendered.push('\t');
        rendered.push_str(approval_status(record));
        rendered.push('\t');
        rendered.push_str(&compact_cell(&record.requested_at.to_string()));
        rendered.push('\t');
        rendered.push_str(&compact_cell(&approval_action_summary(&record.action)));
        rendered.push('\n');
    }
    rendered
}

fn render_approval_record(record: &ApprovalRecord) -> String {
    let mut rendered = String::new();
    rendered.push_str("Approval record\n");
    rendered.push_str(&format!("  id: {}\n", record.id.0));
    rendered.push_str(&format!("  task: {}\n", record.task_id.0));
    rendered.push_str(&format!("  status: {}\n", approval_status(record)));
    rendered.push_str(&format!("  actor: {}\n", compact_cell(&record.actor)));
    rendered.push_str(&format!(
        "  source: {}\n",
        record
            .source
            .as_deref()
            .map(compact_cell)
            .as_deref()
            .unwrap_or("-")
    ));
    rendered.push_str(&format!("  requested: {}\n", record.requested_at));
    rendered.push_str(&format!(
        "  resolved: {}\n",
        record
            .resolved_at
            .as_ref()
            .map(|resolved_at| resolved_at.to_string())
            .unwrap_or_else(|| "-".into())
    ));
    rendered.push_str(&format!(
        "  action: {}\n",
        approval_action_summary(&record.action)
    ));
    rendered.push_str(&format!(
        "  policy: {}\n",
        approval_policy_summary(&record.policy_decision)
    ));
    rendered
}

fn approval_status(record: &ApprovalRecord) -> &'static str {
    match record.decision.as_ref() {
        None => "pending",
        Some(ApprovalDecision::Approved) => "approved",
        Some(ApprovalDecision::Denied) => "denied",
        Some(ApprovalDecision::TimedOut) => "timed-out",
    }
}

fn approval_policy_summary(decision: &ActionDecision) -> String {
    match decision {
        ActionDecision::Allow { rule_id, reason } => {
            format!("allow{}: {}", rule_suffix(rule_id), compact_cell(reason))
        }
        ActionDecision::RequireApproval {
            approval_kind,
            rule_id,
            reason,
            risk,
        } => format!(
            "requires {} approval{}: {}; risk {}",
            compact_cell(approval_kind),
            rule_suffix(rule_id),
            compact_cell(reason),
            risk_label(risk)
        ),
        ActionDecision::Deny { rule_id, reason } => {
            format!("deny{}: {}", rule_suffix(rule_id), compact_cell(reason))
        }
    }
}

fn rule_suffix(rule_id: &Option<String>) -> String {
    rule_id
        .as_deref()
        .map(|id| format!(" by rule {}", compact_cell(id)))
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

fn approval_action_summary(action: &Action) -> String {
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
            "tool call {} with {} parameter(s)",
            tool_action_summary(tool),
            tool.parameters.len()
        ),
        Action::Budget { kind, amount } => format!("budget {kind} amount {amount}"),
    }
}

fn tool_action_summary(tool: &taskfence_core::ToolAction) -> String {
    format!("{} {}.{}", tool.protocol, tool.tool, tool.operation)
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

fn render_task_summary(task: &TaskSummary) -> String {
    let status = task
        .status
        .as_ref()
        .map(|status| format!("{status:?}"))
        .unwrap_or_else(|| "-".into());
    let goal = task
        .goal
        .as_deref()
        .map(compact_cell)
        .unwrap_or_else(|| "-".into());

    let mut rendered = String::new();
    rendered.push_str("Task summary\n");
    rendered.push_str(&format!("  id: {}\n", task.task_id.0));
    rendered.push_str(&format!("  status: {status}\n"));
    rendered.push_str(&format!("  goal: {goal}\n"));
    rendered.push_str(&format!("  artifacts: {}\n", artifact_flags(task)));
    rendered.push_str(&format!("  evidence: {}\n", task.task_dir));
    if task.warnings.is_empty() {
        rendered.push_str("  warnings: -\n");
    } else {
        rendered.push_str("  warnings:\n");
        for warning in &task.warnings {
            rendered.push_str("    - ");
            rendered.push_str(&compact_cell(warning));
            rendered.push('\n');
        }
    }
    rendered
}

fn render_task_status(task: &TaskSummary) -> String {
    let status = task
        .status
        .as_ref()
        .map(|status| format!("{status:?}"))
        .unwrap_or_else(|| "-".into());

    let mut rendered = String::new();
    rendered.push_str("Task status\n");
    rendered.push_str(&format!("  id: {}\n", task.task_id.0));
    rendered.push_str(&format!("  status: {status}\n"));
    rendered.push_str(&format!("  evidence: {}\n", task.task_dir));
    if task.warnings.is_empty() {
        rendered.push_str("  warnings: -\n");
    } else {
        rendered.push_str("  warnings:\n");
        for warning in &task.warnings {
            rendered.push_str("    - ");
            rendered.push_str(&compact_cell(warning));
            rendered.push('\n');
        }
    }
    rendered
}

fn render_task_inputs(contents: &str) -> String {
    if contents.ends_with('\n') {
        contents.to_owned()
    } else {
        format!("{contents}\n")
    }
}

fn render_task_artifacts(task_id: &TaskId, artifacts: &TaskArtifacts) -> String {
    let mut rendered = String::new();
    rendered.push_str("Task artifacts\n");
    rendered.push_str(&format!("  task: {}\n", task_id.0));
    rendered.push_str(&format!("  evidence: {}\n", artifacts.task_dir));
    rendered.push_str("KIND\tBYTES\tPATH\n");
    for file in &artifacts.files {
        rendered.push_str(match file.kind {
            TaskArtifactKind::Evidence => "evidence",
            TaskArtifactKind::Artifact => "artifact",
        });
        rendered.push('\t');
        rendered.push_str(&file.size_bytes.to_string());
        rendered.push('\t');
        rendered.push_str(file.relative_path.as_str());
        rendered.push('\n');
    }
    rendered
}

fn render_task_comparison(left: &TaskSummary, right: &TaskSummary) -> String {
    let mut rendered = String::new();
    rendered.push_str("Task comparison\n");
    rendered.push_str("FIELD\tLEFT\tRIGHT\n");
    push_comparison_row(&mut rendered, "task", &left.task_id.0, &right.task_id.0);
    push_comparison_row(
        &mut rendered,
        "status",
        &task_status_text(left),
        &task_status_text(right),
    );
    push_comparison_row(
        &mut rendered,
        "goal",
        left.goal.as_deref().unwrap_or("-"),
        right.goal.as_deref().unwrap_or("-"),
    );
    push_comparison_row(
        &mut rendered,
        "artifacts",
        &artifact_flags(left),
        &artifact_flags(right),
    );
    push_comparison_row(
        &mut rendered,
        "warnings",
        &left.warnings.len().to_string(),
        &right.warnings.len().to_string(),
    );
    push_comparison_row(
        &mut rendered,
        "evidence",
        left.task_dir.as_str(),
        right.task_dir.as_str(),
    );
    rendered
}

fn push_comparison_row(rendered: &mut String, field: &str, left: &str, right: &str) {
    rendered.push_str(field);
    rendered.push('\t');
    rendered.push_str(&compact_cell(left));
    rendered.push('\t');
    rendered.push_str(&compact_cell(right));
    rendered.push('\n');
}

fn task_status_text(task: &TaskSummary) -> String {
    task.status
        .as_ref()
        .map(|status| format!("{status:?}"))
        .unwrap_or_else(|| "-".into())
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

fn run_gateway_call(
    task_file: Utf8PathBuf,
    protocol: String,
    tool: String,
    operation: String,
    params: Vec<String>,
    approval_mode: GatewayApprovalMode,
) -> taskfence_core::Result<()> {
    let task = load_task_file(&task_file)?;
    let action = normalize_tool_action(ToolAction {
        protocol,
        tool,
        operation,
        parameters: parse_gateway_params(params)?,
    })?;
    let artifacts = LocalArtifactStore::in_workspace();
    let artifact_refs = artifacts.create_task_dir(&task)?;
    artifacts.write_resolved_task(&task)?;
    let events_path = artifact_refs
        .events
        .clone()
        .unwrap_or_else(|| artifact_refs.task_dir.join("events.jsonl"));
    let jsonl = LocalJsonlAuditLogger::new(events_path)?;
    let audit = CollectingAuditLogger::new(&jsonl);
    let state = InMemoryStateStore::new();

    gateway_transition(&audit, &state, &task.id, TaskStatus::Created)?;
    audit.record(AuditEvent::TaskCreated {
        task_id: task.id.clone(),
        at: time::OffsetDateTime::now_utc(),
        goal: task.goal.clone(),
    })?;
    gateway_transition(&audit, &state, &task.id, TaskStatus::Validating)?;

    let registry = gateway_tool_registry(&task)?;
    let supported_protocols = task
        .gateway
        .tools
        .iter()
        .map(|tool| tool.protocol.clone())
        .collect::<Vec<_>>();
    let policy = BuiltInPolicyEngine;
    let adapter = gateway_adapter_for(&task, &action);
    let secret_broker = LocalRedactedSecretBroker;

    gateway_transition(&audit, &state, &task.id, TaskStatus::Preparing)?;
    gateway_transition(&audit, &state, &task.id, TaskStatus::Running)?;
    let context = ToolExecutionContext {
        task_dir: Some(artifact_refs.task_dir.clone()),
        artifact_dir: Some(artifact_refs.task_dir.join("artifacts")),
    };
    let execution = match approval_mode {
        GatewayApprovalMode::FailClosed => {
            let approval = LocalApprovalEngine::fail_closed()
                .with_actor("gateway")
                .with_source("local-fail-closed");
            execute_gateway_action(
                &task,
                action,
                context,
                &policy,
                &audit,
                &registry,
                supported_protocols,
                &approval,
                adapter.as_ref(),
                &secret_broker,
            )?
        }
        GatewayApprovalMode::Approved => {
            let approval = LocalApprovalEngine::preconfigured(ApprovalDecision::Approved)
                .with_actor("gateway")
                .with_source("local-approved");
            execute_gateway_action(
                &task,
                action,
                context,
                &policy,
                &audit,
                &registry,
                supported_protocols,
                &approval,
                adapter.as_ref(),
                &secret_broker,
            )?
        }
        GatewayApprovalMode::External => {
            let approval = LocalExternalApprovalEngine::new(task.workspace_host_path.clone())
                .with_actor("gateway");
            execute_gateway_action(
                &task,
                action,
                context,
                &policy,
                &audit,
                &registry,
                supported_protocols,
                &approval,
                adapter.as_ref(),
                &secret_broker,
            )?
        }
    };

    gateway_transition(&audit, &state, &task.id, TaskStatus::CollectingArtifacts)?;
    record_gateway_artifacts(&audit, &task, &execution)?;

    let final_status = gateway_execution_status(&execution);
    gateway_transition(&audit, &state, &task.id, TaskStatus::Reporting)?;
    gateway_transition(&audit, &state, &task.id, final_status.clone())?;

    let report = MarkdownReportGenerator::new();
    let report_path = report.generate(&task, &artifact_refs, &audit.events()?)?;
    audit.record(AuditEvent::Artifact {
        task_id: task.id.clone(),
        at: time::OffsetDateTime::now_utc(),
        kind: "report".into(),
        path: report_path.clone(),
    })?;

    print_gateway_call_summary(
        &task,
        &artifact_refs,
        &execution,
        &final_status,
        &report_path,
    );

    match final_status {
        TaskStatus::Succeeded => Ok(()),
        status => Err(TaskFenceError::Gateway(format!(
            "gateway call {} finished with status {status:?}",
            task.id.0
        ))),
    }
}

fn run_gateway_spool_process(
    task_file: Utf8PathBuf,
    request_file: Utf8PathBuf,
    approval_mode: GatewayApprovalMode,
) -> taskfence_core::Result<()> {
    let task = load_task_file(&task_file)?;
    let artifacts = LocalArtifactStore::in_workspace();
    let artifact_refs = artifacts.create_task_dir(&task)?;
    artifacts.write_resolved_task(&task)?;
    let events_path = artifact_refs
        .events
        .clone()
        .unwrap_or_else(|| artifact_refs.task_dir.join("events.jsonl"));
    let jsonl = LocalJsonlAuditLogger::new(events_path)?;
    let audit = CollectingAuditLogger::new(&jsonl);
    let state = InMemoryStateStore::new();
    let spool_paths = GatewaySpoolPaths::for_task(&task)?;
    let request_id = gateway_spool_request_id_from_path(&request_file)
        .unwrap_or_else(|_| "malformed-request".into());

    gateway_transition(&audit, &state, &task.id, TaskStatus::Created)?;
    audit.record(AuditEvent::TaskCreated {
        task_id: task.id.clone(),
        at: time::OffsetDateTime::now_utc(),
        goal: task.goal.clone(),
    })?;
    gateway_transition(&audit, &state, &task.id, TaskStatus::Validating)?;

    let request = match read_gateway_spool_request(&spool_paths, &request_file) {
        Ok(request) => request,
        Err(err) => {
            return finish_gateway_spool_error(
                &task,
                &artifact_refs,
                &audit,
                &state,
                &spool_paths,
                GatewaySpoolFailure {
                    request_id,
                    state: GatewaySpoolResponseState::MalformedRequest,
                    kind: ToolExecutionErrorKind::InvalidParameters,
                    message: format!("gateway spool request was malformed: {err}"),
                },
            );
        }
    };

    if request.cancel {
        return finish_gateway_spool_error(
            &task,
            &artifact_refs,
            &audit,
            &state,
            &spool_paths,
            GatewaySpoolFailure {
                request_id: request.request_id,
                state: GatewaySpoolResponseState::Cancelled,
                kind: ToolExecutionErrorKind::AdapterFailed,
                message: "gateway spool request was cancelled".into(),
            },
        );
    }

    if request.timeout_seconds == Some(0) {
        return finish_gateway_spool_error(
            &task,
            &artifact_refs,
            &audit,
            &state,
            &spool_paths,
            GatewaySpoolFailure {
                request_id: request.request_id,
                state: GatewaySpoolResponseState::TimedOut,
                kind: ToolExecutionErrorKind::AdapterFailed,
                message: "gateway spool request timed out before execution".into(),
            },
        );
    }

    let registry = gateway_tool_registry(&task)?;
    let supported_protocols = task
        .gateway
        .tools
        .iter()
        .map(|tool| tool.protocol.clone())
        .collect::<Vec<_>>();
    let policy = BuiltInPolicyEngine;
    let adapter = gateway_adapter_for(&task, &request.action);
    let secret_broker = LocalRedactedSecretBroker;

    gateway_transition(&audit, &state, &task.id, TaskStatus::Preparing)?;
    gateway_transition(&audit, &state, &task.id, TaskStatus::Running)?;
    let context = ToolExecutionContext {
        task_dir: Some(artifact_refs.task_dir.clone()),
        artifact_dir: Some(artifact_refs.task_dir.join("artifacts")),
    };
    let execution = match approval_mode {
        GatewayApprovalMode::FailClosed => {
            let approval = LocalApprovalEngine::fail_closed()
                .with_actor("gateway")
                .with_source("spool-fail-closed");
            execute_gateway_action(
                &task,
                request.action,
                context,
                &policy,
                &audit,
                &registry,
                supported_protocols,
                &approval,
                adapter.as_ref(),
                &secret_broker,
            )?
        }
        GatewayApprovalMode::Approved => {
            let approval = LocalApprovalEngine::preconfigured(ApprovalDecision::Approved)
                .with_actor("gateway")
                .with_source("spool-approved");
            execute_gateway_action(
                &task,
                request.action,
                context,
                &policy,
                &audit,
                &registry,
                supported_protocols,
                &approval,
                adapter.as_ref(),
                &secret_broker,
            )?
        }
        GatewayApprovalMode::External => {
            let approval = LocalExternalApprovalEngine::new(task.workspace_host_path.clone())
                .with_actor("gateway");
            execute_gateway_action(
                &task,
                request.action,
                context,
                &policy,
                &audit,
                &registry,
                supported_protocols,
                &approval,
                adapter.as_ref(),
                &secret_broker,
            )?
        }
    };

    gateway_transition(&audit, &state, &task.id, TaskStatus::CollectingArtifacts)?;
    record_gateway_artifacts(&audit, &task, &execution)?;
    let response = GatewaySpoolResponse::from_execution(request.request_id, execution);
    let response_path = write_gateway_spool_response(&spool_paths, &response)?;
    audit.record(AuditEvent::Artifact {
        task_id: task.id.clone(),
        at: time::OffsetDateTime::now_utc(),
        kind: "gateway_spool_response".into(),
        path: response_path.clone(),
    })?;

    let final_status = gateway_spool_task_status(&response);
    gateway_transition(&audit, &state, &task.id, TaskStatus::Reporting)?;
    gateway_transition(&audit, &state, &task.id, final_status.clone())?;

    let report = MarkdownReportGenerator::new();
    let report_path = report.generate(&task, &artifact_refs, &audit.events()?)?;
    audit.record(AuditEvent::Artifact {
        task_id: task.id.clone(),
        at: time::OffsetDateTime::now_utc(),
        kind: "report".into(),
        path: report_path,
    })?;

    print_gateway_spool_summary(&task, &response, &response_path);
    match final_status {
        TaskStatus::Succeeded => Ok(()),
        status => Err(TaskFenceError::Gateway(format!(
            "gateway spool request {} finished with status {status:?}",
            response.request_id
        ))),
    }
}

fn finish_gateway_spool_error(
    task: &ResolvedTask,
    artifact_refs: &ArtifactRefs,
    audit: &CollectingAuditLogger<'_>,
    state: &dyn StateStore,
    spool_paths: &GatewaySpoolPaths,
    failure: GatewaySpoolFailure,
) -> taskfence_core::Result<()> {
    let response = GatewaySpoolResponse::error(
        failure.request_id,
        failure.state,
        failure.kind,
        failure.message.clone(),
    );
    let response_path = write_gateway_spool_response(spool_paths, &response)?;
    audit.record(AuditEvent::Artifact {
        task_id: task.id.clone(),
        at: time::OffsetDateTime::now_utc(),
        kind: "gateway_spool_response".into(),
        path: response_path.clone(),
    })?;
    let status = gateway_spool_task_status(&response);
    gateway_transition(audit, state, &task.id, TaskStatus::Reporting)?;
    gateway_transition(audit, state, &task.id, status.clone())?;
    let report = MarkdownReportGenerator::new();
    let report_path = report.generate(task, artifact_refs, &audit.events()?)?;
    audit.record(AuditEvent::Artifact {
        task_id: task.id.clone(),
        at: time::OffsetDateTime::now_utc(),
        kind: "report".into(),
        path: report_path,
    })?;
    print_gateway_spool_summary(task, &response, &response_path);
    Err(TaskFenceError::Gateway(format!(
        "gateway spool request {} finished with status {status:?}: {}",
        response.request_id, failure.message
    )))
}

struct GatewaySpoolFailure {
    request_id: String,
    state: GatewaySpoolResponseState,
    kind: ToolExecutionErrorKind,
    message: String,
}

#[allow(clippy::too_many_arguments)]
fn execute_gateway_action(
    task: &ResolvedTask,
    action: ToolAction,
    context: ToolExecutionContext,
    policy: &BuiltInPolicyEngine,
    audit: &dyn AuditLogger,
    registry: &InMemoryToolRegistry,
    supported_protocols: Vec<String>,
    approval: &dyn ApprovalEngine,
    adapter: &dyn ToolAdapter,
    secret_broker: &LocalRedactedSecretBroker,
) -> taskfence_core::Result<ToolExecution> {
    let mediator = GatewayExecutor::new(
        taskfence_gateway::GatewayMediator::new(policy, audit)
            .with_tool_registry(registry)
            .with_supported_protocols(supported_protocols)
            .with_approval(approval),
        audit,
        adapter,
    )
    .with_secret_broker(secret_broker);

    mediator.execute_tool_action(task, action, context)
}

fn parse_gateway_params(
    params: Vec<String>,
) -> taskfence_core::Result<std::collections::BTreeMap<String, RedactedValue>> {
    let mut parsed = std::collections::BTreeMap::new();
    for param in params {
        let (key, value) = param.split_once('=').ok_or_else(|| {
            TaskFenceError::Config(format!("gateway parameter must be KEY=VALUE: {param}"))
        })?;
        let key = key.trim().to_owned();
        if key.is_empty() {
            return Err(TaskFenceError::Config(
                "gateway parameter key must not be empty".into(),
            ));
        }
        parsed.insert(key, RedactedValue::Plain(value.to_owned()));
    }
    Ok(parsed)
}

fn gateway_tool_registry(task: &ResolvedTask) -> taskfence_core::Result<InMemoryToolRegistry> {
    let tools = task
        .gateway
        .tools
        .iter()
        .map(|tool| RegisteredTool::new(&tool.protocol, &tool.tool, &tool.operation))
        .collect::<taskfence_core::Result<Vec<_>>>()?;
    Ok(InMemoryToolRegistry::new(tools))
}

fn gateway_adapter_for(task: &ResolvedTask, action: &ToolAction) -> Box<dyn ToolAdapter> {
    task.gateway
        .tools
        .iter()
        .find(|tool| {
            tool.protocol == action.protocol
                && tool.tool == action.tool
                && tool.operation == action.operation
        })
        .map(|tool| Box::new(LocalFixtureToolAdapter::new(tool.clone())) as Box<dyn ToolAdapter>)
        .unwrap_or_else(|| {
            Box::new(UnsupportedGatewayAdapter::new(
                "unregistered",
                "unregistered",
            ))
        })
}

fn gateway_transition(
    audit: &CollectingAuditLogger<'_>,
    state: &dyn StateStore,
    task_id: &TaskId,
    status: TaskStatus,
) -> taskfence_core::Result<()> {
    state.set_status(task_id, status.clone())?;
    audit.record(AuditEvent::TaskStatusChanged {
        task_id: task_id.clone(),
        at: time::OffsetDateTime::now_utc(),
        status,
    })
}

fn record_gateway_artifacts(
    audit: &CollectingAuditLogger<'_>,
    task: &ResolvedTask,
    execution: &ToolExecution,
) -> taskfence_core::Result<()> {
    let Some(result) = &execution.result else {
        return Ok(());
    };
    for path in &result.artifacts {
        audit.record(AuditEvent::Artifact {
            task_id: task.id.clone(),
            at: time::OffsetDateTime::now_utc(),
            kind: "gateway_fixture".into(),
            path: path.clone(),
        })?;
    }
    Ok(())
}

fn gateway_execution_status(execution: &ToolExecution) -> TaskStatus {
    match execution.error.as_ref().map(|error| &error.kind) {
        None => TaskStatus::Succeeded,
        Some(ToolExecutionErrorKind::PolicyDenied)
        | Some(ToolExecutionErrorKind::ApprovalDeniedOrTimedOut) => TaskStatus::Denied,
        Some(
            ToolExecutionErrorKind::UnsupportedProtocol
            | ToolExecutionErrorKind::UnsupportedTool
            | ToolExecutionErrorKind::UnregisteredTool
            | ToolExecutionErrorKind::InvalidParameters
            | ToolExecutionErrorKind::AdapterFailed
            | ToolExecutionErrorKind::SecretUnavailable,
        ) => TaskStatus::Failed,
    }
}

fn gateway_spool_task_status(response: &GatewaySpoolResponse) -> TaskStatus {
    match response.state {
        GatewaySpoolResponseState::Succeeded => TaskStatus::Succeeded,
        GatewaySpoolResponseState::Denied => TaskStatus::Denied,
        GatewaySpoolResponseState::TimedOut => TaskStatus::TimedOut,
        GatewaySpoolResponseState::Cancelled => TaskStatus::Cancelled,
        GatewaySpoolResponseState::Failed
        | GatewaySpoolResponseState::MalformedRequest
        | GatewaySpoolResponseState::UnsupportedAction => TaskStatus::Failed,
    }
}

fn print_gateway_call_summary(
    task: &ResolvedTask,
    artifacts: &ArtifactRefs,
    execution: &ToolExecution,
    status: &TaskStatus,
    report_path: &Utf8PathBuf,
) {
    println!("Gateway call finished");
    println!("  task: {}", task.id.0);
    println!("  status: {status:?}");
    println!("  artifacts: {}", artifacts.task_dir);
    println!("  report: {report_path}");
    match (&execution.result, &execution.error) {
        (Some(result), None) => println!("  result: {}", result.summary),
        (_, Some(error)) => println!("  error: {:?}: {}", error.kind, error.message),
        (None, None) => println!("  result: no result or error recorded"),
    }
}

fn print_gateway_spool_summary(
    task: &ResolvedTask,
    response: &GatewaySpoolResponse,
    response_path: &Utf8PathBuf,
) {
    println!("Gateway spool request processed");
    println!("  task: {}", task.id.0);
    println!("  request: {}", response.request_id);
    println!("  state: {:?}", response.state);
    println!("  response: {response_path}");
    if let Some(execution) = &response.execution {
        match (&execution.result, &execution.error) {
            (Some(result), None) => println!("  result: {}", result.summary),
            (_, Some(error)) => println!("  error: {:?}: {}", error.kind, error.message),
            (None, None) => println!("  result: no result or error recorded"),
        }
    } else if let Some(error) = &response.error {
        println!("  error: {:?}: {}", error.kind, error.message);
    }
}

struct CollectingAuditLogger<'a> {
    inner: &'a dyn AuditLogger,
    events: std::sync::Mutex<Vec<AuditEvent>>,
}

impl<'a> CollectingAuditLogger<'a> {
    fn new(inner: &'a dyn AuditLogger) -> Self {
        Self {
            inner,
            events: std::sync::Mutex::new(Vec::new()),
        }
    }

    fn events(&self) -> taskfence_core::Result<Vec<AuditEvent>> {
        self.events
            .lock()
            .map(|events| events.clone())
            .map_err(|_| TaskFenceError::Audit("audit event collection lock poisoned".into()))
    }
}

impl AuditLogger for CollectingAuditLogger<'_> {
    fn record(&self, event: AuditEvent) -> taskfence_core::Result<()> {
        self.inner.record(event.clone())?;
        self.events
            .lock()
            .map_err(|_| TaskFenceError::Audit("audit event collection lock poisoned".into()))?
            .push(event);
        Ok(())
    }
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
    use std::collections::BTreeMap;
    use std::fs;
    use std::thread;
    use std::time::{Duration, Instant};
    use taskfence_core::{
        Action, ActionDecision, ApprovalDecision, RedactedValue, RiskLevel, ToolAction,
    };
    use taskfence_gateway::GatewaySpoolRequest;
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
    fn parses_validate_task_file() {
        let cli = Cli::try_parse_from(["taskfence", "validate", "task.yaml"]).unwrap();

        match cli.command {
            Command::Validate { task_file } => {
                assert_eq!(task_file, Utf8PathBuf::from("task.yaml"))
            }
            other => panic!("expected validate command, got {other:?}"),
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
    fn parses_gateway_call() {
        let cli = Cli::try_parse_from([
            "taskfence",
            "gateway",
            "call",
            "task.yaml",
            "github",
            "read_issue",
            "--protocol",
            "mcp",
            "--param",
            "number=42",
        ])
        .unwrap();

        match cli.command {
            Command::Gateway {
                command:
                    GatewayCommand::Call {
                        task_file,
                        protocol,
                        tool,
                        operation,
                        params,
                        approve,
                        external_approval,
                    },
            } => {
                assert_eq!(task_file, Utf8PathBuf::from("task.yaml"));
                assert_eq!(protocol, "mcp");
                assert_eq!(tool, "github");
                assert_eq!(operation, "read_issue");
                assert_eq!(params, vec!["number=42"]);
                assert!(!approve);
                assert!(!external_approval);
            }
            other => panic!("expected gateway call command, got {other:?}"),
        }
    }

    #[test]
    fn parses_gateway_spool_process() {
        let cli = Cli::try_parse_from([
            "taskfence",
            "gateway",
            "spool",
            "process",
            "task.yaml",
            ".taskfence/tasks/task-1/gateway-spool/requests/request-1.json",
            "--approve",
        ])
        .unwrap();

        match cli.command {
            Command::Gateway {
                command:
                    GatewayCommand::Spool {
                        command:
                            GatewaySpoolCommand::Process {
                                task_file,
                                request_file,
                                approve,
                                external_approval,
                            },
                    },
            } => {
                assert_eq!(task_file, Utf8PathBuf::from("task.yaml"));
                assert_eq!(
                    request_file,
                    Utf8PathBuf::from(
                        ".taskfence/tasks/task-1/gateway-spool/requests/request-1.json"
                    )
                );
                assert!(approve);
                assert!(!external_approval);
            }
            other => panic!("expected gateway spool process command, got {other:?}"),
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
    fn parses_task_task_id() {
        let cli = Cli::try_parse_from(["taskfence", "task", "task-123"]).unwrap();

        match cli.command {
            Command::Task { task_id, workspace } => {
                assert_eq!(task_id, "task-123");
                assert_eq!(workspace, Utf8PathBuf::from("."));
            }
            other => panic!("expected task command, got {other:?}"),
        }
    }

    #[test]
    fn parses_task_workspace() {
        let cli =
            Cli::try_parse_from(["taskfence", "task", "task-123", "--workspace", "repo"]).unwrap();

        match cli.command {
            Command::Task { task_id, workspace } => {
                assert_eq!(task_id, "task-123");
                assert_eq!(workspace, Utf8PathBuf::from("repo"));
            }
            other => panic!("expected task command, got {other:?}"),
        }
    }

    #[test]
    fn parses_inputs_task_id() {
        let cli = Cli::try_parse_from(["taskfence", "inputs", "task-123"]).unwrap();

        match cli.command {
            Command::Inputs { task_id, workspace } => {
                assert_eq!(task_id, "task-123");
                assert_eq!(workspace, Utf8PathBuf::from("."));
            }
            other => panic!("expected inputs command, got {other:?}"),
        }
    }

    #[test]
    fn parses_inputs_workspace() {
        let cli = Cli::try_parse_from(["taskfence", "inputs", "task-123", "--workspace", "repo"])
            .unwrap();

        match cli.command {
            Command::Inputs { task_id, workspace } => {
                assert_eq!(task_id, "task-123");
                assert_eq!(workspace, Utf8PathBuf::from("repo"));
            }
            other => panic!("expected inputs command, got {other:?}"),
        }
    }

    #[test]
    fn parses_artifacts_task_id() {
        let cli = Cli::try_parse_from(["taskfence", "artifacts", "task-123"]).unwrap();

        match cli.command {
            Command::Artifacts { task_id, workspace } => {
                assert_eq!(task_id, "task-123");
                assert_eq!(workspace, Utf8PathBuf::from("."));
            }
            other => panic!("expected artifacts command, got {other:?}"),
        }
    }

    #[test]
    fn parses_artifacts_workspace() {
        let cli =
            Cli::try_parse_from(["taskfence", "artifacts", "task-123", "--workspace", "repo"])
                .unwrap();

        match cli.command {
            Command::Artifacts { task_id, workspace } => {
                assert_eq!(task_id, "task-123");
                assert_eq!(workspace, Utf8PathBuf::from("repo"));
            }
            other => panic!("expected artifacts command, got {other:?}"),
        }
    }

    #[test]
    fn parses_compare_task_ids() {
        let cli = Cli::try_parse_from(["taskfence", "compare", "left-task", "right-task"]).unwrap();

        match cli.command {
            Command::Compare {
                left_task_id,
                right_task_id,
                workspace,
            } => {
                assert_eq!(left_task_id, "left-task");
                assert_eq!(right_task_id, "right-task");
                assert_eq!(workspace, Utf8PathBuf::from("."));
            }
            other => panic!("expected compare command, got {other:?}"),
        }
    }

    #[test]
    fn parses_compare_workspace() {
        let cli = Cli::try_parse_from([
            "taskfence",
            "compare",
            "left-task",
            "right-task",
            "--workspace",
            "repo",
        ])
        .unwrap();

        match cli.command {
            Command::Compare {
                left_task_id,
                right_task_id,
                workspace,
            } => {
                assert_eq!(left_task_id, "left-task");
                assert_eq!(right_task_id, "right-task");
                assert_eq!(workspace, Utf8PathBuf::from("repo"));
            }
            other => panic!("expected compare command, got {other:?}"),
        }
    }

    #[test]
    fn parses_status_task_id() {
        let cli = Cli::try_parse_from(["taskfence", "status", "task-123"]).unwrap();

        match cli.command {
            Command::Status { task_id, workspace } => {
                assert_eq!(task_id, "task-123");
                assert_eq!(workspace, Utf8PathBuf::from("."));
            }
            other => panic!("expected status command, got {other:?}"),
        }
    }

    #[test]
    fn parses_status_workspace() {
        let cli = Cli::try_parse_from(["taskfence", "status", "task-123", "--workspace", "repo"])
            .unwrap();

        match cli.command {
            Command::Status { task_id, workspace } => {
                assert_eq!(task_id, "task-123");
                assert_eq!(workspace, Utf8PathBuf::from("repo"));
            }
            other => panic!("expected status command, got {other:?}"),
        }
    }

    #[test]
    fn parses_events_task_id() {
        let cli = Cli::try_parse_from(["taskfence", "events", "task-123"]).unwrap();

        match cli.command {
            Command::Events { task_id, workspace } => {
                assert_eq!(task_id, "task-123");
                assert_eq!(workspace, Utf8PathBuf::from("."));
            }
            other => panic!("expected events command, got {other:?}"),
        }
    }

    #[test]
    fn parses_events_workspace() {
        let cli = Cli::try_parse_from(["taskfence", "events", "task-123", "--workspace", "repo"])
            .unwrap();

        match cli.command {
            Command::Events { task_id, workspace } => {
                assert_eq!(task_id, "task-123");
                assert_eq!(workspace, Utf8PathBuf::from("repo"));
            }
            other => panic!("expected events command, got {other:?}"),
        }
    }

    #[test]
    fn parses_approvals_default_workspace() {
        let cli = Cli::try_parse_from(["taskfence", "approvals"]).unwrap();

        match cli.command {
            Command::Approvals { workspace } => assert_eq!(workspace, Utf8PathBuf::from(".")),
            other => panic!("expected approvals command, got {other:?}"),
        }
    }

    #[test]
    fn parses_approvals_workspace() {
        let cli = Cli::try_parse_from(["taskfence", "approvals", "--workspace", "repo"]).unwrap();

        match cli.command {
            Command::Approvals { workspace } => assert_eq!(workspace, Utf8PathBuf::from("repo")),
            other => panic!("expected approvals command, got {other:?}"),
        }
    }

    #[test]
    fn parses_approval_approval_id() {
        let cli = Cli::try_parse_from(["taskfence", "approval", "approval-123"]).unwrap();

        match cli.command {
            Command::Approval {
                approval_id,
                workspace,
            } => {
                assert_eq!(approval_id, "approval-123");
                assert_eq!(workspace, Utf8PathBuf::from("."));
            }
            other => panic!("expected approval command, got {other:?}"),
        }
    }

    #[test]
    fn parses_approval_workspace() {
        let cli = Cli::try_parse_from([
            "taskfence",
            "approval",
            "approval-123",
            "--workspace",
            "repo",
        ])
        .unwrap();

        match cli.command {
            Command::Approval {
                approval_id,
                workspace,
            } => {
                assert_eq!(approval_id, "approval-123");
                assert_eq!(workspace, Utf8PathBuf::from("repo"));
            }
            other => panic!("expected approval command, got {other:?}"),
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
    fn init_writes_starter_task_file() {
        let temp = tempfile::tempdir().unwrap();
        let path = Utf8PathBuf::from_path_buf(temp.path().join("taskfence.yaml")).unwrap();

        execute(Cli {
            command: Command::Init { path: path.clone() },
        })
        .unwrap();

        let contents = fs::read_to_string(&path).unwrap();
        assert!(contents.contains("id: \"local-demo\""));
        assert!(contents.contains("command: \"echo\""));
        assert!(contents.contains("default: \"disabled\""));

        let task = taskfence_config::parse_task_file(&path, &contents).unwrap();
        assert_eq!(task.id.0, "local-demo");
        assert_eq!(task.goal, "Describe the task goal here");
        assert_eq!(
            task.workspace_host_path,
            canonical_utf8(path.parent().unwrap())
        );
        assert_eq!(task.agent.command, "echo");
        assert_eq!(task.agent.args, vec!["hello from TaskFence"]);
    }

    #[test]
    fn init_creates_parent_directories() {
        let temp = tempfile::tempdir().unwrap();
        let path = Utf8PathBuf::from_path_buf(temp.path().join("tasks/fix.yaml")).unwrap();

        execute(Cli {
            command: Command::Init { path: path.clone() },
        })
        .unwrap();

        assert!(path.is_file());
        let contents = fs::read_to_string(&path).unwrap();
        let task = taskfence_config::parse_task_file(&path, &contents).unwrap();
        assert_eq!(
            task.workspace_host_path,
            canonical_utf8(path.parent().unwrap())
        );
    }

    #[test]
    fn init_refuses_to_overwrite_existing_file() {
        let temp = tempfile::tempdir().unwrap();
        let path = Utf8PathBuf::from_path_buf(temp.path().join("taskfence.yaml")).unwrap();
        fs::write(&path, "existing task file\n").unwrap();

        let err = execute(Cli {
            command: Command::Init { path: path.clone() },
        })
        .unwrap_err();

        assert!(
            matches!(err, TaskFenceError::Config(message) if message.contains("already exists"))
        );
        assert_eq!(fs::read_to_string(&path).unwrap(), "existing task file\n");
    }

    #[test]
    fn validate_accepts_local_task_file_without_running() {
        let temp = tempfile::tempdir().unwrap();
        let workspace = Utf8PathBuf::from_path_buf(temp.path().join("repo")).unwrap();
        fs::create_dir(&workspace).unwrap();
        let task_file = Utf8PathBuf::from_path_buf(temp.path().join("task.yaml")).unwrap();
        fs::write(&task_file, task_yaml("validate-ok", &workspace, "echo")).unwrap();

        let summary = validate_task_file_summary(task_file).unwrap();

        assert_eq!(summary.task_id, "validate-ok");
        assert_eq!(summary.workspace, canonical_utf8(&workspace));
        assert_eq!(summary.agent_command, "echo ok");
        assert!(summary.command_policy.contains("allow"));
        assert_eq!(summary.sandbox_image, "taskfence/runner:latest");
        assert_eq!(summary.network_mode, "none");
        assert_eq!(summary.mount_count, 1);
        assert!(!workspace.join(".taskfence").exists());
    }

    #[test]
    fn validate_rejects_denied_planned_command() {
        let temp = tempfile::tempdir().unwrap();
        let workspace = Utf8PathBuf::from_path_buf(temp.path().join("repo")).unwrap();
        fs::create_dir(&workspace).unwrap();
        let task_file = Utf8PathBuf::from_path_buf(temp.path().join("task.yaml")).unwrap();
        fs::write(
            &task_file,
            task_yaml_with_command_policy("validate-deny", &workspace, &[], &[], &["echo"]),
        )
        .unwrap();

        let err = validate_task_file_summary(task_file).unwrap_err();

        assert!(
            matches!(err, TaskFenceError::Policy(message) if message.contains("planned agent command rejected") && message.contains("deny"))
        );
        assert!(!workspace.join(".taskfence").exists());
    }

    #[test]
    fn validate_rejects_unsupported_domain_allowlist() {
        let temp = tempfile::tempdir().unwrap();
        let workspace = Utf8PathBuf::from_path_buf(temp.path().join("repo")).unwrap();
        fs::create_dir(&workspace).unwrap();
        let task_file = Utf8PathBuf::from_path_buf(temp.path().join("task.yaml")).unwrap();
        fs::write(
            &task_file,
            task_yaml_with_network_allowlist("validate-domain", &workspace, &["example.com"]),
        )
        .unwrap();

        let err = validate_task_file_summary(task_file).unwrap_err();

        assert!(
            matches!(err, TaskFenceError::Runner(message) if message.contains("cannot enforce domain allowlists"))
        );
        assert!(!workspace.join(".taskfence").exists());
    }

    #[test]
    fn validate_rejects_agent_command_with_embedded_arguments() {
        let temp = tempfile::tempdir().unwrap();
        let workspace = Utf8PathBuf::from_path_buf(temp.path().join("repo")).unwrap();
        fs::create_dir(&workspace).unwrap();
        let task_file = Utf8PathBuf::from_path_buf(temp.path().join("task.yaml")).unwrap();
        fs::write(
            &task_file,
            task_yaml_with_agent_command("validate-command-shape", &workspace, "echo ok", &[]),
        )
        .unwrap();

        let err = validate_task_file_summary(task_file).unwrap_err();

        assert!(
            matches!(err, TaskFenceError::Config(message) if message.contains("put arguments in agent.args"))
        );
        assert!(!workspace.join(".taskfence").exists());
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
    fn task_command_reads_local_task_summary() {
        let temp = tempfile::tempdir().unwrap();
        let workspace = Utf8PathBuf::from_path_buf(temp.path().join("repo")).unwrap();
        fs::create_dir(&workspace).unwrap();
        let task_file = Utf8PathBuf::from_path_buf(temp.path().join("task.yaml")).unwrap();
        fs::write(&task_file, task_yaml("cli-detail", &workspace, "echo ok")).unwrap();

        run_task_with_runner(
            task_file,
            &FakeRunner::succeeding(),
            RunApprovalMode::FailClosed,
        )
        .unwrap();

        let text = task_text(workspace, &TaskId("cli-detail".into())).unwrap();

        assert!(text.contains("Task summary\n"));
        assert!(text.contains("  id: cli-detail\n"));
        assert!(text.contains("  status: Succeeded\n"));
        assert!(text.contains("  goal: CLI test\n"));
        assert!(text.contains("  artifacts: report,diff\n"));
        assert!(text.contains(".taskfence/tasks/cli-detail"));
        assert!(text.contains("  warnings: -\n"));
    }

    #[test]
    fn inputs_command_reads_local_resolved_task_input() {
        let temp = tempfile::tempdir().unwrap();
        let workspace = Utf8PathBuf::from_path_buf(temp.path().join("repo")).unwrap();
        fs::create_dir(&workspace).unwrap();
        let task_file = Utf8PathBuf::from_path_buf(temp.path().join("task.yaml")).unwrap();
        fs::write(&task_file, task_yaml("cli-inputs", &workspace, "echo ok")).unwrap();

        run_task_with_runner(
            task_file,
            &FakeRunner::succeeding(),
            RunApprovalMode::FailClosed,
        )
        .unwrap();

        let text = inputs_text(workspace, &TaskId("cli-inputs".into())).unwrap();

        assert!(text.ends_with('\n'));
        assert!(text.contains("\"id\": \"cli-inputs\""));
        assert!(text.contains("\"goal\": \"CLI test\""));
        assert!(text.contains("\"workspace_host_path\""));
        assert!(text.contains("\"permissions\""));
        assert!(!text.contains("Task summary"));
    }

    #[test]
    fn artifacts_command_reads_local_task_artifact_manifest() {
        let temp = tempfile::tempdir().unwrap();
        let workspace = Utf8PathBuf::from_path_buf(temp.path().join("repo")).unwrap();
        fs::create_dir(&workspace).unwrap();
        let task_file = Utf8PathBuf::from_path_buf(temp.path().join("task.yaml")).unwrap();
        fs::write(
            &task_file,
            task_yaml("cli-artifacts", &workspace, "echo ok"),
        )
        .unwrap();

        run_task_with_runner(
            task_file,
            &FakeRunner::succeeding(),
            RunApprovalMode::FailClosed,
        )
        .unwrap();
        let artifacts_dir = workspace.join(".taskfence/tasks/cli-artifacts/artifacts");
        fs::write(artifacts_dir.join("manifest.json"), "{}\n").unwrap();
        fs::create_dir(artifacts_dir.join("nested")).unwrap();

        let text = artifacts_text(workspace, &TaskId("cli-artifacts".into())).unwrap();

        assert!(text.contains("Task artifacts\n"));
        assert!(text.contains("  task: cli-artifacts\n"));
        assert!(text.contains(".taskfence/tasks/cli-artifacts"));
        assert!(text.contains("KIND\tBYTES\tPATH\n"));
        assert!(text.contains("artifact\t3\tartifacts/manifest.json\n"));
        assert!(text.contains("evidence\t"));
        assert!(text.contains("\ttask.resolved.json\n"));
        assert!(text.contains("\tevents.jsonl\n"));
        assert!(text.contains("\tdiff.patch\n"));
        assert!(text.contains("\treport.md\n"));
        assert!(!text.contains("nested"));
        assert!(!text.contains("\"id\": \"cli-artifacts\""));
    }

    #[test]
    fn compare_command_reads_local_task_summaries() {
        let temp = tempfile::tempdir().unwrap();
        let workspace = Utf8PathBuf::from_path_buf(temp.path().join("repo")).unwrap();
        fs::create_dir(&workspace).unwrap();
        let left_task_file = Utf8PathBuf::from_path_buf(temp.path().join("left.yaml")).unwrap();
        fs::write(
            &left_task_file,
            task_yaml("compare-left", &workspace, "echo ok"),
        )
        .unwrap();
        let right_task_file = Utf8PathBuf::from_path_buf(temp.path().join("right.yaml")).unwrap();
        fs::write(
            &right_task_file,
            task_yaml("compare-right", &workspace, "echo ok"),
        )
        .unwrap();

        run_task_with_runner(
            left_task_file,
            &FakeRunner::succeeding(),
            RunApprovalMode::FailClosed,
        )
        .unwrap();
        let right_err = run_task_with_runner(
            right_task_file,
            &FakeRunner::failing(7),
            RunApprovalMode::FailClosed,
        )
        .unwrap_err();
        assert!(matches!(right_err, TaskFenceError::Runner(_)));

        let text = compare_text(
            workspace,
            &TaskId("compare-left".into()),
            &TaskId("compare-right".into()),
        )
        .unwrap();

        assert!(text.contains("Task comparison\n"));
        assert!(text.contains("FIELD\tLEFT\tRIGHT\n"));
        assert!(text.contains("task\tcompare-left\tcompare-right\n"));
        assert!(text.contains("status\tSucceeded\tFailed\n"));
        assert!(text.contains("goal\tCLI test\tCLI test\n"));
        assert!(text.contains("artifacts\treport,diff\treport,diff\n"));
        assert!(text.contains("warnings\t0\t0\n"));
        assert!(text.contains(".taskfence/tasks/compare-left"));
        assert!(text.contains(".taskfence/tasks/compare-right"));
        assert!(!text.contains("# Task Report"));
    }

    #[test]
    fn status_command_reads_local_task_status() {
        let temp = tempfile::tempdir().unwrap();
        let workspace = Utf8PathBuf::from_path_buf(temp.path().join("repo")).unwrap();
        fs::create_dir(&workspace).unwrap();
        let task_file = Utf8PathBuf::from_path_buf(temp.path().join("task.yaml")).unwrap();
        fs::write(&task_file, task_yaml("cli-status", &workspace, "echo ok")).unwrap();

        run_task_with_runner(
            task_file,
            &FakeRunner::succeeding(),
            RunApprovalMode::FailClosed,
        )
        .unwrap();

        let text = status_text(workspace, &TaskId("cli-status".into())).unwrap();

        assert!(text.contains("Task status\n"));
        assert!(text.contains("  id: cli-status\n"));
        assert!(text.contains("  status: Succeeded\n"));
        assert!(text.contains(".taskfence/tasks/cli-status"));
        assert!(text.contains("  warnings: -\n"));
    }

    #[test]
    fn events_command_reads_local_task_events() {
        let temp = tempfile::tempdir().unwrap();
        let workspace = Utf8PathBuf::from_path_buf(temp.path().join("repo")).unwrap();
        fs::create_dir(&workspace).unwrap();
        let task_file = Utf8PathBuf::from_path_buf(temp.path().join("task.yaml")).unwrap();
        fs::write(&task_file, task_yaml("cli-events", &workspace, "echo ok")).unwrap();

        run_task_with_runner(
            task_file,
            &FakeRunner::succeeding(),
            RunApprovalMode::FailClosed,
        )
        .unwrap();

        let text = events_text(workspace, &TaskId("cli-events".into())).unwrap();

        assert!(text.contains("Task events\n"));
        assert!(text.contains("  task: cli-events\n"));
        assert!(text.contains("events.jsonl"));
        assert!(text.contains("KIND\tAT\tSUMMARY\n"));
        assert!(text.contains("task-created\t"));
        assert!(text.contains("goal: CLI test"));
        assert!(text.contains("status\t"));
        assert!(text.contains("status Succeeded"));
        assert!(text.contains("runner-exit\t"));
        assert!(text.contains("exit code 0; timed_out false; signal -"));
    }

    #[test]
    fn events_summary_redacts_tool_parameter_values() {
        let events = TaskEvents {
            task_dir: Utf8PathBuf::from("/tmp/taskfence-events/task-1"),
            path: Utf8PathBuf::from("/tmp/taskfence-events/task-1/events.jsonl"),
            events: vec![AuditEvent::PolicyDecision {
                task_id: TaskId("task-1".into()),
                at: time::OffsetDateTime::now_utc(),
                action: Action::ToolCall(ToolAction {
                    protocol: "mcp".into(),
                    tool: "github".into(),
                    operation: "create_pr".into(),
                    parameters: BTreeMap::from([(
                        "token".into(),
                        RedactedValue::Plain("secret-value".into()),
                    )]),
                }),
                decision: ActionDecision::RequireApproval {
                    approval_kind: "tool_call".into(),
                    rule_id: Some("tool-test".into()),
                    reason: "needs review".into(),
                    risk: RiskLevel::Critical,
                },
            }],
        };

        let text = render_task_events(&TaskId("task-1".into()), &events);

        assert!(text.contains("tool call mcp github.create_pr with 1 parameter(s)"));
        assert!(text.contains(
            "requires tool_call approval by rule tool-test: needs review; risk critical"
        ));
        assert!(!text.contains("secret-value"));
    }

    #[test]
    fn gateway_call_writes_allowed_execution_evidence() {
        let (temp, workspace, task_file) = gateway_fixture_task("gateway-allowed");

        run_gateway_call(
            task_file,
            "mcp".into(),
            "github".into(),
            "read_issue".into(),
            vec!["number=42".into()],
            GatewayApprovalMode::FailClosed,
        )
        .unwrap();

        let task_id = TaskId("gateway-allowed".into());
        let events = LocalTaskEvidenceStore::new(workspace.clone())
            .read_events(&task_id)
            .unwrap();
        assert!(
            events.events.iter().any(|event| {
                matches!(
                    event,
                    AuditEvent::ToolExecutionStarted { request, .. }
                        if request.action.tool == "github"
                            && request.action.operation == "read_issue"
                            && matches!(
                                request.action.parameters.get("number"),
                                Some(RedactedValue::Plain(number)) if number == "42"
                            )
                )
            }),
            "missing read_issue start event in {:#?}",
            events.events
        );
        assert!(events.events.iter().any(|event| {
            matches!(
                event,
                AuditEvent::ToolExecutionFinished {
                    execution:
                        ToolExecution {
                            result: Some(result),
                            error: None,
                            ..
                        },
                    ..
                } if result.summary.contains("read fixture issue #42")
            )
        }));

        let event_text = events_text(workspace.clone(), &task_id).unwrap();
        assert!(event_text.contains("tool-started\t"));
        assert!(event_text.contains("tool-finished\t"));
        assert!(event_text.contains("tool execution succeeded"));
        assert!(!event_text.contains("Ship the fixture gateway"));

        let status = status_text(workspace.clone(), &task_id).unwrap();
        assert!(status.contains("  status: Succeeded\n"));

        let report = report_text(workspace.clone(), &task_id).unwrap();
        assert!(report.contains("Tool Executions"));
        assert!(report.contains("read fixture issue #42"));
        assert!(!report.contains("Ship the fixture gateway"));

        let artifacts = artifacts_text(workspace, &task_id).unwrap();
        assert!(artifacts.contains("task.resolved.json"));
        assert!(artifacts.contains("events.jsonl"));
        assert!(artifacts.contains("report.md"));
        drop(temp);
    }

    #[test]
    fn gateway_call_denied_by_policy_writes_failure_evidence_without_artifact() {
        let (_temp, workspace, task_file) = gateway_fixture_task("gateway-denied");

        let err = run_gateway_call(
            task_file,
            "mcp".into(),
            "github".into(),
            "delete_repo".into(),
            Vec::new(),
            GatewayApprovalMode::FailClosed,
        )
        .unwrap_err();

        assert!(
            matches!(err, TaskFenceError::Gateway(message) if message.contains("status Denied"))
        );
        let task_id = TaskId("gateway-denied".into());
        let events = LocalTaskEvidenceStore::new(workspace.clone())
            .read_events(&task_id)
            .unwrap();
        assert!(events.events.iter().any(|event| {
            matches!(
                event,
                AuditEvent::PolicyDecision {
                    decision: ActionDecision::Deny { .. },
                    ..
                }
            )
        }));
        assert!(events.events.iter().any(|event| {
            matches!(
                event,
                AuditEvent::ToolExecutionFinished {
                    execution:
                        ToolExecution {
                            result: None,
                            error: Some(error),
                            ..
                        },
                    ..
                } if error.kind == ToolExecutionErrorKind::PolicyDenied
            )
        }));
        assert!(!workspace
            .join(".taskfence/tasks/gateway-denied/artifacts/github-pr-proposal.json")
            .exists());
        assert!(report_text(workspace, &task_id)
            .unwrap()
            .contains("PolicyDenied"));
    }

    #[test]
    fn gateway_call_unsupported_protocol_fails_closed_with_evidence() {
        let (_temp, workspace, task_file) = gateway_fixture_task("gateway-http");

        let err = run_gateway_call(
            task_file,
            "http".into(),
            "github".into(),
            "read_issue".into(),
            vec!["number=42".into()],
            GatewayApprovalMode::FailClosed,
        )
        .unwrap_err();

        assert!(
            matches!(err, TaskFenceError::Gateway(message) if message.contains("status Failed"))
        );
        let task_id = TaskId("gateway-http".into());
        let events = LocalTaskEvidenceStore::new(workspace.clone())
            .read_events(&task_id)
            .unwrap();
        assert!(events.events.iter().any(|event| {
            matches!(
                event,
                AuditEvent::Error { message, .. }
                    if message.contains("protocol 'http' is not supported")
            )
        }));
        assert!(events.events.iter().any(|event| {
            matches!(
                event,
                AuditEvent::ToolExecutionFinished {
                    execution:
                        ToolExecution {
                            error: Some(error),
                            ..
                        },
                    ..
                } if error.kind == ToolExecutionErrorKind::UnsupportedProtocol
            )
        }));
        assert!(status_text(workspace, &task_id)
            .unwrap()
            .contains("  status: Failed\n"));
    }

    #[test]
    fn gateway_call_unregistered_tool_fails_closed_with_evidence() {
        let (_temp, workspace, task_file) = gateway_fixture_task("gateway-unregistered");

        let err = run_gateway_call(
            task_file,
            "mcp".into(),
            "github".into(),
            "close_issue".into(),
            vec!["number=42".into()],
            GatewayApprovalMode::FailClosed,
        )
        .unwrap_err();

        assert!(
            matches!(err, TaskFenceError::Gateway(message) if message.contains("status Failed"))
        );
        let task_id = TaskId("gateway-unregistered".into());
        let events = LocalTaskEvidenceStore::new(workspace.clone())
            .read_events(&task_id)
            .unwrap();
        assert!(events.events.iter().any(|event| {
            matches!(
                event,
                AuditEvent::Error { message, .. }
                    if message.contains("not registered")
                        && message.contains("mcp github.close_issue")
            )
        }));
        assert!(events.events.iter().any(|event| {
            matches!(
                event,
                AuditEvent::ToolExecutionFinished {
                    execution:
                        ToolExecution {
                            error: Some(error),
                            ..
                        },
                    ..
                } if error.kind == ToolExecutionErrorKind::UnregisteredTool
            )
        }));
        assert!(events_text(workspace, &task_id)
            .unwrap()
            .contains("UnregisteredTool"));
    }

    #[test]
    fn gateway_call_invalid_adapter_parameters_fail_closed_with_evidence() {
        let (_temp, workspace, task_file) = gateway_fixture_task("gateway-invalid-params");

        let err = run_gateway_call(
            task_file,
            "mcp".into(),
            "github".into(),
            "read_issue".into(),
            Vec::new(),
            GatewayApprovalMode::FailClosed,
        )
        .unwrap_err();

        assert!(
            matches!(err, TaskFenceError::Gateway(message) if message.contains("status Failed"))
        );
        let task_id = TaskId("gateway-invalid-params".into());
        let events = LocalTaskEvidenceStore::new(workspace.clone())
            .read_events(&task_id)
            .unwrap();
        assert!(events.events.iter().any(|event| {
            matches!(
                event,
                AuditEvent::ToolExecutionStarted { request, .. }
                    if request.action.operation == "read_issue"
            )
        }));
        assert!(events.events.iter().any(|event| {
            matches!(
                event,
                AuditEvent::ToolExecutionFinished {
                    execution:
                        ToolExecution {
                            error: Some(error),
                            ..
                        },
                    ..
                } if error.kind == ToolExecutionErrorKind::InvalidParameters
                    && error.message.contains("missing required parameter number")
            )
        }));
        assert!(report_text(workspace, &task_id)
            .unwrap()
            .contains("InvalidParameters"));
    }

    #[test]
    fn gateway_call_default_approval_fails_closed_before_adapter_execution() {
        let (_temp, workspace, task_file) = gateway_fixture_task("gateway-approval-default");

        let err = run_gateway_call(
            task_file,
            "mcp".into(),
            "github".into(),
            "create_pr".into(),
            vec!["title=Needs approval".into()],
            GatewayApprovalMode::FailClosed,
        )
        .unwrap_err();

        assert!(
            matches!(err, TaskFenceError::Gateway(message) if message.contains("status Denied"))
        );
        let task_id = TaskId("gateway-approval-default".into());
        let events = LocalTaskEvidenceStore::new(workspace.clone())
            .read_events(&task_id)
            .unwrap();
        assert!(events.events.iter().any(|event| {
            matches!(
                event,
                AuditEvent::ApprovalRequested { record }
                    if record.action
                        == Action::ToolCall(ToolAction {
                            protocol: "mcp".into(),
                            tool: "github".into(),
                            operation: "create_pr".into(),
                            parameters: BTreeMap::from([(
                                "title".into(),
                                RedactedValue::Plain("Needs approval".into()),
                            )]),
                        })
            )
        }));
        assert!(events.events.iter().any(|event| {
            matches!(
                event,
                AuditEvent::ApprovalResolved { record }
                    if record.decision == Some(ApprovalDecision::Denied)
            )
        }));
        assert!(events.events.iter().any(|event| {
            matches!(
                event,
                AuditEvent::ToolExecutionFinished {
                    execution:
                        ToolExecution {
                            error: Some(error),
                            ..
                        },
                    ..
                } if error.kind == ToolExecutionErrorKind::ApprovalDeniedOrTimedOut
            )
        }));
        assert!(!events
            .events
            .iter()
            .any(|event| { matches!(event, AuditEvent::ToolExecutionStarted { .. }) }));
        assert!(!workspace
            .join(".taskfence/tasks/gateway-approval-default/artifacts/github-pr-proposal.json")
            .exists());
    }

    #[test]
    fn gateway_call_approved_create_pr_redacts_secret_references_and_fixture_output() {
        let (_temp, workspace, task_file) = gateway_fixture_task("gateway-secret-redaction");
        let raw_secret = "ghp_phase4rawsecret";

        run_gateway_call(
            task_file,
            "mcp".into(),
            "github".into(),
            "create_pr".into(),
            vec![
                "title=Fixture PR".into(),
                format!("body=token={raw_secret}"),
            ],
            GatewayApprovalMode::Approved,
        )
        .unwrap();

        let task_id = TaskId("gateway-secret-redaction".into());
        let events = LocalTaskEvidenceStore::new(workspace.clone())
            .read_events(&task_id)
            .unwrap();
        assert!(events.events.iter().any(|event| {
            matches!(
                event,
                AuditEvent::ApprovalResolved { record }
                    if record.decision == Some(ApprovalDecision::Approved)
            )
        }));
        assert!(
            events.events.iter().any(|event| {
                matches!(
                    event,
                    AuditEvent::ToolExecutionStarted { request, .. }
                        if matches!(
                            request.action.parameters.get("authorization"),
                            Some(RedactedValue::Redacted { .. })
                        )
                )
            }),
            "missing redacted authorization start event in {:#?}",
            events.events
        );
        assert!(events.events.iter().any(|event| {
            matches!(
                event,
                AuditEvent::ToolExecutionFinished {
                    execution:
                        ToolExecution {
                            result: Some(result),
                            error: None,
                            ..
                        },
                    ..
                } if result
                    .artifacts
                    .iter()
                    .any(|path| path.ends_with("github-pr-proposal.json"))
            )
        }));

        let events_path = workspace.join(".taskfence/tasks/gateway-secret-redaction/events.jsonl");
        let events_jsonl = fs::read_to_string(events_path).unwrap();
        assert!(!events_jsonl.contains(raw_secret));
        assert!(!events_text(workspace.clone(), &task_id)
            .unwrap()
            .contains(raw_secret));
        assert!(!report_text(workspace.clone(), &task_id)
            .unwrap()
            .contains(raw_secret));

        let proposal_path = workspace
            .join(".taskfence/tasks/gateway-secret-redaction/artifacts/github-pr-proposal.json");
        let proposal = fs::read_to_string(proposal_path).unwrap();
        assert!(proposal.contains("[redacted]"));
        assert!(!proposal.contains(raw_secret));
        assert!(!proposal.contains("github_token"));
    }

    #[test]
    fn gateway_spool_process_writes_success_response_and_evidence() {
        let (_temp, workspace, task_file) = gateway_fixture_task("gateway-spool-ok");
        let request_path = write_gateway_spool_request(
            &task_file,
            "request-42",
            ToolAction {
                protocol: "mcp".into(),
                tool: "github".into(),
                operation: "read_issue".into(),
                parameters: BTreeMap::from([("number".into(), RedactedValue::Plain("42".into()))]),
            },
            Some(30),
            false,
        );

        run_gateway_spool_process(task_file, request_path, GatewayApprovalMode::FailClosed)
            .unwrap();

        let task_id = TaskId("gateway-spool-ok".into());
        let response = read_gateway_spool_response(&workspace, &task_id, "request-42");
        assert_eq!(response.state, GatewaySpoolResponseState::Succeeded);
        assert!(response
            .execution
            .as_ref()
            .and_then(|execution| execution.result.as_ref())
            .is_some_and(|result| result.summary.contains("read fixture issue #42")));

        let events = LocalTaskEvidenceStore::new(workspace.clone())
            .read_events(&task_id)
            .unwrap();
        assert!(events.events.iter().any(|event| {
            matches!(
                event,
                AuditEvent::Artifact { kind, path, .. }
                    if kind == "gateway_spool_response"
                        && path.ends_with("gateway-spool/responses/request-42.json")
            )
        }));
        assert!(report_text(workspace.clone(), &task_id)
            .unwrap()
            .contains("read fixture issue #42"));
        assert!(artifacts_text(workspace, &task_id)
            .unwrap()
            .contains("gateway-spool"));
    }

    #[test]
    fn gateway_spool_process_records_cancelled_request() {
        let (_temp, workspace, task_file) = gateway_fixture_task("gateway-spool-cancelled");
        let request_path = write_gateway_spool_request(
            &task_file,
            "request-cancelled",
            ToolAction {
                protocol: "mcp".into(),
                tool: "github".into(),
                operation: "read_issue".into(),
                parameters: BTreeMap::from([("number".into(), RedactedValue::Plain("42".into()))]),
            },
            Some(30),
            true,
        );

        let err =
            run_gateway_spool_process(task_file, request_path, GatewayApprovalMode::FailClosed)
                .unwrap_err();

        assert!(
            matches!(err, TaskFenceError::Gateway(message) if message.contains("status Cancelled"))
        );
        let task_id = TaskId("gateway-spool-cancelled".into());
        let response = read_gateway_spool_response(&workspace, &task_id, "request-cancelled");
        assert_eq!(response.state, GatewaySpoolResponseState::Cancelled);
        assert!(response
            .error
            .is_some_and(|error| error.message.contains("cancelled")));
        assert!(status_text(workspace, &task_id)
            .unwrap()
            .contains("  status: Cancelled\n"));
    }

    #[test]
    fn gateway_spool_process_records_zero_timeout_request() {
        let (_temp, workspace, task_file) = gateway_fixture_task("gateway-spool-timeout");
        let request_path = write_gateway_spool_request(
            &task_file,
            "request-timeout",
            ToolAction {
                protocol: "mcp".into(),
                tool: "github".into(),
                operation: "read_issue".into(),
                parameters: BTreeMap::from([("number".into(), RedactedValue::Plain("42".into()))]),
            },
            Some(0),
            false,
        );

        let err =
            run_gateway_spool_process(task_file, request_path, GatewayApprovalMode::FailClosed)
                .unwrap_err();

        assert!(
            matches!(err, TaskFenceError::Gateway(message) if message.contains("status TimedOut"))
        );
        let task_id = TaskId("gateway-spool-timeout".into());
        let response = read_gateway_spool_response(&workspace, &task_id, "request-timeout");
        assert_eq!(response.state, GatewaySpoolResponseState::TimedOut);
        assert!(response
            .error
            .is_some_and(|error| error.message.contains("timed out")));
        assert!(status_text(workspace, &task_id)
            .unwrap()
            .contains("  status: TimedOut\n"));
    }

    #[test]
    fn gateway_spool_process_records_malformed_request() {
        let (_temp, workspace, task_file) = gateway_fixture_task("gateway-spool-malformed");
        let task = load_task_file(&task_file).unwrap();
        let spool_paths = GatewaySpoolPaths::for_task(&task).unwrap();
        fs::create_dir_all(&spool_paths.requests_dir).unwrap();
        let request_path = spool_paths.request_path("request-malformed").unwrap();
        fs::write(&request_path, b"{not-json").unwrap();

        let err =
            run_gateway_spool_process(task_file, request_path, GatewayApprovalMode::FailClosed)
                .unwrap_err();

        assert!(
            matches!(err, TaskFenceError::Gateway(message) if message.contains("was malformed"))
        );
        let task_id = TaskId("gateway-spool-malformed".into());
        let response = read_gateway_spool_response(&workspace, &task_id, "request-malformed");
        assert_eq!(response.state, GatewaySpoolResponseState::MalformedRequest);
        assert!(response
            .error
            .is_some_and(|error| error.kind == ToolExecutionErrorKind::InvalidParameters));
        assert!(status_text(workspace, &task_id)
            .unwrap()
            .contains("  status: Failed\n"));
    }

    #[test]
    fn approvals_command_reads_local_approval_list() {
        let temp = tempfile::tempdir().unwrap();
        let workspace = Utf8PathBuf::from_path_buf(temp.path().join("repo")).unwrap();
        fs::create_dir(&workspace).unwrap();
        create_pending_approval(&workspace, "approval-cli-pending");
        let denied_id = create_pending_approval(&workspace, "approval-cli-denied");
        LocalApprovalStore::new(workspace.clone())
            .resolve(&denied_id, ApprovalDecision::Denied)
            .unwrap();

        let text = approvals_text(workspace).unwrap();

        assert!(text.contains("APPROVAL ID\tTASK ID\tSTATUS\tREQUESTED\tACTION\n"));
        assert!(text.contains("approval-cli-denied\tpending-approval\tdenied\t"));
        assert!(text.contains("approval-cli-pending\tpending-approval\tpending\t"));
        assert!(text.contains("command `echo ok`"));
    }

    #[test]
    fn approval_command_reads_local_approval_detail() {
        let temp = tempfile::tempdir().unwrap();
        let workspace = Utf8PathBuf::from_path_buf(temp.path().join("repo")).unwrap();
        fs::create_dir(&workspace).unwrap();
        let approval_id = create_pending_approval(&workspace, "approval-cli-detail");

        let text = approval_text(workspace, &approval_id).unwrap();

        assert!(text.contains("Approval record\n"));
        assert!(text.contains("  id: approval-cli-detail\n"));
        assert!(text.contains("  task: pending-approval\n"));
        assert!(text.contains("  status: pending\n"));
        assert!(text.contains("  actor: local\n"));
        assert!(text.contains("  source: external\n"));
        assert!(text.contains("  resolved: -\n"));
        assert!(text.contains("  action: command `echo ok`\n"));
        assert!(text.contains(
            "  policy: requires command approval by rule test: test approval; risk high\n"
        ));
    }

    #[test]
    fn approval_detail_redacts_tool_parameter_values() {
        let record = taskfence_core::ApprovalRecord {
            id: ApprovalId("approval-tool-detail".into()),
            task_id: TaskId("tool-task".into()),
            actor: "gateway".into(),
            source: Some("mcp".into()),
            requested_at: time::OffsetDateTime::now_utc(),
            resolved_at: None,
            action: Action::ToolCall(ToolAction {
                protocol: "mcp".into(),
                tool: "github".into(),
                operation: "create_pr".into(),
                parameters: BTreeMap::from([(
                    "token".into(),
                    RedactedValue::Plain("secret-value".into()),
                )]),
            }),
            policy_decision: ActionDecision::RequireApproval {
                approval_kind: "tool_call".into(),
                rule_id: Some("tool-test".into()),
                reason: "needs review".into(),
                risk: RiskLevel::Critical,
            },
            decision: None,
        };

        let text = render_approval_record(&record);

        assert!(text.contains("tool call mcp github.create_pr with 1 parameter(s)"));
        assert!(text.contains(
            "requires tool_call approval by rule tool-test: needs review; risk critical"
        ));
        assert!(!text.contains("secret-value"));
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
    fn status_command_surfaces_missing_task_errors() {
        let temp = tempfile::tempdir().unwrap();
        let workspace = Utf8PathBuf::from_path_buf(temp.path().join("repo")).unwrap();

        let err = status_text(workspace, &TaskId("missing".into())).unwrap_err();

        assert!(
            matches!(err, TaskFenceError::State(message) if message.contains("directory not found"))
        );
    }

    #[test]
    fn inputs_command_surfaces_missing_task_errors() {
        let temp = tempfile::tempdir().unwrap();
        let workspace = Utf8PathBuf::from_path_buf(temp.path().join("repo")).unwrap();

        let err = inputs_text(workspace, &TaskId("missing".into())).unwrap_err();

        assert!(
            matches!(err, TaskFenceError::State(message) if message.contains("directory not found"))
        );
    }

    #[test]
    fn artifacts_command_surfaces_missing_task_errors() {
        let temp = tempfile::tempdir().unwrap();
        let workspace = Utf8PathBuf::from_path_buf(temp.path().join("repo")).unwrap();

        let err = artifacts_text(workspace, &TaskId("missing".into())).unwrap_err();

        assert!(
            matches!(err, TaskFenceError::State(message) if message.contains("directory not found"))
        );
    }

    #[test]
    fn compare_command_surfaces_missing_task_errors() {
        let temp = tempfile::tempdir().unwrap();
        let workspace = Utf8PathBuf::from_path_buf(temp.path().join("repo")).unwrap();

        let err = compare_text(
            workspace,
            &TaskId("missing-left".into()),
            &TaskId("missing-right".into()),
        )
        .unwrap_err();

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

    fn write_gateway_spool_request(
        task_file: &Utf8PathBuf,
        request_id: &str,
        action: ToolAction,
        timeout_seconds: Option<u64>,
        cancel: bool,
    ) -> Utf8PathBuf {
        let task = load_task_file(task_file).unwrap();
        let spool_paths = GatewaySpoolPaths::for_task(&task).unwrap();
        fs::create_dir_all(&spool_paths.requests_dir).unwrap();
        let request_path = spool_paths.request_path(request_id).unwrap();
        let request = GatewaySpoolRequest {
            request_id: request_id.into(),
            action,
            timeout_seconds,
            cancel,
        };
        let bytes = serde_json::to_vec_pretty(&request).unwrap();
        fs::write(&request_path, bytes).unwrap();
        request_path
    }

    fn read_gateway_spool_response(
        workspace: &Utf8PathBuf,
        task_id: &TaskId,
        request_id: &str,
    ) -> GatewaySpoolResponse {
        let path = workspace
            .join(".taskfence")
            .join("tasks")
            .join(task_id.0.as_str())
            .join("gateway-spool")
            .join("responses")
            .join(format!("{request_id}.json"));
        serde_json::from_slice(&fs::read(path).unwrap()).unwrap()
    }

    fn gateway_fixture_task(id: &str) -> (tempfile::TempDir, Utf8PathBuf, Utf8PathBuf) {
        let temp = tempfile::tempdir().unwrap();
        let workspace = Utf8PathBuf::from_path_buf(temp.path().join("repo")).unwrap();
        fs::create_dir(&workspace).unwrap();
        let fixtures_dir = workspace.join("fixtures");
        fs::create_dir(&fixtures_dir).unwrap();
        let fixture_path = fixtures_dir.join("github.json");
        fs::write(
            &fixture_path,
            r#"{
  "repository": "taskfence/example",
  "default_branch": "main",
  "issues": [
    {
      "number": 42,
      "title": "Ship the fixture gateway",
      "state": "open",
      "body": "Use local evidence only"
    }
  ]
}
"#,
        )
        .unwrap();
        let task_file = Utf8PathBuf::from_path_buf(temp.path().join("task.yaml")).unwrap();
        fs::write(&task_file, gateway_task_yaml(id, &workspace, &fixture_path)).unwrap();

        (temp, workspace, task_file)
    }

    fn gateway_task_yaml(id: &str, workspace: &Utf8PathBuf, fixture_path: &Utf8PathBuf) -> String {
        format!(
            r#"id: "{id}"
goal: "Gateway CLI test"
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
      - "echo"
  network:
    default: "disabled"
  tools:
    allow:
      - "github.read_issue"
    approval_required:
      - "github.create_pr"
    deny:
      - "github.delete_repo"
secrets:
  expose_to_agent: false
  available_to_gateway:
    - name: "github_token"
      use_for:
        - "github.create_pr"
gateway:
  tools:
    - protocol: "mcp"
      tool: "github"
      operation: "read_issue"
      connector:
        type: "local_fixture"
        kind: "github"
        path: "{fixture_path}"
    - protocol: "mcp"
      tool: "github"
      operation: "create_pr"
      connector:
        type: "local_fixture"
        kind: "github"
        path: "{fixture_path}"
      secret_refs:
        - name: "github_token"
          parameter: "authorization"
          scope: "github.create_pr"
    - protocol: "mcp"
      tool: "github"
      operation: "delete_repo"
      connector:
        type: "local_fixture"
        kind: "github"
        path: "{fixture_path}"
"#
        )
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

    fn task_yaml_with_network_allowlist(
        id: &str,
        workspace: &Utf8PathBuf,
        allow_domains: &[&str],
    ) -> String {
        let mut domains = String::new();
        for domain in allow_domains {
            domains.push_str("      - \"");
            domains.push_str(domain);
            domains.push_str("\"\n");
        }

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
      - "echo"
  network:
    default: "deny"
    allow_domains:
{domains}"#
        )
    }

    fn task_yaml_with_agent_command(
        id: &str,
        workspace: &Utf8PathBuf,
        command: &str,
        args: &[&str],
    ) -> String {
        let rendered_args = if args.is_empty() {
            "  args: []\n".to_owned()
        } else {
            let mut rendered_args = String::from("  args:\n");
            for arg in args {
                rendered_args.push_str("    - \"");
                rendered_args.push_str(arg);
                rendered_args.push_str("\"\n");
            }
            rendered_args
        };

        format!(
            r#"id: "{id}"
goal: "CLI test"
workspace: "{workspace}"
agent:
  type: "generic"
  command: "{command}"
{rendered_args}sandbox:
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
      - "echo"
  network:
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

    fn canonical_utf8(path: &camino::Utf8Path) -> Utf8PathBuf {
        Utf8PathBuf::from_path_buf(fs::canonicalize(path.as_std_path()).unwrap()).unwrap()
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
