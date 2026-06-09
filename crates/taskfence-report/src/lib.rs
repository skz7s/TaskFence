//! Markdown and compliance report rendering from structured TaskFence evidence.
//!
//! This crate renders reports from resolved task data, artifact references, and
//! audit events. Reports are review artifacts; local and team state should
//! continue to use structured evidence as the source of truth.

use camino::{Utf8Path, Utf8PathBuf};
use std::fs::{self, File};
use std::io::Write;
use std::time::{SystemTime, UNIX_EPOCH};
use taskfence_core::{
    Action, ActionDecision, ApprovalDecision, ArtifactRefs, AuditEvent, ExitStatus, LogStream,
    ReportFormat, ReportGenerator, ResolvedTask, Result, TaskFenceError, TaskStatus,
};

const REDACTION_MARKER: &str = "[redacted]";

#[derive(Clone, Debug, Default)]
pub struct MarkdownReportGenerator;

impl MarkdownReportGenerator {
    pub fn new() -> Self {
        Self
    }
}

impl ReportGenerator for MarkdownReportGenerator {
    fn generate(
        &self,
        task: &ResolvedTask,
        artifacts: &ArtifactRefs,
        events: &[AuditEvent],
    ) -> Result<Utf8PathBuf> {
        if task.audit.report.format != ReportFormat::Markdown {
            return Err(TaskFenceError::Report(
                "MarkdownReportGenerator only supports markdown reports".into(),
            ));
        }

        let report_path = artifacts
            .report
            .clone()
            .unwrap_or_else(|| artifacts.task_dir.join("report.md"));
        let report = render_markdown(task, artifacts, events);
        atomic_write(&report_path, report.as_bytes())?;
        Ok(report_path)
    }
}

#[derive(Clone, Debug, Default)]
pub struct ComplianceReportGenerator;

impl ComplianceReportGenerator {
    pub fn new() -> Self {
        Self
    }

    pub fn render_markdown(
        &self,
        task: &ResolvedTask,
        artifacts: &ArtifactRefs,
        events: &[AuditEvent],
    ) -> String {
        render_compliance_markdown(task, artifacts, events)
    }

    pub fn generate(
        &self,
        task: &ResolvedTask,
        artifacts: &ArtifactRefs,
        events: &[AuditEvent],
        output: &Utf8Path,
    ) -> Result<Utf8PathBuf> {
        let report = self.render_markdown(task, artifacts, events);
        atomic_write(output, report.as_bytes())?;
        Ok(output.to_path_buf())
    }
}

