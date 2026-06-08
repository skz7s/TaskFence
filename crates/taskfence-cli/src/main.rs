use camino::Utf8PathBuf;
use clap::{Parser, Subcommand};
use serde::Serialize;
use std::fs::{self, OpenOptions};
use std::io::{BufRead, BufReader, ErrorKind, Write};
use std::net::{TcpListener, TcpStream};
use std::process::ExitCode;
use taskfence_agent::GenericAgentAdapter;
use taskfence_approval::{LocalApprovalEngine, LocalApprovalStore, LocalExternalApprovalEngine};
use taskfence_artifacts::LocalArtifactStore;
use taskfence_audit::LocalJsonlAuditLogger;
use taskfence_config::load_task_file;
use taskfence_core::{
    validate_task_for_run, Action, ActionDecision, ApprovalDecision, ApprovalEngine, ApprovalId,
    ApprovalRecord, ArtifactRefs, ArtifactStore, AuditEvent, AuditLogger, ExitStatus,
    GatewayConnectorConfig, GatewayMode, LogStream, NetworkDefault, Orchestrator, RedactedValue,
    ReportGenerator, ResolvedTask, RiskLevel, Runner, SandboxKind, StateStore, TaskFenceError,
    TaskId, TaskResult, TaskStatus, TaskValidation, ToolAction, ToolExecution,
    ToolExecutionContext, ToolExecutionErrorKind, GATEWAY_EGRESS_TOOL_NAME,
    GATEWAY_EGRESS_TOOL_OPERATION, GATEWAY_EGRESS_TOOL_PROTOCOL,
};
use taskfence_gateway::{
    gateway_spool_request_id_from_path, normalize_tool_action, read_gateway_spool_request,
    write_gateway_spool_response, DatabaseConnectorAdapter, EnterpriseConnectorAdapter,
    EnvironmentSecretBroker, GatewayEgressAdapter, GatewayExecutor, GatewaySpoolPaths,
    GatewaySpoolResponse, GatewaySpoolResponseState, GitHubRestAdapter, InMemoryToolRegistry,
    LocalFixtureToolAdapter, LocalRedactedSecretBroker, PostgresDatabaseClient, RegisteredTool,
    SecretBroker, ToolAdapter, UnsupportedGatewayAdapter, UreqEnterpriseHttpClient,
    UreqGatewayEgressClient, UreqGitHubClient,
};
use taskfence_policy::BuiltInPolicyEngine;
use taskfence_report::{ComplianceReportGenerator, MarkdownReportGenerator};
use taskfence_runner::ExpandedRunner;
use taskfence_state::{
    AuditExportSinkConfig, AuditExportSinkKind, InMemoryStateStore, LocalReviewIndex,
    LocalTaskComparison, LocalTaskEvidenceStore, LocalTaskReview, LocalTeamStateStore,
    OrganizationPolicy, RbacGrant, ReplayEvaluation, ReplayPlan, ReplayRunRecord, TaskArtifactKind,
    TaskArtifacts, TaskEvents, TaskLogs, TaskSummary, TeamAuditExportStatus, TeamRole,
    TeamStateService, TeamTaskRecord, WorkerLeaseState,
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
    /// Render compliance evidence from structured local task events.
    Compliance {
        /// Task ID to report.
        task_id: String,
        /// Workspace that owns the .taskfence task evidence directory.
        #[arg(long, default_value = ".")]
        workspace: Utf8PathBuf,
        /// Output Markdown path. Defaults to the task artifact directory.
        #[arg(long)]
        output: Option<Utf8PathBuf>,
    },
    /// Build a local review page from workspace evidence.
    Review {
        /// Workspace that owns the .taskfence task evidence directory.
        #[arg(long, default_value = ".")]
        workspace: Utf8PathBuf,
        /// Write the generated page here instead of .taskfence/review/index.html.
        #[arg(long)]
        output: Option<Utf8PathBuf>,
        /// Serve the local review page on 127.0.0.1 until interrupted.
        #[arg(long)]
        serve: bool,
        /// Port for --serve, or 0 to ask the OS for an available port.
        #[arg(long, default_value_t = 0)]
        port: u16,
    },
    /// Plan or execute replay from saved task evidence.
    Replay {
        #[command(subcommand)]
        command: ReplayCommand,
    },
    /// Manage the workspace-local structured state index.
    State {
        #[command(subcommand)]
        command: StateCommand,
    },
    /// Manage persistent team state and worker leases without requiring local mode.
    Team {
        #[command(subcommand)]
        command: TeamCommand,
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
enum ReplayCommand {
    /// Show replay inputs, blockers, and determinism limits for one local task.
    Plan {
        /// Task ID to plan replay for.
        task_id: String,
        /// Workspace that owns the .taskfence task evidence directory.
        #[arg(long, default_value = ".")]
        workspace: Utf8PathBuf,
    },
    /// Execute a supported local replay from saved structured task evidence.
    Run {
        /// Task ID to replay.
        task_id: String,
        /// Workspace that owns the .taskfence task evidence directory.
        #[arg(long, default_value = ".")]
        workspace: Utf8PathBuf,
        /// Override the replay task id. Defaults to <task-id>-replay.
        #[arg(long)]
        replay_id: Option<String>,
        /// Execute despite recorded non-deterministic limitations.
        #[arg(long)]
        accept_limitations: bool,
    },
}

#[derive(Debug, Subcommand)]
enum StateCommand {
    /// Refresh and print the workspace-local structured state index.
    Index {
        /// Workspace that owns the .taskfence task evidence directory.
        #[arg(long, default_value = ".")]
        workspace: Utf8PathBuf,
        /// Read the existing index instead of rebuilding it from structured evidence.
        #[arg(long)]
        read_only: bool,
    },
}

#[derive(Debug, Subcommand)]
enum TeamCommand {
    /// Show or initialize a durable local team state file.
    State {
        /// Durable local team state JSON file.
        #[arg(long, default_value = ".taskfence/team/state.json")]
        state_file: Utf8PathBuf,
        /// Organization name.
        #[arg(long, default_value = "default")]
        organization: String,
    },
    /// Import structured local evidence into durable team state.
    MigrateLocal {
        /// Workspace that owns the .taskfence task evidence directory.
        #[arg(long, default_value = ".")]
        workspace: Utf8PathBuf,
        /// Durable local team state JSON file.
        #[arg(long, default_value = ".taskfence/team/state.json")]
        state_file: Utf8PathBuf,
        /// Organization name.
        #[arg(long, default_value = "default")]
        organization: String,
        /// Actor performing the import.
        #[arg(long, default_value = "operator")]
        actor: String,
    },
    /// Export a registered task's structured audit events to a team-owned sink artifact.
    AuditExport {
        /// Task ID already registered in team state.
        task_id: String,
        /// Durable local team state JSON file.
        #[arg(long, default_value = ".taskfence/team/state.json")]
        state_file: Utf8PathBuf,
        /// Organization name.
        #[arg(long, default_value = "default")]
        organization: String,
        /// Actor requesting the export.
        #[arg(long, default_value = "auditor")]
        actor: String,
        /// Sink family.
        #[arg(long, value_enum, default_value_t = AuditExportSinkArg::Siem)]
        sink_kind: AuditExportSinkArg,
        /// Non-secret destination reference such as soc-pipeline.
        #[arg(long)]
        destination_ref: String,
        /// Environment variable name that will hold the sink credential in deployments.
        #[arg(long, default_value = "TASKFENCE_AUDIT_EXPORT_TOKEN")]
        credential_env: String,
    },
    /// Manage durable worker leases.
    Worker {
        #[command(subcommand)]
        command: TeamWorkerCommand,
    },
}

#[derive(Clone, Debug, clap::ValueEnum)]
enum AuditExportSinkArg {
    Siem,
    Webhook,
    ObjectStorage,
}

#[derive(Debug, Subcommand)]
enum TeamWorkerCommand {
    /// Enqueue a task id for team execution.
    Enqueue {
        task_id: String,
        #[arg(long, default_value = ".taskfence/team/state.json")]
        state_file: Utf8PathBuf,
        #[arg(long, default_value = "default")]
        organization: String,
        #[arg(long, default_value = "operator")]
        actor: String,
    },
    /// Lease the next pending task for a worker.
    Lease {
        #[arg(long)]
        worker_id: String,
        #[arg(long, default_value = ".taskfence/team/state.json")]
        state_file: Utf8PathBuf,
        #[arg(long, default_value = "default")]
        organization: String,
        #[arg(long, default_value = "operator")]
        actor: String,
    },
    /// Mark a leased task complete.
    Complete {
        task_id: String,
        #[arg(long)]
        worker_id: String,
        #[arg(long, default_value = ".taskfence/team/state.json")]
        state_file: Utf8PathBuf,
        #[arg(long, default_value = "default")]
        organization: String,
        #[arg(long, default_value = "operator")]
        actor: String,
    },
    /// Mark a leased task failed.
    Fail {
        task_id: String,
        #[arg(long)]
        worker_id: String,
        #[arg(long)]
        reason: String,
        #[arg(long, default_value = ".taskfence/team/state.json")]
        state_file: Utf8PathBuf,
        #[arg(long, default_value = "default")]
        organization: String,
        #[arg(long, default_value = "operator")]
        actor: String,
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
    /// Start a foreground task-scoped local gateway listener.
    Listen {
        /// TaskFence YAML task file.
        task_file: Utf8PathBuf,
        /// Resolve approval-required listener calls with a local approved decision.
        #[arg(long)]
        approve: bool,
        /// Wait for taskfence approve/deny to resolve approval-required listener calls.
        #[arg(long)]
        external_approval: bool,
        /// Port for the loopback listener, or 0 to ask the OS for an available port.
        #[arg(long, default_value_t = 0)]
        port: u16,
        /// Stop after this many requests. Defaults to foreground server mode.
        #[arg(long)]
        once: bool,
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
            let runner = ExpandedRunner::new();
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
            GatewayCommand::Listen {
                task_file,
                approve,
                external_approval,
                port,
                once,
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
                run_gateway_listener(task_file, approval_mode, port, once)
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
        Command::Compliance {
            task_id,
            workspace,
            output,
        } => show_compliance(workspace, task_id, output),
        Command::Review {
            workspace,
            output,
            serve,
            port,
        } => show_review(workspace, output, serve, port),
        Command::Replay { command } => match command {
            ReplayCommand::Plan { task_id, workspace } => show_replay_plan(workspace, task_id),
            ReplayCommand::Run {
                task_id,
                workspace,
                replay_id,
                accept_limitations,
            } => show_replay_run(workspace, task_id, replay_id, accept_limitations),
        },
        Command::State { command } => match command {
            StateCommand::Index {
                workspace,
                read_only,
            } => show_state_index(workspace, read_only),
        },
        Command::Team { command } => match command {
            TeamCommand::State {
                state_file,
                organization,
            } => show_team_state(state_file, organization),
            TeamCommand::MigrateLocal {
                workspace,
                state_file,
                organization,
                actor,
            } => migrate_local_to_team(workspace, state_file, organization, actor),
            TeamCommand::AuditExport {
                task_id,
                state_file,
                organization,
                actor,
                sink_kind,
                destination_ref,
                credential_env,
            } => team_audit_export(
                state_file,
                organization,
                actor,
                task_id,
                sink_kind,
                destination_ref,
                credential_env,
            ),
            TeamCommand::Worker { command } => match command {
                TeamWorkerCommand::Enqueue {
                    task_id,
                    state_file,
                    organization,
                    actor,
                } => team_worker_enqueue(state_file, organization, actor, task_id),
                TeamWorkerCommand::Lease {
                    worker_id,
                    state_file,
                    organization,
                    actor,
                } => team_worker_lease(state_file, organization, actor, worker_id),
                TeamWorkerCommand::Complete {
                    task_id,
                    worker_id,
                    state_file,
                    organization,
                    actor,
                } => team_worker_complete(state_file, organization, actor, task_id, worker_id),
                TeamWorkerCommand::Fail {
                    task_id,
                    worker_id,
                    reason,
                    state_file,
                    organization,
                    actor,
                } => team_worker_fail(state_file, organization, actor, task_id, worker_id, reason),
            },
        },
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
    let runner = ExpandedRunner::new();
    Ok(validation_summary(validate_task_for_run(
        &task, &adapter, &policy, &runner,
    )?))
}

fn validation_summary(validation: TaskValidation) -> ValidationSummary {
    let network_mode = match validation.prepared.runner_kind {
        SandboxKind::RemoteSsh => "ssh",
        _ => match validation.prepared.network.default {
            NetworkDefault::Disabled | NetworkDefault::Deny => "none",
            NetworkDefault::Allow => "bridge",
        },
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

fn show_compliance(
    workspace: Utf8PathBuf,
    task_id: String,
    output: Option<Utf8PathBuf>,
) -> taskfence_core::Result<()> {
    let workspace = make_absolute_utf8(&workspace)?;
    let task_id = TaskId(task_id);
    let store = LocalTaskEvidenceStore::new(workspace);
    let inputs = store.read_inputs(&task_id)?;
    let events = store.read_events(&task_id)?;
    let artifacts = artifact_refs_from_task_dir(&events.task_dir);
    let output = match output {
        Some(path) => make_absolute_utf8(&path)?,
        None => events
            .task_dir
            .join("artifacts")
            .join("compliance-report.md"),
    };
    let generator = ComplianceReportGenerator::new();
    let path = generator.generate(&inputs.task, &artifacts, &events.events, &output)?;
    println!("Compliance report written");
    println!("  task: {}", task_id.0);
    println!("  events: {}", events.events.len());
    println!("  report: {path}");
    Ok(())
}

fn artifact_refs_from_task_dir(task_dir: &Utf8PathBuf) -> ArtifactRefs {
    let maybe = |name: &str| {
        let path = task_dir.join(name);
        path.is_file().then_some(path)
    };
    let gateway_spool = task_dir
        .join("gateway-spool")
        .exists()
        .then_some(task_dir.join("gateway-spool"));
    ArtifactRefs {
        task_dir: task_dir.clone(),
        resolved_task: maybe("task.resolved.json"),
        events: maybe("events.jsonl"),
        stdout: maybe("stdout.log"),
        stderr: maybe("stderr.log"),
        diff: maybe("diff.patch"),
        report: maybe("report.md"),
        gateway_spool,
    }
}

fn show_review(
    workspace: Utf8PathBuf,
    output: Option<Utf8PathBuf>,
    serve: bool,
    port: u16,
) -> taskfence_core::Result<()> {
    if output.is_some() && serve {
        return Err(TaskFenceError::Config(
            "--output and --serve cannot be used together".into(),
        ));
    }
    if serve {
        return serve_review_page(workspace, port);
    }
    let path = write_review_page(workspace, output)?;
    println!("Review page written");
    println!("  path: {path}");
    Ok(())
}

fn show_replay_plan(workspace: Utf8PathBuf, task_id: String) -> taskfence_core::Result<()> {
    let text = replay_plan_text(workspace, &TaskId(task_id))?;
    print!("{text}");
    Ok(())
}

fn show_replay_run(
    workspace: Utf8PathBuf,
    task_id: String,
    replay_id: Option<String>,
    accept_limitations: bool,
) -> taskfence_core::Result<()> {
    let runner = ExpandedRunner::new();
    let record = replay_run_with_runner(
        workspace,
        &TaskId(task_id),
        replay_id.map(TaskId),
        accept_limitations,
        &runner,
    )?;
    println!("Replay finished");
    println!("  source: {}", record.source_task_id.0);
    println!("  replay: {}", record.replay_task_id.0);
    println!(
        "  source_status: {}",
        record
            .evaluation
            .source_status
            .as_ref()
            .map(|status| format!("{status:?}"))
            .unwrap_or_else(|| "-".into())
    );
    println!(
        "  replay_status: {}",
        record
            .evaluation
            .replay_status
            .as_ref()
            .map(|status| format!("{status:?}"))
            .unwrap_or_else(|| "-".into())
    );
    println!("  deterministic: {}", record.deterministic);
    push_stdout_list("differing_fields", &record.evaluation.differing_fields);
    push_stdout_list("notes", &record.evaluation.notes);
    Ok(())
}

fn show_state_index(workspace: Utf8PathBuf, read_only: bool) -> taskfence_core::Result<()> {
    println!("{}", state_index_json(workspace, read_only)?);
    Ok(())
}

fn show_team_state(state_file: Utf8PathBuf, organization: String) -> taskfence_core::Result<()> {
    let mut service = open_local_team_service(state_file, organization)?;
    let tasks = service.list_tasks("viewer")?;
    println!("Team state");
    println!("  organization: {}", service.policy().organization);
    println!("  state_file: {}", service.backend_mut().state_file());
    println!("  tasks: {}", tasks.len());
    Ok(())
}

fn migrate_local_to_team(
    workspace: Utf8PathBuf,
    state_file: Utf8PathBuf,
    organization: String,
    actor: String,
) -> taskfence_core::Result<()> {
    let workspace = make_absolute_utf8(&workspace)?;
    let store = LocalTaskEvidenceStore::new(workspace.clone());
    let migration = store.migration_plan(organization.clone())?;
    let mut service = open_local_team_service(state_file, organization)?;
    for task_id in &migration.tasks {
        let summary = store.read_task_summary(task_id)?;
        service.put_task(
            &actor,
            TeamTaskRecord {
                organization: service.policy().organization.clone(),
                task_id: task_id.clone(),
                status: summary.status,
                goal: summary.goal,
                evidence_dir: summary.task_dir,
            },
        )?;
    }
    println!("Team migration planned");
    println!("  organization: {}", service.policy().organization);
    println!("  workspace: {workspace}");
    println!("  tasks: {}", migration.tasks.len());
    push_stdout_list("warnings", &migration.warnings);
    Ok(())
}

fn team_worker_enqueue(
    state_file: Utf8PathBuf,
    organization: String,
    actor: String,
    task_id: String,
) -> taskfence_core::Result<()> {
    let mut service = open_local_team_service(state_file, organization)?;
    let lease = service.enqueue_task(&actor, TaskId(task_id))?;
    print_worker_lease("Team task enqueued", &lease);
    Ok(())
}

fn team_audit_export(
    state_file: Utf8PathBuf,
    organization: String,
    actor: String,
    task_id: String,
    sink_kind: AuditExportSinkArg,
    destination_ref: String,
    credential_env: String,
) -> taskfence_core::Result<()> {
    let mut service = open_local_team_service(state_file, organization)?;
    let sink = AuditExportSinkConfig::new(
        audit_export_sink_kind(sink_kind),
        destination_ref,
        credential_env,
    )?;
    let record = service.export_task_audit(&actor, &TaskId(task_id), sink)?;
    println!("Team audit export recorded");
    println!("  organization: {}", record.organization);
    println!("  requested_by: {}", record.requested_by);
    println!("  sink: {}", record.sink.destination_ref);
    match record.status {
        TeamAuditExportStatus::Completed { artifact } => {
            println!("  status: completed");
            println!("  artifact: {artifact}");
        }
        TeamAuditExportStatus::Failed { reason } => {
            println!("  status: failed");
            println!("  reason: {reason}");
        }
        TeamAuditExportStatus::Planned => {
            println!("  status: planned");
        }
    }
    Ok(())
}

fn audit_export_sink_kind(kind: AuditExportSinkArg) -> AuditExportSinkKind {
    match kind {
        AuditExportSinkArg::Siem => AuditExportSinkKind::Siem,
        AuditExportSinkArg::Webhook => AuditExportSinkKind::Webhook,
        AuditExportSinkArg::ObjectStorage => AuditExportSinkKind::ObjectStorage,
    }
}

fn team_worker_lease(
    state_file: Utf8PathBuf,
    organization: String,
    actor: String,
    worker_id: String,
) -> taskfence_core::Result<()> {
    let mut service = open_local_team_service(state_file, organization)?;
    match service.lease_next(&actor, &worker_id)? {
        Some(lease) => print_worker_lease("Team task leased", &lease),
        None => println!("No pending team tasks"),
    }
    Ok(())
}

fn team_worker_complete(
    state_file: Utf8PathBuf,
    organization: String,
    actor: String,
    task_id: String,
    worker_id: String,
) -> taskfence_core::Result<()> {
    let mut service = open_local_team_service(state_file, organization)?;
    let lease = service.complete_task(&actor, &TaskId(task_id), &worker_id)?;
    print_worker_lease("Team task completed", &lease);
    Ok(())
}

fn team_worker_fail(
    state_file: Utf8PathBuf,
    organization: String,
    actor: String,
    task_id: String,
    worker_id: String,
    reason: String,
) -> taskfence_core::Result<()> {
    let mut service = open_local_team_service(state_file, organization)?;
    let lease = service.fail_task(&actor, &TaskId(task_id), &worker_id, &reason)?;
    print_worker_lease("Team task failed", &lease);
    Ok(())
}

fn open_local_team_service(
    state_file: Utf8PathBuf,
    organization: String,
) -> taskfence_core::Result<TeamStateService<LocalTeamStateStore>> {
    let organization = organization.trim();
    if organization.is_empty() {
        return Err(TaskFenceError::State(
            "team organization must not be empty".into(),
        ));
    }
    let state_file = make_absolute_utf8(&state_file)?;
    let policy = default_local_team_policy(organization);
    let backend = LocalTeamStateStore::open(policy.clone(), state_file)?;
    Ok(TeamStateService::new(policy, backend))
}

fn default_local_team_policy(organization: &str) -> OrganizationPolicy {
    OrganizationPolicy {
        organization: organization.to_owned(),
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
        require_approval_owner: true,
        allowed_artifact_roots: vec![make_absolute_utf8(&Utf8PathBuf::from(
            ".taskfence/team/artifacts",
        ))
        .unwrap_or_else(|_| Utf8PathBuf::from(".taskfence/team/artifacts"))],
    }
}

fn print_worker_lease(label: &str, lease: &taskfence_state::WorkerLease) {
    println!("{label}");
    println!("  task: {}", lease.task_id.0);
    println!("  organization: {}", lease.organization);
    println!("  state: {}", worker_lease_state_label(&lease.state));
}

fn worker_lease_state_label(state: &WorkerLeaseState) -> String {
    match state {
        WorkerLeaseState::Pending => "pending".into(),
        WorkerLeaseState::Leased { worker_id } => format!("leased:{worker_id}"),
        WorkerLeaseState::Completed => "completed".into(),
        WorkerLeaseState::Failed { reason } => format!("failed:{reason}"),
    }
}

fn make_absolute_utf8(path: &Utf8PathBuf) -> taskfence_core::Result<Utf8PathBuf> {
    if path.is_absolute() {
        Ok(path.clone())
    } else {
        let cwd = std::env::current_dir()
            .map_err(|err| TaskFenceError::Config(format!("failed to read cwd: {err}")))?;
        let cwd = Utf8PathBuf::from_path_buf(cwd).map_err(|path| {
            TaskFenceError::Config(format!("cwd is not valid UTF-8: {}", path.display()))
        })?;
        Ok(cwd.join(path))
    }
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

fn replay_plan_text(workspace: Utf8PathBuf, task_id: &TaskId) -> taskfence_core::Result<String> {
    let store = LocalTaskEvidenceStore::new(workspace);
    Ok(render_replay_plan(&store.replay_plan(task_id)?))
}

fn replay_run_with_runner(
    workspace: Utf8PathBuf,
    task_id: &TaskId,
    replay_id: Option<TaskId>,
    accept_limitations: bool,
    runner: &dyn Runner,
) -> taskfence_core::Result<ReplayRunRecord> {
    let store = LocalTaskEvidenceStore::new(workspace.clone());
    let plan = store.replay_plan(task_id)?;
    validate_replay_plan(&plan, accept_limitations)?;
    let inputs = store.read_inputs(task_id)?;
    let mut replay_task = inputs.task;
    let replay_task_id = replay_id.unwrap_or_else(|| default_replay_task_id(task_id));
    let replay_task_dir = store.task_dir(&replay_task_id)?;
    if replay_task_dir.as_std_path().exists() {
        return Err(TaskFenceError::State(format!(
            "replay task id {} already has evidence at {replay_task_dir}",
            replay_task_id.0
        )));
    }
    replay_task.id = replay_task_id.clone();
    validate_replay_task_contract(&replay_task)?;

    let result = run_replay_resolved_task_with_runner(replay_task, runner)?;
    let comparison =
        LocalTaskEvidenceStore::new(workspace).compare_tasks(task_id, &replay_task_id)?;
    let record = replay_record_from_result(
        task_id,
        &replay_task_id,
        accept_limitations,
        &plan,
        &comparison,
        &result,
    );
    write_replay_record(&result.artifacts.task_dir, &record)?;
    Ok(record)
}

fn validate_replay_plan(plan: &ReplayPlan, accept_limitations: bool) -> taskfence_core::Result<()> {
    if !plan.blockers.is_empty() {
        return Err(TaskFenceError::State(format!(
            "task {} cannot be replayed: {}",
            plan.task_id.0,
            plan.blockers.join("; ")
        )));
    }
    if !plan.limitations.is_empty() && !accept_limitations {
        return Err(TaskFenceError::State(format!(
            "task {} replay has limitations; rerun with --accept-limitations to execute: {}",
            plan.task_id.0,
            plan.limitations.join("; ")
        )));
    }
    Ok(())
}

fn validate_replay_task_contract(task: &ResolvedTask) -> taskfence_core::Result<()> {
    if task
        .gateway
        .tools
        .iter()
        .any(|tool| !matches!(tool.connector, GatewayConnectorConfig::LocalFixture { .. }))
    {
        return Err(TaskFenceError::Gateway(
            "replay execution blocks live or contract-only gateway connector effects".into(),
        ));
    }
    if task.gateway.mode == GatewayMode::LocalListener {
        return Err(TaskFenceError::Gateway(
            "replay execution blocks foreground local listener effects".into(),
        ));
    }
    if !task.permissions.network.allow_domains.is_empty()
        || task.permissions.network.default == NetworkDefault::Allow
    {
        return Err(TaskFenceError::Runner(
            "replay execution requires network disabled in the saved task input".into(),
        ));
    }
    Ok(())
}

fn default_replay_task_id(task_id: &TaskId) -> TaskId {
    TaskId(format!("{}-replay", task_id.0))
}

fn run_replay_resolved_task_with_runner(
    task: ResolvedTask,
    runner: &dyn Runner,
) -> taskfence_core::Result<TaskResult> {
    let approval = LocalApprovalEngine::fail_closed();
    run_resolved_task_collecting_result(task, runner, &approval)
}

fn run_resolved_task_collecting_result(
    task: ResolvedTask,
    runner: &dyn Runner,
    approval: &dyn ApprovalEngine,
) -> taskfence_core::Result<TaskResult> {
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
    orchestrator.run(task)
}

fn replay_record_from_result(
    source_task_id: &TaskId,
    replay_task_id: &TaskId,
    accepted_limitations: bool,
    plan: &ReplayPlan,
    comparison: &LocalTaskComparison,
    result: &TaskResult,
) -> ReplayRunRecord {
    let mut notes = Vec::new();
    if result.status != TaskStatus::Succeeded {
        notes.push(
            result
                .message
                .clone()
                .unwrap_or_else(|| "replay task did not finish successfully".into()),
        );
    }
    if !accepted_limitations && !plan.limitations.is_empty() {
        notes.push("replay limitations were not accepted".into());
    }
    ReplayRunRecord {
        source_task_id: source_task_id.clone(),
        replay_task_id: replay_task_id.clone(),
        accepted_limitations,
        deterministic: plan.deterministic && comparison.differing_fields.is_empty(),
        limitations: plan.limitations.clone(),
        evaluation: ReplayEvaluation {
            source_status: comparison.left.status.clone(),
            replay_status: comparison.right.status.clone(),
            differing_fields: comparison.differing_fields.clone(),
            notes,
        },
    }
}

fn write_replay_record(
    task_dir: &Utf8PathBuf,
    record: &ReplayRunRecord,
) -> taskfence_core::Result<Utf8PathBuf> {
    let path = task_dir.join("artifacts/replay.json");
    let bytes = serde_json::to_vec_pretty(record).map_err(|err| {
        TaskFenceError::State(format!("failed to serialize replay record: {err}"))
    })?;
    fs::write(path.as_std_path(), bytes).map_err(|err| {
        TaskFenceError::State(format!("failed to write replay record {path}: {err}"))
    })?;
    Ok(path)
}

fn state_index_json(workspace: Utf8PathBuf, read_only: bool) -> taskfence_core::Result<String> {
    let store = LocalTaskEvidenceStore::new(workspace);
    let index = if read_only {
        store.read_index()?
    } else {
        store.refresh_index()?
    };
    json_pretty(&index)
}

fn json_pretty<T: Serialize + ?Sized>(value: &T) -> taskfence_core::Result<String> {
    serde_json::to_string_pretty(value)
        .map_err(|err| TaskFenceError::State(format!("failed to serialize local API JSON: {err}")))
}

fn write_review_page(
    workspace: Utf8PathBuf,
    output: Option<Utf8PathBuf>,
) -> taskfence_core::Result<Utf8PathBuf> {
    let store = LocalTaskEvidenceStore::new(workspace.clone());
    let index = store.review_index()?;
    let mut reviews = Vec::new();
    for task in &index.tasks {
        reviews.push(store.read_task_review(&task.task_id)?);
    }
    let approvals = LocalApprovalStore::new(workspace.clone()).list()?;
    let html = render_review_page(&index, &reviews, &approvals);
    let path = output.unwrap_or_else(|| workspace.join(".taskfence/review/index.html"));
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent.as_std_path()).map_err(|err| {
            TaskFenceError::Artifact(format!("failed to create review directory {parent}: {err}"))
        })?;
    }
    fs::write(path.as_std_path(), html).map_err(|err| {
        TaskFenceError::Artifact(format!("failed to write review page {path}: {err}"))
    })?;
    Ok(path)
}

fn serve_review_page(workspace: Utf8PathBuf, port: u16) -> taskfence_core::Result<()> {
    let listener = TcpListener::bind(("127.0.0.1", port)).map_err(|err| {
        TaskFenceError::State(format!("failed to bind local review server: {err}"))
    })?;
    let address = listener
        .local_addr()
        .map_err(|err| TaskFenceError::State(format!("failed to inspect review server: {err}")))?;
    println!("Review page serving");
    println!("  url: http://{address}/");
    println!("  workspace: {workspace}");
    for stream in listener.incoming() {
        let stream = stream.map_err(|err| {
            TaskFenceError::State(format!("review server connection failed: {err}"))
        })?;
        handle_review_http_stream(stream, workspace.clone())?;
    }
    Ok(())
}

fn handle_review_http_stream(
    mut stream: TcpStream,
    workspace: Utf8PathBuf,
) -> taskfence_core::Result<()> {
    let mut reader = BufReader::new(stream.try_clone().map_err(|err| {
        TaskFenceError::State(format!("failed to clone review connection: {err}"))
    })?);
    let request = read_http_request(&mut reader)?;
    let response = review_http_response(&workspace, &request);
    stream
        .write_all(&response)
        .map_err(|err| TaskFenceError::State(format!("failed to write review response: {err}")))
}

#[derive(Debug, PartialEq, Eq)]
struct ReviewHttpRequest {
    method: String,
    path: String,
}

fn read_http_request(reader: &mut impl BufRead) -> taskfence_core::Result<ReviewHttpRequest> {
    let mut request_line = String::new();
    reader.read_line(&mut request_line).map_err(|err| {
        TaskFenceError::State(format!("failed to read review request line: {err}"))
    })?;
    let parts = request_line.split_whitespace().collect::<Vec<_>>();
    if parts.len() < 2 {
        return Err(TaskFenceError::State(
            "review request line is malformed".into(),
        ));
    }
    let mut header = String::new();
    loop {
        header.clear();
        let read = reader.read_line(&mut header).map_err(|err| {
            TaskFenceError::State(format!("failed to read review request header: {err}"))
        })?;
        if read == 0 || header == "\r\n" || header == "\n" {
            break;
        }
    }
    Ok(ReviewHttpRequest {
        method: parts[0].to_owned(),
        path: parts[1].to_owned(),
    })
}

fn review_http_response(workspace: &Utf8PathBuf, request: &ReviewHttpRequest) -> Vec<u8> {
    match (request.method.as_str(), request.path.as_str()) {
        ("GET", "/") | ("GET", "/index.html") => match review_page_html(workspace.clone()) {
            Ok(html) => http_response("200 OK", "text/html; charset=utf-8", &html),
            Err(err) => http_response(
                "500 Internal Server Error",
                "text/plain; charset=utf-8",
                &err.to_string(),
            ),
        },
        ("GET", path) if path == "/api/index" => local_api_index_response(workspace),
        ("GET", path) if path == "/api/tasks" => local_api_tasks_response(workspace),
        ("GET", path) if path.starts_with("/api/task/") => local_api_task_response(workspace, path),
        ("GET", path) if path.starts_with("/api/artifacts/") => {
            local_api_artifacts_response(workspace, path)
        }
        ("GET", path) if path.starts_with("/api/events/") => {
            local_api_events_response(workspace, path)
        }
        ("GET", path) if path.starts_with("/api/logs/") => local_api_logs_response(workspace, path),
        ("GET", path) if path.starts_with("/api/diff/") => local_api_diff_response(workspace, path),
        ("GET", path) if path.starts_with("/api/report/") => {
            local_api_report_response(workspace, path)
        }
        ("GET", path) if path.starts_with("/api/replay/") => {
            local_api_replay_response(workspace, path)
        }
        ("GET", path) if path.starts_with("/api/compare/") => {
            local_api_compare_response(workspace, path)
        }
        ("GET", path) if path == "/api/approvals" => local_api_approvals_response(workspace),
        ("GET", path) if path.starts_with("/api/approval/") => {
            local_api_approval_response(workspace, path)
        }
        ("POST", path) if path.starts_with("/api/approval/") => {
            local_api_resolve_approval_response(workspace, path)
        }
        ("GET", path) if path.starts_with("/artifact/") => {
            artifact_download_response(workspace, path)
        }
        ("POST", path) if path.starts_with("/approval/") => {
            match resolve_review_approval_path(workspace, path) {
                Ok(message) => http_redirect("/", &message),
                Err(err) => http_response(
                    "400 Bad Request",
                    "text/plain; charset=utf-8",
                    &err.to_string(),
                ),
            }
        }
        _ => http_response("404 Not Found", "text/plain; charset=utf-8", "not found"),
    }
}

fn review_page_html(workspace: Utf8PathBuf) -> taskfence_core::Result<String> {
    let store = LocalTaskEvidenceStore::new(workspace.clone());
    let index = store.review_index()?;
    let mut reviews = Vec::new();
    for task in &index.tasks {
        reviews.push(store.read_task_review(&task.task_id)?);
    }
    let approvals = LocalApprovalStore::new(workspace).list()?;
    Ok(render_review_page(&index, &reviews, &approvals))
}

#[derive(Serialize)]
struct LocalApiTaskEvents {
    task_id: TaskId,
    path: Utf8PathBuf,
    events: Vec<LocalApiEvent>,
}

#[derive(Serialize)]
struct LocalApiEvent {
    kind: String,
    at: String,
    summary: String,
}

#[derive(Serialize)]
struct LocalApiApproval {
    id: ApprovalId,
    task_id: TaskId,
    status: String,
    actor: String,
    source: Option<String>,
    requested_at: String,
    resolved_at: Option<String>,
    action: String,
    policy: String,
}

fn local_api_index_response(workspace: &Utf8PathBuf) -> Vec<u8> {
    let store = LocalTaskEvidenceStore::new(workspace.clone());
    match store.refresh_index() {
        Ok(index) => http_json_response("200 OK", &index),
        Err(err) => http_server_error(err),
    }
}

fn local_api_tasks_response(workspace: &Utf8PathBuf) -> Vec<u8> {
    match LocalTaskEvidenceStore::new(workspace.clone()).list_tasks() {
        Ok(tasks) => http_json_response("200 OK", &tasks),
        Err(err) => http_server_error(err),
    }
}

fn local_api_task_response(workspace: &Utf8PathBuf, path: &str) -> Vec<u8> {
    let Some(task_id) = route_suffix(path, "/api/task/") else {
        return http_response("404 Not Found", "text/plain; charset=utf-8", "not found");
    };
    let store = LocalTaskEvidenceStore::new(workspace.clone());
    match store.read_task_review(&TaskId(task_id)) {
        Ok(review) => http_json_response("200 OK", &review),
        Err(err) => http_not_found(err),
    }
}

fn local_api_artifacts_response(workspace: &Utf8PathBuf, path: &str) -> Vec<u8> {
    let Some(task_id) = route_suffix(path, "/api/artifacts/") else {
        return http_response("404 Not Found", "text/plain; charset=utf-8", "not found");
    };
    let store = LocalTaskEvidenceStore::new(workspace.clone());
    match store.read_artifacts(&TaskId(task_id)) {
        Ok(artifacts) => http_json_response("200 OK", &artifacts),
        Err(err) => http_not_found(err),
    }
}

fn local_api_events_response(workspace: &Utf8PathBuf, path: &str) -> Vec<u8> {
    let Some(task_id) = route_suffix(path, "/api/events/") else {
        return http_response("404 Not Found", "text/plain; charset=utf-8", "not found");
    };
    let task_id = TaskId(task_id);
    let store = LocalTaskEvidenceStore::new(workspace.clone());
    match store.read_events(&task_id) {
        Ok(events) => {
            let response = LocalApiTaskEvents {
                task_id,
                path: events.path.clone(),
                events: events
                    .events
                    .iter()
                    .map(|event| LocalApiEvent {
                        kind: event_kind(event).into(),
                        at: event_time(event),
                        summary: event_summary(event),
                    })
                    .collect(),
            };
            http_json_response("200 OK", &response)
        }
        Err(err) => http_not_found(err),
    }
}

fn local_api_logs_response(workspace: &Utf8PathBuf, path: &str) -> Vec<u8> {
    let Some(task_id) = route_suffix(path, "/api/logs/") else {
        return http_response("404 Not Found", "text/plain; charset=utf-8", "not found");
    };
    let store = LocalTaskEvidenceStore::new(workspace.clone());
    match store.read_logs(&TaskId(task_id)) {
        Ok(logs) => http_json_response("200 OK", &logs),
        Err(err) => http_not_found(err),
    }
}

fn local_api_diff_response(workspace: &Utf8PathBuf, path: &str) -> Vec<u8> {
    let Some(task_id) = route_suffix(path, "/api/diff/") else {
        return http_response("404 Not Found", "text/plain; charset=utf-8", "not found");
    };
    let store = LocalTaskEvidenceStore::new(workspace.clone());
    match store.read_diff(&TaskId(task_id)) {
        Ok(diff) => http_json_response("200 OK", &diff),
        Err(err) => http_not_found(err),
    }
}

fn local_api_report_response(workspace: &Utf8PathBuf, path: &str) -> Vec<u8> {
    let Some(task_id) = route_suffix(path, "/api/report/") else {
        return http_response("404 Not Found", "text/plain; charset=utf-8", "not found");
    };
    let store = LocalTaskEvidenceStore::new(workspace.clone());
    match store.read_report(&TaskId(task_id)) {
        Ok(report) => http_json_response("200 OK", &report),
        Err(err) => http_not_found(err),
    }
}

fn local_api_replay_response(workspace: &Utf8PathBuf, path: &str) -> Vec<u8> {
    let Some(task_id) = route_suffix(path, "/api/replay/") else {
        return http_response("404 Not Found", "text/plain; charset=utf-8", "not found");
    };
    let store = LocalTaskEvidenceStore::new(workspace.clone());
    match store.replay_plan(&TaskId(task_id)) {
        Ok(plan) => http_json_response("200 OK", &plan),
        Err(err) => http_not_found(err),
    }
}

fn local_api_compare_response(workspace: &Utf8PathBuf, path: &str) -> Vec<u8> {
    let Some(suffix) = route_suffix(path, "/api/compare/") else {
        return http_response("404 Not Found", "text/plain; charset=utf-8", "not found");
    };
    let parts = suffix.split('/').collect::<Vec<_>>();
    if parts.len() != 2 {
        return http_response(
            "400 Bad Request",
            "text/plain; charset=utf-8",
            "compare route requires /api/compare/<left>/<right>",
        );
    }
    let store = LocalTaskEvidenceStore::new(workspace.clone());
    match store.compare_tasks(&TaskId(parts[0].into()), &TaskId(parts[1].into())) {
        Ok(comparison) => http_json_response("200 OK", &comparison),
        Err(err) => http_not_found(err),
    }
}

fn local_api_approvals_response(workspace: &Utf8PathBuf) -> Vec<u8> {
    match LocalApprovalStore::new(workspace.clone()).list() {
        Ok(records) => {
            let approvals = records.iter().map(local_api_approval).collect::<Vec<_>>();
            http_json_response("200 OK", &approvals)
        }
        Err(err) => http_server_error(err),
    }
}

fn local_api_approval_response(workspace: &Utf8PathBuf, path: &str) -> Vec<u8> {
    let Some(approval_id) = route_suffix(path, "/api/approval/") else {
        return http_response("404 Not Found", "text/plain; charset=utf-8", "not found");
    };
    match LocalApprovalStore::new(workspace.clone()).read(&ApprovalId(approval_id)) {
        Ok(record) => http_json_response("200 OK", &local_api_approval(&record)),
        Err(err) => http_not_found(err),
    }
}

fn local_api_resolve_approval_response(workspace: &Utf8PathBuf, path: &str) -> Vec<u8> {
    let Some(suffix) = route_suffix(path, "/api/approval/") else {
        return http_response("404 Not Found", "text/plain; charset=utf-8", "not found");
    };
    match resolve_review_approval_path(workspace, &format!("/approval/{suffix}")) {
        Ok(_) => local_api_approval_response(
            workspace,
            &format!(
                "/api/approval/{}",
                suffix.split('/').next().unwrap_or_default()
            ),
        ),
        Err(err) => http_error(err),
    }
}

fn artifact_download_response(workspace: &Utf8PathBuf, path: &str) -> Vec<u8> {
    let Some(suffix) = route_suffix(path, "/artifact/") else {
        return http_response("404 Not Found", "text/plain; charset=utf-8", "not found");
    };
    let mut parts = suffix.splitn(2, '/');
    let Some(task_id) = parts.next().filter(|part| !part.is_empty()) else {
        return http_response("404 Not Found", "text/plain; charset=utf-8", "not found");
    };
    let Some(relative_path) = parts.next().filter(|part| !part.is_empty()) else {
        return http_response(
            "400 Bad Request",
            "text/plain; charset=utf-8",
            "artifact route requires /artifact/<task-id>/<relative-path>",
        );
    };
    let relative_path = match url_decode_path_component(relative_path) {
        Ok(value) => Utf8PathBuf::from(value),
        Err(err) => return http_error(err),
    };
    let store = LocalTaskEvidenceStore::new(workspace.clone());
    match store.artifact_route(&TaskId(task_id.into()), &relative_path) {
        Ok(route) => match fs::read(route.path.as_std_path()) {
            Ok(bytes) => http_bytes_response("200 OK", "application/octet-stream", &bytes),
            Err(err) => http_server_error(TaskFenceError::State(format!(
                "failed to read artifact route {}: {err}",
                route.relative_path
            ))),
        },
        Err(err) => http_not_found(err),
    }
}

fn local_api_approval(record: &ApprovalRecord) -> LocalApiApproval {
    LocalApiApproval {
        id: record.id.clone(),
        task_id: record.task_id.clone(),
        status: approval_status(record).into(),
        actor: record.actor.clone(),
        source: record.source.clone(),
        requested_at: record.requested_at.to_string(),
        resolved_at: record.resolved_at.map(|time| time.to_string()),
        action: approval_action_summary(&record.action),
        policy: approval_policy_summary(&record.policy_decision),
    }
}

fn route_suffix(path: &str, prefix: &str) -> Option<String> {
    let path = path.split('?').next().unwrap_or(path);
    path.strip_prefix(prefix)
        .filter(|suffix| !suffix.is_empty())
        .and_then(|suffix| url_decode_path_component(suffix).ok())
}

fn resolve_review_approval_path(
    workspace: &Utf8PathBuf,
    path: &str,
) -> taskfence_core::Result<String> {
    let mut parts = path.trim_start_matches('/').split('/');
    let Some("approval") = parts.next() else {
        return Err(TaskFenceError::Approval(
            "approval route is malformed".into(),
        ));
    };
    let approval_id = parts
        .next()
        .ok_or_else(|| TaskFenceError::Approval("approval id is missing".into()))?;
    let action = parts
        .next()
        .ok_or_else(|| TaskFenceError::Approval("approval action is missing".into()))?;
    if parts.next().is_some() {
        return Err(TaskFenceError::Approval(
            "approval route has extra segments".into(),
        ));
    }
    let decision = match action {
        "approve" => ApprovalDecision::Approved,
        "deny" => ApprovalDecision::Denied,
        _ => {
            return Err(TaskFenceError::Approval(format!(
                "unsupported approval action {action}"
            )));
        }
    };
    let record = LocalApprovalStore::new(workspace.clone()).resolve_with_actor(
        &ApprovalId(url_decode_path_component(approval_id)?),
        decision,
        "local-review",
        Some("review-ui".into()),
    )?;
    Ok(format!("approval {} resolved", record.id.0))
}

fn url_decode_path_component(value: &str) -> taskfence_core::Result<String> {
    let mut decoded = String::new();
    let bytes = value.as_bytes();
    let mut index = 0;
    while index < bytes.len() {
        match bytes[index] {
            b'%' if index + 2 < bytes.len() => {
                let hex = std::str::from_utf8(&bytes[index + 1..index + 3]).map_err(|err| {
                    TaskFenceError::State(format!("invalid percent encoding: {err}"))
                })?;
                let byte = u8::from_str_radix(hex, 16).map_err(|err| {
                    TaskFenceError::State(format!("invalid percent encoding {hex}: {err}"))
                })?;
                decoded.push(char::from(byte));
                index += 3;
            }
            b'+' => {
                decoded.push(' ');
                index += 1;
            }
            byte => {
                decoded.push(char::from(byte));
                index += 1;
            }
        }
    }
    Ok(decoded)
}

fn http_response(status: &str, content_type: &str, body: &str) -> Vec<u8> {
    http_bytes_response(status, content_type, body.as_bytes())
}

fn http_json_response<T: Serialize>(status: &str, value: &T) -> Vec<u8> {
    match serde_json::to_vec_pretty(value) {
        Ok(bytes) => http_bytes_response(status, "application/json; charset=utf-8", &bytes),
        Err(err) => http_response(
            "500 Internal Server Error",
            "text/plain; charset=utf-8",
            &format!("failed to serialize local API JSON: {err}"),
        ),
    }
}

fn http_bytes_response(status: &str, content_type: &str, body: &[u8]) -> Vec<u8> {
    let mut response = format!(
        "HTTP/1.1 {status}\r\nContent-Type: {content_type}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
        body.len()
    )
    .into_bytes();
    response.extend_from_slice(body);
    response
}

fn http_redirect(location: &str, body: &str) -> Vec<u8> {
    let mut response = format!(
        "HTTP/1.1 303 See Other\r\nContent-Type: text/plain; charset=utf-8\r\nContent-Length: {}\r\nConnection: close\r\nLocation: {location}\r\n\r\n",
        body.len()
    )
    .into_bytes();
    response.extend_from_slice(body.as_bytes());
    response
}

fn http_error(err: TaskFenceError) -> Vec<u8> {
    http_response(
        "400 Bad Request",
        "text/plain; charset=utf-8",
        &err.to_string(),
    )
}

fn http_not_found(err: TaskFenceError) -> Vec<u8> {
    http_response(
        "404 Not Found",
        "text/plain; charset=utf-8",
        &err.to_string(),
    )
}

fn http_server_error(err: TaskFenceError) -> Vec<u8> {
    http_response(
        "500 Internal Server Error",
        "text/plain; charset=utf-8",
        &err.to_string(),
    )
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
        AuditEvent::BudgetUsageRecorded { .. } => "budget-usage",
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
        | AuditEvent::BudgetUsageRecorded { at, .. }
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
        AuditEvent::BudgetUsageRecorded { record, .. } => {
            let limit = record
                .limit
                .as_ref()
                .map(|limit| limit.max_amount.to_string())
                .unwrap_or_else(|| "-".into());
            format!(
                "budget usage {} amount {} limit {} => {}",
                record.usage.kind,
                record.usage.amount,
                limit,
                approval_policy_summary(&record.decision)
            )
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

fn render_replay_plan(plan: &ReplayPlan) -> String {
    let mut rendered = String::new();
    rendered.push_str("Replay plan\n");
    rendered.push_str(&format!("  task: {}\n", plan.task_id.0));
    rendered.push_str(&format!("  can_replay: {}\n", plan.can_replay));
    rendered.push_str(&format!("  deterministic: {}\n", plan.deterministic));
    rendered.push_str(&format!(
        "  status: {}\n",
        plan.last_status
            .as_ref()
            .map(|status| format!("{status:?}"))
            .unwrap_or_else(|| "-".into())
    ));
    rendered.push_str(&format!(
        "  task_file: {}\n",
        plan.source_task_file
            .as_ref()
            .map(|path| compact_cell(path.as_str()))
            .unwrap_or_else(|| "-".into())
    ));
    rendered.push_str(&format!(
        "  resolved_task: {}\n",
        plan.resolved_task_path
            .as_ref()
            .map(|path| compact_cell(path.as_str()))
            .unwrap_or_else(|| "-".into())
    ));
    rendered.push_str(&format!(
        "  events: {}\n",
        plan.event_log_path
            .as_ref()
            .map(|path| compact_cell(path.as_str()))
            .unwrap_or_else(|| "-".into())
    ));
    rendered.push_str(&format!("  artifacts: {}\n", plan.artifact_dir));
    push_text_list(&mut rendered, "blockers", &plan.blockers);
    push_text_list(&mut rendered, "limitations", &plan.limitations);
    rendered
}

fn push_text_list(rendered: &mut String, label: &str, values: &[String]) {
    rendered.push_str(&format!("  {label}:"));
    if values.is_empty() {
        rendered.push_str(" -\n");
        return;
    }
    rendered.push('\n');
    for value in values {
        rendered.push_str("    - ");
        rendered.push_str(&compact_cell(value));
        rendered.push('\n');
    }
}

fn push_stdout_list(label: &str, values: &[String]) {
    if values.is_empty() {
        println!("  {label}: -");
        return;
    }
    println!("  {label}:");
    for value in values {
        println!("    - {value}");
    }
}

fn render_review_page(
    index: &LocalReviewIndex,
    reviews: &[LocalTaskReview],
    approvals: &[ApprovalRecord],
) -> String {
    let mut html = String::new();
    html.push_str("<!doctype html><html lang=\"en\"><head><meta charset=\"utf-8\">");
    html.push_str("<meta name=\"viewport\" content=\"width=device-width, initial-scale=1\">");
    html.push_str("<title>TaskFence Review</title>");
    html.push_str("<link rel=\"icon\" href=\"data:,\">");
    html.push_str("<style>");
    html.push_str(
        "body{margin:0;font-family:Arial,sans-serif;background:#f6f7f9;color:#1f2933;letter-spacing:0}",
    );
    html.push_str("header{background:#17324d;color:#fff;padding:20px 24px}");
    html.push_str("main{max-width:1180px;margin:0 auto;padding:20px 16px 48px}");
    html.push_str("h1{font-size:24px;margin:0 0 6px}h2{font-size:18px;margin:28px 0 10px}h3{font-size:16px;margin:18px 0 8px}");
    html.push_str("p{margin:4px 0 10px}.muted{color:#607085}.status{font-weight:700}.ok{color:#176b3a}.blocked{color:#9a3412}");
    html.push_str("table{width:100%;border-collapse:collapse;background:#fff;border:1px solid #d9dee7}th,td{padding:9px 10px;border-bottom:1px solid #e7ebf0;text-align:left;vertical-align:top}th{background:#eef2f7;font-size:13px}td{font-size:13px}");
    html.push_str("section{margin-top:18px}.panel{background:#fff;border:1px solid #d9dee7;padding:14px;border-radius:6px}.toolbar{display:flex;gap:8px;flex-wrap:wrap}.button{display:inline-block;border:1px solid #b9c3d0;border-radius:4px;padding:5px 8px;background:#fff;color:#17324d;text-decoration:none;font-size:12px}");
    html.push_str("pre{background:#101923;color:#f5f7fa;padding:12px;overflow:auto;border-radius:4px;max-height:280px;font-size:12px;line-height:1.45}");
    html.push_str("@media(max-width:760px){main{padding:14px 8px}table{display:block;overflow-x:auto}header{padding:16px}th,td{white-space:nowrap}.panel{padding:10px}}");
    html.push_str("</style></head><body>");
    html.push_str("<header><h1>TaskFence Review</h1><p>Workspace: ");
    html.push_str(&html_escape(index.workspace.as_str()));
    html.push_str(
        "</p><p><a class=\"button\" href=\"/api/index\">Structured API</a></p></header><main>",
    );

    html.push_str("<section><h2>Tasks</h2>");
    if index.tasks.is_empty() {
        html.push_str("<p class=\"muted\">No local task evidence was found.</p>");
    } else {
        html.push_str("<table><thead><tr><th>Task</th><th>Status</th><th>Artifacts</th><th>Warnings</th><th>Goal</th></tr></thead><tbody>");
        for task in &index.tasks {
            html.push_str("<tr><td><a href=\"#task-");
            html.push_str(&html_escape(&task.task_id.0));
            html.push_str("\">");
            html.push_str(&html_escape(&task.task_id.0));
            html.push_str("</a></td><td>");
            html.push_str(&html_escape(&task_status_text(task)));
            html.push_str("</td><td>");
            html.push_str(&html_escape(&artifact_flags(task)));
            html.push_str("</td><td>");
            html.push_str(&task.warnings.len().to_string());
            html.push_str("</td><td>");
            html.push_str(&html_escape(task.goal.as_deref().unwrap_or("-")));
            html.push_str("</td></tr>");
        }
        html.push_str("</tbody></table>");
    }
    html.push_str("</section>");

    render_review_comparison(&mut html, &index.tasks);

    html.push_str("<section><h2>Pending Approvals</h2>");
    let pending = approvals
        .iter()
        .filter(|approval| approval.decision.is_none())
        .collect::<Vec<_>>();
    if pending.is_empty() {
        html.push_str("<p class=\"muted\">No pending approvals.</p>");
    } else {
        html.push_str("<table><thead><tr><th>Approval</th><th>Task</th><th>Action</th><th>Resolve</th></tr></thead><tbody>");
        for approval in pending {
            html.push_str("<tr><td>");
            html.push_str(&html_escape(&approval.id.0));
            html.push_str("</td><td>");
            html.push_str(&html_escape(&approval.task_id.0));
            html.push_str("</td><td>");
            html.push_str(&html_escape(&approval_action_summary(&approval.action)));
            html.push_str("</td><td><code>taskfence approve ");
            html.push_str(&html_escape(&approval.id.0));
            html.push_str(" --workspace ");
            html.push_str(&html_escape(index.workspace.as_str()));
            html.push_str("</code><br><code>taskfence deny ");
            html.push_str(&html_escape(&approval.id.0));
            html.push_str(" --workspace ");
            html.push_str(&html_escape(index.workspace.as_str()));
            html.push_str("</code><div class=\"toolbar\" style=\"margin-top:8px\"><form method=\"post\" action=\"/approval/");
            html.push_str(&html_escape(&approval.id.0));
            html.push_str("/approve\"><button class=\"button\" type=\"submit\">Approve</button></form><form method=\"post\" action=\"/approval/");
            html.push_str(&html_escape(&approval.id.0));
            html.push_str("/deny\"><button class=\"button\" type=\"submit\">Deny</button></form></div></td></tr>");
        }
        html.push_str("</tbody></table>");
    }
    html.push_str("</section>");

    html.push_str("<section><h2>Task Review</h2>");
    for review in reviews {
        render_review_task(&mut html, review);
    }
    html.push_str("</section></main></body></html>");
    html
}

fn render_review_comparison(html: &mut String, tasks: &[TaskSummary]) {
    html.push_str("<section><h2>Run Comparison</h2>");
    if tasks.len() < 2 {
        html.push_str("<p class=\"muted\">At least two task runs are needed for comparison.</p>");
        html.push_str("</section>");
        return;
    }

    html.push_str("<table><thead><tr><th>Task</th><th>Status</th><th>Goal</th><th>Artifacts</th><th>Warnings</th><th>Evidence</th></tr></thead><tbody>");
    for task in tasks {
        html.push_str("<tr><td>");
        html.push_str(&html_escape(&task.task_id.0));
        html.push_str("</td><td>");
        html.push_str(&html_escape(&task_status_text(task)));
        html.push_str("</td><td>");
        html.push_str(&html_escape(task.goal.as_deref().unwrap_or("-")));
        html.push_str("</td><td>");
        html.push_str(&html_escape(&artifact_flags(task)));
        html.push_str("</td><td>");
        html.push_str(&task.warnings.len().to_string());
        html.push_str("</td><td>");
        html.push_str(&html_escape(task.task_dir.as_str()));
        html.push_str("</td></tr>");
    }
    html.push_str("</tbody></table></section>");
}

fn render_review_task(html: &mut String, review: &LocalTaskReview) {
    let task = &review.summary;
    html.push_str("<section class=\"panel\" id=\"task-");
    html.push_str(&html_escape(&task.task_id.0));
    html.push_str("\"><h3>");
    html.push_str(&html_escape(&task.task_id.0));
    html.push_str("</h3><p><span class=\"status\">Status:</span> ");
    html.push_str(&html_escape(&task_status_text(task)));
    html.push_str(" | <span class=\"status\">Artifacts:</span> ");
    html.push_str(&html_escape(&artifact_flags(task)));
    html.push_str("</p><p>");
    html.push_str(&html_escape(task.goal.as_deref().unwrap_or("-")));
    html.push_str("</p>");
    if let Some(artifacts) = &review.artifacts {
        html.push_str("<div class=\"toolbar\">");
        for artifact in &artifacts.files {
            html.push_str("<a class=\"button\" href=\"/artifact/");
            html.push_str(&html_escape(&task.task_id.0));
            html.push('/');
            html.push_str(&html_escape(artifact.relative_path.as_str()));
            html.push_str("\">");
            html.push_str(&html_escape(artifact.relative_path.as_str()));
            html.push_str("</a>");
        }
        html.push_str("</div>");
    }

    html.push_str("<h3>Replay</h3><p>");
    if review.replay.can_replay {
        html.push_str("<span class=\"ok\">Ready from saved inputs</span>");
    } else {
        html.push_str("<span class=\"blocked\">Blocked</span>");
    }
    html.push_str("; deterministic: ");
    html.push_str(if review.replay.deterministic {
        "yes"
    } else {
        "no"
    });
    html.push_str("</p>");
    render_html_list(html, "Blockers", &review.replay.blockers);
    render_html_list(html, "Limitations", &review.replay.limitations);

    if !review.warnings.is_empty() {
        render_html_list(html, "Evidence warnings", &review.warnings);
    }
    if let Some(events) = &review.events {
        html.push_str("<h3>Timeline</h3><pre>");
        html.push_str(&html_escape(&render_task_events(&task.task_id, events)));
        html.push_str("</pre>");
    }
    if let Some(diff) = &review.diff {
        html.push_str("<h3>Diff</h3><pre>");
        html.push_str(&html_escape(&snippet(&diff.contents)));
        html.push_str("</pre>");
    }
    if let Some(logs) = &review.logs {
        html.push_str("<h3>Logs</h3><pre>");
        html.push_str(&html_escape(&snippet(&render_logs(logs))));
        html.push_str("</pre>");
    }
    if let Some(report) = &review.report {
        html.push_str("<h3>Report</h3><pre>");
        html.push_str(&html_escape(&snippet(&report.contents)));
        html.push_str("</pre>");
    }
    html.push_str("</section>");
}

fn render_html_list(html: &mut String, label: &str, values: &[String]) {
    html.push_str("<p><strong>");
    html.push_str(&html_escape(label));
    html.push_str(":</strong>");
    if values.is_empty() {
        html.push_str(" none</p>");
        return;
    }
    html.push_str("</p><ul>");
    for value in values {
        html.push_str("<li>");
        html.push_str(&html_escape(&compact_cell(value)));
        html.push_str("</li>");
    }
    html.push_str("</ul>");
}

fn snippet(value: &str) -> String {
    const LIMIT: usize = 6000;
    if value.len() <= LIMIT {
        return value.to_owned();
    }
    let mut end = LIMIT;
    while !value.is_char_boundary(end) {
        end -= 1;
    }
    format!("{}\n... truncated ...\n", &value[..end])
}

fn html_escape(value: &str) -> String {
    let mut escaped = String::with_capacity(value.len());
    for ch in value.chars() {
        match ch {
            '&' => escaped.push_str("&amp;"),
            '<' => escaped.push_str("&lt;"),
            '>' => escaped.push_str("&gt;"),
            '"' => escaped.push_str("&quot;"),
            '\'' => escaped.push_str("&#39;"),
            _ => escaped.push(ch),
        }
    }
    escaped
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
    let secret_broker = gateway_secret_broker_for(&task, &action);

    gateway_transition(&audit, &state, &task.id, TaskStatus::Preparing)?;
    gateway_transition(&audit, &state, &task.id, TaskStatus::Running)?;
    let context = ToolExecutionContext {
        task_dir: Some(artifact_refs.task_dir.clone()),
        artifact_dir: Some(artifact_refs.task_dir.join("artifacts")),
    };
    let approval = gateway_approval_engine(&task, approval_mode, "local");
    let execution = execute_gateway_action(
        &task,
        action,
        context,
        &policy,
        &audit,
        &registry,
        supported_protocols,
        approval.as_ref(),
        adapter.as_ref(),
        secret_broker.as_ref(),
    )?;

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

fn run_gateway_listener(
    task_file: Utf8PathBuf,
    approval_mode: GatewayApprovalMode,
    port: u16,
    once: bool,
) -> taskfence_core::Result<()> {
    let task = load_task_file(&task_file)?;
    if task.gateway.tools.is_empty() {
        return Err(TaskFenceError::Gateway(
            "gateway listener requires configured gateway.tools".into(),
        ));
    }
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
    gateway_transition(&audit, &state, &task.id, TaskStatus::Running)?;

    let listener = TcpListener::bind(("127.0.0.1", port)).map_err(|err| {
        TaskFenceError::Gateway(format!("failed to bind local gateway listener: {err}"))
    })?;
    let address = listener.local_addr().map_err(|err| {
        TaskFenceError::Gateway(format!("failed to inspect local gateway listener: {err}"))
    })?;
    println!("Gateway listener serving");
    println!("  url: http://{address}/tool");
    println!("  task: {}", task.id.0);
    println!("  artifacts: {}", artifact_refs.task_dir);

    let mut handled = 0usize;
    for stream in listener.incoming() {
        let stream = stream.map_err(|err| {
            TaskFenceError::Gateway(format!("gateway listener connection failed: {err}"))
        })?;
        handle_gateway_listener_stream(
            stream,
            &task,
            &artifact_refs,
            &audit,
            &state,
            approval_mode,
        )?;
        handled += 1;
        if once {
            break;
        }
    }

    gateway_transition(&audit, &state, &task.id, TaskStatus::Reporting)?;
    let report = MarkdownReportGenerator::new();
    let report_path = report.generate(&task, &artifact_refs, &audit.events()?)?;
    audit.record(AuditEvent::Artifact {
        task_id: task.id.clone(),
        at: time::OffsetDateTime::now_utc(),
        kind: "report".into(),
        path: report_path.clone(),
    })?;
    gateway_transition(&audit, &state, &task.id, TaskStatus::Succeeded)?;
    println!("Gateway listener stopped");
    println!("  handled: {handled}");
    println!("  report: {report_path}");
    Ok(())
}

fn handle_gateway_listener_stream(
    mut stream: TcpStream,
    task: &ResolvedTask,
    artifact_refs: &ArtifactRefs,
    audit: &CollectingAuditLogger<'_>,
    state: &dyn StateStore,
    approval_mode: GatewayApprovalMode,
) -> taskfence_core::Result<()> {
    let mut reader = BufReader::new(stream.try_clone().map_err(|err| {
        TaskFenceError::Gateway(format!("failed to clone gateway listener stream: {err}"))
    })?);
    let action = match read_gateway_listener_action(&mut reader) {
        Ok(action) => normalize_tool_action(action)?,
        Err(err) => {
            let body = serde_json::json!({ "error": err.to_string() }).to_string();
            let response =
                http_response("400 Bad Request", "application/json; charset=utf-8", &body);
            stream.write_all(&response).map_err(|write_err| {
                TaskFenceError::Gateway(format!(
                    "failed to write gateway listener response: {write_err}"
                ))
            })?;
            return Ok(());
        }
    };

    let registry = gateway_tool_registry(task)?;
    let supported_protocols = task
        .gateway
        .tools
        .iter()
        .map(|tool| tool.protocol.clone())
        .collect::<Vec<_>>();
    let policy = BuiltInPolicyEngine;
    let adapter = gateway_adapter_for(task, &action);
    let secret_broker = gateway_secret_broker_for(task, &action);
    let approval = gateway_approval_engine(task, approval_mode, "listener");
    let context = ToolExecutionContext {
        task_dir: Some(artifact_refs.task_dir.clone()),
        artifact_dir: Some(artifact_refs.task_dir.join("artifacts")),
    };

    let execution = execute_gateway_action(
        task,
        action,
        context,
        &policy,
        audit,
        &registry,
        supported_protocols,
        approval.as_ref(),
        adapter.as_ref(),
        secret_broker.as_ref(),
    )?;
    record_gateway_artifacts(audit, task, &execution)?;
    let status = gateway_execution_status(&execution);
    gateway_transition(audit, state, &task.id, status.clone())?;

    let body = serde_json::to_string_pretty(&execution).map_err(|err| {
        TaskFenceError::Gateway(format!(
            "failed to serialize gateway listener response: {err}"
        ))
    })?;
    let response = http_response(
        gateway_listener_http_status(&status),
        "application/json; charset=utf-8",
        &body,
    );
    stream.write_all(&response).map_err(|err| {
        TaskFenceError::Gateway(format!("failed to write gateway listener response: {err}"))
    })
}

fn read_gateway_listener_action(reader: &mut impl BufRead) -> taskfence_core::Result<ToolAction> {
    let mut request_line = String::new();
    reader.read_line(&mut request_line).map_err(|err| {
        TaskFenceError::Gateway(format!(
            "failed to read gateway listener request line: {err}"
        ))
    })?;
    let parts = request_line.split_whitespace().collect::<Vec<_>>();
    if parts.len() < 2 {
        return Err(TaskFenceError::Gateway(
            "gateway listener request line is malformed".into(),
        ));
    }
    if parts[0] != "POST" || parts[1] != "/tool" {
        drain_gateway_listener_headers(reader)?;
        return Err(TaskFenceError::Gateway(
            "gateway listener accepts only POST /tool".into(),
        ));
    }

    let mut content_length = None;
    loop {
        let mut header = String::new();
        let read = reader.read_line(&mut header).map_err(|err| {
            TaskFenceError::Gateway(format!("failed to read gateway listener header: {err}"))
        })?;
        if read == 0 || header == "\r\n" || header == "\n" {
            break;
        }
        if let Some((name, value)) = header.split_once(':') {
            if name.trim().eq_ignore_ascii_case("content-length") {
                content_length = Some(value.trim().parse::<usize>().map_err(|err| {
                    TaskFenceError::Gateway(format!(
                        "gateway listener content-length is invalid: {err}"
                    ))
                })?);
            }
        }
    }

    let content_length = content_length.ok_or_else(|| {
        TaskFenceError::Gateway("gateway listener request is missing content-length".into())
    })?;
    if content_length > 128 * 1024 {
        return Err(TaskFenceError::Gateway(
            "gateway listener request body is too large".into(),
        ));
    }
    let mut body = vec![0u8; content_length];
    reader.read_exact(&mut body).map_err(|err| {
        TaskFenceError::Gateway(format!("failed to read gateway listener body: {err}"))
    })?;
    serde_json::from_slice::<ToolAction>(&body).map_err(|err| {
        TaskFenceError::Gateway(format!("malformed gateway listener tool action: {err}"))
    })
}

fn drain_gateway_listener_headers(reader: &mut impl BufRead) -> taskfence_core::Result<()> {
    loop {
        let mut header = String::new();
        let read = reader.read_line(&mut header).map_err(|err| {
            TaskFenceError::Gateway(format!("failed to read gateway listener header: {err}"))
        })?;
        if read == 0 || header == "\r\n" || header == "\n" {
            return Ok(());
        }
    }
}

fn gateway_listener_http_status(status: &TaskStatus) -> &'static str {
    match status {
        TaskStatus::Succeeded => "200 OK",
        TaskStatus::Denied => "403 Forbidden",
        TaskStatus::TimedOut => "408 Request Timeout",
        TaskStatus::Cancelled => "409 Conflict",
        _ => "502 Bad Gateway",
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
    let secret_broker = gateway_secret_broker_for(&task, &request.action);

    gateway_transition(&audit, &state, &task.id, TaskStatus::Preparing)?;
    gateway_transition(&audit, &state, &task.id, TaskStatus::Running)?;
    let context = ToolExecutionContext {
        task_dir: Some(artifact_refs.task_dir.clone()),
        artifact_dir: Some(artifact_refs.task_dir.join("artifacts")),
    };
    let approval = gateway_approval_engine(&task, approval_mode, "spool");
    let execution = execute_gateway_action(
        &task,
        request.action.clone(),
        context,
        &policy,
        &audit,
        &registry,
        supported_protocols,
        approval.as_ref(),
        adapter.as_ref(),
        secret_broker.as_ref(),
    )?;

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
    secret_broker: &dyn SecretBroker,
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

fn gateway_approval_engine(
    task: &ResolvedTask,
    mode: GatewayApprovalMode,
    source_prefix: &str,
) -> Box<dyn ApprovalEngine> {
    match mode {
        GatewayApprovalMode::FailClosed => Box::new(
            LocalApprovalEngine::fail_closed()
                .with_actor("gateway")
                .with_source(format!("{source_prefix}-fail-closed")),
        ),
        GatewayApprovalMode::Approved => Box::new(
            LocalApprovalEngine::preconfigured(ApprovalDecision::Approved)
                .with_actor("gateway")
                .with_source(format!("{source_prefix}-approved")),
        ),
        GatewayApprovalMode::External => Box::new(
            LocalExternalApprovalEngine::new(task.workspace_host_path.clone())
                .with_actor("gateway"),
        ),
    }
}

fn gateway_adapter_for(task: &ResolvedTask, action: &ToolAction) -> Box<dyn ToolAdapter> {
    if action.protocol == GATEWAY_EGRESS_TOOL_PROTOCOL
        && action.tool == GATEWAY_EGRESS_TOOL_NAME
        && action.operation == GATEWAY_EGRESS_TOOL_OPERATION
    {
        return Box::new(GatewayEgressAdapter::new(UreqGatewayEgressClient));
    }
    task.gateway
        .tools
        .iter()
        .find(|tool| {
            tool.protocol == action.protocol
                && tool.tool == action.tool
                && tool.operation == action.operation
        })
        .map(|tool| match &tool.connector {
            GatewayConnectorConfig::LocalFixture { .. } => {
                Box::new(LocalFixtureToolAdapter::new(tool.clone())) as Box<dyn ToolAdapter>
            }
            GatewayConnectorConfig::GitHubRest { .. }
            | GatewayConnectorConfig::GitHubEnterpriseRest { .. } => {
                Box::new(GitHubRestAdapter::new(tool.clone(), UreqGitHubClient))
                    as Box<dyn ToolAdapter>
            }
            GatewayConnectorConfig::GitLab { .. }
            | GatewayConnectorConfig::Jira { .. }
            | GatewayConnectorConfig::Feishu { .. }
            | GatewayConnectorConfig::WeCom { .. }
            | GatewayConnectorConfig::DingTalk { .. }
            | GatewayConnectorConfig::Gitee { .. }
            | GatewayConnectorConfig::Coding { .. }
            | GatewayConnectorConfig::InternalHttp { .. }
            | GatewayConnectorConfig::SiemExport { .. } => Box::new(
                EnterpriseConnectorAdapter::new(tool.clone(), UreqEnterpriseHttpClient),
            ) as Box<dyn ToolAdapter>,
            GatewayConnectorConfig::Database { .. } => Box::new(DatabaseConnectorAdapter::new(
                tool.clone(),
                PostgresDatabaseClient,
            )) as Box<dyn ToolAdapter>,
            GatewayConnectorConfig::Unsupported { kind } => {
                Box::new(UnsupportedGatewayAdapter::new("unsupported", kind.clone()))
                    as Box<dyn ToolAdapter>
            }
        })
        .unwrap_or_else(|| {
            Box::new(UnsupportedGatewayAdapter::new(
                "unregistered",
                "unregistered",
            ))
        })
}

fn gateway_secret_broker_for(task: &ResolvedTask, action: &ToolAction) -> Box<dyn SecretBroker> {
    let uses_live_connector = task.gateway.tools.iter().any(|tool| {
        tool.protocol == action.protocol
            && tool.tool == action.tool
            && tool.operation == action.operation
            && matches!(
                tool.connector,
                GatewayConnectorConfig::GitHubRest { .. }
                    | GatewayConnectorConfig::GitHubEnterpriseRest { .. }
                    | GatewayConnectorConfig::GitLab { .. }
                    | GatewayConnectorConfig::Jira { .. }
                    | GatewayConnectorConfig::Feishu { .. }
                    | GatewayConnectorConfig::WeCom { .. }
                    | GatewayConnectorConfig::DingTalk { .. }
                    | GatewayConnectorConfig::Gitee { .. }
                    | GatewayConnectorConfig::Coding { .. }
                    | GatewayConnectorConfig::Database { .. }
                    | GatewayConnectorConfig::InternalHttp { .. }
                    | GatewayConnectorConfig::SiemExport { .. }
            )
    });

    if uses_live_connector {
        Box::new(EnvironmentSecretBroker::new())
    } else {
        Box::new(LocalRedactedSecretBroker)
    }
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
        | Some(ToolExecutionErrorKind::ApprovalDeniedOrTimedOut)
        | Some(ToolExecutionErrorKind::BudgetExceeded) => TaskStatus::Denied,
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
    fn parses_gateway_listen() {
        let cli = Cli::try_parse_from([
            "taskfence",
            "gateway",
            "listen",
            "task.yaml",
            "--approve",
            "--port",
            "0",
            "--once",
        ])
        .unwrap();

        match cli.command {
            Command::Gateway {
                command:
                    GatewayCommand::Listen {
                        task_file,
                        approve,
                        external_approval,
                        port,
                        once,
                    },
            } => {
                assert_eq!(task_file, Utf8PathBuf::from("task.yaml"));
                assert!(approve);
                assert!(!external_approval);
                assert_eq!(port, 0);
                assert!(once);
            }
            other => panic!("expected gateway listen command, got {other:?}"),
        }
    }

    #[test]
    fn gateway_listener_parses_post_tool_action() {
        let body = serde_json::to_string(&ToolAction {
            protocol: "http".into(),
            tool: "egress".into(),
            operation: "fetch".into(),
            parameters: BTreeMap::from([(
                "url".into(),
                RedactedValue::Plain("https://api.github.com/".into()),
            )]),
        })
        .unwrap();
        let request = format!(
            "POST /tool HTTP/1.1\r\nHost: 127.0.0.1\r\nContent-Length: {}\r\n\r\n{}",
            body.len(),
            body
        );
        let mut reader = BufReader::new(request.as_bytes());

        let action = read_gateway_listener_action(&mut reader).unwrap();

        assert_eq!(action.protocol, "http");
        assert_eq!(action.tool, "egress");
        assert_eq!(action.operation, "fetch");
    }

    #[test]
    fn gateway_listener_rejects_wrong_method_or_path() {
        let request = "GET /tool HTTP/1.1\r\nHost: 127.0.0.1\r\n\r\n";
        let mut reader = BufReader::new(request.as_bytes());

        let err = read_gateway_listener_action(&mut reader).unwrap_err();

        assert!(err.to_string().contains("POST /tool"));
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
    fn parses_compliance_output_command() {
        let cli = Cli::try_parse_from([
            "taskfence",
            "compliance",
            "task-123",
            "--workspace",
            "repo",
            "--output",
            "compliance.md",
        ])
        .unwrap();

        match cli.command {
            Command::Compliance {
                task_id,
                workspace,
                output,
            } => {
                assert_eq!(task_id, "task-123");
                assert_eq!(workspace, Utf8PathBuf::from("repo"));
                assert_eq!(output, Some(Utf8PathBuf::from("compliance.md")));
            }
            other => panic!("expected compliance command, got {other:?}"),
        }
    }

    #[test]
    fn parses_review_output_and_serve_options() {
        let cli = Cli::try_parse_from([
            "taskfence",
            "review",
            "--workspace",
            "repo",
            "--output",
            "review.html",
        ])
        .unwrap();

        match cli.command {
            Command::Review {
                workspace,
                output,
                serve,
                port,
            } => {
                assert_eq!(workspace, Utf8PathBuf::from("repo"));
                assert_eq!(output, Some(Utf8PathBuf::from("review.html")));
                assert!(!serve);
                assert_eq!(port, 0);
            }
            other => panic!("expected review command, got {other:?}"),
        }

        let cli =
            Cli::try_parse_from(["taskfence", "review", "--serve", "--port", "8765"]).unwrap();
        match cli.command {
            Command::Review {
                workspace,
                output,
                serve,
                port,
            } => {
                assert_eq!(workspace, Utf8PathBuf::from("."));
                assert_eq!(output, None);
                assert!(serve);
                assert_eq!(port, 8765);
            }
            other => panic!("expected review command, got {other:?}"),
        }
    }

    #[test]
    fn parses_replay_plan_workspace() {
        let cli = Cli::try_parse_from([
            "taskfence",
            "replay",
            "plan",
            "task-123",
            "--workspace",
            "repo",
        ])
        .unwrap();

        match cli.command {
            Command::Replay {
                command: ReplayCommand::Plan { task_id, workspace },
            } => {
                assert_eq!(task_id, "task-123");
                assert_eq!(workspace, Utf8PathBuf::from("repo"));
            }
            other => panic!("expected replay plan command, got {other:?}"),
        }
    }

    #[test]
    fn parses_replay_run_command() {
        let cli = Cli::try_parse_from([
            "taskfence",
            "replay",
            "run",
            "task-123",
            "--workspace",
            "repo",
            "--replay-id",
            "task-123-retry",
            "--accept-limitations",
        ])
        .unwrap();

        match cli.command {
            Command::Replay {
                command:
                    ReplayCommand::Run {
                        task_id,
                        workspace,
                        replay_id,
                        accept_limitations,
                    },
            } => {
                assert_eq!(task_id, "task-123");
                assert_eq!(workspace, Utf8PathBuf::from("repo"));
                assert_eq!(replay_id.as_deref(), Some("task-123-retry"));
                assert!(accept_limitations);
            }
            other => panic!("expected replay run command, got {other:?}"),
        }
    }

    #[test]
    fn parses_state_index_command() {
        let cli = Cli::try_parse_from([
            "taskfence",
            "state",
            "index",
            "--workspace",
            "repo",
            "--read-only",
        ])
        .unwrap();

        match cli.command {
            Command::State {
                command:
                    StateCommand::Index {
                        workspace,
                        read_only,
                    },
            } => {
                assert_eq!(workspace, Utf8PathBuf::from("repo"));
                assert!(read_only);
            }
            other => panic!("expected state index command, got {other:?}"),
        }
    }

    #[test]
    fn parses_team_state_command() {
        let cli = Cli::try_parse_from([
            "taskfence",
            "team",
            "state",
            "--state-file",
            "team.json",
            "--organization",
            "acme",
        ])
        .unwrap();

        match cli.command {
            Command::Team {
                command:
                    TeamCommand::State {
                        state_file,
                        organization,
                    },
            } => {
                assert_eq!(state_file, Utf8PathBuf::from("team.json"));
                assert_eq!(organization, "acme");
            }
            other => panic!("expected team state command, got {other:?}"),
        }
    }

    #[test]
    fn parses_team_audit_export_command() {
        let cli = Cli::try_parse_from([
            "taskfence",
            "team",
            "audit-export",
            "task-123",
            "--state-file",
            "team.json",
            "--organization",
            "acme",
            "--actor",
            "auditor",
            "--sink-kind",
            "siem",
            "--destination-ref",
            "soc-pipeline",
            "--credential-env",
            "TASKFENCE_AUDIT_EXPORT_TOKEN",
        ])
        .unwrap();

        match cli.command {
            Command::Team {
                command:
                    TeamCommand::AuditExport {
                        task_id,
                        state_file,
                        organization,
                        actor,
                        sink_kind,
                        destination_ref,
                        credential_env,
                    },
            } => {
                assert_eq!(task_id, "task-123");
                assert_eq!(state_file, Utf8PathBuf::from("team.json"));
                assert_eq!(organization, "acme");
                assert_eq!(actor, "auditor");
                assert!(matches!(sink_kind, AuditExportSinkArg::Siem));
                assert_eq!(destination_ref, "soc-pipeline");
                assert_eq!(credential_env, "TASKFENCE_AUDIT_EXPORT_TOKEN");
            }
            other => panic!("expected team audit-export command, got {other:?}"),
        }
    }

    #[test]
    fn parses_team_worker_lease_command() {
        let cli = Cli::try_parse_from([
            "taskfence",
            "team",
            "worker",
            "lease",
            "--worker-id",
            "worker-1",
            "--state-file",
            "team.json",
            "--organization",
            "acme",
            "--actor",
            "operator",
        ])
        .unwrap();

        match cli.command {
            Command::Team {
                command:
                    TeamCommand::Worker {
                        command:
                            TeamWorkerCommand::Lease {
                                worker_id,
                                state_file,
                                organization,
                                actor,
                            },
                    },
            } => {
                assert_eq!(worker_id, "worker-1");
                assert_eq!(state_file, Utf8PathBuf::from("team.json"));
                assert_eq!(organization, "acme");
                assert_eq!(actor, "operator");
            }
            other => panic!("expected team worker lease command, got {other:?}"),
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
    fn validate_rejects_unavailable_remote_runner_contracts() {
        let temp = tempfile::tempdir().unwrap();
        let workspace = Utf8PathBuf::from_path_buf(temp.path().join("repo")).unwrap();
        fs::create_dir(&workspace).unwrap();
        let task_file = Utf8PathBuf::from_path_buf(temp.path().join("task.yaml")).unwrap();
        fs::write(
            &task_file,
            task_yaml_with_sandbox_type("validate-remote", &workspace, "remote_ssh"),
        )
        .unwrap();

        let err = validate_task_file_summary(task_file).unwrap_err();

        assert!(
            matches!(err, TaskFenceError::Config(message) if message.contains("sandbox.ssh is required"))
        );
        assert!(!workspace.join(".taskfence").exists());
    }

    #[test]
    fn validate_accepts_declared_remote_ssh_runner_contract_without_running_ssh() {
        let temp = tempfile::tempdir().unwrap();
        let workspace = Utf8PathBuf::from_path_buf(temp.path().join("repo")).unwrap();
        fs::create_dir(&workspace).unwrap();
        let task_file = Utf8PathBuf::from_path_buf(temp.path().join("task.yaml")).unwrap();
        fs::write(
            &task_file,
            task_yaml_with_remote_ssh_contract("validate-remote-ok", &workspace),
        )
        .unwrap();

        let summary = validate_task_file_summary(task_file).unwrap();

        assert_eq!(summary.task_id, "validate-remote-ok");
        assert_eq!(summary.sandbox_image, "-");
        assert_eq!(summary.network_mode, "ssh");
        assert_eq!(summary.mount_count, 0);
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
    fn replay_plan_command_reads_saved_inputs_and_limitations() {
        let temp = tempfile::tempdir().unwrap();
        let workspace = Utf8PathBuf::from_path_buf(temp.path().join("repo")).unwrap();
        fs::create_dir(&workspace).unwrap();
        let task_file = Utf8PathBuf::from_path_buf(temp.path().join("task.yaml")).unwrap();
        fs::write(&task_file, task_yaml("cli-replay", &workspace, "echo ok")).unwrap();

        run_task_with_runner(
            task_file,
            &FakeRunner::succeeding(),
            RunApprovalMode::FailClosed,
        )
        .unwrap();

        let text = replay_plan_text(workspace, &TaskId("cli-replay".into())).unwrap();

        assert!(text.contains("Replay plan\n"));
        assert!(text.contains("  task: cli-replay\n"));
        assert!(text.contains("  can_replay: true\n"));
        assert!(text.contains("  deterministic: false\n"));
        assert!(text.contains("task.resolved.json"));
        assert!(text.contains("events.jsonl"));
        assert!(text.contains("runner image availability"));
    }

    #[test]
    fn replay_run_requires_accepting_recorded_limitations() {
        let temp = tempfile::tempdir().unwrap();
        let workspace = Utf8PathBuf::from_path_buf(temp.path().join("repo")).unwrap();
        fs::create_dir(&workspace).unwrap();
        let task_file = Utf8PathBuf::from_path_buf(temp.path().join("task.yaml")).unwrap();
        fs::write(
            &task_file,
            task_yaml("cli-replay-limits", &workspace, "echo ok"),
        )
        .unwrap();
        run_task_with_runner(
            task_file,
            &FakeRunner::succeeding(),
            RunApprovalMode::FailClosed,
        )
        .unwrap();

        let err = replay_run_with_runner(
            workspace,
            &TaskId("cli-replay-limits".into()),
            None,
            false,
            &FakeRunner::succeeding(),
        )
        .unwrap_err();

        assert!(
            matches!(err, TaskFenceError::State(message) if message.contains("--accept-limitations"))
        );
    }

    #[test]
    fn replay_run_executes_saved_local_task_and_writes_evaluation_artifact() {
        let temp = tempfile::tempdir().unwrap();
        let workspace = Utf8PathBuf::from_path_buf(temp.path().join("repo")).unwrap();
        fs::create_dir(&workspace).unwrap();
        let task_file = Utf8PathBuf::from_path_buf(temp.path().join("task.yaml")).unwrap();
        fs::write(
            &task_file,
            task_yaml("cli-replay-run", &workspace, "echo ok"),
        )
        .unwrap();
        run_task_with_runner(
            task_file,
            &FakeRunner::succeeding(),
            RunApprovalMode::FailClosed,
        )
        .unwrap();

        let record = replay_run_with_runner(
            workspace.clone(),
            &TaskId("cli-replay-run".into()),
            None,
            true,
            &FakeRunner::succeeding(),
        )
        .unwrap();

        assert_eq!(record.source_task_id, TaskId("cli-replay-run".into()));
        assert_eq!(
            record.replay_task_id,
            TaskId("cli-replay-run-replay".into())
        );
        assert!(record.accepted_limitations);
        assert_eq!(record.evaluation.source_status, Some(TaskStatus::Succeeded));
        assert_eq!(record.evaluation.replay_status, Some(TaskStatus::Succeeded));
        assert!(record.evaluation.differing_fields.is_empty());
        let replay_record_path =
            workspace.join(".taskfence/tasks/cli-replay-run-replay/artifacts/replay.json");
        let replay_json: serde_json::Value =
            serde_json::from_slice(&fs::read(replay_record_path).unwrap()).unwrap();
        assert_eq!(replay_json["source_task_id"], "cli-replay-run");
        assert_eq!(replay_json["replay_task_id"], "cli-replay-run-replay");
    }

    #[test]
    fn replay_run_rejects_existing_replay_task_id() {
        let temp = tempfile::tempdir().unwrap();
        let workspace = Utf8PathBuf::from_path_buf(temp.path().join("repo")).unwrap();
        fs::create_dir(&workspace).unwrap();
        let task_file = Utf8PathBuf::from_path_buf(temp.path().join("task.yaml")).unwrap();
        fs::write(
            &task_file,
            task_yaml("cli-replay-existing", &workspace, "echo ok"),
        )
        .unwrap();
        run_task_with_runner(
            task_file,
            &FakeRunner::succeeding(),
            RunApprovalMode::FailClosed,
        )
        .unwrap();
        fs::create_dir_all(workspace.join(".taskfence/tasks/cli-replay-existing-replay")).unwrap();

        let err = replay_run_with_runner(
            workspace,
            &TaskId("cli-replay-existing".into()),
            None,
            true,
            &FakeRunner::succeeding(),
        )
        .unwrap_err();

        assert!(
            matches!(err, TaskFenceError::State(message) if message.contains("already has evidence"))
        );
    }

    #[test]
    fn replay_run_blocks_live_gateway_connector_effects() {
        let (_temp, workspace, task_file) = gateway_github_rest_task("cli-replay-live-gateway");
        run_task_with_runner(
            task_file,
            &FakeRunner::succeeding(),
            RunApprovalMode::FailClosed,
        )
        .unwrap();

        let err = replay_run_with_runner(
            workspace,
            &TaskId("cli-replay-live-gateway".into()),
            None,
            true,
            &FakeRunner::succeeding(),
        )
        .unwrap_err();

        assert!(
            matches!(err, TaskFenceError::Gateway(message) if message.contains("live or contract-only gateway"))
        );
    }

    #[test]
    fn state_index_command_refreshes_and_reads_structured_index() {
        let temp = tempfile::tempdir().unwrap();
        let workspace = Utf8PathBuf::from_path_buf(temp.path().join("repo")).unwrap();
        fs::create_dir(&workspace).unwrap();
        let task_file = Utf8PathBuf::from_path_buf(temp.path().join("task.yaml")).unwrap();
        fs::write(
            &task_file,
            task_yaml("cli-state-index", &workspace, "echo ok"),
        )
        .unwrap();
        run_task_with_runner(
            task_file,
            &FakeRunner::succeeding(),
            RunApprovalMode::FailClosed,
        )
        .unwrap();

        let refreshed = state_index_json(workspace.clone(), false).unwrap();
        let read_only = state_index_json(workspace.clone(), true).unwrap();

        assert!(workspace
            .join(".taskfence/state/local-index.json")
            .is_file());
        assert!(refreshed.contains("\"source\": \"StructuredEvidence\""));
        assert!(refreshed.contains("\"task_id\": \"cli-state-index\""));
        assert_eq!(refreshed, read_only);
    }

    #[test]
    fn team_worker_commands_persist_durable_local_leases() {
        let temp = tempfile::tempdir().unwrap();
        let state_file = Utf8PathBuf::from_path_buf(temp.path().join("team/state.json")).unwrap();

        team_worker_enqueue(
            state_file.clone(),
            "acme".into(),
            "operator".into(),
            "team-task".into(),
        )
        .unwrap();
        team_worker_lease(
            state_file.clone(),
            "acme".into(),
            "operator".into(),
            "worker-1".into(),
        )
        .unwrap();
        team_worker_complete(
            state_file.clone(),
            "acme".into(),
            "operator".into(),
            "team-task".into(),
            "worker-1".into(),
        )
        .unwrap();

        let contents = fs::read_to_string(state_file).unwrap();
        assert!(contents.contains("team-task"));
        assert!(contents.contains("Completed"));
    }

    #[test]
    fn team_migrate_local_imports_structured_evidence_without_requiring_run_mode() {
        let temp = tempfile::tempdir().unwrap();
        let workspace = Utf8PathBuf::from_path_buf(temp.path().join("repo")).unwrap();
        fs::create_dir(&workspace).unwrap();
        let task_file = Utf8PathBuf::from_path_buf(temp.path().join("task.yaml")).unwrap();
        fs::write(&task_file, task_yaml("team-import", &workspace, "echo ok")).unwrap();
        run_task_with_runner(
            task_file,
            &FakeRunner::succeeding(),
            RunApprovalMode::FailClosed,
        )
        .unwrap();
        let state_file = Utf8PathBuf::from_path_buf(temp.path().join("team/state.json")).unwrap();

        migrate_local_to_team(
            workspace.clone(),
            state_file.clone(),
            "acme".into(),
            "operator".into(),
        )
        .unwrap();
        let mut service = open_local_team_service(state_file, "acme".into()).unwrap();
        let tasks = service.list_tasks("viewer").unwrap();

        assert_eq!(tasks.len(), 1);
        assert_eq!(tasks[0].task_id, TaskId("team-import".into()));
        assert_eq!(tasks[0].organization, "acme");
        assert!(tasks[0].evidence_dir.starts_with(&workspace));
    }

    #[test]
    fn review_command_writes_local_html_from_structured_evidence() {
        let temp = tempfile::tempdir().unwrap();
        let workspace = Utf8PathBuf::from_path_buf(temp.path().join("repo")).unwrap();
        fs::create_dir(&workspace).unwrap();
        let task_file = Utf8PathBuf::from_path_buf(temp.path().join("task.yaml")).unwrap();
        fs::write(&task_file, task_yaml("cli-review", &workspace, "echo ok")).unwrap();
        let compare_task_file =
            Utf8PathBuf::from_path_buf(temp.path().join("compare.yaml")).unwrap();
        fs::write(
            &compare_task_file,
            task_yaml("cli-review-compare", &workspace, "echo ok"),
        )
        .unwrap();

        run_task_with_runner(
            task_file,
            &FakeRunner::succeeding(),
            RunApprovalMode::FailClosed,
        )
        .unwrap();
        run_task_with_runner(
            compare_task_file,
            &FakeRunner::failing(2),
            RunApprovalMode::FailClosed,
        )
        .unwrap_err();
        let approval_id = create_pending_approval(&workspace, "approval-review-1");
        let output = Utf8PathBuf::from_path_buf(temp.path().join("review.html")).unwrap();

        let written = write_review_page(workspace.clone(), Some(output.clone())).unwrap();

        assert_eq!(written, output);
        let html = fs::read_to_string(output).unwrap();
        assert!(html.contains("<title>TaskFence Review</title>"));
        assert!(html.contains("cli-review"));
        assert!(html.contains("Succeeded"));
        assert!(html.contains("Run Comparison"));
        assert!(html.contains("cli-review-compare"));
        assert!(html.contains("Failed"));
        assert!(html.contains("Replay"));
        assert!(html.contains("Ready from saved inputs"));
        assert!(html.contains(&approval_id.0));
        assert!(html.contains("/approval/approval-review-1/approve"));
        assert!(html.contains("taskfence deny approval-review-1"));
        assert!(html.contains("Task events"));
        assert!(html.contains("TaskFence Report"));
        assert!(html.contains("/api/index"));
        assert!(html.contains("/artifact/cli-review/report.md"));
        assert!(!html.contains("secret-value"));
    }

    #[test]
    fn review_command_rejects_output_and_serve_together() {
        let err = execute(Cli {
            command: Command::Review {
                workspace: Utf8PathBuf::from("."),
                output: Some(Utf8PathBuf::from("review.html")),
                serve: true,
                port: 0,
            },
        })
        .unwrap_err();

        assert!(
            matches!(err, TaskFenceError::Config(message) if message.contains("--output and --serve"))
        );
    }

    #[test]
    fn review_http_handler_resolves_pending_approval() {
        let temp = tempfile::tempdir().unwrap();
        let workspace = Utf8PathBuf::from_path_buf(temp.path().join("repo")).unwrap();
        fs::create_dir(&workspace).unwrap();
        let approval_id = create_pending_approval(&workspace, "approval-http-1");
        let request = ReviewHttpRequest {
            method: "POST".into(),
            path: "/approval/approval-http-1/approve".into(),
        };

        let response = review_http_response(&workspace, &request);

        assert!(response_text(&response).starts_with("HTTP/1.1 303 See Other"));
        let record = LocalApprovalStore::new(workspace)
            .read(&approval_id)
            .unwrap();
        assert_eq!(record.decision, Some(ApprovalDecision::Approved));
        assert_eq!(record.actor, "local-review");
        assert_eq!(record.source.as_deref(), Some("review-ui"));
    }

    #[test]
    fn review_http_api_routes_return_structured_state_and_resolve_approval() {
        let temp = tempfile::tempdir().unwrap();
        let workspace = Utf8PathBuf::from_path_buf(temp.path().join("repo")).unwrap();
        fs::create_dir(&workspace).unwrap();
        let task_file = Utf8PathBuf::from_path_buf(temp.path().join("task.yaml")).unwrap();
        fs::write(&task_file, task_yaml("api-task", &workspace, "echo ok")).unwrap();
        run_task_with_runner(
            task_file,
            &FakeRunner::succeeding(),
            RunApprovalMode::FailClosed,
        )
        .unwrap();
        let approval_id = create_pending_approval(&workspace, "approval-api-1");

        let index_response = review_http_response(
            &workspace,
            &ReviewHttpRequest {
                method: "GET".into(),
                path: "/api/index".into(),
            },
        );
        let index_json: serde_json::Value =
            serde_json::from_str(&response_body_text(&index_response)).unwrap();
        assert_eq!(index_json["source"], "StructuredEvidence");
        assert_eq!(index_json["tasks"][0]["task_id"], "api-task");

        let task_response = review_http_response(
            &workspace,
            &ReviewHttpRequest {
                method: "GET".into(),
                path: "/api/task/api-task".into(),
            },
        );
        let task_json: serde_json::Value =
            serde_json::from_str(&response_body_text(&task_response)).unwrap();
        assert_eq!(task_json["summary"]["status"], "Succeeded");
        assert!(task_json["events"]["events"].as_array().unwrap().len() >= 2);

        let events_response = review_http_response(
            &workspace,
            &ReviewHttpRequest {
                method: "GET".into(),
                path: "/api/events/api-task".into(),
            },
        );
        let events_json: serde_json::Value =
            serde_json::from_str(&response_body_text(&events_response)).unwrap();
        assert_eq!(events_json["task_id"], "api-task");
        assert!(events_json["events"]
            .as_array()
            .unwrap()
            .iter()
            .any(|event| event["summary"]
                .as_str()
                .is_some_and(|summary| summary.contains("goal: CLI test"))));

        let approval_response = review_http_response(
            &workspace,
            &ReviewHttpRequest {
                method: "POST".into(),
                path: "/api/approval/approval-api-1/approve".into(),
            },
        );
        let approval_json: serde_json::Value =
            serde_json::from_str(&response_body_text(&approval_response)).unwrap();
        assert_eq!(approval_json["id"], approval_id.0);
        assert_eq!(approval_json["status"], "approved");
        assert_eq!(
            LocalApprovalStore::new(workspace)
                .read(&approval_id)
                .unwrap()
                .decision,
            Some(ApprovalDecision::Approved)
        );
    }

    #[test]
    fn review_http_artifact_route_serves_valid_artifacts_and_rejects_escape() {
        let temp = tempfile::tempdir().unwrap();
        let workspace = Utf8PathBuf::from_path_buf(temp.path().join("repo")).unwrap();
        fs::create_dir(&workspace).unwrap();
        let task_file = Utf8PathBuf::from_path_buf(temp.path().join("task.yaml")).unwrap();
        fs::write(&task_file, task_yaml("artifact-api", &workspace, "echo ok")).unwrap();
        run_task_with_runner(
            task_file,
            &FakeRunner::succeeding(),
            RunApprovalMode::FailClosed,
        )
        .unwrap();
        fs::write(
            workspace.join(".taskfence/tasks/artifact-api/artifacts/manifest.json"),
            "{}\n",
        )
        .unwrap();

        let response = review_http_response(
            &workspace,
            &ReviewHttpRequest {
                method: "GET".into(),
                path: "/artifact/artifact-api/artifacts/manifest.json".into(),
            },
        );
        assert!(response_text(&response).starts_with("HTTP/1.1 200 OK"));
        assert_eq!(response_body_bytes(&response), b"{}\n");

        let escape = review_http_response(
            &workspace,
            &ReviewHttpRequest {
                method: "GET".into(),
                path: "/artifact/artifact-api/../secret.txt".into(),
            },
        );
        assert!(response_text(&escape).starts_with("HTTP/1.1 404 Not Found"));
    }

    #[test]
    fn review_html_escapes_task_goal() {
        let review = LocalTaskReview {
            summary: TaskSummary {
                task_id: TaskId("escape-task".into()),
                task_dir: Utf8PathBuf::from("/tmp/task"),
                status: Some(TaskStatus::Succeeded),
                goal: Some("<script>alert(1)</script>".into()),
                has_report: false,
                has_diff: false,
                has_stdout: false,
                has_stderr: false,
                warnings: Vec::new(),
            },
            inputs: None,
            artifacts: None,
            events: None,
            logs: None,
            diff: None,
            report: None,
            replay: ReplayPlan {
                task_id: TaskId("escape-task".into()),
                source_task_file: None,
                resolved_task_path: None,
                event_log_path: None,
                artifact_dir: Utf8PathBuf::from("/tmp/task"),
                last_status: Some(TaskStatus::Succeeded),
                can_replay: false,
                deterministic: false,
                blockers: vec!["missing <input>".into()],
                limitations: Vec::new(),
            },
            warnings: Vec::new(),
        };
        let index = LocalReviewIndex {
            workspace: Utf8PathBuf::from("/tmp/repo"),
            tasks: vec![review.summary.clone()],
        };

        let html = render_review_page(&index, &[review], &[]);

        assert!(html.contains("&lt;script&gt;alert(1)&lt;/script&gt;"));
        assert!(html.contains("missing &lt;input&gt;"));
        assert!(!html.contains("<script>alert(1)</script>"));
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
    fn gateway_call_github_rest_missing_env_secret_fails_closed_with_evidence() {
        let (_temp, workspace, task_file) = gateway_github_rest_task("gateway-live-missing-secret");
        std::env::remove_var("TASKFENCE_GATEWAY_SECRET_GITHUB_TOKEN_PHASE2_MISSING");

        let err = run_gateway_call(
            task_file,
            "mcp".into(),
            "github".into(),
            "read_issue".into(),
            vec!["number=42".into()],
            GatewayApprovalMode::FailClosed,
        )
        .unwrap_err();

        assert!(
            matches!(err, TaskFenceError::Gateway(message) if message.contains("status Failed"))
        );
        let task_id = TaskId("gateway-live-missing-secret".into());
        let events = LocalTaskEvidenceStore::new(workspace.clone())
            .read_events(&task_id)
            .unwrap();
        assert!(!events
            .events
            .iter()
            .any(|event| matches!(event, AuditEvent::ToolExecutionStarted { .. })));
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
                } if error.kind == ToolExecutionErrorKind::SecretUnavailable
                    && error
                        .message
                        .contains("TASKFENCE_GATEWAY_SECRET_GITHUB_TOKEN_PHASE2_MISSING")
            )
        }));

        let event_text = events_text(workspace.clone(), &task_id).unwrap();
        let report = report_text(workspace, &task_id).unwrap();
        assert!(event_text.contains("SecretUnavailable"));
        assert!(report.contains("SecretUnavailable"));
        assert!(!event_text.contains("ghp_"));
        assert!(!report.contains("ghp_"));
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

    fn gateway_github_rest_task(id: &str) -> (tempfile::TempDir, Utf8PathBuf, Utf8PathBuf) {
        let temp = tempfile::tempdir().unwrap();
        let workspace = Utf8PathBuf::from_path_buf(temp.path().join("repo")).unwrap();
        fs::create_dir(&workspace).unwrap();
        let task_file = Utf8PathBuf::from_path_buf(temp.path().join("task.yaml")).unwrap();
        fs::write(&task_file, gateway_github_rest_task_yaml(id, &workspace)).unwrap();

        (temp, workspace, task_file)
    }

    fn gateway_github_rest_task_yaml(id: &str, workspace: &Utf8PathBuf) -> String {
        format!(
            r#"id: "{id}"
goal: "Gateway REST CLI test"
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
  budget:
    allow:
      - kind: "gateway_calls"
        max_amount: 1
secrets:
  expose_to_agent: false
  available_to_gateway:
    - name: "github_token_phase2_missing"
      use_for:
        - "github.read_issue"
gateway:
  tools:
    - protocol: "mcp"
      tool: "github"
      operation: "read_issue"
      connector:
        type: "github_rest"
        api_base: "https://api.github.invalid"
        repository: "taskfence/example"
      secret_refs:
        - name: "github_token_phase2_missing"
          parameter: "authorization"
          scope: "github.read_issue"
"#
        )
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

    fn task_yaml_with_sandbox_type(
        id: &str,
        workspace: &Utf8PathBuf,
        sandbox_type: &str,
    ) -> String {
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
  type: "{sandbox_type}"
permissions:
  paths:
    read:
      - "{workspace}"
    write: []
  commands:
    allow:
      - "echo"
  network:
    default: "disabled"
"#
        )
    }

    fn task_yaml_with_remote_ssh_contract(id: &str, workspace: &Utf8PathBuf) -> String {
        format!(
            r#"id: "{id}"
goal: "CLI remote SSH validation test"
workspace: "{workspace}"
agent:
  type: "generic"
  command: "/usr/bin/true"
sandbox:
  type: "remote_ssh"
  ssh:
    host: "runner.example"
    user: "taskfence"
    port: 2222
    workspace: "/srv/taskfence/workspaces/{id}"
    identity_file: "/tmp/taskfence/id_ed25519"
    known_hosts_file: "/tmp/taskfence/known_hosts"
    isolated_workspace: true
    isolated_secrets: true
    terminates_remote_processes: true
    network_policy: "uncontrolled_allow"
permissions:
  commands:
    allow:
      - "/usr/bin/true"
  network:
    default: "allow"
audit:
  capture:
    file_diff: false
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

    fn response_text(response: &[u8]) -> String {
        String::from_utf8_lossy(response).into_owned()
    }

    fn response_body_bytes(response: &[u8]) -> &[u8] {
        let split = response
            .windows(4)
            .position(|window| window == b"\r\n\r\n")
            .map(|index| index + 4)
            .expect("HTTP response should contain a header/body separator");
        &response[split..]
    }

    fn response_body_text(response: &[u8]) -> String {
        String::from_utf8_lossy(response_body_bytes(response)).into_owned()
    }
}
