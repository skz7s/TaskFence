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
    ApprovalRecord, AuditEvent, ExitStatus, LogStream, NetworkDefault, Orchestrator, ResolvedTask,
    RiskLevel, Runner, TaskFenceError, TaskId, TaskStatus, TaskValidation,
};
use taskfence_policy::BuiltInPolicyEngine;
use taskfence_report::MarkdownReportGenerator;
use taskfence_runner::DockerRunner;
use taskfence_state::{
    InMemoryStateStore, LocalTaskEvidenceStore, TaskEvents, TaskLogs, TaskSummary,
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
        Command::Logs { task_id, workspace } => show_logs(workspace, task_id),
        Command::Diff { task_id, workspace } => show_diff(workspace, task_id),
        Command::Tasks { workspace } => show_tasks(workspace),
        Command::Task { task_id, workspace } => show_task(workspace, task_id),
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
            "tool call {} {}.{} with {} parameter(s)",
            tool.protocol,
            tool.tool,
            tool.operation,
            tool.parameters.len()
        ),
        Action::Budget { kind, amount } => format!("budget {kind} amount {amount}"),
    }
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