fn render_markdown(task: &ResolvedTask, artifacts: &ArtifactRefs, events: &[AuditEvent]) -> String {
    let data = ReportData::from_events(events);
    let mut md = String::new();

    md.push_str(&format!("# TaskFence Report: {}\n\n", inline(&task.id.0)));
    md.push_str("## Summary\n\n");
    md.push_str(&format!("- Task ID: {}\n", inline(&task.id.0)));
    md.push_str(&format!("- Goal: {}\n", text(&task.goal)));
    md.push_str(&format!(
        "- Final status: {}\n",
        data.final_status
            .as_ref()
            .map(format_status)
            .unwrap_or_else(|| "Unknown".into())
    ));
    md.push_str(&format!(
        "- Runner exit: {}\n",
        data.exit_status
            .as_ref()
            .map(format_exit_status)
            .unwrap_or_else(|| "Not recorded".into())
    ));
    md.push_str(&format!("- Audit events: {}\n\n", events.len()));

    md.push_str("## Task Input\n\n");
    md.push_str(&format!("- Task file: {}\n", inline_path(&task.task_file)));
    md.push_str(&format!(
        "- Workspace: {}\n",
        inline_path(&task.workspace_host_path)
    ));
    md.push_str(&format!(
        "- Container workspace: {}\n\n",
        inline_path(&task.workspace_container_path)
    ));

    md.push_str("## Agent and Model\n\n");
    md.push_str(&format!(
        "- Agent kind: {}\n",
        inline(&format!("{:?}", task.agent.kind))
    ));
    md.push_str(&format!("- Command: {}\n", inline(&task.agent.command)));
    md.push_str(&format!(
        "- Args: {}\n\n",
        inline(&task.agent.args.join(" "))
    ));

    md.push_str("## Policy Summary\n\n");
    md.push_str(&format!("- Allowed decisions: {}\n", data.allowed));
    md.push_str(&format!(
        "- Approval-required decisions: {}\n",
        data.approval_required
    ));
    md.push_str(&format!("- Denied decisions: {}\n\n", data.denied));

    md.push_str("## Budget Usage\n\n");
    push_table_or_none(
        &mut md,
        &data.budget_usage,
        &[
            "Kind",
            "Amount",
            "Limit",
            "Provider",
            "Model",
            "Operation",
            "Decision",
        ],
    );

    md.push_str("## Sandbox Summary\n\n");
    md.push_str(&format!(
        "- Sandbox kind: {}\n",
        inline(&format!("{:?}", task.sandbox.kind))
    ));
    md.push_str(&format!(
        "- Image: {}\n",
        task.sandbox
            .image
            .as_deref()
            .map(inline)
            .unwrap_or_else(|| "Not configured".into())
    ));
    md.push_str(&format!(
        "- Network default: {}\n\n",
        inline(&format!("{:?}", task.permissions.network.default))
    ));

    md.push_str("## Timeline\n\n");
    if data.timeline.is_empty() {
        md.push_str("No timeline events recorded.\n\n");
    } else {
        md.push_str("| Time | Event |\n| --- | --- |\n");
        for row in &data.timeline {
            md.push_str(&format!(
                "| {} | {} |\n",
                table_cell(&row.0),
                table_cell(&row.1)
            ));
        }
        md.push('\n');
    }

    md.push_str("## Commands\n\n");
    push_table_or_none(&mut md, &data.commands, &["Command", "Decision", "Reason"]);

    md.push_str("## Tool Calls\n\n");
    push_table_or_none(
        &mut md,
        &data.tool_calls,
        &["Tool", "Operation", "Decision"],
    );

    md.push_str("## Tool Executions\n\n");
    push_table_or_none(
        &mut md,
        &data.tool_executions,
        &["Tool", "Adapter", "Outcome", "Summary"],
    );

    md.push_str("## Approvals\n\n");
    push_table_or_none(
        &mut md,
        &data.approvals,
        &["Approval ID", "Actor", "Decision"],
    );

    md.push_str("## Denied Actions\n\n");
    if data.denied_actions.is_empty() {
        md.push_str("None recorded.\n\n");
    } else {
        for denied in &data.denied_actions {
            md.push_str(&format!("- {}\n", text(denied)));
        }
        md.push('\n');
    }

    md.push_str("## Network Destinations\n\n");
    push_table_or_none(&mut md, &data.network, &["Host", "Port", "Decision"]);

    md.push_str("## File Changes\n\n");
    match &artifacts.diff {
        Some(path) => {
            md.push_str(&format!("- Diff artifact: {}\n", inline_path(path)));
            for note in diff_metadata_notes(path) {
                md.push_str(&format!("- {}\n", text(&note)));
            }
            md.push('\n');
        }
        None => md.push_str("No diff artifact was supplied.\n\n"),
    }

    md.push_str("## Test Results\n\n");
    md.push_str("No structured test results recorded.\n\n");

    md.push_str("## Artifacts\n\n");
    let artifact_rows = artifact_rows(artifacts, &data.artifacts);
    push_table_or_none(&mut md, &artifact_rows, &["Kind", "Path"]);

    md.push_str("## Residual Risks\n\n");
    let risks = residual_risks(events, artifacts, &data);
    if risks.is_empty() {
        md.push_str("No residual risks were identified from structured evidence.\n");
    } else {
        for risk in risks {
            md.push_str(&format!("- {}\n", text(&risk)));
        }
    }

    md
}

fn render_compliance_markdown(
    task: &ResolvedTask,
    artifacts: &ArtifactRefs,
    events: &[AuditEvent],
) -> String {
    let data = ReportData::from_events(events);
    let mut md = String::new();

    md.push_str(&format!(
        "# TaskFence Compliance Evidence: {}\n\n",
        inline(&task.id.0)
    ));
    md.push_str("## Scope\n\n");
    md.push_str(&format!("- Task ID: {}\n", inline(&task.id.0)));
    md.push_str(&format!("- Goal: {}\n", text(&task.goal)));
    md.push_str(&format!(
        "- Workspace: {}\n",
        inline_path(&task.workspace_host_path)
    ));
    md.push_str(&format!(
        "- Event source: {}\n",
        artifacts
            .events
            .as_deref()
            .map(inline_path)
            .unwrap_or_else(|| "Not recorded".into())
    ));
    md.push_str(&format!("- Structured events: {}\n\n", events.len()));

    md.push_str("## Control Summary\n\n");
    md.push_str(&format!("- Allowed policy decisions: {}\n", data.allowed));
    md.push_str(&format!(
        "- Approval-required policy decisions: {}\n",
        data.approval_required
    ));
    md.push_str(&format!("- Denied policy decisions: {}\n", data.denied));
    md.push_str(&format!("- Approval records: {}\n", data.approvals.len()));
    md.push_str(&format!(
        "- Tool execution records: {}\n",
        data.tool_executions.len()
    ));
    md.push_str(&format!(
        "- Budget records: {}\n\n",
        data.budget_usage.len()
    ));

    md.push_str("## Policy Decisions\n\n");
    push_table_or_none(&mut md, &data.commands, &["Command", "Decision", "Reason"]);
    push_table_or_none(
        &mut md,
        &data.tool_calls,
        &["Tool", "Operation", "Decision"],
    );
    push_table_or_none(&mut md, &data.network, &["Host", "Port", "Decision"]);

    md.push_str("## Approval Evidence\n\n");
    push_table_or_none(
        &mut md,
        &data.approvals,
        &["Approval ID", "Actor", "Decision"],
    );

    md.push_str("## Gateway Evidence\n\n");
    push_table_or_none(
        &mut md,
        &data.tool_executions,
        &["Tool", "Adapter", "Outcome", "Summary"],
    );

    md.push_str("## Budget Evidence\n\n");
    push_table_or_none(
        &mut md,
        &data.budget_usage,
        &[
            "Kind",
            "Amount",
            "Limit",
            "Provider",
            "Model",
            "Operation",
            "Decision",
        ],
    );

    md.push_str("## Denied Actions\n\n");
    if data.denied_actions.is_empty() {
        md.push_str("None recorded.\n\n");
    } else {
        for denied in &data.denied_actions {
            md.push_str(&format!("- {}\n", text(denied)));
        }
        md.push('\n');
    }

    md.push_str("## Evidence Artifacts\n\n");
    let artifact_rows = artifact_rows(artifacts, &data.artifacts);
    push_table_or_none(&mut md, &artifact_rows, &["Kind", "Path"]);

    md.push_str("## Residual Risks\n\n");
    let risks = residual_risks(events, artifacts, &data);
    if risks.is_empty() {
        md.push_str("No residual risks were identified from structured evidence.\n");
    } else {
        for risk in risks {
            md.push_str(&format!("- {}\n", text(&risk)));
        }
    }

    md
}

