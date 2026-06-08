use camino::Utf8PathBuf;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use thiserror::Error;
use time::OffsetDateTime;
use uuid::Uuid;

pub type Result<T> = std::result::Result<T, TaskFenceError>;

pub const GATEWAY_SPOOL_DIR_NAME: &str = "gateway-spool";
pub const GATEWAY_SPOOL_REQUESTS_DIR_NAME: &str = "requests";
pub const GATEWAY_SPOOL_RESPONSES_DIR_NAME: &str = "responses";
pub const GATEWAY_SPOOL_WRAPPER_FILE_NAME: &str = "taskfence-gateway-submit";
pub const GATEWAY_SPOOL_CONTAINER_PATH: &str = "/taskfence/gateway-spool";
pub const GATEWAY_EGRESS_TOOL_PROTOCOL: &str = "http";
pub const GATEWAY_EGRESS_TOOL_NAME: &str = "egress";
pub const GATEWAY_EGRESS_TOOL_OPERATION: &str = "fetch";
pub const TASKFENCE_GATEWAY_MODE_ENV: &str = "TASKFENCE_GATEWAY_MODE";
pub const TASKFENCE_GATEWAY_SPOOL_ENV: &str = "TASKFENCE_GATEWAY_SPOOL";
pub const TASKFENCE_GATEWAY_EGRESS_ALLOW_DOMAINS_ENV: &str =
    "TASKFENCE_GATEWAY_EGRESS_ALLOW_DOMAINS";

#[derive(Debug, Error)]
pub enum TaskFenceError {
    #[error("configuration error: {0}")]
    Config(String),
    #[error("policy error: {0}")]
    Policy(String),
    #[error("approval error: {0}")]
    Approval(String),
    #[error("runner error: {0}")]
    Runner(String),
    #[error("audit error: {0}")]
    Audit(String),
    #[error("artifact error: {0}")]
    Artifact(String),
    #[error("report error: {0}")]
    Report(String),
    #[error("state error: {0}")]
    State(String),
    #[error("gateway error: {0}")]
    Gateway(String),
    #[error("unsupported operation: {0}")]
    Unsupported(String),
}