#[derive(Debug, Default)]
struct ReportData {
    final_status: Option<TaskStatus>,
    exit_status: Option<ExitStatus>,
    allowed: usize,
    approval_required: usize,
    denied: usize,
    timeline: Vec<(String, String)>,
    commands: Vec<Vec<String>>,
    tool_calls: Vec<Vec<String>>,
    tool_executions: Vec<Vec<String>>,
    budget_usage: Vec<Vec<String>>,
    approvals: Vec<Vec<String>>,
    denied_actions: Vec<String>,
    network: Vec<Vec<String>>,
    artifacts: Vec<(String, Utf8PathBuf)>,
    errors: Vec<String>,
}

impl ReportData {
    fn from_events(events: &[AuditEvent]) -> Self {
        let mut data = Self::default();

        for event in events {
            match event {
                AuditEvent::TaskCreated { at, goal, .. } => {
                    data.timeline
                        .push((at.to_string(), format!("task created: {}", text(goal))));
                }
                AuditEvent::TaskStatusChanged { at, status, .. } => {
                    data.final_status = Some(status.clone());
                    data.timeline.push((
                        at.to_string(),
                        format!("status changed to {}", format_status(status)),
                    ));
                }
                AuditEvent::PolicyDecision {
                    at,
                    action,
                    decision,
                    ..
                } => {
                    let (label, reason) = decision_parts(decision);
                    match decision {
                        ActionDecision::Allow { .. } => data.allowed += 1,
                        ActionDecision::RequireApproval { .. } => data.approval_required += 1,
                        ActionDecision::Deny { .. } => {
                            data.denied += 1;
                            data.denied_actions.push(format!(
                                "{} denied: {}",
                                action_summary(action),
                                reason
                            ));
                        }
                    }
                    data.timeline.push((
                        at.to_string(),
                        format!("policy decision {label}: {}", action_summary(action)),
                    ));
                    match action {
                        Action::Command(command) => data.commands.push(vec![
                            command.raw.clone(),
                            label.clone(),
                            reason.clone(),
                        ]),
                        Action::ToolCall(tool) => data.tool_calls.push(vec![
                            format!("{}:{}", tool.protocol, tool.tool),
                            tool.operation.clone(),
                            label.clone(),
                        ]),
                        Action::Network { host, port } => data.network.push(vec![
                            host.clone(),
                            port.map(|port| port.to_string())
                                .unwrap_or_else(|| "-".into()),
                            label.clone(),
                        ]),
                        Action::FileRead { .. }
                        | Action::FileWrite { .. }
                        | Action::EnvExpose { .. }
                        | Action::SecretAccess { .. }
                        | Action::Budget { .. } => {}
                    }
                }
                AuditEvent::ToolExecutionStarted { at, request, .. } => {
                    data.timeline.push((
                        at.to_string(),
                        format!(
                            "tool execution started: {}",
                            tool_action_summary(&request.action)
                        ),
                    ));
                    data.tool_executions.push(vec![
                        tool_action_summary(&request.action),
                        request
                            .adapter
                            .as_ref()
                            .map(adapter_summary)
                            .unwrap_or_else(|| "-".into()),
                        "started".into(),
                        "-".into(),
                    ]);
                }
                AuditEvent::ToolExecutionFinished { at, execution, .. } => {
                    let action = &execution.request.action;
                    let (outcome, summary) = match (&execution.result, &execution.error) {
                        (Some(result), None) => ("succeeded".to_owned(), result.summary.clone()),
                        (None, Some(error)) => (
                            "failed".to_owned(),
                            format!("{:?}: {}", error.kind, error.message),
                        ),
                        (Some(result), Some(error)) => (
                            "failed".to_owned(),
                            format!(
                                "{:?}: {}; partial result: {}",
                                error.kind, error.message, result.summary
                            ),
                        ),
                        (None, None) => (
                            "unknown".to_owned(),
                            "no result or error recorded".to_owned(),
                        ),
                    };
                    data.timeline.push((
                        at.to_string(),
                        format!("tool execution {outcome}: {}", tool_action_summary(action)),
                    ));
                    data.tool_executions.push(vec![
                        tool_action_summary(action),
                        execution
                            .request
                            .adapter
                            .as_ref()
                            .map(adapter_summary)
                            .unwrap_or_else(|| "-".into()),
                        outcome,
                        summary,
                    ]);
                }
                AuditEvent::BudgetUsageRecorded { at, record, .. } => {
                    let (label, reason) = decision_parts(&record.decision);
                    data.timeline.push((
                        at.to_string(),
                        format!(
                            "budget usage {label}: {} amount {}",
                            record.usage.kind, record.usage.amount
                        ),
                    ));
                    data.budget_usage.push(vec![
                        record.usage.kind.clone(),
                        record.usage.amount.to_string(),
                        record
                            .limit
                            .as_ref()
                            .map(|limit| limit.max_amount.to_string())
                            .unwrap_or_else(|| "-".into()),
                        record.usage.provider.clone().unwrap_or_else(|| "-".into()),
                        record.usage.model.clone().unwrap_or_else(|| "-".into()),
                        record.usage.operation.clone().unwrap_or_else(|| "-".into()),
                        format!("{label}: {reason}"),
                    ]);
                }
                AuditEvent::ApprovalRequested { record } => {
                    data.approvals.push(vec![
                        record.id.0.clone(),
                        record.actor.clone(),
                        "requested".into(),
                    ]);
                }
                AuditEvent::ApprovalResolved { record } => {
                    data.approvals.push(vec![
                        record.id.0.clone(),
                        record.actor.clone(),
                        record
                            .decision
                            .as_ref()
                            .map(format_approval_decision)
                            .unwrap_or_else(|| "unresolved".into()),
                    ]);
                }
                AuditEvent::Log { chunk, .. } => {
                    let stream = match &chunk.stream {
                        LogStream::Stdout => "stdout",
                        LogStream::Stderr => "stderr",
                    };
                    data.timeline.push((
                        chunk.timestamp.to_string(),
                        format!("{stream} log chunk captured ({} bytes)", chunk.text.len()),
                    ));
                }
                AuditEvent::RunnerExit {
                    at, exit_status, ..
                } => {
                    data.exit_status = Some(exit_status.clone());
                    data.timeline.push((
                        at.to_string(),
                        format!("runner exit {}", format_exit_status(exit_status)),
                    ));
                }
                AuditEvent::Artifact { at, kind, path, .. } => {
                    data.artifacts.push((kind.clone(), path.clone()));
                    data.timeline
                        .push((at.to_string(), format!("artifact recorded: {kind}")));
                }
                AuditEvent::Error { at, message, .. } => {
                    data.errors.push(message.clone());
                    data.timeline
                        .push((at.to_string(), format!("error recorded: {}", text(message))));
                }
            }
        }

        data
    }
}

fn decision_parts(decision: &ActionDecision) -> (String, String) {
    match decision {
        ActionDecision::Allow { reason, .. } => ("allow".into(), reason.clone()),
        ActionDecision::RequireApproval { reason, .. } => {
            ("approval required".into(), reason.clone())
        }
        ActionDecision::Deny { reason, .. } => ("deny".into(), reason.clone()),
    }
}

fn action_summary(action: &Action) -> String {
    match action {
        Action::FileRead { path } => format!("file read {}", path),
        Action::FileWrite { path } => format!("file write {}", path),
        Action::Command(command) => format!("command {}", command.raw),
        Action::Network { host, port } => match port {
            Some(port) => format!("network {host}:{port}"),
            None => format!("network {host}"),
        },
        Action::EnvExpose { name } => format!("env expose {name}"),
        Action::SecretAccess { name, scope } => {
            format!("secret access {name} for {scope}")
        }
        Action::ToolCall(tool) => format!("tool call {}", tool_action_summary(tool)),
        Action::Budget { kind, amount } => format!("budget {kind} {amount}"),
    }
}

fn tool_action_summary(tool: &taskfence_core::ToolAction) -> String {
    format!("{} {}.{}", tool.protocol, tool.tool, tool.operation)
}

fn adapter_summary(adapter: &taskfence_core::ToolAdapterIdentity) -> String {
    format!("{}:{}", adapter.kind, adapter.name)
}