impl From<std::io::Error> for TaskFenceError {
    fn from(value: std::io::Error) -> Self {
        Self::Artifact(value.to_string())
    }
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct TaskId(pub String);

impl TaskId {
    pub fn new(prefix: impl AsRef<str>) -> Self {
        Self(format!("{}-{}", prefix.as_ref(), Uuid::now_v7()))
    }
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct ApprovalId(pub String);

impl ApprovalId {
    pub fn new() -> Self {
        Self(format!("approval-{}", Uuid::new_v4()))
    }
}

impl Default for ApprovalId {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum TaskStatus {
    Created,
    Validating,
    Preparing,
    Running,
    WaitingForApproval,
    Stopping,
    CollectingArtifacts,
    Reporting,
    Succeeded,
    Failed,
    Denied,
    TimedOut,
    Cancelled,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum SandboxKind {
    Docker,
    RemoteSsh,
    KubernetesJob,
    MicroVm,
    ManagedCloud,
    Unsupported(String),
}

impl SandboxKind {
    pub fn label(&self) -> &str {
        match self {
            Self::Docker => "docker",
            Self::RemoteSsh => "remote_ssh",
            Self::KubernetesJob => "kubernetes_job",
            Self::MicroVm => "microvm",
            Self::ManagedCloud => "managed_cloud",
            Self::Unsupported(kind) => kind,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum AgentKind {
    Generic,
    Specialized(String),
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum NetworkDefault {
    Allow,
    #[default]
    Deny,
    Disabled,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct TaskDefinition {
    pub id: Option<String>,
    pub goal: String,
    pub workspace: Utf8PathBuf,
    pub agent: AgentConfig,
    pub sandbox: SandboxConfig,
    pub permissions: PermissionConfig,
    pub secrets: SecretConfig,
    pub approval: ApprovalConfig,
    #[serde(default)]
    pub gateway: GatewayConfig,
    pub audit: AuditConfig,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ResolvedTask {
    pub id: TaskId,
    pub task_file: Utf8PathBuf,
    pub goal: String,
    pub workspace_host_path: Utf8PathBuf,
    pub workspace_container_path: Utf8PathBuf,
    pub agent: AgentConfig,
    pub sandbox: SandboxConfig,
    pub permissions: PermissionConfig,
    pub secrets: SecretConfig,
    pub approval: ApprovalConfig,
    #[serde(default)]
    pub gateway: GatewayConfig,
    pub audit: AuditConfig,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AgentConfig {
    pub kind: AgentKind,
    pub command: String,
    pub args: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SandboxConfig {
    pub kind: SandboxKind,
    pub image: Option<String>,
    pub limits: LimitConfig,
}

#[derive(Clone, Debug, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LimitConfig {
    pub timeout_minutes: Option<u64>,
    pub cpu: Option<u64>,
    pub memory: Option<String>,
    pub disk: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PermissionConfig {
    pub paths: PathPermissions,
    pub commands: CommandPermissions,
    pub network: NetworkPermissions,
    pub env: EnvPermissions,
    pub tools: ToolPermissions,
    #[serde(default)]
    pub budget: BudgetPermissions,
}

#[derive(Clone, Debug, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PathPermissions {
    #[serde(default)]
    pub read: Vec<Utf8PathBuf>,
    #[serde(default)]
    pub write: Vec<Utf8PathBuf>,
}

#[derive(Clone, Debug, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CommandPermissions {
    #[serde(default)]
    pub allow: Vec<String>,
    #[serde(default)]
    pub approval_required: Vec<String>,
    #[serde(default)]
    pub deny: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NetworkPermissions {
    pub default: NetworkDefault,
    pub allow_domains: Vec<String>,
}

impl Default for NetworkPermissions {
    fn default() -> Self {
        Self {
            default: NetworkDefault::Deny,
            allow_domains: Vec::new(),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EnvPermissions {
    #[serde(default)]
    pub allow: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ToolPermissions {
    #[serde(default)]
    pub allow: Vec<String>,
    #[serde(default)]
    pub approval_required: Vec<String>,
    #[serde(default)]
    pub deny: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BudgetPermissions {
    #[serde(default)]
    pub allow: Vec<BudgetLimit>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BudgetLimit {
    pub kind: String,
    pub max_amount: u64,
}

#[derive(Clone, Debug, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SecretConfig {
    pub expose_to_agent: bool,
    pub available_to_gateway: Vec<SecretGrant>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SecretGrant {
    pub name: String,
    pub use_for: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ApprovalConfig {
    pub require_for: Vec<String>,
    pub timeout_minutes: Option<u64>,
}

impl Default for ApprovalConfig {
    fn default() -> Self {
        Self {
            require_for: Vec::new(),
            timeout_minutes: Some(60),
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GatewayConfig {
    #[serde(default)]
    pub mode: GatewayMode,
    #[serde(default)]
    pub egress: GatewayEgressConfig,
    #[serde(default)]
    pub tools: Vec<GatewayToolConfig>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GatewayMode {
    #[default]
    SpoolOnly,
    LocalListener,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GatewayEgressConfig {
    #[serde(default)]
    pub allow_domains: bool,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GatewayToolConfig {
    pub protocol: String,
    pub tool: String,
    pub operation: String,
    pub connector: GatewayConnectorConfig,
    #[serde(default)]
    pub secret_refs: Vec<GatewaySecretReferenceConfig>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum GatewayConnectorConfig {
    LocalFixture {
        kind: String,
        path: Utf8PathBuf,
    },
    #[serde(rename = "github_rest")]
    GitHubRest {
        api_base: String,
        repository: String,
    },
    #[serde(rename = "github_enterprise_rest")]
    GitHubEnterpriseRest {
        api_base: String,
        repository: String,
    },
    #[serde(rename = "gitlab")]
    GitLab {
        api_base: String,
        project: String,
    },
    Jira {
        api_base: String,
        project_key: String,
    },
    Feishu {
        api_base: String,
        app: String,
    },
    #[serde(rename = "wecom")]
    WeCom {
        api_base: String,
        corp_id: String,
    },
    #[serde(rename = "dingtalk")]
    DingTalk {
        api_base: String,
        tenant: String,
    },
    Gitee {
        api_base: String,
        repository: String,
    },
    #[serde(rename = "coding")]
    Coding {
        api_base: String,
        project: String,
    },
    Database {
        engine: String,
        database_ref: String,
    },
    InternalHttp {
        api_base: String,
        service: String,
    },
    SiemExport {
        sink: String,
    },
    Unsupported {
        kind: String,
    },
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GatewaySecretReferenceConfig {
    pub name: String,
    pub parameter: String,
    pub scope: String,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AuditConfig {
    pub report: ReportConfig,
    pub capture: CaptureConfig,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ReportConfig {
    pub format: ReportFormat,
}

impl Default for ReportConfig {
    fn default() -> Self {
        Self {
            format: ReportFormat::Markdown,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum ReportFormat {
    Markdown,
    Html,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CaptureConfig {
    #[serde(default = "default_true")]
    pub stdout: bool,
    #[serde(default = "default_true")]
    pub stderr: bool,
    #[serde(default = "default_true")]
    pub file_diff: bool,
    #[serde(default = "default_true")]
    pub network_destinations: bool,
    #[serde(default = "default_true")]
    pub approvals: bool,
}

impl Default for CaptureConfig {
    fn default() -> Self {
        Self {
            stdout: true,
            stderr: true,
            file_diff: true,
            network_destinations: true,
            approvals: true,
        }
    }
}

fn default_true() -> bool {
    true
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Action {
    FileRead { path: Utf8PathBuf },
    FileWrite { path: Utf8PathBuf },
    Command(CommandAction),
    Network { host: String, port: Option<u16> },
    EnvExpose { name: String },
    SecretAccess { name: String, scope: String },
    ToolCall(ToolAction),
    Budget { kind: String, amount: u64 },
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BudgetUsage {
    pub kind: String,
    pub amount: u64,
    #[serde(default)]
    pub provider: Option<String>,
    #[serde(default)]
    pub model: Option<String>,
    #[serde(default)]
    pub operation: Option<String>,
    #[serde(default)]
    pub metadata: BTreeMap<String, RedactedValue>,
}

impl BudgetUsage {
    pub fn normalized(mut self) -> Result<Self> {
        self.kind = normalize_budget_usage_text("budget usage kind", self.kind)?;
        if self.amount == 0 {
            return Err(TaskFenceError::Policy(
                "budget usage amount must be positive".into(),
            ));
        }
        self.provider = normalize_optional_budget_usage_text(self.provider);
        self.model = normalize_optional_budget_usage_text(self.model);
        self.operation = normalize_optional_budget_usage_text(self.operation);

        let mut metadata = BTreeMap::new();
        for (key, value) in self.metadata {
            let key = key.trim().to_owned();
            if key.is_empty() {
                return Err(TaskFenceError::Policy(
                    "budget usage metadata key must not be empty".into(),
                ));
            }
            metadata.insert(key, value);
        }
        self.metadata = metadata;
        Ok(self)
    }
}

fn normalize_budget_usage_text(field: &str, value: String) -> Result<String> {
    let normalized = value.trim().to_ascii_lowercase();
    if normalized.is_empty() {
        return Err(TaskFenceError::Policy(format!("{field} must not be empty")));
    }
    Ok(normalized)
}

fn normalize_optional_budget_usage_text(value: Option<String>) -> Option<String> {
    value
        .map(|value| value.trim().to_owned())
        .filter(|value| !value.is_empty())
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BudgetUsageRecord {
    pub usage: BudgetUsage,
    #[serde(default)]
    pub limit: Option<BudgetLimit>,
    pub decision: ActionDecision,
}

pub fn budget_limit_for(task: &ResolvedTask, kind: &str) -> Option<BudgetLimit> {
    let normalized = kind.trim().to_ascii_lowercase();
    task.permissions
        .budget
        .allow
        .iter()
        .find(|limit| limit.kind == normalized)
        .cloned()
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct CommandAction {
    pub executable: String,
    pub args: Vec<String>,
    pub raw: String,
    pub shell_wrapped: bool,
}

impl CommandAction {
    pub fn parse(raw: impl Into<String>) -> Self {
        let raw = raw.into();
        let parts = split_command_words(&raw);
        let executable = parts.first().cloned().unwrap_or_default();
        let args = parts.iter().skip(1).cloned().collect::<Vec<_>>();
        let shell_wrapped = matches!(
            executable.as_str(),
            "sh" | "bash" | "zsh" | "/bin/sh" | "/bin/bash" | "/bin/zsh"
        ) && args.iter().any(|arg| arg == "-c");
        Self {
            executable,
            args,
            raw,
            shell_wrapped,
        }
    }
}

fn split_command_words(raw: &str) -> Vec<String> {
    let mut words = Vec::new();
    let mut current = String::new();
    let mut quote = None;
    let mut escaped = false;

    for ch in raw.chars() {
        if escaped {
            current.push(ch);
            escaped = false;
            continue;
        }

        match ch {
            '\\' => escaped = true,
            '\'' | '"' if quote == Some(ch) => quote = None,
            '\'' | '"' if quote.is_none() => quote = Some(ch),
            c if c.is_whitespace() && quote.is_none() => {
                if !current.is_empty() {
                    words.push(std::mem::take(&mut current));
                }
            }
            c => current.push(c),
        }
    }

    if !current.is_empty() {
        words.push(current);
    }

    words
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ToolAction {
    pub protocol: String,
    pub tool: String,
    pub operation: String,
    pub parameters: BTreeMap<String, RedactedValue>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum RedactedValue {
    Plain(String),
    Redacted { reason: String },
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ToolAdapterIdentity {
    pub kind: String,
    pub name: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ToolRequest {
    pub action: ToolAction,
    pub adapter: Option<ToolAdapterIdentity>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ToolExecutionContext {
    pub task_dir: Option<Utf8PathBuf>,
    pub artifact_dir: Option<Utf8PathBuf>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ToolResult {
    pub summary: String,
    #[serde(default)]
    pub values: BTreeMap<String, RedactedValue>,
    #[serde(default)]
    pub artifacts: Vec<Utf8PathBuf>,
    #[serde(default)]
    pub usage: Vec<BudgetUsage>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum ToolExecutionErrorKind {
    UnsupportedProtocol,
    UnsupportedTool,
    UnregisteredTool,
    InvalidParameters,
    PolicyDenied,
    ApprovalDeniedOrTimedOut,
    BudgetExceeded,
    AdapterFailed,
    SecretUnavailable,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ToolExecutionError {
    pub kind: ToolExecutionErrorKind,
    pub message: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ToolExecution {
    pub request: ToolRequest,
    pub result: Option<ToolResult>,
    pub error: Option<ToolExecutionError>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum ActionDecision {
    Allow {
        rule_id: Option<String>,
        reason: String,
    },
    RequireApproval {
        approval_kind: String,
        rule_id: Option<String>,
        reason: String,
        risk: RiskLevel,
    },
    Deny {
        rule_id: Option<String>,
        reason: String,
    },
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum RiskLevel {
    Low,
    Medium,
    High,
    Critical,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum ApprovalDecision {
    Approved,
    Denied,
    TimedOut,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ApprovalRecord {
    pub id: ApprovalId,
    pub task_id: TaskId,
    pub actor: String,
    pub source: Option<String>,
    pub requested_at: OffsetDateTime,
    pub resolved_at: Option<OffsetDateTime>,
    pub action: Action,
    pub policy_decision: ActionDecision,
    pub decision: Option<ApprovalDecision>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct AgentInvocation {
    pub executable: String,
    pub args: Vec<String>,
    pub env: BTreeMap<String, String>,
    pub working_dir: Utf8PathBuf,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct MountPlan {
    pub host_path: Utf8PathBuf,
    pub container_path: Utf8PathBuf,
    pub mode: MountMode,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum MountMode {
    ReadOnly,
    ReadWrite,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct PreparedRun {
    pub task_id: TaskId,
    pub image: Option<String>,
    pub mounts: Vec<MountPlan>,
    pub env: BTreeMap<String, String>,
    pub network: NetworkPermissions,
    pub gateway: PreparedGateway,
    pub limits: LimitConfig,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PreparedGateway {
    pub mode: GatewayMode,
    pub spool_container_path: Option<Utf8PathBuf>,
    pub egress: Option<PreparedGatewayEgress>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PreparedGatewayEgress {
    pub allow_domains: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TaskValidation {
    pub task_id: TaskId,
    pub workspace_host_path: Utf8PathBuf,
    pub invocation: AgentInvocation,
    pub command_action: CommandAction,
    pub command_decision: ActionDecision,
    pub prepared: PreparedRun,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct RunningTask {
    pub task_id: TaskId,
    pub runner_ref: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum LogStream {
    Stdout,
    Stderr,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct LogChunk {
    pub stream: LogStream,
    pub text: String,
    pub timestamp: OffsetDateTime,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ExitStatus {
    pub code: Option<i32>,
    pub timed_out: bool,
    pub signal: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct RunOutput {
    pub exit_status: ExitStatus,
    pub stdout: String,
    pub stderr: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ArtifactRefs {
    pub task_dir: Utf8PathBuf,
    pub resolved_task: Option<Utf8PathBuf>,
    pub events: Option<Utf8PathBuf>,
    pub stdout: Option<Utf8PathBuf>,
    pub stderr: Option<Utf8PathBuf>,
    pub diff: Option<Utf8PathBuf>,
    pub report: Option<Utf8PathBuf>,
    pub gateway_spool: Option<Utf8PathBuf>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorkspaceBaseline {
    pub dirty_before_run: bool,
    pub summary: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct TaskResult {
    pub task_id: TaskId,
    pub status: TaskStatus,
    pub exit_status: Option<ExitStatus>,
    pub artifacts: ArtifactRefs,
    pub message: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum AuditEvent {
    TaskCreated {
        task_id: TaskId,
        at: OffsetDateTime,
        goal: String,
    },
    TaskStatusChanged {
        task_id: TaskId,
        at: OffsetDateTime,
        status: TaskStatus,
    },
    PolicyDecision {
        task_id: TaskId,
        at: OffsetDateTime,
        action: Action,
        decision: ActionDecision,
    },
    ToolExecutionStarted {
        task_id: TaskId,
        at: OffsetDateTime,
        request: ToolRequest,
    },
    ToolExecutionFinished {
        task_id: TaskId,
        at: OffsetDateTime,
        execution: ToolExecution,
    },
    BudgetUsageRecorded {
        task_id: TaskId,
        at: OffsetDateTime,
        record: BudgetUsageRecord,
    },
    ApprovalRequested {
        record: ApprovalRecord,
    },
    ApprovalResolved {
        record: ApprovalRecord,
    },
    Log {
        task_id: TaskId,
        chunk: LogChunk,
    },
    RunnerExit {
        task_id: TaskId,
        at: OffsetDateTime,
        exit_status: ExitStatus,
    },
    Artifact {
        task_id: TaskId,
        at: OffsetDateTime,
        kind: String,
        path: Utf8PathBuf,
    },
    Error {
        task_id: TaskId,
        at: OffsetDateTime,
        message: String,
    },
}

pub trait PolicyEngine {
    fn evaluate(&self, task: &ResolvedTask, action: &Action) -> Result<ActionDecision>;
}

pub trait ApprovalEngine {
    fn request(
        &self,
        task: &ResolvedTask,
        action: Action,
        decision: ActionDecision,
    ) -> Result<ApprovalRecord>;
    fn wait(&self, approval_id: &ApprovalId) -> Result<ApprovalRecord>;
}

pub trait AuditLogger {
    fn record(&self, event: AuditEvent) -> Result<()>;
}

pub trait ArtifactStore {
    fn create_task_dir(&self, task: &ResolvedTask) -> Result<ArtifactRefs>;
    fn write_resolved_task(&self, task: &ResolvedTask) -> Result<Utf8PathBuf>;
    fn write_log(
        &self,
        task: &ResolvedTask,
        stream: LogStream,
        contents: &str,
    ) -> Result<Utf8PathBuf>;
    fn capture_baseline(&self, task: &ResolvedTask) -> Result<WorkspaceBaseline>;
    fn collect_diff(
        &self,
        task: &ResolvedTask,
        baseline: &WorkspaceBaseline,
    ) -> Result<Option<Utf8PathBuf>>;
}

pub trait AgentAdapter {
    fn build_invocation(&self, task: &ResolvedTask) -> Result<AgentInvocation>;
}

pub trait Runner {
    fn prepare(&self, task: &ResolvedTask) -> Result<PreparedRun>;
    fn start(&self, prepared: PreparedRun, invocation: AgentInvocation) -> Result<RunningTask>;
    fn stop(&self, running: &RunningTask) -> Result<()>;
    fn collect_exit(&self, running: &RunningTask) -> Result<RunOutput>;
}

pub trait ReportGenerator {
    fn generate(
        &self,
        task: &ResolvedTask,
        artifacts: &ArtifactRefs,
        events: &[AuditEvent],
    ) -> Result<Utf8PathBuf>;
}

pub trait StateStore {
    fn set_status(&self, task_id: &TaskId, status: TaskStatus) -> Result<()>;
    fn get_status(&self, task_id: &TaskId) -> Result<Option<TaskStatus>>;
}

pub struct Orchestrator<'a> {
    pub policy: &'a dyn PolicyEngine,
    pub approval: &'a dyn ApprovalEngine,
    pub audit: &'a dyn AuditLogger,
    pub artifacts: &'a dyn ArtifactStore,
    pub adapter: &'a dyn AgentAdapter,
    pub runner: &'a dyn Runner,
    pub report: &'a dyn ReportGenerator,
    pub state: &'a dyn StateStore,
}

pub fn validate_task_for_run(
    task: &ResolvedTask,
    adapter: &dyn AgentAdapter,
    policy: &dyn PolicyEngine,
    runner: &dyn Runner,
) -> Result<TaskValidation> {
    let invocation = adapter.build_invocation(task)?;
    let command_action = command_action_from_invocation(&invocation);
    let command_decision = policy.evaluate(task, &Action::Command(command_action.clone()))?;
    if matches!(command_decision, ActionDecision::Deny { .. }) {
        return Err(TaskFenceError::Policy(format!(
            "planned agent command rejected: {}",
            action_decision_summary(&command_decision)
        )));
    }
    let prepared = runner.prepare(task)?;

    Ok(TaskValidation {
        task_id: task.id.clone(),
        workspace_host_path: task.workspace_host_path.clone(),
        invocation,
        command_action,
        command_decision,
        prepared,
    })
}

fn command_action_from_invocation(invocation: &AgentInvocation) -> CommandAction {
    let raw = std::iter::once(invocation.executable.clone())
        .chain(invocation.args.iter().cloned())
        .collect::<Vec<_>>()
        .join(" ");
    CommandAction {
        executable: invocation.executable.clone(),
        args: invocation.args.clone(),
        shell_wrapped: CommandAction::parse(raw.clone()).shell_wrapped,
        raw,
    }
}

fn action_decision_summary(decision: &ActionDecision) -> String {
    match decision {
        ActionDecision::Allow { reason, .. } => format!("allow: {reason}"),
        ActionDecision::RequireApproval {
            approval_kind,
            reason,
            risk,
            ..
        } => format!("requires {approval_kind} approval: {reason}; risk {risk:?}"),
        ActionDecision::Deny { reason, .. } => format!("deny: {reason}"),
    }
}

impl<'a> Orchestrator<'a> {
    pub fn run(&self, task: ResolvedTask) -> Result<TaskResult> {
        let mut events = Vec::new();
        self.transition(&mut events, &task.id, TaskStatus::Created)?;
        self.record_event(
            &mut events,
            AuditEvent::TaskCreated {
                task_id: task.id.clone(),
                at: OffsetDateTime::now_utc(),
                goal: task.goal.clone(),
            },
        )?;

        self.transition(&mut events, &task.id, TaskStatus::Validating)?;
        let invocation = self.adapter.build_invocation(&task)?;
        let command_action = command_action_from_invocation(&invocation);
        self.transition(&mut events, &task.id, TaskStatus::Preparing)?;
        let artifacts = self.artifacts.create_task_dir(&task)?;
        self.artifacts.write_resolved_task(&task)?;

        if let Err(err) =
            self.evaluate_or_request_approval(&mut events, &task, Action::Command(command_action))
        {
            return self.finish_pre_run_failure(events, task, artifacts, err);
        }

        let baseline = self.artifacts.capture_baseline(&task)?;
        let prepared = self.runner.prepare(&task)?;

        self.transition(&mut events, &task.id, TaskStatus::Running)?;
        let running = match self.runner.start(prepared, invocation) {
            Ok(running) => running,
            Err(err) => {
                self.record_error(&mut events, &task.id, &err.to_string())?;
                self.transition(&mut events, &task.id, TaskStatus::Failed)?;
                let _ = self.generate_report(&mut events, &task, &artifacts);
                return Ok(TaskResult {
                    task_id: task.id,
                    status: TaskStatus::Failed,
                    exit_status: None,
                    artifacts,
                    message: Some(err.to_string()),
                });
            }
        };

        let run_output = match self.runner.collect_exit(&running) {
            Ok(output) => output,
            Err(err) => {
                let _ = self.runner.stop(&running);
                self.record_error(&mut events, &task.id, &err.to_string())?;
                self.transition(&mut events, &task.id, TaskStatus::Failed)?;
                let _ = self.generate_report(&mut events, &task, &artifacts);
                return Ok(TaskResult {
                    task_id: task.id,
                    status: TaskStatus::Failed,
                    exit_status: None,
                    artifacts,
                    message: Some(err.to_string()),
                });
            }
        };

        let mut failure_message = None;
        if let Err(err) = self.record_run_logs(&mut events, &task, &run_output) {
            let message = err.to_string();
            self.record_error(&mut events, &task.id, &message)?;
            failure_message = Some(message);
        }
        self.record_event(
            &mut events,
            AuditEvent::RunnerExit {
                task_id: task.id.clone(),
                at: OffsetDateTime::now_utc(),
                exit_status: run_output.exit_status.clone(),
            },
        )?;

        self.transition(&mut events, &task.id, TaskStatus::CollectingArtifacts)?;
        match self.artifacts.collect_diff(&task, &baseline) {
            Ok(Some(diff_path)) => {
                self.record_event(
                    &mut events,
                    AuditEvent::Artifact {
                        task_id: task.id.clone(),
                        at: OffsetDateTime::now_utc(),
                        kind: "diff".into(),
                        path: diff_path,
                    },
                )?;
            }
            Ok(None) => {}
            Err(err) => {
                let message = err.to_string();
                self.record_error(&mut events, &task.id, &message)?;
                if failure_message.is_none() {
                    failure_message = Some(message);
                }
            }
        }

        let final_status = if failure_message.is_some() {
            TaskStatus::Failed
        } else if run_output.exit_status.timed_out {
            TaskStatus::TimedOut
        } else if run_output.exit_status.code == Some(0) {
            TaskStatus::Succeeded
        } else {
            TaskStatus::Failed
        };

        self.transition(&mut events, &task.id, TaskStatus::Reporting)?;
        self.transition(&mut events, &task.id, final_status.clone())?;
        if let Err(err) = self.generate_report(&mut events, &task, &artifacts) {
            self.record_error(&mut events, &task.id, &err.to_string())?;
            self.transition(&mut events, &task.id, TaskStatus::Failed)?;
            return Ok(TaskResult {
                task_id: task.id,
                status: TaskStatus::Failed,
                exit_status: Some(run_output.exit_status),
                artifacts,
                message: Some(err.to_string()),
            });
        }

        Ok(TaskResult {
            task_id: task.id,
            status: final_status,
            exit_status: Some(run_output.exit_status),
            artifacts,
            message: failure_message,
        })
    }

    fn finish_pre_run_failure(
        &self,
        mut events: Vec<AuditEvent>,
        task: ResolvedTask,
        artifacts: ArtifactRefs,
        err: TaskFenceError,
    ) -> Result<TaskResult> {
        let status = if has_status(&events, &TaskStatus::Denied) {
            TaskStatus::Denied
        } else {
            self.transition(&mut events, &task.id, TaskStatus::Failed)?;
            TaskStatus::Failed
        };
        let message = err.to_string();
        self.record_error(&mut events, &task.id, &message)?;

        if let Err(report_err) = self.generate_report(&mut events, &task, &artifacts) {
            self.record_error(&mut events, &task.id, &report_err.to_string())?;
            self.transition(&mut events, &task.id, TaskStatus::Failed)?;
            return Ok(TaskResult {
                task_id: task.id,
                status: TaskStatus::Failed,
                exit_status: None,
                artifacts,
                message: Some(report_err.to_string()),
            });
        }

        Ok(TaskResult {
            task_id: task.id,
            status,
            exit_status: None,
            artifacts,
            message: Some(message),
        })
    }

    fn evaluate_or_request_approval(
        &self,
        events: &mut Vec<AuditEvent>,
        task: &ResolvedTask,
        action: Action,
    ) -> Result<()> {
        let decision = self.policy.evaluate(task, &action)?;
        self.record_event(
            events,
            AuditEvent::PolicyDecision {
                task_id: task.id.clone(),
                at: OffsetDateTime::now_utc(),
                action: action.clone(),
                decision: decision.clone(),
            },
        )?;

        match decision {
            ActionDecision::Allow { .. } => Ok(()),
            ActionDecision::Deny { reason, .. } => {
                self.transition(events, &task.id, TaskStatus::Denied)?;
                Err(TaskFenceError::Policy(reason))
            }
            ActionDecision::RequireApproval { .. } => {
                self.transition(events, &task.id, TaskStatus::WaitingForApproval)?;
                let requested = self.approval.request(task, action, decision)?;
                self.record_event(
                    events,
                    AuditEvent::ApprovalRequested {
                        record: requested.clone(),
                    },
                )?;
                let resolved = self.approval.wait(&requested.id)?;
                self.record_event(
                    events,
                    AuditEvent::ApprovalResolved {
                        record: resolved.clone(),
                    },
                )?;
                match resolved.decision {
                    Some(ApprovalDecision::Approved) => Ok(()),
                    Some(ApprovalDecision::Denied) | Some(ApprovalDecision::TimedOut) | None => {
                        self.transition(events, &task.id, TaskStatus::Denied)?;
                        Err(TaskFenceError::Approval(
                            "approval denied or timed out".into(),
                        ))
                    }
                }
            }
        }
    }

    fn transition(
        &self,
        events: &mut Vec<AuditEvent>,
        task_id: &TaskId,
        status: TaskStatus,
    ) -> Result<()> {
        self.state.set_status(task_id, status.clone())?;
        self.record_event(
            events,
            AuditEvent::TaskStatusChanged {
                task_id: task_id.clone(),
                at: OffsetDateTime::now_utc(),
                status,
            },
        )
    }

    fn record_run_logs(
        &self,
        events: &mut Vec<AuditEvent>,
        task: &ResolvedTask,
        output: &RunOutput,
    ) -> Result<()> {
        if task.audit.capture.stdout && !output.stdout.is_empty() {
            let path = self
                .artifacts
                .write_log(task, LogStream::Stdout, &output.stdout)?;
            self.record_event(
                events,
                AuditEvent::Artifact {
                    task_id: task.id.clone(),
                    at: OffsetDateTime::now_utc(),
                    kind: "stdout".into(),
                    path,
                },
            )?;
            self.record_event(
                events,
                AuditEvent::Log {
                    task_id: task.id.clone(),
                    chunk: LogChunk {
                        stream: LogStream::Stdout,
                        text: output.stdout.clone(),
                        timestamp: OffsetDateTime::now_utc(),
                    },
                },
            )?;
        }

        if task.audit.capture.stderr && !output.stderr.is_empty() {
            let path = self
                .artifacts
                .write_log(task, LogStream::Stderr, &output.stderr)?;
            self.record_event(
                events,
                AuditEvent::Artifact {
                    task_id: task.id.clone(),
                    at: OffsetDateTime::now_utc(),
                    kind: "stderr".into(),
                    path,
                },
            )?;
            self.record_event(
                events,
                AuditEvent::Log {
                    task_id: task.id.clone(),
                    chunk: LogChunk {
                        stream: LogStream::Stderr,
                        text: output.stderr.clone(),
                        timestamp: OffsetDateTime::now_utc(),
                    },
                },
            )?;
        }

        Ok(())
    }

    fn generate_report(
        &self,
        events: &mut Vec<AuditEvent>,
        task: &ResolvedTask,
        artifacts: &ArtifactRefs,
    ) -> Result<()> {
        let report_path = self.report.generate(task, artifacts, events)?;
        self.record_event(
            events,
            AuditEvent::Artifact {
                task_id: task.id.clone(),
                at: OffsetDateTime::now_utc(),
                kind: "report".into(),
                path: report_path,
            },
        )
    }

    fn record_error(
        &self,
        events: &mut Vec<AuditEvent>,
        task_id: &TaskId,
        message: &str,
    ) -> Result<()> {
        self.record_event(
            events,
            AuditEvent::Error {
                task_id: task_id.clone(),
                at: OffsetDateTime::now_utc(),
                message: message.into(),
            },
        )
    }

    fn record_event(&self, events: &mut Vec<AuditEvent>, event: AuditEvent) -> Result<()> {
        self.audit.record(event.clone())?;
        events.push(event);
        Ok(())
    }
}

fn has_status(events: &[AuditEvent], expected: &TaskStatus) -> bool {
    events.iter().any(|event| {
        matches!(
            event,
            AuditEvent::TaskStatusChanged { status, .. } if status == expected
        )
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::RefCell;
    use std::collections::BTreeMap;

    #[test]
    fn command_parser_marks_shell_wrappers() {
        let action = CommandAction::parse("sh -c \"git push origin main\"");
        assert_eq!(action.executable, "sh");
        assert!(action.shell_wrapped);
    }

    #[test]
    fn report_receives_structured_events_and_nonzero_exit_still_reports() {
        let audit = MemoryAudit::default();
        let artifacts = MemoryArtifacts::default();
        let report = CapturingReport::default();
        let runner = StaticRunner::failing(7)
            .with_stdout("out")
            .with_stderr("err");
        let state = MemoryState::default();
        let adapter = StaticAdapter;
        let approval = DenyingApproval;
        let policy = AllowPolicy;
        let orchestrator = Orchestrator {
            policy: &policy,
            approval: &approval,
            audit: &audit,
            artifacts: &artifacts,
            adapter: &adapter,
            runner: &runner,
            report: &report,
            state: &state,
        };

        let result = orchestrator.run(sample_task()).unwrap();

        assert_eq!(result.status, TaskStatus::Failed);
        assert_eq!(result.exit_status.unwrap().code, Some(7));
        assert_eq!(artifacts.stdout.borrow().as_deref(), Some("out"));
        assert_eq!(artifacts.stderr.borrow().as_deref(), Some("err"));
        let report_events = report.events.borrow();
        assert!(report_events
            .iter()
            .any(|event| matches!(event, AuditEvent::RunnerExit { exit_status, .. } if exit_status.code == Some(7))));
        assert!(report_events.iter().any(|event| matches!(
            event,
            AuditEvent::TaskStatusChanged {
                status: TaskStatus::Failed,
                ..
            }
        )));
    }

    #[test]
    fn report_generation_failure_returns_failed_task_result() {
        let report = CapturingReport {
            error: Some("report disk full".into()),
            ..CapturingReport::default()
        };
        let audit = MemoryAudit::default();
        let artifacts = MemoryArtifacts::default();
        let runner = StaticRunner::succeeding();
        let state = MemoryState::default();
        let adapter = StaticAdapter;
        let approval = DenyingApproval;
        let policy = AllowPolicy;
        let orchestrator = Orchestrator {
            policy: &policy,
            approval: &approval,
            audit: &audit,
            artifacts: &artifacts,
            adapter: &adapter,
            runner: &runner,
            report: &report,
            state: &state,
        };

        let result = orchestrator.run(sample_task()).unwrap();

        assert_eq!(result.status, TaskStatus::Failed);
        assert_eq!(result.exit_status.unwrap().code, Some(0));
        assert!(result.message.unwrap().contains("report disk full"));
        assert!(audit.events.borrow().iter().any(
            |event| matches!(event, AuditEvent::Error { message, .. } if message.contains("report disk full"))
        ));
    }

    #[test]
    fn artifact_log_failure_records_partial_failure_report() {
        let artifacts = MemoryArtifacts {
            log_error: Some("stdout artifact write failed".into()),
            ..MemoryArtifacts::default()
        };
        let audit = MemoryAudit::default();
        let report = CapturingReport::default();
        let runner = StaticRunner::succeeding().with_stdout("out");
        let state = MemoryState::default();
        let adapter = StaticAdapter;
        let approval = DenyingApproval;
        let policy = AllowPolicy;
        let orchestrator = Orchestrator {
            policy: &policy,
            approval: &approval,
            audit: &audit,
            artifacts: &artifacts,
            adapter: &adapter,
            runner: &runner,
            report: &report,
            state: &state,
        };

        let result = orchestrator.run(sample_task()).unwrap();

        assert_eq!(result.status, TaskStatus::Failed);
        assert_eq!(result.exit_status.unwrap().code, Some(0));
        assert!(result
            .message
            .unwrap()
            .contains("stdout artifact write failed"));
        assert!(report.events.borrow().iter().any(
            |event| matches!(event, AuditEvent::Error { message, .. } if message.contains("stdout artifact write failed"))
        ));
    }

    #[test]
    fn policy_denial_returns_denied_result_and_reports_without_starting_runner() {
        let audit = MemoryAudit::default();
        let artifacts = MemoryArtifacts::default();
        let report = CapturingReport::default();
        let runner = StaticRunner::succeeding();
        let state = MemoryState::default();
        let adapter = StaticAdapter;
        let approval = DenyingApproval;
        let policy = DenyPolicy;
        let orchestrator = Orchestrator {
            policy: &policy,
            approval: &approval,
            audit: &audit,
            artifacts: &artifacts,
            adapter: &adapter,
            runner: &runner,
            report: &report,
            state: &state,
        };

        let result = orchestrator.run(sample_task()).unwrap();

        assert_eq!(result.status, TaskStatus::Denied);
        assert!(result.exit_status.is_none());
        assert_eq!(*runner.prepared.borrow(), 0);
        assert_eq!(*runner.started.borrow(), 0);
        let report_events = report.events.borrow();
        assert!(report_events.iter().any(|event| matches!(
            event,
            AuditEvent::PolicyDecision {
                decision: ActionDecision::Deny { reason, .. },
                ..
            } if reason == "test deny"
        )));
        assert!(report_events.iter().any(|event| matches!(
            event,
            AuditEvent::TaskStatusChanged {
                status: TaskStatus::Denied,
                ..
            }
        )));
        assert!(!report_events
            .iter()
            .any(|event| matches!(event, AuditEvent::RunnerExit { .. })));
    }

    #[test]
    fn approval_denial_returns_denied_result_and_reports_without_starting_runner() {
        let audit = MemoryAudit::default();
        let artifacts = MemoryArtifacts::default();
        let report = CapturingReport::default();
        let runner = StaticRunner::succeeding();
        let state = MemoryState::default();
        let adapter = StaticAdapter;
        let approval = DenyingApproval;
        let policy = ApprovalPolicy;
        let orchestrator = Orchestrator {
            policy: &policy,
            approval: &approval,
            audit: &audit,
            artifacts: &artifacts,
            adapter: &adapter,
            runner: &runner,
            report: &report,
            state: &state,
        };

        let result = orchestrator.run(sample_task()).unwrap();

        assert_eq!(result.status, TaskStatus::Denied);
        assert!(result.exit_status.is_none());
        assert_eq!(*runner.prepared.borrow(), 0);
        assert_eq!(*runner.started.borrow(), 0);
        let report_events = report.events.borrow();
        assert!(report_events
            .iter()
            .any(|event| matches!(event, AuditEvent::ApprovalRequested { .. })));
        assert!(report_events.iter().any(|event| matches!(
            event,
            AuditEvent::ApprovalResolved { record } if record.decision == Some(ApprovalDecision::Denied)
        )));
        assert!(report_events.iter().any(|event| matches!(
            event,
            AuditEvent::TaskStatusChanged {
                status: TaskStatus::Denied,
                ..
            }
        )));
        assert!(!report_events
            .iter()
            .any(|event| matches!(event, AuditEvent::RunnerExit { .. })));
    }

    fn sample_task() -> ResolvedTask {
        ResolvedTask {
            id: TaskId("task-1".into()),
            task_file: "/tmp/task.yaml".into(),
            goal: "test task".into(),
            workspace_host_path: "/tmp/repo".into(),
            workspace_container_path: "/workspace".into(),
            agent: AgentConfig {
                kind: AgentKind::Generic,
                command: "echo".into(),
                args: vec!["ok".into()],
            },
            sandbox: SandboxConfig {
                kind: SandboxKind::Docker,
                image: Some("debian:bookworm-slim".into()),
                limits: LimitConfig::default(),
            },
            permissions: PermissionConfig {
                commands: CommandPermissions {
                    allow: vec!["echo ok".into()],
                    approval_required: Vec::new(),
                    deny: Vec::new(),
                },
                ..PermissionConfig::default()
            },
            secrets: SecretConfig::default(),
            approval: ApprovalConfig::default(),
            gateway: Default::default(),
            audit: AuditConfig::default(),
        }
    }

    #[derive(Debug)]
    struct AllowPolicy;

    impl PolicyEngine for AllowPolicy {
        fn evaluate(&self, _task: &ResolvedTask, _action: &Action) -> Result<ActionDecision> {
            Ok(ActionDecision::Allow {
                rule_id: Some("allow-test".into()),
                reason: "test allow".into(),
            })
        }
    }

    #[derive(Debug)]
    struct DenyPolicy;

    impl PolicyEngine for DenyPolicy {
        fn evaluate(&self, _task: &ResolvedTask, _action: &Action) -> Result<ActionDecision> {
            Ok(ActionDecision::Deny {
                rule_id: Some("deny-test".into()),
                reason: "test deny".into(),
            })
        }
    }

    #[derive(Debug)]
    struct ApprovalPolicy;

    impl PolicyEngine for ApprovalPolicy {
        fn evaluate(&self, _task: &ResolvedTask, _action: &Action) -> Result<ActionDecision> {
            Ok(ActionDecision::RequireApproval {
                approval_kind: "command".into(),
                rule_id: Some("approval-test".into()),
                reason: "test approval".into(),
                risk: RiskLevel::High,
            })
        }
    }

    #[derive(Debug)]
    struct DenyingApproval;

    impl ApprovalEngine for DenyingApproval {
        fn request(
            &self,
            task: &ResolvedTask,
            action: Action,
            decision: ActionDecision,
        ) -> Result<ApprovalRecord> {
            Ok(ApprovalRecord {
                id: ApprovalId("approval-1".into()),
                task_id: task.id.clone(),
                actor: "test".into(),
                source: Some("test".into()),
                requested_at: OffsetDateTime::now_utc(),
                resolved_at: None,
                action,
                policy_decision: decision,
                decision: None,
            })
        }

        fn wait(&self, approval_id: &ApprovalId) -> Result<ApprovalRecord> {
            Ok(ApprovalRecord {
                id: approval_id.clone(),
                task_id: TaskId("task-1".into()),
                actor: "test".into(),
                source: Some("test".into()),
                requested_at: OffsetDateTime::now_utc(),
                resolved_at: Some(OffsetDateTime::now_utc()),
                action: Action::Budget {
                    kind: "test".into(),
                    amount: 1,
                },
                policy_decision: ActionDecision::Deny {
                    rule_id: None,
                    reason: "unused".into(),
                },
                decision: Some(ApprovalDecision::Denied),
            })
        }
    }

    #[derive(Debug, Default)]
    struct MemoryAudit {
        events: RefCell<Vec<AuditEvent>>,
    }

    impl AuditLogger for MemoryAudit {
        fn record(&self, event: AuditEvent) -> Result<()> {
            self.events.borrow_mut().push(event);
            Ok(())
        }
    }

    #[derive(Debug, Default)]
    struct MemoryArtifacts {
        stdout: RefCell<Option<String>>,
        stderr: RefCell<Option<String>>,
        log_error: Option<String>,
    }

    impl ArtifactStore for MemoryArtifacts {
        fn create_task_dir(&self, task: &ResolvedTask) -> Result<ArtifactRefs> {
            Ok(ArtifactRefs {
                task_dir: Utf8PathBuf::from("/tmp/taskfence-test").join(&task.id.0),
                resolved_task: Some("/tmp/taskfence-test/task.resolved.json".into()),
                events: Some("/tmp/taskfence-test/events.jsonl".into()),
                stdout: Some("/tmp/taskfence-test/stdout.log".into()),
                stderr: Some("/tmp/taskfence-test/stderr.log".into()),
                diff: Some("/tmp/taskfence-test/diff.patch".into()),
                report: Some("/tmp/taskfence-test/report.md".into()),
                gateway_spool: Some("/tmp/taskfence-test/gateway-spool".into()),
            })
        }

        fn write_resolved_task(&self, _task: &ResolvedTask) -> Result<Utf8PathBuf> {
            Ok("/tmp/taskfence-test/task.resolved.json".into())
        }

        fn write_log(
            &self,
            _task: &ResolvedTask,
            stream: LogStream,
            contents: &str,
        ) -> Result<Utf8PathBuf> {
            if let Some(message) = &self.log_error {
                return Err(TaskFenceError::Artifact(message.clone()));
            }
            match stream {
                LogStream::Stdout => self.stdout.replace(Some(contents.into())),
                LogStream::Stderr => self.stderr.replace(Some(contents.into())),
            };
            Ok(match stream {
                LogStream::Stdout => "/tmp/taskfence-test/stdout.log".into(),
                LogStream::Stderr => "/tmp/taskfence-test/stderr.log".into(),
            })
        }

        fn capture_baseline(&self, _task: &ResolvedTask) -> Result<WorkspaceBaseline> {
            Ok(WorkspaceBaseline {
                dirty_before_run: false,
                summary: "clean".into(),
            })
        }

        fn collect_diff(
            &self,
            _task: &ResolvedTask,
            _baseline: &WorkspaceBaseline,
        ) -> Result<Option<Utf8PathBuf>> {
            Ok(Some("/tmp/taskfence-test/diff.patch".into()))
        }
    }

    #[derive(Debug)]
    struct StaticAdapter;

    impl AgentAdapter for StaticAdapter {
        fn build_invocation(&self, task: &ResolvedTask) -> Result<AgentInvocation> {
            Ok(AgentInvocation {
                executable: task.agent.command.clone(),
                args: task.agent.args.clone(),
                env: BTreeMap::new(),
                working_dir: task.workspace_container_path.clone(),
            })
        }
    }

    #[derive(Debug)]
    struct StaticRunner {
        output: RunOutput,
        prepared: RefCell<usize>,
        started: RefCell<usize>,
    }

    impl StaticRunner {
        fn succeeding() -> Self {
            Self::with_code(0)
        }

        fn failing(code: i32) -> Self {
            Self::with_code(code)
        }

        fn with_code(code: i32) -> Self {
            Self {
                output: RunOutput {
                    exit_status: ExitStatus {
                        code: Some(code),
                        timed_out: false,
                        signal: None,
                    },
                    stdout: String::new(),
                    stderr: String::new(),
                },
                prepared: RefCell::new(0),
                started: RefCell::new(0),
            }
        }

        fn with_stdout(mut self, stdout: impl Into<String>) -> Self {
            self.output.stdout = stdout.into();
            self
        }

        fn with_stderr(mut self, stderr: impl Into<String>) -> Self {
            self.output.stderr = stderr.into();
            self
        }
    }

    impl Runner for StaticRunner {
        fn prepare(&self, task: &ResolvedTask) -> Result<PreparedRun> {
            *self.prepared.borrow_mut() += 1;
            Ok(PreparedRun {
                task_id: task.id.clone(),
                image: task.sandbox.image.clone(),
                mounts: Vec::new(),
                env: BTreeMap::new(),
                network: task.permissions.network.clone(),
                gateway: PreparedGateway::default(),
                limits: task.sandbox.limits.clone(),
            })
        }

        fn start(
            &self,
            prepared: PreparedRun,
            _invocation: AgentInvocation,
        ) -> Result<RunningTask> {
            *self.started.borrow_mut() += 1;
            Ok(RunningTask {
                task_id: prepared.task_id.clone(),
                runner_ref: format!("runner:{}", prepared.task_id.0),
            })
        }

        fn stop(&self, _running: &RunningTask) -> Result<()> {
            Ok(())
        }

        fn collect_exit(&self, _running: &RunningTask) -> Result<RunOutput> {
            Ok(self.output.clone())
        }
    }

    #[derive(Debug, Default)]
    struct CapturingReport {
        events: RefCell<Vec<AuditEvent>>,
        error: Option<String>,
    }

    impl ReportGenerator for CapturingReport {
        fn generate(
            &self,
            _task: &ResolvedTask,
            artifacts: &ArtifactRefs,
            events: &[AuditEvent],
        ) -> Result<Utf8PathBuf> {
            self.events.borrow_mut().extend_from_slice(events);
            if let Some(message) = &self.error {
                return Err(TaskFenceError::Report(message.clone()));
            }
            Ok(artifacts
                .report
                .clone()
                .unwrap_or_else(|| artifacts.task_dir.join("report.md")))
        }
    }

    #[derive(Debug, Default)]
    struct MemoryState {
        statuses: RefCell<BTreeMap<TaskId, TaskStatus>>,
    }

    impl StateStore for MemoryState {
        fn set_status(&self, task_id: &TaskId, status: TaskStatus) -> Result<()> {
            self.statuses.borrow_mut().insert(task_id.clone(), status);
            Ok(())
        }

        fn get_status(&self, task_id: &TaskId) -> Result<Option<TaskStatus>> {
            Ok(self.statuses.borrow().get(task_id).cloned())
        }
    }
}