fn artifact_rows(artifacts: &ArtifactRefs, recorded: &[(String, Utf8PathBuf)]) -> Vec<Vec<String>> {
    let mut rows = Vec::new();
    rows.push(vec!["task_dir".into(), artifacts.task_dir.to_string()]);
    for (kind, maybe_path) in [
        ("resolved_task", &artifacts.resolved_task),
        ("events", &artifacts.events),
        ("stdout", &artifacts.stdout),
        ("stderr", &artifacts.stderr),
        ("diff", &artifacts.diff),
        ("report", &artifacts.report),
        ("gateway_spool", &artifacts.gateway_spool),
    ] {
        if let Some(path) = maybe_path {
            rows.push(vec![kind.into(), path.to_string()]);
        }
    }
    for (kind, path) in recorded {
        rows.push(vec![format!("recorded:{kind}"), path.to_string()]);
    }
    rows
}

fn residual_risks(
    events: &[AuditEvent],
    artifacts: &ArtifactRefs,
    data: &ReportData,
) -> Vec<String> {
    let mut risks = Vec::new();
    if events.is_empty() {
        risks.push("No audit events were provided to the report generator.".into());
    }
    if artifacts.diff.is_none() {
        risks.push("No diff artifact was available, so file-change evidence is incomplete.".into());
    }
    for error in &data.errors {
        risks.push(format!("Task recorded an error: {error}"));
    }
    risks
}

fn diff_metadata_notes(path: &Utf8Path) -> Vec<String> {
    let Ok(contents) = fs::read_to_string(path.as_std_path()) else {
        return Vec::new();
    };

    let mut notes = Vec::new();
    for line in contents.lines().take(40) {
        if line.starts_with("dirty_before_run:")
            || line.starts_with("warning:")
            || line.starts_with("diff_status:")
        {
            notes.push(line.to_owned());
        }
    }
    notes
}

fn push_table_or_none(md: &mut String, rows: &[Vec<String>], headers: &[&str]) {
    if rows.is_empty() {
        md.push_str("None recorded.\n\n");
        return;
    }
    md.push('|');
    for header in headers {
        md.push(' ');
        md.push_str(header);
        md.push_str(" |");
    }
    md.push('\n');
    md.push('|');
    for _ in headers {
        md.push_str(" --- |");
    }
    md.push('\n');
    for row in rows {
        md.push('|');
        for index in 0..headers.len() {
            md.push(' ');
            md.push_str(&table_cell(
                row.get(index).map(String::as_str).unwrap_or(""),
            ));
            md.push_str(" |");
        }
        md.push('\n');
    }
    md.push('\n');
}

fn format_status(status: &TaskStatus) -> String {
    format!("{status:?}")
}

fn format_approval_decision(decision: &ApprovalDecision) -> String {
    match decision {
        ApprovalDecision::Approved => "approved".into(),
        ApprovalDecision::Denied => "denied".into(),
        ApprovalDecision::TimedOut => "timed out".into(),
    }
}

fn format_exit_status(status: &ExitStatus) -> String {
    if status.timed_out {
        return "timed out".into();
    }
    match (&status.code, &status.signal) {
        (Some(code), Some(signal)) => format!("code {code}, signal {signal}"),
        (Some(code), None) => format!("code {code}"),
        (None, Some(signal)) => format!("signal {signal}"),
        (None, None) => "unknown".into(),
    }
}

fn inline_path(path: &Utf8Path) -> String {
    inline(path.as_str())
}

fn inline(value: &str) -> String {
    format!("`{}`", text(value).replace('`', "\\`"))
}

fn table_cell(value: &str) -> String {
    text(value).replace('|', "\\|").replace('\n', "<br>")
}

fn text(value: &str) -> String {
    redact_secret_like_text(value)
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

fn redact_secret_like_text(input: &str) -> String {
    let mut output = strip_terminal_controls(input);
    for prefix in ["sk-", "ghp_", "github_pat_", "xoxb-", "xoxp-"] {
        output = redact_token_prefix(&output, prefix);
    }
    for marker in [
        "token=",
        "token:",
        "password=",
        "password:",
        "secret=",
        "secret:",
        "api_key=",
        "api_key:",
        "authorization=",
        "authorization:",
        "bearer ",
    ] {
        output = redact_after_marker_case_insensitive(&output, marker);
    }
    output
}

fn strip_terminal_controls(input: &str) -> String {
    input
        .chars()
        .map(|ch| {
            if ch.is_control() && !matches!(ch, '\n' | '\r' | '\t') {
                ' '
            } else {
                ch
            }
        })
        .collect()
}

fn redact_token_prefix(input: &str, prefix: &str) -> String {
    let mut output = String::with_capacity(input.len());
    let mut cursor = 0;

    while let Some(relative_start) = input[cursor..].find(prefix) {
        let start = cursor + relative_start;
        output.push_str(&input[cursor..start]);
        output.push_str(REDACTION_MARKER);

        let mut end = start + prefix.len();
        for (offset, ch) in input[end..].char_indices() {
            if ch.is_ascii_alphanumeric() || matches!(ch, '_' | '-' | '.') {
                end = start + prefix.len() + offset + ch.len_utf8();
            } else {
                break;
            }
        }
        cursor = end;
    }

    output.push_str(&input[cursor..]);
    output
}

fn redact_after_marker_case_insensitive(input: &str, marker: &str) -> String {
    let lower = input.to_ascii_lowercase();
    let mut output = String::with_capacity(input.len());
    let mut cursor = 0;

    while let Some(relative_start) = lower[cursor..].find(marker) {
        let marker_start = cursor + relative_start;
        let value_start = marker_start + marker.len();
        output.push_str(&input[cursor..value_start]);
        output.push_str(REDACTION_MARKER);

        let mut value_end = value_start;
        for (offset, ch) in input[value_start..].char_indices() {
            if ch.is_whitespace() || matches!(ch, ',' | ';') {
                break;
            }
            value_end = value_start + offset + ch.len_utf8();
        }
        cursor = value_end;
    }

    output.push_str(&input[cursor..]);
    output
}

fn atomic_write(path: &Utf8Path, bytes: &[u8]) -> Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .map_err(|err| TaskFenceError::Report(format!("failed to create report dir: {err}")))?;
    }
    let tmp_path = path.with_extension(format!(
        "tmp.{}.{}",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(|err| TaskFenceError::Report(err.to_string()))?
            .as_nanos()
    ));
    {
        let mut file = File::create(tmp_path.as_std_path())
            .map_err(|err| TaskFenceError::Report(format!("failed to create report: {err}")))?;
        file.write_all(bytes)
            .map_err(|err| TaskFenceError::Report(format!("failed to write report: {err}")))?;
        file.sync_all()
            .map_err(|err| TaskFenceError::Report(format!("failed to sync report: {err}")))?;
    }
    fs::rename(tmp_path.as_std_path(), path.as_std_path())
        .map_err(|err| TaskFenceError::Report(format!("failed to publish report: {err}")))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;
    use std::fs;
    use taskfence_core::{
        Action, ActionDecision, AuditEvent, CommandAction, ExitStatus, RedactedValue,
        ReportGenerator, RiskLevel, TaskStatus, ToolAction,
    };
    use taskfence_testkit::sample_task;
    use time::macros::datetime;

    #[test]
    fn writes_markdown_report_from_structured_events() {
        let temp = tempfile::tempdir().unwrap();
        let task_dir = Utf8PathBuf::from_path_buf(temp.path().join("task")).unwrap();
        fs::create_dir_all(&task_dir).unwrap();
        let diff_path = task_dir.join("diff.patch");
        fs::write(
            &diff_path,
            "TaskFence diff metadata\ndirty_before_run: false\n",
        )
        .unwrap();
        let report_path = task_dir.join("report.md");
        let task = sample_task();
        let artifacts = ArtifactRefs {
            task_dir: task_dir.clone(),
            resolved_task: Some(task_dir.join("task.resolved.json")),
            events: Some(task_dir.join("events.jsonl")),
            stdout: Some(task_dir.join("stdout.log")),
            stderr: Some(task_dir.join("stderr.log")),
            diff: Some(diff_path),
            report: Some(report_path.clone()),
            gateway_spool: Some(task_dir.join("gateway-spool")),
        };
        let events = vec![
            AuditEvent::TaskCreated {
                task_id: task.id.clone(),
                at: datetime!(2024-01-01 00:00 UTC),
                goal: task.goal.clone(),
            },
            AuditEvent::PolicyDecision {
                task_id: task.id.clone(),
                at: datetime!(2024-01-01 00:01 UTC),
                action: Action::Command(CommandAction::parse("npm test --token sk-testsecret")),
                decision: ActionDecision::Deny {
                    rule_id: Some("deny-token".into()),
                    reason: "command contains token".into(),
                },
            },
            AuditEvent::RunnerExit {
                task_id: task.id.clone(),
                at: datetime!(2024-01-01 00:02 UTC),
                exit_status: ExitStatus {
                    code: Some(1),
                    timed_out: false,
                    signal: None,
                },
            },
            AuditEvent::TaskStatusChanged {
                task_id: task.id.clone(),
                at: datetime!(2024-01-01 00:03 UTC),
                status: TaskStatus::Failed,
            },
        ];

        let path = MarkdownReportGenerator::new()
            .generate(&task, &artifacts, &events)
            .unwrap();
        let contents = fs::read_to_string(path).unwrap();

        assert!(contents.contains("## Summary"));
        assert!(contents.contains("Final status: Failed"));
        assert!(contents.contains("dirty_before_run: false"));
        assert!(!contents.contains("sk-testsecret"));
        assert!(contents.contains("[redacted]"));
    }

    #[test]
    fn writes_approval_denial_report_from_structured_events() {
        let temp = tempfile::tempdir().unwrap();
        let task_dir = Utf8PathBuf::from_path_buf(temp.path().join("task")).unwrap();
        fs::create_dir_all(&task_dir).unwrap();
        let report_path = task_dir.join("report.md");
        let task = sample_task();
        let action = Action::Command(CommandAction::parse("git push origin main"));
        let decision = ActionDecision::RequireApproval {
            rule_id: Some("approval-push".into()),
            approval_kind: "command".into(),
            reason: "push requires review".into(),
            risk: taskfence_core::RiskLevel::High,
        };
        let record = taskfence_core::ApprovalRecord {
            id: taskfence_core::ApprovalId("approval-1".into()),
            task_id: task.id.clone(),
            actor: "local".into(),
            source: Some("interactive".into()),
            requested_at: datetime!(2024-01-01 00:01 UTC),
            resolved_at: Some(datetime!(2024-01-01 00:02 UTC)),
            action: action.clone(),
            policy_decision: decision.clone(),
            decision: Some(ApprovalDecision::Denied),
        };
        let artifacts = ArtifactRefs {
            task_dir: task_dir.clone(),
            resolved_task: Some(task_dir.join("task.resolved.json")),
            events: Some(task_dir.join("events.jsonl")),
            stdout: Some(task_dir.join("stdout.log")),
            stderr: Some(task_dir.join("stderr.log")),
            diff: None,
            report: Some(report_path.clone()),
            gateway_spool: Some(task_dir.join("gateway-spool")),
        };
        let events = vec![
            AuditEvent::TaskCreated {
                task_id: task.id.clone(),
                at: datetime!(2024-01-01 00:00 UTC),
                goal: task.goal.clone(),
            },
            AuditEvent::PolicyDecision {
                task_id: task.id.clone(),
                at: datetime!(2024-01-01 00:01 UTC),
                action,
                decision,
            },
            AuditEvent::ApprovalRequested {
                record: record.clone(),
            },
            AuditEvent::ApprovalResolved { record },
            AuditEvent::TaskStatusChanged {
                task_id: task.id.clone(),
                at: datetime!(2024-01-01 00:03 UTC),
                status: TaskStatus::Denied,
            },
        ];

        let path = MarkdownReportGenerator::new()
            .generate(&task, &artifacts, &events)
            .unwrap();
        let contents = fs::read_to_string(path).unwrap();

        assert!(contents.contains("Final status: Denied"));
        assert!(contents.contains("Approval-required decisions: 1"));
        assert!(contents.contains("| approval-1 | local | denied |"));
        assert!(contents.contains("No diff artifact was supplied."));
        assert!(contents.contains("No diff artifact was available"));
    }

    #[test]
    fn writes_tool_policy_evidence_without_parameter_values() {
        let temp = tempfile::tempdir().unwrap();
        let task_dir = Utf8PathBuf::from_path_buf(temp.path().join("task")).unwrap();
        fs::create_dir_all(&task_dir).unwrap();
        let report_path = task_dir.join("report.md");
        let task = sample_task();
        let denied_action = Action::ToolCall(ToolAction {
            protocol: "mcp".into(),
            tool: "github".into(),
            operation: "delete_repo".into(),
            parameters: BTreeMap::from([(
                "token".into(),
                RedactedValue::Plain("ghp_secret_should_not_render".into()),
            )]),
        });
        let approval_action = Action::ToolCall(ToolAction {
            protocol: "mcp".into(),
            tool: "github".into(),
            operation: "create_pr".into(),
            parameters: BTreeMap::from([(
                "authorization".into(),
                RedactedValue::Plain("Bearer sk-secret-should-not-render".into()),
            )]),
        });
        let approval_decision = ActionDecision::RequireApproval {
            approval_kind: "tool_call".into(),
            rule_id: Some("tools.approval".into()),
            reason: "tool call matched approval rule".into(),
            risk: RiskLevel::Medium,
        };
        let record = taskfence_core::ApprovalRecord {
            id: taskfence_core::ApprovalId("approval-tool-1".into()),
            task_id: task.id.clone(),
            actor: "gateway".into(),
            source: Some("mcp".into()),
            requested_at: datetime!(2024-01-01 00:02 UTC),
            resolved_at: Some(datetime!(2024-01-01 00:03 UTC)),
            action: approval_action.clone(),
            policy_decision: approval_decision.clone(),
            decision: Some(ApprovalDecision::Approved),
        };
        let artifacts = ArtifactRefs {
            task_dir: task_dir.clone(),
            resolved_task: Some(task_dir.join("task.resolved.json")),
            events: Some(task_dir.join("events.jsonl")),
            stdout: Some(task_dir.join("stdout.log")),
            stderr: Some(task_dir.join("stderr.log")),
            diff: None,
            report: Some(report_path.clone()),
            gateway_spool: Some(task_dir.join("gateway-spool")),
        };
        let events = vec![
            AuditEvent::PolicyDecision {
                task_id: task.id.clone(),
                at: datetime!(2024-01-01 00:01 UTC),
                action: denied_action,
                decision: ActionDecision::Deny {
                    rule_id: Some("tools.deny".into()),
                    reason: "tool call matched deny rule".into(),
                },
            },
            AuditEvent::PolicyDecision {
                task_id: task.id.clone(),
                at: datetime!(2024-01-01 00:02 UTC),
                action: approval_action,
                decision: approval_decision,
            },
            AuditEvent::ApprovalRequested {
                record: record.clone(),
            },
            AuditEvent::ApprovalResolved { record },
        ];

        let path = MarkdownReportGenerator::new()
            .generate(&task, &artifacts, &events)
            .unwrap();
        let contents = fs::read_to_string(path).unwrap();

        assert!(contents.contains("## Tool Calls"));
        assert!(contents.contains("| mcp:github | delete_repo | deny |"));
        assert!(contents.contains("| mcp:github | create_pr | approval required |"));
        assert!(contents.contains("Denied decisions: 1"));
        assert!(contents.contains("Approval-required decisions: 1"));
        assert!(contents
            .contains("tool call mcp github.delete_repo denied: tool call matched deny rule"));
        assert!(contents.contains("| approval-tool-1 | gateway | approved |"));
        assert!(!contents.contains("ghp_secret_should_not_render"));
        assert!(!contents.contains("sk-secret-should-not-render"));
    }

    #[test]
    fn compliance_report_uses_structured_events_without_raw_parameters() {
        let temp = tempfile::tempdir().unwrap();
        let task_dir = Utf8PathBuf::from_path_buf(temp.path().join("task")).unwrap();
        fs::create_dir_all(&task_dir).unwrap();
        let task = sample_task();
        let artifacts = ArtifactRefs {
            task_dir: task_dir.clone(),
            resolved_task: Some(task_dir.join("task.resolved.json")),
            events: Some(task_dir.join("events.jsonl")),
            stdout: None,
            stderr: None,
            diff: None,
            report: None,
            gateway_spool: None,
        };
        let action = Action::ToolCall(ToolAction {
            protocol: "mcp".into(),
            tool: "database".into(),
            operation: "write".into(),
            parameters: BTreeMap::from([(
                "query".into(),
                RedactedValue::Plain("select token=raw-secret".into()),
            )]),
        });
        let events = vec![AuditEvent::PolicyDecision {
            task_id: task.id.clone(),
            at: datetime!(2024-01-01 00:01 UTC),
            action,
            decision: ActionDecision::RequireApproval {
                approval_kind: "tool_call".into(),
                rule_id: Some("database-write".into()),
                reason: "database writes require review".into(),
                risk: RiskLevel::High,
            },
        }];

        let contents = ComplianceReportGenerator::new().render_markdown(&task, &artifacts, &events);

        assert!(contents.contains("TaskFence Compliance Evidence"));
        assert!(contents.contains("Approval-required policy decisions: 1"));
        assert!(contents.contains("| mcp:database | write | approval required |"));
        assert!(!contents.contains("raw-secret"));
    }

    #[test]
    fn golden_report_stays_stable() {
        let temp = tempfile::tempdir().unwrap();
        let task_dir = Utf8PathBuf::from_path_buf(temp.path().join("task")).unwrap();
        fs::create_dir_all(&task_dir).unwrap();
        let diff_path = task_dir.join("diff.patch");
        fs::write(
            &diff_path,
            concat!(
                "TaskFence diff metadata\n",
                "dirty_before_run: true\n",
                "warning: workspace was dirty before the task; final diffs are not attributed exclusively to the agent\n",
            ),
        )
        .unwrap();
        let mut task = sample_task();
        task.workspace_host_path = Utf8PathBuf::from("/workspace/repo");
        let artifacts = ArtifactRefs {
            task_dir: task_dir.clone(),
            resolved_task: Some(task_dir.join("task.resolved.json")),
            events: Some(task_dir.join("events.jsonl")),
            stdout: Some(task_dir.join("stdout.log")),
            stderr: Some(task_dir.join("stderr.log")),
            diff: Some(diff_path),
            report: Some(task_dir.join("report.md")),
            gateway_spool: Some(task_dir.join("gateway-spool")),
        };
        let events = vec![
            AuditEvent::TaskCreated {
                task_id: task.id.clone(),
                at: datetime!(2024-02-03 04:05 UTC),
                goal: task.goal.clone(),
            },
            AuditEvent::PolicyDecision {
                task_id: task.id.clone(),
                at: datetime!(2024-02-03 04:06 UTC),
                action: Action::Command(CommandAction::parse("npm test")),
                decision: ActionDecision::Allow {
                    rule_id: Some("allow-tests".into()),
                    reason: "command matched allow rule".into(),
                },
            },
            AuditEvent::RunnerExit {
                task_id: task.id.clone(),
                at: datetime!(2024-02-03 04:07 UTC),
                exit_status: ExitStatus {
                    code: Some(0),
                    timed_out: false,
                    signal: None,
                },
            },
            AuditEvent::TaskStatusChanged {
                task_id: task.id.clone(),
                at: datetime!(2024-02-03 04:08 UTC),
                status: TaskStatus::Succeeded,
            },
        ];

        let generated_path = MarkdownReportGenerator::new()
            .generate(&task, &artifacts, &events)
            .unwrap();
        let generated = fs::read_to_string(generated_path)
            .unwrap()
            .replace(task_dir.as_str(), "/tmp/taskfence-golden/task");
        let expected = include_str!("../../taskfence-testkit/fixtures/report/basic.md");

        assert_eq!(generated, expected);
    }
}
