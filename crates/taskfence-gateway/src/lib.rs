use base64::Engine as _;
use camino::{Utf8Path, Utf8PathBuf};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use std::fs::{self, OpenOptions};
use std::io::Read;

use taskfence_core::{
    budget_limit_for, Action, ActionDecision, ApprovalDecision, ApprovalEngine, ApprovalRecord,
    AuditEvent, AuditLogger, BudgetUsage, BudgetUsageRecord, GatewayConnectorConfig,
    GatewaySecretReferenceConfig, GatewayToolConfig, PolicyEngine, RedactedValue, ResolvedTask,
    TaskFenceError, ToolAction, ToolAdapterIdentity, ToolExecution, ToolExecutionContext,
    ToolExecutionError, ToolExecutionErrorKind, ToolRequest, ToolResult, GATEWAY_EGRESS_TOOL_NAME,
    GATEWAY_EGRESS_TOOL_OPERATION, GATEWAY_EGRESS_TOOL_PROTOCOL, GATEWAY_SPOOL_DIR_NAME,
    GATEWAY_SPOOL_REQUESTS_DIR_NAME, GATEWAY_SPOOL_RESPONSES_DIR_NAME,
};
use time::OffsetDateTime;

const REDACTION_MARKER: &str = "[redacted]";

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GatewaySpoolPaths {
    pub root: Utf8PathBuf,
    pub requests_dir: Utf8PathBuf,
    pub responses_dir: Utf8PathBuf,
}

impl GatewaySpoolPaths {
    pub fn for_task(task: &ResolvedTask) -> taskfence_core::Result<Self> {
        validate_spool_component("task id", &task.id.0)?;
        let root = task
            .workspace_host_path
            .join(".taskfence")
            .join("tasks")
            .join(task.id.0.as_str())
            .join(GATEWAY_SPOOL_DIR_NAME);
        Self::new(root)
    }

    pub fn new(root: impl Into<Utf8PathBuf>) -> taskfence_core::Result<Self> {
        let root = root.into();
        reject_parent_component("gateway spool root", &root)?;
        Ok(Self {
            requests_dir: root.join(GATEWAY_SPOOL_REQUESTS_DIR_NAME),
            responses_dir: root.join(GATEWAY_SPOOL_RESPONSES_DIR_NAME),
            root,
        })
    }

    pub fn request_path(&self, request_id: &str) -> taskfence_core::Result<Utf8PathBuf> {
        validate_spool_component("gateway spool request id", request_id)?;
        Ok(self.requests_dir.join(format!("{request_id}.json")))
    }

    pub fn response_path(&self, request_id: &str) -> taskfence_core::Result<Utf8PathBuf> {
        validate_spool_component("gateway spool request id", request_id)?;
        Ok(self.responses_dir.join(format!("{request_id}.json")))
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GatewaySpoolRequest {
    pub request_id: String,
    pub action: ToolAction,
    #[serde(default)]
    pub timeout_seconds: Option<u64>,
    #[serde(default)]
    pub cancel: bool,
}

impl GatewaySpoolRequest {
    pub fn normalized(self) -> taskfence_core::Result<Self> {
        validate_spool_component("gateway spool request id", &self.request_id)?;
        Ok(Self {
            request_id: self.request_id,
            action: normalize_tool_action(self.action)?,
            timeout_seconds: self.timeout_seconds,
            cancel: self.cancel,
        })
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum GatewaySpoolResponseState {
    Succeeded,
    Failed,
    Denied,
    TimedOut,
    Cancelled,
    MalformedRequest,
    UnsupportedAction,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GatewaySpoolResponse {
    pub request_id: String,
    pub state: GatewaySpoolResponseState,
    pub execution: Option<ToolExecution>,
    pub error: Option<ToolExecutionError>,
}

impl GatewaySpoolResponse {
    pub fn from_execution(request_id: impl Into<String>, execution: ToolExecution) -> Self {
        let state = match execution.error.as_ref().map(|error| &error.kind) {
            None => GatewaySpoolResponseState::Succeeded,
            Some(
                ToolExecutionErrorKind::PolicyDenied
                | ToolExecutionErrorKind::ApprovalDeniedOrTimedOut
                | ToolExecutionErrorKind::BudgetExceeded,
            ) => GatewaySpoolResponseState::Denied,
            Some(
                ToolExecutionErrorKind::UnsupportedProtocol
                | ToolExecutionErrorKind::UnsupportedTool,
            ) => GatewaySpoolResponseState::UnsupportedAction,
            Some(
                ToolExecutionErrorKind::UnregisteredTool
                | ToolExecutionErrorKind::InvalidParameters
                | ToolExecutionErrorKind::AdapterFailed
                | ToolExecutionErrorKind::SecretUnavailable,
            ) => GatewaySpoolResponseState::Failed,
        };
        Self {
            request_id: request_id.into(),
            state,
            execution: Some(execution),
            error: None,
        }
    }

    pub fn error(
        request_id: impl Into<String>,
        state: GatewaySpoolResponseState,
        kind: ToolExecutionErrorKind,
        message: impl Into<String>,
    ) -> Self {
        Self {
            request_id: request_id.into(),
            state,
            execution: None,
            error: Some(ToolExecutionError {
                kind,
                message: message.into(),
            }),
        }
    }
}

pub fn read_gateway_spool_request(
    paths: &GatewaySpoolPaths,
    request_path: &Utf8Path,
) -> taskfence_core::Result<GatewaySpoolRequest> {
    let request_path =
        validate_existing_spool_file("gateway spool request", request_path, &paths.requests_dir)?;
    let request_id = request_id_from_path(&request_path)?;
    let contents = fs::read_to_string(request_path.as_std_path()).map_err(|err| {
        TaskFenceError::Gateway(format!(
            "failed to read gateway spool request {request_path}: {err}"
        ))
    })?;
    let request = serde_json::from_str::<GatewaySpoolRequest>(&contents).map_err(|err| {
        TaskFenceError::Gateway(format!(
            "malformed gateway spool request {request_path}: {err}"
        ))
    })?;
    let request = request.normalized()?;
    if request.request_id != request_id {
        return Err(TaskFenceError::Gateway(format!(
            "gateway spool request id {} does not match request file {request_id}.json",
            request.request_id
        )));
    }
    Ok(request)
}

pub fn gateway_spool_request_id_from_path(path: &Utf8Path) -> taskfence_core::Result<String> {
    request_id_from_path(path)
}

pub fn write_gateway_spool_response(
    paths: &GatewaySpoolPaths,
    response: &GatewaySpoolResponse,
) -> taskfence_core::Result<Utf8PathBuf> {
    validate_spool_component("gateway spool request id", &response.request_id)?;
    fs::create_dir_all(paths.responses_dir.as_std_path()).map_err(|err| {
        TaskFenceError::Gateway(format!(
            "failed to create gateway spool response directory {}: {err}",
            paths.responses_dir
        ))
    })?;
    let response_path = paths.response_path(&response.request_id)?;
    validate_new_spool_file(
        "gateway spool response",
        &response_path,
        &paths.responses_dir,
    )?;
    let bytes = serde_json::to_vec_pretty(response).map_err(|err| {
        TaskFenceError::Gateway(format!("failed to serialize gateway spool response: {err}"))
    })?;
    write_spool_response_file(&response_path, &bytes)?;
    Ok(response_path)
}

fn validate_spool_component(field: &str, value: &str) -> taskfence_core::Result<()> {
    if value.is_empty()
        || value == "."
        || value == ".."
        || value.contains('/')
        || value.contains('\\')
        || value.chars().any(char::is_control)
    {
        return Err(TaskFenceError::Gateway(format!(
            "{field} is not a safe gateway spool path component: {value:?}"
        )));
    }
    if !value
        .chars()
        .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '_' | '-' | '.'))
    {
        return Err(TaskFenceError::Gateway(format!(
            "{field} may only contain ASCII letters, digits, '.', '_' or '-': {value:?}"
        )));
    }
    Ok(())
}

fn reject_parent_component(field: &str, path: &Utf8Path) -> taskfence_core::Result<()> {
    if path
        .components()
        .any(|component| component.as_str() == "..")
    {
        Err(TaskFenceError::Gateway(format!(
            "{field} must not contain '..': {path}"
        )))
    } else {
        Ok(())
    }
}

fn validate_existing_spool_file(
    field: &str,
    path: &Utf8Path,
    allowed_dir: &Utf8Path,
) -> taskfence_core::Result<Utf8PathBuf> {
    reject_parent_component(field, path)?;
    reject_parent_component("gateway spool allowed directory", allowed_dir)?;
    if path.extension() != Some("json") {
        return Err(TaskFenceError::Gateway(format!(
            "{field} must be a .json file: {path}"
        )));
    }

    let metadata = fs::symlink_metadata(path.as_std_path()).map_err(|err| {
        TaskFenceError::Gateway(format!("failed to inspect {field} {path}: {err}"))
    })?;
    if metadata.file_type().is_symlink() {
        return Err(TaskFenceError::Gateway(format!(
            "{field} must not be a symlink: {path}"
        )));
    }
    if !metadata.is_file() {
        return Err(TaskFenceError::Gateway(format!(
            "{field} must be a regular file: {path}"
        )));
    }

    let canonical_path = canonical_utf8(path)?;
    let canonical_dir = canonical_utf8(allowed_dir)?;
    if !canonical_path.starts_with(&canonical_dir) {
        return Err(TaskFenceError::Gateway(format!(
            "{field} escapes gateway spool directory {canonical_dir}: {canonical_path}"
        )));
    }
    Ok(canonical_path)
}

fn validate_new_spool_file(
    field: &str,
    path: &Utf8Path,
    allowed_dir: &Utf8Path,
) -> taskfence_core::Result<()> {
    reject_parent_component(field, path)?;
    reject_parent_component("gateway spool allowed directory", allowed_dir)?;
    if path.extension() != Some("json") {
        return Err(TaskFenceError::Gateway(format!(
            "{field} must be a .json file: {path}"
        )));
    }

    let canonical_dir = canonical_utf8(allowed_dir)?;
    let parent = path.parent().ok_or_else(|| {
        TaskFenceError::Gateway(format!("{field} path has no parent directory: {path}"))
    })?;
    let canonical_parent = canonical_utf8(parent)?;
    if canonical_parent != canonical_dir {
        return Err(TaskFenceError::Gateway(format!(
            "{field} must be written directly under gateway spool directory {canonical_dir}: {path}"
        )));
    }

    match fs::symlink_metadata(path.as_std_path()) {
        Ok(metadata) if metadata.file_type().is_symlink() => Err(TaskFenceError::Gateway(format!(
            "{field} must not overwrite a symlink: {path}"
        ))),
        Ok(_) => Err(TaskFenceError::Gateway(format!(
            "{field} already exists: {path}"
        ))),
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(err) => Err(TaskFenceError::Gateway(format!(
            "failed to inspect {field} {path}: {err}"
        ))),
    }
}

fn request_id_from_path(path: &Utf8Path) -> taskfence_core::Result<String> {
    let stem = path.file_stem().ok_or_else(|| {
        TaskFenceError::Gateway(format!("gateway spool request file has no stem: {path}"))
    })?;
    validate_spool_component("gateway spool request id", stem)?;
    Ok(stem.to_owned())
}

fn canonical_utf8(path: &Utf8Path) -> taskfence_core::Result<Utf8PathBuf> {
    let canonical = fs::canonicalize(path.as_std_path()).map_err(|err| {
        TaskFenceError::Gateway(format!(
            "failed to canonicalize gateway spool path {path}: {err}"
        ))
    })?;
    Utf8PathBuf::from_path_buf(canonical).map_err(|path| {
        TaskFenceError::Gateway(format!(
            "gateway spool path is not valid UTF-8: {}",
            path.display()
        ))
    })
}

fn write_spool_response_file(path: &Utf8Path, bytes: &[u8]) -> taskfence_core::Result<()> {
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path.as_std_path())
        .map_err(|err| {
            TaskFenceError::Gateway(format!(
                "failed to create gateway spool response {path}: {err}"
            ))
        })?;
    use std::io::Write as _;
    file.write_all(bytes).map_err(|err| {
        TaskFenceError::Gateway(format!(
            "failed to write gateway spool response {path}: {err}"
        ))
    })
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SecretReference {
    pub name: String,
    pub scope: String,
    pub handle: String,
}

impl SecretReference {
    pub fn as_redacted_value(&self) -> RedactedValue {
        RedactedValue::Redacted {
            reason: format!("gateway secret reference for {}", self.name),
        }
    }
}

pub trait SecretBroker {
    fn issue_reference(
        &self,
        _task: &ResolvedTask,
        name: &str,
        scope: &str,
    ) -> taskfence_core::Result<SecretReference>;
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct McpToolRequest {
    pub server: String,
    pub tool: String,
    pub arguments: BTreeMap<String, RedactedValue>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct HttpToolRequest {
    pub connector: String,
    pub operation: String,
    pub parameters: BTreeMap<String, RedactedValue>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum GatewayTransportKind {
    McpServer,
    BoundedHttpAdapter,
    SdkWebhookEntryPoint,
    ArbitraryHttpProxy,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum GatewayTransportStatus {
    ReadyForImplementation,
    ContractOnly,
    Unsupported,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct GatewayTransportLimits {
    pub max_request_bytes: usize,
    pub max_response_bytes: usize,
    pub timeout_seconds: u16,
    pub rate_limit_per_minute: u16,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct GatewayTransportHardening {
    pub kind: GatewayTransportKind,
    pub priority: u8,
    pub status: GatewayTransportStatus,
    pub request_authentication: bool,
    pub structured_error_schema: bool,
    pub response_redaction: bool,
    pub destination_policy_required: bool,
    pub secret_broker_required: bool,
    pub limits: GatewayTransportLimits,
    pub compatible_local_surfaces: Vec<String>,
    pub unsupported_reason: Option<String>,
}

impl GatewayTransportHardening {
    pub fn production_priority_order() -> Vec<Self> {
        vec![
            Self::contract(
                GatewayTransportKind::McpServer,
                1,
                GatewayTransportStatus::ReadyForImplementation,
            ),
            Self::contract(
                GatewayTransportKind::BoundedHttpAdapter,
                2,
                GatewayTransportStatus::ContractOnly,
            ),
            Self::contract(
                GatewayTransportKind::SdkWebhookEntryPoint,
                3,
                GatewayTransportStatus::ContractOnly,
            ),
            Self::unsupported_arbitrary_http_proxy(),
        ]
    }

    pub fn unsupported_start_error(&self) -> Option<TaskFenceError> {
        (self.status == GatewayTransportStatus::Unsupported).then(|| {
            TaskFenceError::Unsupported(
                self.unsupported_reason
                    .clone()
                    .unwrap_or_else(|| "gateway transport is unsupported".into()),
            )
        })
    }

    fn contract(kind: GatewayTransportKind, priority: u8, status: GatewayTransportStatus) -> Self {
        Self {
            kind,
            priority,
            status,
            request_authentication: true,
            structured_error_schema: true,
            response_redaction: true,
            destination_policy_required: true,
            secret_broker_required: true,
            limits: GatewayTransportLimits {
                max_request_bytes: 64 * 1024,
                max_response_bytes: 1024 * 1024,
                timeout_seconds: 30,
                rate_limit_per_minute: 120,
            },
            compatible_local_surfaces: vec![
                "taskfence gateway call".into(),
                "taskfence gateway listen".into(),
                "taskfence gateway spool process".into(),
            ],
            unsupported_reason: None,
        }
    }

    fn unsupported_arbitrary_http_proxy() -> Self {
        let mut contract = Self::contract(
            GatewayTransportKind::ArbitraryHttpProxy,
            4,
            GatewayTransportStatus::Unsupported,
        );
        contract.unsupported_reason = Some(
            "arbitrary HTTP proxying is unsupported until request inspection, destination policy, streaming limits, response redaction, secret handling, audit, and bypass controls are implemented"
                .into(),
        );
        contract
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct UnsupportedToolExecution {
    pub action: ToolAction,
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct ToolKey {
    pub protocol: String,
    pub tool: String,
    pub operation: String,
}

impl ToolKey {
    pub fn new(
        protocol: impl Into<String>,
        tool: impl Into<String>,
        operation: impl Into<String>,
    ) -> taskfence_core::Result<Self> {
        Ok(Self {
            protocol: normalize_required_segment("protocol", protocol.into())?,
            tool: normalize_required_segment("tool", tool.into())?,
            operation: normalize_required_segment("operation", operation.into())?,
        })
    }

    pub fn from_action(action: &ToolAction) -> taskfence_core::Result<Self> {
        Self::new(
            action.protocol.clone(),
            action.tool.clone(),
            action.operation.clone(),
        )
    }

    pub fn display_name(&self) -> String {
        format!("{} {}.{}", self.protocol, self.tool, self.operation)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RegisteredTool {
    pub key: ToolKey,
}

impl RegisteredTool {
    pub fn new(
        protocol: impl Into<String>,
        tool: impl Into<String>,
        operation: impl Into<String>,
    ) -> taskfence_core::Result<Self> {
        Ok(Self {
            key: ToolKey::new(protocol, tool, operation)?,
        })
    }
}

pub trait ToolRegistry {
    fn contains(&self, action: &ToolAction) -> taskfence_core::Result<bool>;
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct InMemoryToolRegistry {
    tools: BTreeSet<ToolKey>,
}

impl InMemoryToolRegistry {
    pub fn new(tools: impl IntoIterator<Item = RegisteredTool>) -> Self {
        Self {
            tools: tools.into_iter().map(|tool| tool.key).collect(),
        }
    }

    pub fn is_empty(&self) -> bool {
        self.tools.is_empty()
    }
}

impl ToolRegistry for InMemoryToolRegistry {
    fn contains(&self, action: &ToolAction) -> taskfence_core::Result<bool> {
        let key = ToolKey::from_action(action)?;
        Ok(self.tools.contains(&key))
    }
}

#[derive(Clone, Debug, Default)]
pub struct McpGatewayAdapter;

impl McpGatewayAdapter {
    pub fn to_tool_action(&self, request: McpToolRequest) -> taskfence_core::Result<ToolAction> {
        normalize_tool_action(ToolAction {
            protocol: "mcp".into(),
            tool: request.server,
            operation: request.tool,
            parameters: request.arguments,
        })
    }

    pub fn execute(
        &self,
        task: &ResolvedTask,
        request: McpToolRequest,
        executor: &GatewayExecutor<'_>,
        context: ToolExecutionContext,
    ) -> taskfence_core::Result<ToolExecution> {
        let action = self.to_tool_action(request)?;
        executor.execute_tool_action(task, action, context)
    }
}

#[derive(Clone, Debug, Default)]
pub struct HttpGatewayAdapter;

impl HttpGatewayAdapter {
    pub fn to_tool_action(&self, request: HttpToolRequest) -> taskfence_core::Result<ToolAction> {
        normalize_tool_action(ToolAction {
            protocol: "http".into(),
            tool: request.connector,
            operation: request.operation,
            parameters: request.parameters,
        })
    }

    pub fn execute(
        &self,
        task: &ResolvedTask,
        request: HttpToolRequest,
        executor: &GatewayExecutor<'_>,
        context: ToolExecutionContext,
    ) -> taskfence_core::Result<ToolExecution> {
        let action = self.to_tool_action(request)?;
        executor.execute_tool_action(task, action, context)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GatewayMediation {
    pub action: ToolAction,
    pub decision: ActionDecision,
    pub approval: Option<ApprovalRecord>,
}

pub struct GatewayMediator<'a> {
    policy: &'a dyn PolicyEngine,
    audit: &'a dyn AuditLogger,
    approval: Option<&'a dyn ApprovalEngine>,
    registry: Option<&'a dyn ToolRegistry>,
    supported_protocols: BTreeSet<String>,
}

pub trait ToolAdapter {
    fn identity(&self) -> ToolAdapterIdentity;

    fn secret_references(&self) -> &[GatewaySecretReferenceConfig] {
        &[]
    }

    fn execute(
        &self,
        task: &ResolvedTask,
        action: &ToolAction,
        context: &ToolExecutionContext,
    ) -> std::result::Result<ToolResult, ToolExecutionError>;

    fn planned_budget_usage(
        &self,
        _task: &ResolvedTask,
        _action: &ToolAction,
        _context: &ToolExecutionContext,
    ) -> std::result::Result<Vec<BudgetUsage>, ToolExecutionError> {
        Ok(Vec::new())
    }

    fn execute_with_secrets(
        &self,
        task: &ResolvedTask,
        action: &ToolAction,
        context: &ToolExecutionContext,
        _secrets: &[GatewaySecretBinding],
    ) -> std::result::Result<ToolResult, ToolExecutionError> {
        self.execute(task, action, context)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GatewaySecretBinding {
    pub parameter: String,
    pub reference: SecretReference,
}

pub struct GatewayExecutor<'a> {
    mediator: GatewayMediator<'a>,
    audit: &'a dyn AuditLogger,
    adapter: &'a dyn ToolAdapter,
    secret_broker: Option<&'a dyn SecretBroker>,
}

impl<'a> GatewayExecutor<'a> {
    pub fn new(
        mediator: GatewayMediator<'a>,
        audit: &'a dyn AuditLogger,
        adapter: &'a dyn ToolAdapter,
    ) -> Self {
        Self {
            mediator,
            audit,
            adapter,
            secret_broker: None,
        }
    }

    pub fn with_secret_broker(mut self, broker: &'a dyn SecretBroker) -> Self {
        self.secret_broker = Some(broker);
        self
    }

    pub fn execute_tool_action(
        &self,
        task: &ResolvedTask,
        action: ToolAction,
        context: ToolExecutionContext,
    ) -> taskfence_core::Result<ToolExecution> {
        let raw_request = self.request(action);
        let mediation = match self
            .mediator
            .mediate_tool_action(task, raw_request.action.clone())
        {
            Ok(mediation) => mediation,
            Err(err) => {
                return self.record_finished(
                    task,
                    ToolExecution {
                        request: raw_request,
                        result: None,
                        error: Some(tool_execution_error_from_taskfence(&err)),
                    },
                );
            }
        };

        let request = self.request(mediation.action.clone());
        match &mediation.decision {
            ActionDecision::Deny { reason, .. } => {
                return self.record_finished(
                    task,
                    ToolExecution {
                        request,
                        result: None,
                        error: Some(ToolExecutionError {
                            kind: ToolExecutionErrorKind::PolicyDenied,
                            message: reason.clone(),
                        }),
                    },
                );
            }
            ActionDecision::RequireApproval { .. } if mediation.approval.is_none() => {
                return self.record_finished(
                    task,
                    ToolExecution {
                        request,
                        result: None,
                        error: Some(ToolExecutionError {
                            kind: ToolExecutionErrorKind::ApprovalDeniedOrTimedOut,
                            message: "gateway approval engine is not configured".into(),
                        }),
                    },
                );
            }
            ActionDecision::Allow { .. } | ActionDecision::RequireApproval { .. } => {}
        }

        match self
            .adapter
            .planned_budget_usage(task, &request.action, &context)
        {
            Ok(usages) => {
                if let Some(error) = self.record_budget_usages(task, usages)? {
                    return self.record_finished(
                        task,
                        ToolExecution {
                            request,
                            result: None,
                            error: Some(error),
                        },
                    );
                }
            }
            Err(error) => {
                return self.record_finished(
                    task,
                    ToolExecution {
                        request,
                        result: None,
                        error: Some(error),
                    },
                );
            }
        }

        if let Some(error) = self.enforce_gateway_egress_policy(task, &request.action)? {
            return self.record_finished(
                task,
                ToolExecution {
                    request,
                    result: None,
                    error: Some(error),
                },
            );
        }

        let (request, secret_bindings) =
            match self.attach_configured_secret_references(task, request.clone()) {
                Ok(bound) => bound,
                Err(error) => {
                    return self.record_finished(
                        task,
                        ToolExecution {
                            request,
                            result: None,
                            error: Some(error),
                        },
                    );
                }
            };

        self.audit.record(AuditEvent::ToolExecutionStarted {
            task_id: task.id.clone(),
            at: OffsetDateTime::now_utc(),
            request: request.clone(),
        })?;

        let execution = match self.adapter.execute_with_secrets(
            task,
            &request.action,
            &context,
            &secret_bindings,
        ) {
            Ok(result) => {
                let budget_error = self.record_budget_usages(task, result.usage.clone())?;
                ToolExecution {
                    request,
                    result: Some(result),
                    error: budget_error,
                }
            }
            Err(error) => ToolExecution {
                request,
                result: None,
                error: Some(error),
            },
        };
        self.record_finished(task, execution)
    }

    fn enforce_gateway_egress_policy(
        &self,
        task: &ResolvedTask,
        action: &ToolAction,
    ) -> taskfence_core::Result<Option<ToolExecutionError>> {
        if !is_gateway_egress_action(action) {
            return Ok(None);
        }

        let destination = match gateway_egress_destination(action) {
            Ok(destination) => destination,
            Err(error) => return Ok(Some(error)),
        };
        let wrapped = Action::Network {
            host: destination.host.clone(),
            port: destination.port,
        };
        let decision = self.mediator.policy.evaluate(task, &wrapped)?;
        self.audit.record(AuditEvent::PolicyDecision {
            task_id: task.id.clone(),
            at: OffsetDateTime::now_utc(),
            action: wrapped.clone(),
            decision: decision.clone(),
        })?;

        match &decision {
            ActionDecision::Allow { .. } => Ok(None),
            ActionDecision::Deny { reason, .. } => Ok(Some(ToolExecutionError {
                kind: ToolExecutionErrorKind::PolicyDenied,
                message: reason.clone(),
            })),
            ActionDecision::RequireApproval { .. } => {
                if self.mediator.approval.is_none() {
                    return Ok(Some(ToolExecutionError {
                        kind: ToolExecutionErrorKind::ApprovalDeniedOrTimedOut,
                        message: "gateway egress approval engine is not configured".into(),
                    }));
                }
                match self
                    .mediator
                    .request_tool_approval(task, wrapped, decision.clone())
                {
                    Ok(_) => Ok(None),
                    Err(err) => Ok(Some(tool_execution_error_from_taskfence(&err))),
                }
            }
        }
    }

    fn request(&self, action: ToolAction) -> ToolRequest {
        ToolRequest {
            action,
            adapter: Some(self.adapter.identity()),
        }
    }

    fn attach_configured_secret_references(
        &self,
        task: &ResolvedTask,
        request: ToolRequest,
    ) -> std::result::Result<(ToolRequest, Vec<GatewaySecretBinding>), ToolExecutionError> {
        if self.adapter.secret_references().is_empty() {
            return Ok((request, Vec::new()));
        }

        let broker = self.secret_broker.ok_or_else(|| ToolExecutionError {
            kind: ToolExecutionErrorKind::SecretUnavailable,
            message: "gateway secret reference broker is not configured".into(),
        })?;

        let mut action = request.action;
        let mut bindings = Vec::new();
        for secret_ref in self.adapter.secret_references() {
            let reference =
                gateway_secret_reference(task, broker, &secret_ref.name, &secret_ref.scope)
                    .map_err(|err| tool_execution_error_from_taskfence(&err))?;
            action = attach_secret_reference(action, &secret_ref.parameter, &reference)
                .map_err(|err| tool_execution_error_from_taskfence(&err))?;
            bindings.push(GatewaySecretBinding {
                parameter: secret_ref.parameter.clone(),
                reference,
            });
        }

        Ok((
            ToolRequest {
                action,
                adapter: request.adapter,
            },
            bindings,
        ))
    }

    fn record_budget_usages(
        &self,
        task: &ResolvedTask,
        usages: Vec<BudgetUsage>,
    ) -> taskfence_core::Result<Option<ToolExecutionError>> {
        let mut denial = None;
        for usage in usages {
            let usage = match usage.normalized() {
                Ok(usage) => usage,
                Err(err) => {
                    return Ok(Some(ToolExecutionError {
                        kind: ToolExecutionErrorKind::BudgetExceeded,
                        message: err.to_string(),
                    }));
                }
            };
            let action = Action::Budget {
                kind: usage.kind.clone(),
                amount: usage.amount,
            };
            let decision = self.mediator.policy.evaluate(task, &action)?;
            self.audit.record(AuditEvent::PolicyDecision {
                task_id: task.id.clone(),
                at: OffsetDateTime::now_utc(),
                action,
                decision: decision.clone(),
            })?;
            let record = BudgetUsageRecord {
                limit: budget_limit_for(task, &usage.kind),
                usage,
                decision: decision.clone(),
            };
            self.audit.record(AuditEvent::BudgetUsageRecorded {
                task_id: task.id.clone(),
                at: OffsetDateTime::now_utc(),
                record,
            })?;

            if let ActionDecision::Deny { reason, .. } = decision {
                denial.get_or_insert(ToolExecutionError {
                    kind: ToolExecutionErrorKind::BudgetExceeded,
                    message: reason,
                });
            }
        }
        Ok(denial)
    }

    fn record_finished(
        &self,
        task: &ResolvedTask,
        execution: ToolExecution,
    ) -> taskfence_core::Result<ToolExecution> {
        self.audit.record(AuditEvent::ToolExecutionFinished {
            task_id: task.id.clone(),
            at: OffsetDateTime::now_utc(),
            execution: execution.clone(),
        })?;
        Ok(execution)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GatewayEgressRequest {
    pub method: String,
    pub url: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GatewayEgressResponse {
    pub status: u16,
    pub body: String,
}

pub trait GatewayEgressClient {
    fn fetch(
        &self,
        request: &GatewayEgressRequest,
    ) -> std::result::Result<GatewayEgressResponse, ToolExecutionError>;
}

#[derive(Clone, Debug, Default)]
pub struct UreqGatewayEgressClient;

impl GatewayEgressClient for UreqGatewayEgressClient {
    fn fetch(
        &self,
        request: &GatewayEgressRequest,
    ) -> std::result::Result<GatewayEgressResponse, ToolExecutionError> {
        let response = match request.method.as_str() {
            "GET" => ureq::get(&request.url)
                .call()
                .map_err(gateway_egress_ureq_error)?,
            "HEAD" => ureq::head(&request.url)
                .call()
                .map_err(gateway_egress_ureq_error)?,
            _ => {
                return Err(ToolExecutionError {
                    kind: ToolExecutionErrorKind::InvalidParameters,
                    message: "gateway egress supports only GET and HEAD".into(),
                });
            }
        };
        let status = response.status();
        let body = if request.method == "HEAD" {
            String::new()
        } else {
            let mut body = String::new();
            response
                .into_reader()
                .take(32 * 1024)
                .read_to_string(&mut body)
                .map_err(|err| ToolExecutionError {
                    kind: ToolExecutionErrorKind::AdapterFailed,
                    message: format!("failed to read gateway egress response: {err}"),
                })?;
            body
        };
        Ok(GatewayEgressResponse { status, body })
    }
}

#[derive(Clone, Debug)]
pub struct GatewayEgressAdapter<C> {
    client: C,
}

impl<C> GatewayEgressAdapter<C> {
    pub fn new(client: C) -> Self {
        Self { client }
    }
}

impl<C> ToolAdapter for GatewayEgressAdapter<C>
where
    C: GatewayEgressClient,
{
    fn identity(&self) -> ToolAdapterIdentity {
        ToolAdapterIdentity {
            kind: "gateway_egress".into(),
            name: "local".into(),
        }
    }

    fn planned_budget_usage(
        &self,
        _task: &ResolvedTask,
        action: &ToolAction,
        _context: &ToolExecutionContext,
    ) -> std::result::Result<Vec<BudgetUsage>, ToolExecutionError> {
        if !is_gateway_egress_action(action) {
            return Err(ToolExecutionError {
                kind: ToolExecutionErrorKind::UnsupportedTool,
                message: format!(
                    "gateway egress adapter cannot execute {}.{}",
                    action.tool, action.operation
                ),
            });
        }
        Ok(vec![BudgetUsage {
            kind: "gateway_calls".into(),
            amount: 1,
            provider: Some("gateway".into()),
            model: None,
            operation: Some("egress.fetch".into()),
            metadata: BTreeMap::new(),
        }])
    }

    fn execute(
        &self,
        _task: &ResolvedTask,
        action: &ToolAction,
        _context: &ToolExecutionContext,
    ) -> std::result::Result<ToolResult, ToolExecutionError> {
        if !is_gateway_egress_action(action) {
            return Err(ToolExecutionError {
                kind: ToolExecutionErrorKind::UnsupportedTool,
                message: format!(
                    "gateway egress adapter cannot execute {}.{}",
                    action.tool, action.operation
                ),
            });
        }
        let destination = gateway_egress_destination(action)?;
        let response = self.client.fetch(&GatewayEgressRequest {
            method: destination.method.clone(),
            url: destination.url.clone(),
        })?;
        let body = redact_secret_like_text(&response.body);
        Ok(ToolResult {
            summary: format!(
                "fetched {} with HTTP status {} through gateway egress",
                destination.host, response.status
            ),
            values: BTreeMap::from([
                ("method".into(), RedactedValue::Plain(destination.method)),
                ("url".into(), RedactedValue::Plain(destination.url)),
                ("host".into(), RedactedValue::Plain(destination.host)),
                (
                    "status".into(),
                    RedactedValue::Plain(response.status.to_string()),
                ),
                ("body".into(), RedactedValue::Plain(body)),
            ]),
            artifacts: Vec::new(),
            usage: Vec::new(),
        })
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct GatewayEgressDestination {
    method: String,
    url: String,
    host: String,
    port: Option<u16>,
}

fn is_gateway_egress_action(action: &ToolAction) -> bool {
    action.protocol == GATEWAY_EGRESS_TOOL_PROTOCOL
        && action.tool == GATEWAY_EGRESS_TOOL_NAME
        && action.operation == GATEWAY_EGRESS_TOOL_OPERATION
}

fn gateway_egress_destination(
    action: &ToolAction,
) -> std::result::Result<GatewayEgressDestination, ToolExecutionError> {
    let method = plain_optional_parameter(action, "method")
        .unwrap_or_else(|| "GET".into())
        .trim()
        .to_ascii_uppercase();
    if !matches!(method.as_str(), "GET" | "HEAD") {
        return Err(ToolExecutionError {
            kind: ToolExecutionErrorKind::InvalidParameters,
            message: "gateway egress supports only GET and HEAD".into(),
        });
    }
    let url = plain_required_parameter(action, "url")?.to_owned();
    parse_gateway_egress_url(method, url)
}

fn parse_gateway_egress_url(
    method: String,
    url: String,
) -> std::result::Result<GatewayEgressDestination, ToolExecutionError> {
    let trimmed = url.trim();
    let lower = trimmed.to_ascii_lowercase();
    if !lower.starts_with("https://") {
        return Err(ToolExecutionError {
            kind: ToolExecutionErrorKind::InvalidParameters,
            message: "gateway egress URL must use https".into(),
        });
    }
    if trimmed.chars().any(char::is_whitespace)
        || trimmed.contains('@')
        || trimmed.contains('#')
        || secret_like_url(trimmed)
    {
        return Err(ToolExecutionError {
            kind: ToolExecutionErrorKind::InvalidParameters,
            message: "gateway egress URL must not contain userinfo, fragments, whitespace, or secret-like query material".into(),
        });
    }

    let without_scheme = &trimmed["https://".len()..];
    let authority_end = without_scheme
        .find(['/', '?'])
        .unwrap_or(without_scheme.len());
    let authority = &without_scheme[..authority_end];
    if authority.is_empty() || authority.starts_with('[') || authority.matches(':').count() > 1 {
        return Err(ToolExecutionError {
            kind: ToolExecutionErrorKind::InvalidParameters,
            message: "gateway egress URL host is invalid".into(),
        });
    }

    let (host, port) = match authority.split_once(':') {
        Some((host, port)) => {
            let port = port.parse::<u16>().map_err(|err| ToolExecutionError {
                kind: ToolExecutionErrorKind::InvalidParameters,
                message: format!("gateway egress URL port is invalid: {err}"),
            })?;
            (host, Some(port))
        }
        None => (authority, None),
    };
    let host = normalize_egress_host(host)?;
    if has_parent_url_path(without_scheme, authority_end) {
        return Err(ToolExecutionError {
            kind: ToolExecutionErrorKind::InvalidParameters,
            message: "gateway egress URL path must not contain '..'".into(),
        });
    }

    Ok(GatewayEgressDestination {
        method,
        url: trimmed.into(),
        host,
        port,
    })
}

fn normalize_egress_host(host: &str) -> std::result::Result<String, ToolExecutionError> {
    let host = host.trim().trim_end_matches('.').to_ascii_lowercase();
    if host.is_empty()
        || host.contains('/')
        || host.contains(':')
        || host.contains('*')
        || host.split('.').any(|label| label.is_empty())
        || !host
            .chars()
            .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '.' | '-'))
    {
        return Err(ToolExecutionError {
            kind: ToolExecutionErrorKind::InvalidParameters,
            message: "gateway egress URL host is invalid".into(),
        });
    }
    Ok(host)
}

fn has_parent_url_path(without_scheme: &str, authority_end: usize) -> bool {
    let path_start = without_scheme[authority_end..]
        .find('/')
        .map(|offset| authority_end + offset);
    let Some(path_start) = path_start else {
        return false;
    };
    let path_end = without_scheme[path_start..]
        .find('?')
        .map(|offset| path_start + offset)
        .unwrap_or(without_scheme.len());
    without_scheme[path_start..path_end]
        .split('/')
        .any(|segment| segment == ".." || segment.eq_ignore_ascii_case("%2e%2e"))
}

fn secret_like_url(url: &str) -> bool {
    let lower = url.to_ascii_lowercase();
    [
        "token=",
        "password=",
        "secret=",
        "api_key=",
        "authorization=",
        "bearer%20",
        "bearer+",
    ]
    .iter()
    .any(|marker| lower.contains(marker))
}

fn gateway_egress_ureq_error(err: ureq::Error) -> ToolExecutionError {
    match err {
        ureq::Error::Status(status, response) => {
            let message = response
                .into_string()
                .unwrap_or_else(|_| "unable to read egress error response".into());
            ToolExecutionError {
                kind: ToolExecutionErrorKind::AdapterFailed,
                message: format!(
                    "gateway egress returned HTTP {status}: {}",
                    redact_secret_like_text(&message)
                ),
            }
        }
        ureq::Error::Transport(transport) => ToolExecutionError {
            kind: ToolExecutionErrorKind::AdapterFailed,
            message: format!("gateway egress transport failed: {transport}"),
        },
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct UnsupportedGatewayAdapter {
    identity: ToolAdapterIdentity,
    secret_refs: Vec<GatewaySecretReferenceConfig>,
    contract_only: bool,
    template_supports_operation: bool,
}

impl UnsupportedGatewayAdapter {
    pub fn new(kind: impl Into<String>, name: impl Into<String>) -> Self {
        Self {
            identity: ToolAdapterIdentity {
                kind: kind.into(),
                name: name.into(),
            },
            secret_refs: Vec::new(),
            contract_only: false,
            template_supports_operation: false,
        }
    }

    pub fn for_contract_tool(tool: GatewayToolConfig) -> Self {
        let template_supports_operation =
            connector_supports_operation(&tool.connector, &tool.tool, &tool.operation);
        Self {
            identity: connector_identity(&tool.connector),
            secret_refs: tool.secret_refs,
            contract_only: true,
            template_supports_operation,
        }
    }
}

impl ToolAdapter for UnsupportedGatewayAdapter {
    fn identity(&self) -> ToolAdapterIdentity {
        self.identity.clone()
    }

    fn secret_references(&self) -> &[GatewaySecretReferenceConfig] {
        &self.secret_refs
    }

    fn execute(
        &self,
        _task: &ResolvedTask,
        action: &ToolAction,
        _context: &ToolExecutionContext,
    ) -> std::result::Result<ToolResult, ToolExecutionError> {
        if self.contract_only {
            if !self.template_supports_operation {
                return Err(ToolExecutionError {
                    kind: ToolExecutionErrorKind::UnsupportedTool,
                    message: format!(
                        "{} connector template does not support {}.{}",
                        self.identity.kind, action.tool, action.operation
                    ),
                });
            }
            return Err(ToolExecutionError {
                kind: ToolExecutionErrorKind::UnsupportedTool,
                message: format!(
                    "{} connector is contract-only; no live adapter is implemented for {}.{}",
                    self.identity.kind, action.tool, action.operation
                ),
            });
        }
        Err(ToolExecutionError {
            kind: ToolExecutionErrorKind::UnsupportedTool,
            message: format!(
                "{} adapter does not support {}.{}",
                self.identity.name, action.tool, action.operation
            ),
        })
    }

    fn planned_budget_usage(
        &self,
        _task: &ResolvedTask,
        action: &ToolAction,
        _context: &ToolExecutionContext,
    ) -> std::result::Result<Vec<BudgetUsage>, ToolExecutionError> {
        if self.contract_only && !self.template_supports_operation {
            return Err(ToolExecutionError {
                kind: ToolExecutionErrorKind::UnsupportedTool,
                message: format!(
                    "{} connector template does not support {}.{}",
                    self.identity.kind, action.tool, action.operation
                ),
            });
        }
        Ok(Vec::new())
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GitHubIssue {
    pub number: u64,
    pub title: String,
    pub state: String,
    pub body: String,
    pub html_url: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GitHubPullRequestInput {
    pub title: String,
    pub head: String,
    pub base: String,
    pub body: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GitHubPullRequest {
    pub number: u64,
    pub title: String,
    pub state: String,
    pub html_url: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GitHubIssueComment {
    pub id: u64,
    pub body: String,
    pub html_url: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GitHubBranch {
    pub name: String,
    pub sha: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GitHubFileCommitInput {
    pub path: String,
    pub message: String,
    pub content: String,
    pub branch: String,
    pub sha: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GitHubFileCommit {
    pub path: String,
    pub sha: String,
    pub html_url: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GitHubPullRequestUpdateInput {
    pub number: u64,
    pub title: Option<String>,
    pub body: Option<String>,
    pub state: Option<String>,
    pub base: Option<String>,
}

pub trait GitHubApiClient {
    fn read_issue(
        &self,
        api_base: &str,
        repository: &str,
        token: &str,
        number: u64,
    ) -> std::result::Result<GitHubIssue, ToolExecutionError>;

    fn create_pull_request(
        &self,
        api_base: &str,
        repository: &str,
        token: &str,
        input: GitHubPullRequestInput,
    ) -> std::result::Result<GitHubPullRequest, ToolExecutionError>;

    fn create_issue_comment(
        &self,
        api_base: &str,
        repository: &str,
        token: &str,
        number: u64,
        body: String,
    ) -> std::result::Result<GitHubIssueComment, ToolExecutionError>;

    fn create_branch(
        &self,
        api_base: &str,
        repository: &str,
        token: &str,
        branch: &str,
        from_ref: &str,
    ) -> std::result::Result<GitHubBranch, ToolExecutionError>;

    fn commit_file(
        &self,
        api_base: &str,
        repository: &str,
        token: &str,
        input: GitHubFileCommitInput,
    ) -> std::result::Result<GitHubFileCommit, ToolExecutionError>;

    fn update_pull_request(
        &self,
        api_base: &str,
        repository: &str,
        token: &str,
        input: GitHubPullRequestUpdateInput,
    ) -> std::result::Result<GitHubPullRequest, ToolExecutionError>;
}

#[derive(Clone, Debug, Default)]
pub struct UreqGitHubClient;

impl GitHubApiClient for UreqGitHubClient {
    fn read_issue(
        &self,
        api_base: &str,
        repository: &str,
        token: &str,
        number: u64,
    ) -> std::result::Result<GitHubIssue, ToolExecutionError> {
        let url = github_api_url(api_base, &format!("repos/{repository}/issues/{number}"));
        let value = github_get_json(&url, token)?;
        Ok(GitHubIssue {
            number: json_u64_field(&value, "number").unwrap_or(number),
            title: json_string_field(&value, "title"),
            state: json_string_field(&value, "state"),
            body: json_string_field(&value, "body"),
            html_url: json_string_field(&value, "html_url"),
        })
    }

    fn create_pull_request(
        &self,
        api_base: &str,
        repository: &str,
        token: &str,
        input: GitHubPullRequestInput,
    ) -> std::result::Result<GitHubPullRequest, ToolExecutionError> {
        let url = github_api_url(api_base, &format!("repos/{repository}/pulls"));
        let value = github_post_json(
            &url,
            token,
            serde_json::json!({
                "title": input.title,
                "head": input.head,
                "base": input.base,
                "body": input.body,
            }),
        )?;
        Ok(GitHubPullRequest {
            number: json_u64_field(&value, "number").unwrap_or_default(),
            title: json_string_field(&value, "title"),
            state: json_string_field(&value, "state"),
            html_url: json_string_field(&value, "html_url"),
        })
    }

    fn create_issue_comment(
        &self,
        api_base: &str,
        repository: &str,
        token: &str,
        number: u64,
        body: String,
    ) -> std::result::Result<GitHubIssueComment, ToolExecutionError> {
        let url = github_api_url(
            api_base,
            &format!("repos/{repository}/issues/{number}/comments"),
        );
        let value = github_post_json(&url, token, serde_json::json!({ "body": body }))?;
        Ok(GitHubIssueComment {
            id: json_u64_field(&value, "id").unwrap_or_default(),
            body: json_string_field(&value, "body"),
            html_url: json_string_field(&value, "html_url"),
        })
    }

    fn create_branch(
        &self,
        api_base: &str,
        repository: &str,
        token: &str,
        branch: &str,
        from_ref: &str,
    ) -> std::result::Result<GitHubBranch, ToolExecutionError> {
        let from = github_get_json(
            &github_api_url(api_base, &format!("repos/{repository}/git/ref/{from_ref}")),
            token,
        )?;
        let sha = from
            .get("object")
            .and_then(|object| object.get("sha"))
            .and_then(serde_json::Value::as_str)
            .ok_or_else(|| ToolExecutionError {
                kind: ToolExecutionErrorKind::AdapterFailed,
                message: format!("GitHub ref {from_ref} did not include an object sha"),
            })?;
        let value = github_post_json(
            &github_api_url(api_base, &format!("repos/{repository}/git/refs")),
            token,
            serde_json::json!({
                "ref": format!("refs/heads/{branch}"),
                "sha": sha,
            }),
        )?;
        let created_ref = json_string_field(&value, "ref");
        let created_sha = value
            .get("object")
            .and_then(|object| object.get("sha"))
            .and_then(serde_json::Value::as_str)
            .unwrap_or(sha)
            .to_owned();
        Ok(GitHubBranch {
            name: created_ref
                .strip_prefix("refs/heads/")
                .unwrap_or(branch)
                .to_owned(),
            sha: created_sha,
        })
    }

    fn commit_file(
        &self,
        api_base: &str,
        repository: &str,
        token: &str,
        input: GitHubFileCommitInput,
    ) -> std::result::Result<GitHubFileCommit, ToolExecutionError> {
        let mut body = serde_json::json!({
            "message": input.message,
            "content": base64::engine::general_purpose::STANDARD.encode(input.content.as_bytes()),
            "branch": input.branch,
        });
        if let Some(sha) = input.sha {
            body["sha"] = serde_json::Value::String(sha);
        }
        let value = github_put_json(
            &github_api_url(
                api_base,
                &format!("repos/{repository}/contents/{}", input.path),
            ),
            token,
            body,
        )?;
        let content = value.get("content").unwrap_or(&serde_json::Value::Null);
        Ok(GitHubFileCommit {
            path: json_string_field(content, "path"),
            sha: json_string_field(content, "sha"),
            html_url: json_string_field(content, "html_url"),
        })
    }

    fn update_pull_request(
        &self,
        api_base: &str,
        repository: &str,
        token: &str,
        input: GitHubPullRequestUpdateInput,
    ) -> std::result::Result<GitHubPullRequest, ToolExecutionError> {
        let mut body = serde_json::Map::new();
        if let Some(title) = input.title {
            body.insert("title".into(), serde_json::Value::String(title));
        }
        if let Some(body_text) = input.body {
            body.insert("body".into(), serde_json::Value::String(body_text));
        }
        if let Some(state) = input.state {
            body.insert("state".into(), serde_json::Value::String(state));
        }
        if let Some(base) = input.base {
            body.insert("base".into(), serde_json::Value::String(base));
        }
        let value = github_patch_json(
            &github_api_url(
                api_base,
                &format!("repos/{repository}/pulls/{}", input.number),
            ),
            token,
            serde_json::Value::Object(body),
        )?;
        Ok(GitHubPullRequest {
            number: json_u64_field(&value, "number").unwrap_or(input.number),
            title: json_string_field(&value, "title"),
            state: json_string_field(&value, "state"),
            html_url: json_string_field(&value, "html_url"),
        })
    }
}

#[derive(Clone, Debug)]
pub struct GitHubRestAdapter<C> {
    tool: GatewayToolConfig,
    client: C,
}

impl<C> GitHubRestAdapter<C> {
    pub fn new(tool: GatewayToolConfig, client: C) -> Self {
        Self { tool, client }
    }
}

impl<C> ToolAdapter for GitHubRestAdapter<C>
where
    C: GitHubApiClient,
{
    fn identity(&self) -> ToolAdapterIdentity {
        ToolAdapterIdentity {
            kind: connector_kind(&self.tool.connector),
            name: connector_name(&self.tool.connector),
        }
    }

    fn secret_references(&self) -> &[GatewaySecretReferenceConfig] {
        &self.tool.secret_refs
    }

    fn execute(
        &self,
        task: &ResolvedTask,
        action: &ToolAction,
        context: &ToolExecutionContext,
    ) -> std::result::Result<ToolResult, ToolExecutionError> {
        self.execute_with_secrets(task, action, context, &[])
    }

    fn planned_budget_usage(
        &self,
        _task: &ResolvedTask,
        action: &ToolAction,
        _context: &ToolExecutionContext,
    ) -> std::result::Result<Vec<BudgetUsage>, ToolExecutionError> {
        let connector = connector_kind(&self.tool.connector);
        if !connector_supports_operation(&self.tool.connector, &action.tool, &action.operation) {
            return Err(ToolExecutionError {
                kind: ToolExecutionErrorKind::UnsupportedTool,
                message: format!(
                    "{} connector template does not support {}.{}",
                    connector, action.tool, action.operation
                ),
            });
        }
        Ok(vec![BudgetUsage {
            kind: "gateway_calls".into(),
            amount: 1,
            provider: Some("github".into()),
            model: None,
            operation: Some(format!("github.{}", action.operation)),
            metadata: BTreeMap::from([("connector".into(), RedactedValue::Plain(connector))]),
        }])
    }

    fn execute_with_secrets(
        &self,
        _task: &ResolvedTask,
        action: &ToolAction,
        _context: &ToolExecutionContext,
        secrets: &[GatewaySecretBinding],
    ) -> std::result::Result<ToolResult, ToolExecutionError> {
        if self.tool.protocol != action.protocol
            || self.tool.tool != action.tool
            || self.tool.operation != action.operation
        {
            return Err(ToolExecutionError {
                kind: ToolExecutionErrorKind::UnsupportedTool,
                message: format!(
                    "github_rest adapter {}.{} cannot execute {}.{}",
                    self.tool.tool, self.tool.operation, action.tool, action.operation
                ),
            });
        }

        let Some((api_base, repository)) = github_rest_connector_parts(&self.tool.connector) else {
            return Err(ToolExecutionError {
                kind: ToolExecutionErrorKind::UnsupportedTool,
                message: "gateway connector is not a bounded GitHub REST connector".into(),
            });
        };

        if action.tool != "github" {
            return Err(ToolExecutionError {
                kind: ToolExecutionErrorKind::UnsupportedTool,
                message: format!(
                    "github_rest connector does not support tool {}",
                    action.tool
                ),
            });
        }

        let token = github_token_from_bindings(secrets)?;
        match action.operation.as_str() {
            "read_issue" => {
                let number = plain_u64_parameter(action, "number")?;
                let issue = self
                    .client
                    .read_issue(api_base, repository, token, number)?;
                Ok(ToolResult {
                    summary: format!("read GitHub issue #{} from {repository}", issue.number),
                    values: BTreeMap::from([
                        (
                            "repository".into(),
                            RedactedValue::Plain(repository.to_owned()),
                        ),
                        (
                            "number".into(),
                            RedactedValue::Plain(issue.number.to_string()),
                        ),
                        (
                            "title".into(),
                            RedactedValue::Plain(redact_secret_like_text(&issue.title)),
                        ),
                        (
                            "state".into(),
                            RedactedValue::Plain(redact_secret_like_text(&issue.state)),
                        ),
                        (
                            "body".into(),
                            RedactedValue::Plain(redact_secret_like_text(&issue.body)),
                        ),
                        (
                            "html_url".into(),
                            RedactedValue::Plain(redact_secret_like_text(&issue.html_url)),
                        ),
                    ]),
                    artifacts: Vec::new(),
                    usage: Vec::new(),
                })
            }
            "create_pr" => {
                let input = GitHubPullRequestInput {
                    title: bounded_plain_parameter(action, "title", 512)?,
                    head: safe_git_ref_parameter(action, "head")?,
                    base: safe_git_ref_parameter(action, "base")?,
                    body: optional_bounded_plain_parameter(action, "body", 65_536)?
                        .unwrap_or_default(),
                };
                let pull = self
                    .client
                    .create_pull_request(api_base, repository, token, input)?;
                Ok(ToolResult {
                    summary: format!(
                        "created GitHub pull request #{} in {repository}",
                        pull.number
                    ),
                    values: BTreeMap::from([
                        (
                            "repository".into(),
                            RedactedValue::Plain(repository.to_owned()),
                        ),
                        (
                            "number".into(),
                            RedactedValue::Plain(pull.number.to_string()),
                        ),
                        (
                            "title".into(),
                            RedactedValue::Plain(redact_secret_like_text(&pull.title)),
                        ),
                        (
                            "html_url".into(),
                            RedactedValue::Plain(redact_secret_like_text(&pull.html_url)),
                        ),
                    ]),
                    artifacts: Vec::new(),
                    usage: Vec::new(),
                })
            }
            "comment_issue" => {
                let number = plain_u64_parameter(action, "number")?;
                let body = bounded_plain_parameter(action, "body", 65_536)?;
                let comment = self
                    .client
                    .create_issue_comment(api_base, repository, token, number, body)?;
                Ok(ToolResult {
                    summary: format!(
                        "created GitHub issue comment {} on {repository}#{number}",
                        comment.id
                    ),
                    values: BTreeMap::from([
                        (
                            "repository".into(),
                            RedactedValue::Plain(repository.to_owned()),
                        ),
                        ("number".into(), RedactedValue::Plain(number.to_string())),
                        (
                            "comment_id".into(),
                            RedactedValue::Plain(comment.id.to_string()),
                        ),
                        (
                            "body".into(),
                            RedactedValue::Plain(redact_secret_like_text(&comment.body)),
                        ),
                        (
                            "html_url".into(),
                            RedactedValue::Plain(redact_secret_like_text(&comment.html_url)),
                        ),
                    ]),
                    artifacts: Vec::new(),
                    usage: Vec::new(),
                })
            }
            "create_branch" => {
                let branch = safe_git_ref_parameter(action, "branch")?;
                let from_ref = safe_git_ref_parameter(action, "from_ref")?;
                let branch_result = self
                    .client
                    .create_branch(api_base, repository, token, &branch, &from_ref)?;
                Ok(ToolResult {
                    summary: format!(
                        "created GitHub branch {} in {repository}",
                        branch_result.name
                    ),
                    values: BTreeMap::from([
                        (
                            "repository".into(),
                            RedactedValue::Plain(repository.to_owned()),
                        ),
                        ("branch".into(), RedactedValue::Plain(branch_result.name)),
                        (
                            "sha".into(),
                            RedactedValue::Plain(redact_secret_like_text(&branch_result.sha)),
                        ),
                    ]),
                    artifacts: Vec::new(),
                    usage: Vec::new(),
                })
            }
            "commit_file" => {
                let input = GitHubFileCommitInput {
                    path: safe_github_file_path_parameter(action, "path")?,
                    message: bounded_plain_parameter(action, "message", 1024)?,
                    content: bounded_plain_parameter(action, "content", 256 * 1024)?,
                    branch: safe_git_ref_parameter(action, "branch")?,
                    sha: optional_safe_sha_parameter(action, "sha")?,
                };
                let commit = self
                    .client
                    .commit_file(api_base, repository, token, input)?;
                Ok(ToolResult {
                    summary: format!("committed file {} in {repository}", commit.path),
                    values: BTreeMap::from([
                        (
                            "repository".into(),
                            RedactedValue::Plain(repository.to_owned()),
                        ),
                        ("path".into(), RedactedValue::Plain(commit.path)),
                        (
                            "sha".into(),
                            RedactedValue::Plain(redact_secret_like_text(&commit.sha)),
                        ),
                        (
                            "html_url".into(),
                            RedactedValue::Plain(redact_secret_like_text(&commit.html_url)),
                        ),
                    ]),
                    artifacts: Vec::new(),
                    usage: Vec::new(),
                })
            }
            "update_pr" => {
                let input = GitHubPullRequestUpdateInput {
                    number: plain_u64_parameter(action, "number")?,
                    title: optional_bounded_plain_parameter(action, "title", 512)?,
                    body: optional_bounded_plain_parameter(action, "body", 65_536)?,
                    state: optional_pr_state_parameter(action, "state")?,
                    base: optional_safe_git_ref_parameter(action, "base")?,
                };
                if input.title.is_none()
                    && input.body.is_none()
                    && input.state.is_none()
                    && input.base.is_none()
                {
                    return Err(ToolExecutionError {
                        kind: ToolExecutionErrorKind::InvalidParameters,
                        message: "github.update_pr requires at least one update field".into(),
                    });
                }
                let pull = self
                    .client
                    .update_pull_request(api_base, repository, token, input)?;
                Ok(ToolResult {
                    summary: format!(
                        "updated GitHub pull request #{} in {repository}",
                        pull.number
                    ),
                    values: BTreeMap::from([
                        (
                            "repository".into(),
                            RedactedValue::Plain(repository.to_owned()),
                        ),
                        (
                            "number".into(),
                            RedactedValue::Plain(pull.number.to_string()),
                        ),
                        (
                            "title".into(),
                            RedactedValue::Plain(redact_secret_like_text(&pull.title)),
                        ),
                        (
                            "state".into(),
                            RedactedValue::Plain(redact_secret_like_text(&pull.state)),
                        ),
                        (
                            "html_url".into(),
                            RedactedValue::Plain(redact_secret_like_text(&pull.html_url)),
                        ),
                    ]),
                    artifacts: Vec::new(),
                    usage: Vec::new(),
                })
            }
            "comment_report" => {
                let number = plain_u64_parameter(action, "number")?;
                let body = report_comment_body(action)?;
                let comment = self
                    .client
                    .create_issue_comment(api_base, repository, token, number, body)?;
                Ok(ToolResult {
                    summary: format!(
                        "posted GitHub report comment {} on {repository} PR #{number}",
                        comment.id
                    ),
                    values: BTreeMap::from([
                        (
                            "repository".into(),
                            RedactedValue::Plain(repository.to_owned()),
                        ),
                        ("number".into(), RedactedValue::Plain(number.to_string())),
                        (
                            "comment_id".into(),
                            RedactedValue::Plain(comment.id.to_string()),
                        ),
                        (
                            "html_url".into(),
                            RedactedValue::Plain(redact_secret_like_text(&comment.html_url)),
                        ),
                    ]),
                    artifacts: Vec::new(),
                    usage: Vec::new(),
                })
            }
            _ => Err(ToolExecutionError {
                kind: ToolExecutionErrorKind::UnsupportedTool,
                message: format!(
                    "github_rest connector does not support github.{}",
                    action.operation
                ),
            }),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EnterpriseHttpRequest {
    pub method: String,
    pub url: String,
    pub bearer_token: String,
    pub body: serde_json::Value,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EnterpriseHttpResponse {
    pub status: u16,
    pub body: serde_json::Value,
}

pub trait EnterpriseHttpClient {
    fn execute(
        &self,
        request: &EnterpriseHttpRequest,
    ) -> std::result::Result<EnterpriseHttpResponse, ToolExecutionError>;
}

#[derive(Clone, Debug, Default)]
pub struct UreqEnterpriseHttpClient;

impl EnterpriseHttpClient for UreqEnterpriseHttpClient {
    fn execute(
        &self,
        request: &EnterpriseHttpRequest,
    ) -> std::result::Result<EnterpriseHttpResponse, ToolExecutionError> {
        let builder = match request.method.as_str() {
            "GET" => ureq::get(&request.url),
            "POST" => ureq::post(&request.url),
            "PATCH" => ureq::patch(&request.url),
            other => {
                return Err(ToolExecutionError {
                    kind: ToolExecutionErrorKind::InvalidParameters,
                    message: format!("enterprise HTTP method {other} is not supported"),
                });
            }
        };
        let response = builder
            .set("Authorization", &format!("Bearer {}", request.bearer_token))
            .set("Accept", "application/json")
            .set("Content-Type", "application/json")
            .set("User-Agent", "taskfence-gateway")
            .send_json(request.body.clone())
            .map_err(enterprise_http_ureq_error)?;
        let status = response.status();
        let body = response
            .into_json::<serde_json::Value>()
            .unwrap_or(serde_json::Value::Null);
        Ok(EnterpriseHttpResponse { status, body })
    }
}

#[derive(Clone, Debug)]
pub struct EnterpriseConnectorAdapter<C> {
    tool: GatewayToolConfig,
    client: C,
}

impl<C> EnterpriseConnectorAdapter<C> {
    pub fn new(tool: GatewayToolConfig, client: C) -> Self {
        Self { tool, client }
    }
}

impl<C> ToolAdapter for EnterpriseConnectorAdapter<C>
where
    C: EnterpriseHttpClient,
{
    fn identity(&self) -> ToolAdapterIdentity {
        connector_identity(&self.tool.connector)
    }

    fn secret_references(&self) -> &[GatewaySecretReferenceConfig] {
        &self.tool.secret_refs
    }

    fn planned_budget_usage(
        &self,
        _task: &ResolvedTask,
        action: &ToolAction,
        _context: &ToolExecutionContext,
    ) -> std::result::Result<Vec<BudgetUsage>, ToolExecutionError> {
        let connector = connector_kind(&self.tool.connector);
        if !connector_supports_operation(&self.tool.connector, &action.tool, &action.operation) {
            return Err(ToolExecutionError {
                kind: ToolExecutionErrorKind::UnsupportedTool,
                message: format!(
                    "{} connector template does not support {}.{}",
                    connector, action.tool, action.operation
                ),
            });
        }
        Ok(vec![BudgetUsage {
            kind: "gateway_calls".into(),
            amount: 1,
            provider: Some(connector.clone()),
            model: None,
            operation: Some(format!("{}.{}", action.tool, action.operation)),
            metadata: BTreeMap::from([("connector".into(), RedactedValue::Plain(connector))]),
        }])
    }

    fn execute(
        &self,
        task: &ResolvedTask,
        action: &ToolAction,
        context: &ToolExecutionContext,
    ) -> std::result::Result<ToolResult, ToolExecutionError> {
        self.execute_with_secrets(task, action, context, &[])
    }

    fn execute_with_secrets(
        &self,
        _task: &ResolvedTask,
        action: &ToolAction,
        _context: &ToolExecutionContext,
        secrets: &[GatewaySecretBinding],
    ) -> std::result::Result<ToolResult, ToolExecutionError> {
        ensure_tool_matches_config(&self.tool, action)?;
        let token = live_secret_from_bindings(secrets, &action.tool)?;
        let request = enterprise_http_request(&self.tool.connector, action, token)?;
        let response = self.client.execute(&request)?;
        if !(200..300).contains(&response.status) {
            return Err(ToolExecutionError {
                kind: ToolExecutionErrorKind::AdapterFailed,
                message: format!(
                    "{} connector returned HTTP {}",
                    connector_kind(&self.tool.connector),
                    response.status
                ),
            });
        }
        enterprise_http_result(&self.tool.connector, action, &request, response)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DatabaseStatement {
    pub engine: String,
    pub database_ref: String,
    pub sql: String,
    pub read_only: bool,
    pub max_rows: u32,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DatabaseStatementResult {
    pub rows_affected: u64,
    pub rows: Vec<BTreeMap<String, String>>,
}

pub trait DatabaseConnectorClient {
    fn execute(
        &self,
        credential: &str,
        statement: DatabaseStatement,
    ) -> std::result::Result<DatabaseStatementResult, ToolExecutionError>;
}

#[derive(Clone, Debug, Default)]
pub struct PostgresDatabaseClient;

impl DatabaseConnectorClient for PostgresDatabaseClient {
    fn execute(
        &self,
        credential: &str,
        statement: DatabaseStatement,
    ) -> std::result::Result<DatabaseStatementResult, ToolExecutionError> {
        if statement.engine != "postgres" {
            return Err(ToolExecutionError {
                kind: ToolExecutionErrorKind::UnsupportedTool,
                message: format!("database engine {} is not supported", statement.engine),
            });
        }
        let mut client = postgres::Client::connect(credential, postgres::NoTls).map_err(|err| {
            ToolExecutionError {
                kind: ToolExecutionErrorKind::AdapterFailed,
                message: format!("failed to connect to Postgres database: {err}"),
            }
        })?;
        if statement.read_only {
            let rows = client
                .query(&statement.sql, &[])
                .map_err(|err| ToolExecutionError {
                    kind: ToolExecutionErrorKind::AdapterFailed,
                    message: format!("Postgres read failed: {err}"),
                })?;
            let mut values = Vec::new();
            for row in rows.into_iter().take(statement.max_rows as usize) {
                let mut rendered = BTreeMap::new();
                for column in row.columns() {
                    let name = column.name();
                    let value = row
                        .try_get::<_, String>(name)
                        .or_else(|_| row.try_get::<_, i64>(name).map(|value| value.to_string()))
                        .or_else(|_| row.try_get::<_, bool>(name).map(|value| value.to_string()))
                        .unwrap_or_else(|_| "<unrendered>".into());
                    rendered.insert(name.to_owned(), redact_secret_like_text(&value));
                }
                values.push(rendered);
            }
            Ok(DatabaseStatementResult {
                rows_affected: values.len() as u64,
                rows: values,
            })
        } else {
            let rows_affected =
                client
                    .execute(&statement.sql, &[])
                    .map_err(|err| ToolExecutionError {
                        kind: ToolExecutionErrorKind::AdapterFailed,
                        message: format!("Postgres write failed: {err}"),
                    })?;
            Ok(DatabaseStatementResult {
                rows_affected,
                rows: Vec::new(),
            })
        }
    }
}

#[derive(Clone, Debug)]
pub struct DatabaseConnectorAdapter<C> {
    tool: GatewayToolConfig,
    client: C,
}

impl<C> DatabaseConnectorAdapter<C> {
    pub fn new(tool: GatewayToolConfig, client: C) -> Self {
        Self { tool, client }
    }
}

impl<C> ToolAdapter for DatabaseConnectorAdapter<C>
where
    C: DatabaseConnectorClient,
{
    fn identity(&self) -> ToolAdapterIdentity {
        connector_identity(&self.tool.connector)
    }

    fn secret_references(&self) -> &[GatewaySecretReferenceConfig] {
        &self.tool.secret_refs
    }

    fn planned_budget_usage(
        &self,
        _task: &ResolvedTask,
        action: &ToolAction,
        _context: &ToolExecutionContext,
    ) -> std::result::Result<Vec<BudgetUsage>, ToolExecutionError> {
        if !connector_supports_operation(&self.tool.connector, &action.tool, &action.operation) {
            return Err(ToolExecutionError {
                kind: ToolExecutionErrorKind::UnsupportedTool,
                message: format!(
                    "database connector template does not support {}.{}",
                    action.tool, action.operation
                ),
            });
        }
        Ok(vec![BudgetUsage {
            kind: "gateway_calls".into(),
            amount: 1,
            provider: Some("database".into()),
            model: None,
            operation: Some(format!("database.{}", action.operation)),
            metadata: BTreeMap::from([(
                "connector".into(),
                RedactedValue::Plain("database".into()),
            )]),
        }])
    }

    fn execute(
        &self,
        task: &ResolvedTask,
        action: &ToolAction,
        context: &ToolExecutionContext,
    ) -> std::result::Result<ToolResult, ToolExecutionError> {
        self.execute_with_secrets(task, action, context, &[])
    }

    fn execute_with_secrets(
        &self,
        _task: &ResolvedTask,
        action: &ToolAction,
        _context: &ToolExecutionContext,
        secrets: &[GatewaySecretBinding],
    ) -> std::result::Result<ToolResult, ToolExecutionError> {
        ensure_tool_matches_config(&self.tool, action)?;
        let GatewayConnectorConfig::Database {
            engine,
            database_ref,
        } = &self.tool.connector
        else {
            return Err(ToolExecutionError {
                kind: ToolExecutionErrorKind::UnsupportedTool,
                message: "gateway connector is not a database connector".into(),
            });
        };
        let read_only = match action.operation.as_str() {
            "read" => true,
            "write" => false,
            _ => {
                return Err(ToolExecutionError {
                    kind: ToolExecutionErrorKind::UnsupportedTool,
                    message: format!("database connector does not support {}", action.operation),
                });
            }
        };
        let sql = bounded_plain_parameter(action, "query", 32 * 1024)?;
        validate_sql_statement(&sql, read_only)?;
        let statement = DatabaseStatement {
            engine: engine.clone(),
            database_ref: database_ref.clone(),
            sql,
            read_only,
            max_rows: optional_u32_parameter(action, "max_rows")?
                .unwrap_or(100)
                .min(1000),
        };
        let credential = live_secret_from_bindings(secrets, &action.tool)?;
        let result = self.client.execute(credential, statement)?;
        let rows_json = serde_json::to_string(&result.rows).map_err(|err| ToolExecutionError {
            kind: ToolExecutionErrorKind::AdapterFailed,
            message: format!("failed to serialize database rows: {err}"),
        })?;
        Ok(ToolResult {
            summary: format!(
                "executed database {} against {}",
                if read_only { "read" } else { "write" },
                database_ref
            ),
            values: BTreeMap::from([
                (
                    "database_ref".into(),
                    RedactedValue::Plain(database_ref.clone()),
                ),
                ("engine".into(), RedactedValue::Plain(engine.clone())),
                (
                    "rows_affected".into(),
                    RedactedValue::Plain(result.rows_affected.to_string()),
                ),
                (
                    "rows".into(),
                    RedactedValue::Plain(redact_secret_like_text(&rows_json)),
                ),
            ]),
            artifacts: Vec::new(),
            usage: Vec::new(),
        })
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LocalFixtureToolAdapter {
    tool: GatewayToolConfig,
}

impl LocalFixtureToolAdapter {
    pub fn new(tool: GatewayToolConfig) -> Self {
        Self { tool }
    }
}

impl ToolAdapter for LocalFixtureToolAdapter {
    fn identity(&self) -> ToolAdapterIdentity {
        connector_identity(&self.tool.connector)
    }

    fn secret_references(&self) -> &[GatewaySecretReferenceConfig] {
        &self.tool.secret_refs
    }

    fn execute(
        &self,
        _task: &ResolvedTask,
        action: &ToolAction,
        context: &ToolExecutionContext,
    ) -> std::result::Result<ToolResult, ToolExecutionError> {
        if self.tool.protocol != action.protocol
            || self.tool.tool != action.tool
            || self.tool.operation != action.operation
        {
            return Err(ToolExecutionError {
                kind: ToolExecutionErrorKind::UnsupportedTool,
                message: format!(
                    "fixture adapter {}.{} cannot execute {}.{}",
                    self.tool.tool, self.tool.operation, action.tool, action.operation
                ),
            });
        }

        let GatewayConnectorConfig::LocalFixture { kind, path } = &self.tool.connector else {
            return Err(ToolExecutionError {
                kind: ToolExecutionErrorKind::UnsupportedTool,
                message: "gateway connector is not a local fixture".into(),
            });
        };

        match (
            kind.as_str(),
            action.tool.as_str(),
            action.operation.as_str(),
        ) {
            ("github", "github", "read_issue") => execute_github_read_issue(path, action),
            ("github", "github", "create_pr") => execute_github_create_pr(path, action, context),
            _ => Err(ToolExecutionError {
                kind: ToolExecutionErrorKind::UnsupportedTool,
                message: format!(
                    "local fixture connector {kind} does not support {}.{}",
                    action.tool, action.operation
                ),
            }),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ConnectorPolicyTemplate {
    pub connector: String,
    pub supported_operations: Vec<String>,
    pub approval_required_operations: Vec<String>,
    pub secret_scopes: Vec<String>,
}

pub fn connector_policy_template(kind: &str) -> Option<ConnectorPolicyTemplate> {
    match kind.trim().to_ascii_lowercase().as_str() {
        "github_rest" | "github_enterprise_rest" => Some(template(
            kind,
            &[
                "github.read_issue",
                "github.create_branch",
                "github.commit_file",
                "github.create_pr",
                "github.update_pr",
                "github.comment_issue",
                "github.comment_report",
            ],
            &[
                "github.create_branch",
                "github.commit_file",
                "github.create_pr",
                "github.update_pr",
                "github.comment_issue",
                "github.comment_report",
            ],
            &[
                "github.read_issue",
                "github.create_branch",
                "github.commit_file",
                "github.create_pr",
                "github.update_pr",
                "github.comment_issue",
                "github.comment_report",
            ],
        )),
        "gitlab" => Some(template(
            "gitlab",
            &[
                "gitlab.read_issue",
                "gitlab.create_merge_request",
                "gitlab.comment_issue",
            ],
            &["gitlab.create_merge_request", "gitlab.comment_issue"],
            &[
                "gitlab.read_issue",
                "gitlab.create_merge_request",
                "gitlab.comment_issue",
            ],
        )),
        "jira" => Some(template(
            "jira",
            &["jira.read_issue", "jira.create_issue", "jira.comment_issue"],
            &["jira.create_issue", "jira.comment_issue"],
            &["jira.read_issue", "jira.create_issue", "jira.comment_issue"],
        )),
        "feishu" => Some(template(
            "feishu",
            &["feishu.send_message", "feishu.create_doc"],
            &["feishu.send_message", "feishu.create_doc"],
            &["feishu.send_message", "feishu.create_doc"],
        )),
        "wecom" => Some(template(
            "wecom",
            &["wecom.send_message"],
            &["wecom.send_message"],
            &["wecom.send_message"],
        )),
        "dingtalk" => Some(template(
            "dingtalk",
            &["dingtalk.send_message"],
            &["dingtalk.send_message"],
            &["dingtalk.send_message"],
        )),
        "gitee" => Some(template(
            "gitee",
            &["gitee.read_issue", "gitee.create_pr", "gitee.comment_issue"],
            &["gitee.create_pr", "gitee.comment_issue"],
            &["gitee.read_issue", "gitee.create_pr", "gitee.comment_issue"],
        )),
        "coding" => Some(template(
            "coding",
            &[
                "coding.read_issue",
                "coding.create_merge_request",
                "coding.comment_issue",
            ],
            &["coding.create_merge_request", "coding.comment_issue"],
            &[
                "coding.read_issue",
                "coding.create_merge_request",
                "coding.comment_issue",
            ],
        )),
        "database" => Some(template(
            "database",
            &["database.read", "database.write"],
            &["database.write"],
            &["database.read", "database.write"],
        )),
        "internal_http" => Some(template(
            "internal_http",
            &["internal_http.call"],
            &["internal_http.call"],
            &["internal_http.call"],
        )),
        "siem_export" => Some(template(
            "siem_export",
            &["siem.export_events"],
            &["siem.export_events"],
            &["siem.export_events"],
        )),
        _ => None,
    }
}

pub fn connector_operation_supported(
    connector: &GatewayConnectorConfig,
    action: &ToolAction,
) -> bool {
    connector_supports_operation(connector, &action.tool, &action.operation)
}

pub fn connector_supports_operation(
    connector: &GatewayConnectorConfig,
    tool: &str,
    operation: &str,
) -> bool {
    let operation = format!("{tool}.{operation}");
    connector_policy_template(&connector_kind(connector))
        .map(|template| {
            template
                .supported_operations
                .iter()
                .any(|item| item == &operation)
        })
        .unwrap_or(false)
}

pub fn connector_identity(connector: &GatewayConnectorConfig) -> ToolAdapterIdentity {
    ToolAdapterIdentity {
        kind: connector_kind(connector),
        name: connector_name(connector),
    }
}

pub fn connector_kind(connector: &GatewayConnectorConfig) -> String {
    match connector {
        GatewayConnectorConfig::LocalFixture { .. } => "local_fixture".into(),
        GatewayConnectorConfig::GitHubRest { .. } => "github_rest".into(),
        GatewayConnectorConfig::GitHubEnterpriseRest { .. } => "github_enterprise_rest".into(),
        GatewayConnectorConfig::GitLab { .. } => "gitlab".into(),
        GatewayConnectorConfig::Jira { .. } => "jira".into(),
        GatewayConnectorConfig::Feishu { .. } => "feishu".into(),
        GatewayConnectorConfig::WeCom { .. } => "wecom".into(),
        GatewayConnectorConfig::DingTalk { .. } => "dingtalk".into(),
        GatewayConnectorConfig::Gitee { .. } => "gitee".into(),
        GatewayConnectorConfig::Coding { .. } => "coding".into(),
        GatewayConnectorConfig::Database { .. } => "database".into(),
        GatewayConnectorConfig::InternalHttp { .. } => "internal_http".into(),
        GatewayConnectorConfig::SiemExport { .. } => "siem_export".into(),
        GatewayConnectorConfig::Unsupported { .. } => "unsupported".into(),
    }
}

pub fn connector_name(connector: &GatewayConnectorConfig) -> String {
    match connector {
        GatewayConnectorConfig::LocalFixture { kind, .. } => kind.clone(),
        GatewayConnectorConfig::GitHubRest { .. }
        | GatewayConnectorConfig::GitHubEnterpriseRest { .. } => "github".into(),
        GatewayConnectorConfig::GitLab { project, .. } => project.clone(),
        GatewayConnectorConfig::Jira { project_key, .. } => project_key.clone(),
        GatewayConnectorConfig::Feishu { app, .. } => app.clone(),
        GatewayConnectorConfig::WeCom { corp_id, .. } => corp_id.clone(),
        GatewayConnectorConfig::DingTalk { tenant, .. } => tenant.clone(),
        GatewayConnectorConfig::Gitee { repository, .. } => repository.clone(),
        GatewayConnectorConfig::Coding { project, .. } => project.clone(),
        GatewayConnectorConfig::Database { database_ref, .. } => database_ref.clone(),
        GatewayConnectorConfig::InternalHttp { service, .. } => service.clone(),
        GatewayConnectorConfig::SiemExport { sink, .. } => sink.clone(),
        GatewayConnectorConfig::Unsupported { kind } => kind.clone(),
    }
}

fn github_rest_connector_parts(connector: &GatewayConnectorConfig) -> Option<(&str, &str)> {
    match connector {
        GatewayConnectorConfig::GitHubRest {
            api_base,
            repository,
        }
        | GatewayConnectorConfig::GitHubEnterpriseRest {
            api_base,
            repository,
        } => Some((api_base, repository)),
        _ => None,
    }
}

fn template(
    connector: &str,
    supported_operations: &[&str],
    approval_required_operations: &[&str],
    secret_scopes: &[&str],
) -> ConnectorPolicyTemplate {
    ConnectorPolicyTemplate {
        connector: connector.trim().to_ascii_lowercase(),
        supported_operations: supported_operations
            .iter()
            .map(|operation| (*operation).into())
            .collect(),
        approval_required_operations: approval_required_operations
            .iter()
            .map(|operation| (*operation).into())
            .collect(),
        secret_scopes: secret_scopes.iter().map(|scope| (*scope).into()).collect(),
    }
}

#[derive(Clone, Debug, Default)]
pub struct LocalRedactedSecretBroker;

impl SecretBroker for LocalRedactedSecretBroker {
    fn issue_reference(
        &self,
        task: &ResolvedTask,
        name: &str,
        scope: &str,
    ) -> taskfence_core::Result<SecretReference> {
        Ok(SecretReference {
            name: name.into(),
            scope: scope.into(),
            handle: format!("taskfence://gateway/{}/{name}/{scope}", task.id.0),
        })
    }
}

#[derive(Clone, Debug, Default)]
pub struct EnvironmentSecretBroker;

impl EnvironmentSecretBroker {
    pub fn new() -> Self {
        Self
    }
}

impl SecretBroker for EnvironmentSecretBroker {
    fn issue_reference(
        &self,
        _task: &ResolvedTask,
        name: &str,
        scope: &str,
    ) -> taskfence_core::Result<SecretReference> {
        let env_name = gateway_secret_env_name(name)?;
        match std::env::var(&env_name) {
            Ok(value) if !value.trim().is_empty() => Ok(SecretReference {
                name: name.into(),
                scope: scope.into(),
                handle: value,
            }),
            _ => Err(TaskFenceError::Gateway(format!(
                "gateway secret {name} for {scope} is unavailable; set {env_name} for live connector execution"
            ))),
        }
    }
}

fn gateway_secret_env_name(name: &str) -> taskfence_core::Result<String> {
    let name = normalize_required_segment("secret name", name.to_owned())?;
    Ok(format!(
        "TASKFENCE_GATEWAY_SECRET_{}",
        name.chars()
            .map(|ch| {
                if ch.is_ascii_alphanumeric() {
                    ch.to_ascii_uppercase()
                } else {
                    '_'
                }
            })
            .collect::<String>()
    ))
}

fn tool_execution_error_from_taskfence(err: &TaskFenceError) -> ToolExecutionError {
    let message = err.to_string();
    let kind = match err {
        TaskFenceError::Unsupported(_) => ToolExecutionErrorKind::UnsupportedProtocol,
        TaskFenceError::Approval(_) => ToolExecutionErrorKind::ApprovalDeniedOrTimedOut,
        TaskFenceError::Policy(_) => ToolExecutionErrorKind::PolicyDenied,
        TaskFenceError::Gateway(inner) if inner.contains("not registered") => {
            ToolExecutionErrorKind::UnregisteredTool
        }
        TaskFenceError::Gateway(inner)
            if inner.contains("must not be empty") || inner.contains("parameter") =>
        {
            ToolExecutionErrorKind::InvalidParameters
        }
        TaskFenceError::Gateway(inner) if inner.contains("secret") => {
            ToolExecutionErrorKind::SecretUnavailable
        }
        TaskFenceError::Gateway(_) => ToolExecutionErrorKind::AdapterFailed,
        TaskFenceError::Config(_)
        | TaskFenceError::Runner(_)
        | TaskFenceError::Audit(_)
        | TaskFenceError::Artifact(_)
        | TaskFenceError::Report(_)
        | TaskFenceError::State(_) => ToolExecutionErrorKind::AdapterFailed,
    };
    ToolExecutionError { kind, message }
}

fn github_token_from_bindings(
    secrets: &[GatewaySecretBinding],
) -> std::result::Result<&str, ToolExecutionError> {
    live_secret_from_bindings(secrets, "github_rest")
}

fn live_secret_from_bindings<'a>(
    secrets: &'a [GatewaySecretBinding],
    connector: &str,
) -> std::result::Result<&'a str, ToolExecutionError> {
    let binding = secrets
        .iter()
        .find(|binding| binding.parameter == "authorization" || binding.parameter == "token")
        .or_else(|| secrets.first())
        .ok_or_else(|| ToolExecutionError {
            kind: ToolExecutionErrorKind::SecretUnavailable,
            message: format!("{connector} connector requires a gateway-side secret"),
        })?;
    if binding.reference.handle.starts_with("taskfence://")
        || binding.reference.handle.trim().is_empty()
    {
        return Err(ToolExecutionError {
            kind: ToolExecutionErrorKind::SecretUnavailable,
            message: format!(
                "gateway secret {} for {} is not backed by a live credential",
                binding.reference.name, binding.reference.scope
            ),
        });
    }
    Ok(binding.reference.handle.trim())
}

fn github_api_url(api_base: &str, path: &str) -> String {
    format!(
        "{}/{}",
        api_base.trim_end_matches('/'),
        path.trim_start_matches('/')
    )
}

fn github_get_json(
    url: &str,
    token: &str,
) -> std::result::Result<serde_json::Value, ToolExecutionError> {
    let response = github_request_headers(ureq::get(url), token)
        .call()
        .map_err(github_ureq_error)?;
    response
        .into_json::<serde_json::Value>()
        .map_err(|err| ToolExecutionError {
            kind: ToolExecutionErrorKind::AdapterFailed,
            message: format!("failed to parse GitHub response: {err}"),
        })
}

fn github_post_json(
    url: &str,
    token: &str,
    body: serde_json::Value,
) -> std::result::Result<serde_json::Value, ToolExecutionError> {
    let response = github_request_headers(ureq::post(url), token)
        .send_json(body)
        .map_err(github_ureq_error)?;
    response
        .into_json::<serde_json::Value>()
        .map_err(|err| ToolExecutionError {
            kind: ToolExecutionErrorKind::AdapterFailed,
            message: format!("failed to parse GitHub response: {err}"),
        })
}

fn github_patch_json(
    url: &str,
    token: &str,
    body: serde_json::Value,
) -> std::result::Result<serde_json::Value, ToolExecutionError> {
    let response = github_request_headers(ureq::patch(url), token)
        .send_json(body)
        .map_err(github_ureq_error)?;
    response
        .into_json::<serde_json::Value>()
        .map_err(|err| ToolExecutionError {
            kind: ToolExecutionErrorKind::AdapterFailed,
            message: format!("failed to parse GitHub response: {err}"),
        })
}

fn github_put_json(
    url: &str,
    token: &str,
    body: serde_json::Value,
) -> std::result::Result<serde_json::Value, ToolExecutionError> {
    let response = github_request_headers(ureq::put(url), token)
        .send_json(body)
        .map_err(github_ureq_error)?;
    response
        .into_json::<serde_json::Value>()
        .map_err(|err| ToolExecutionError {
            kind: ToolExecutionErrorKind::AdapterFailed,
            message: format!("failed to parse GitHub response: {err}"),
        })
}

fn github_request_headers(request: ureq::Request, token: &str) -> ureq::Request {
    request
        .set("Authorization", &format!("Bearer {token}"))
        .set("Accept", "application/vnd.github+json")
        .set("X-GitHub-Api-Version", "2022-11-28")
        .set("User-Agent", "taskfence-gateway")
}

fn github_ureq_error(err: ureq::Error) -> ToolExecutionError {
    match err {
        ureq::Error::Status(status, response) => {
            let message = response
                .into_string()
                .unwrap_or_else(|_| "unable to read GitHub error response".into());
            ToolExecutionError {
                kind: ToolExecutionErrorKind::AdapterFailed,
                message: format!(
                    "GitHub API returned HTTP {status}: {}",
                    redact_secret_like_text(&message)
                ),
            }
        }
        ureq::Error::Transport(transport) => ToolExecutionError {
            kind: ToolExecutionErrorKind::AdapterFailed,
            message: format!("GitHub API transport failed: {transport}"),
        },
    }
}

fn enterprise_http_ureq_error(err: ureq::Error) -> ToolExecutionError {
    match err {
        ureq::Error::Status(status, response) => {
            let message = response
                .into_string()
                .unwrap_or_else(|_| "unable to read enterprise connector error response".into());
            ToolExecutionError {
                kind: ToolExecutionErrorKind::AdapterFailed,
                message: format!(
                    "enterprise connector returned HTTP {status}: {}",
                    redact_secret_like_text(&message)
                ),
            }
        }
        ureq::Error::Transport(transport) => ToolExecutionError {
            kind: ToolExecutionErrorKind::AdapterFailed,
            message: format!("enterprise connector transport failed: {transport}"),
        },
    }
}

fn json_u64_field(value: &serde_json::Value, key: &str) -> Option<u64> {
    value.get(key).and_then(serde_json::Value::as_u64)
}

fn ensure_tool_matches_config(
    tool: &GatewayToolConfig,
    action: &ToolAction,
) -> std::result::Result<(), ToolExecutionError> {
    if tool.protocol != action.protocol
        || tool.tool != action.tool
        || tool.operation != action.operation
    {
        return Err(ToolExecutionError {
            kind: ToolExecutionErrorKind::UnsupportedTool,
            message: format!(
                "{} adapter {}.{} cannot execute {}.{}",
                connector_kind(&tool.connector),
                tool.tool,
                tool.operation,
                action.tool,
                action.operation
            ),
        });
    }
    if !connector_supports_operation(&tool.connector, &action.tool, &action.operation) {
        return Err(ToolExecutionError {
            kind: ToolExecutionErrorKind::UnsupportedTool,
            message: format!(
                "{} connector template does not support {}.{}",
                connector_kind(&tool.connector),
                action.tool,
                action.operation
            ),
        });
    }
    Ok(())
}

fn enterprise_http_request(
    connector: &GatewayConnectorConfig,
    action: &ToolAction,
    token: &str,
) -> std::result::Result<EnterpriseHttpRequest, ToolExecutionError> {
    match connector {
        GatewayConnectorConfig::GitLab { api_base, project } => {
            let encoded_project = path_encode(project);
            match (action.tool.as_str(), action.operation.as_str()) {
                ("gitlab", "read_issue") => {
                    let iid = plain_u64_parameter(action, "iid")?;
                    Ok(json_request(
                        "GET",
                        &format!("{api_base}/projects/{encoded_project}/issues/{iid}"),
                        token,
                        serde_json::Value::Null,
                    ))
                }
                ("gitlab", "create_merge_request") => {
                    let body = serde_json::json!({
                        "title": bounded_plain_parameter(action, "title", 512)?,
                        "source_branch": safe_git_ref_parameter(action, "source_branch")?,
                        "target_branch": safe_git_ref_parameter(action, "target_branch")?,
                        "description": optional_bounded_plain_parameter(action, "description", 65_536)?.unwrap_or_default(),
                    });
                    Ok(json_request(
                        "POST",
                        &format!("{api_base}/projects/{encoded_project}/merge_requests"),
                        token,
                        body,
                    ))
                }
                ("gitlab", "comment_issue") => {
                    let iid = plain_u64_parameter(action, "iid")?;
                    let body = serde_json::json!({
                        "body": bounded_plain_parameter(action, "body", 65_536)?,
                    });
                    Ok(json_request(
                        "POST",
                        &format!("{api_base}/projects/{encoded_project}/issues/{iid}/notes"),
                        token,
                        body,
                    ))
                }
                _ => unsupported_connector_action("gitlab", action),
            }
        }
        GatewayConnectorConfig::Jira {
            api_base,
            project_key,
        } => match (action.tool.as_str(), action.operation.as_str()) {
            ("jira", "read_issue") => {
                let issue_key = safe_connector_key_parameter(action, "issue_key")?;
                Ok(json_request(
                    "GET",
                    &format!("{api_base}/issue/{issue_key}"),
                    token,
                    serde_json::Value::Null,
                ))
            }
            ("jira", "create_issue") => {
                let body = serde_json::json!({
                    "fields": {
                        "project": { "key": project_key },
                        "summary": bounded_plain_parameter(action, "summary", 512)?,
                        "description": jira_doc_text(optional_bounded_plain_parameter(action, "description", 16 * 1024)?.unwrap_or_default()),
                        "issuetype": { "name": optional_bounded_plain_parameter(action, "issue_type", 64)?.unwrap_or_else(|| "Task".into()) },
                    }
                });
                Ok(json_request(
                    "POST",
                    &format!("{api_base}/issue"),
                    token,
                    body,
                ))
            }
            ("jira", "comment_issue") => {
                let issue_key = safe_connector_key_parameter(action, "issue_key")?;
                let body = serde_json::json!({
                    "body": jira_doc_text(bounded_plain_parameter(action, "body", 16 * 1024)?),
                });
                Ok(json_request(
                    "POST",
                    &format!("{api_base}/issue/{issue_key}/comment"),
                    token,
                    body,
                ))
            }
            _ => unsupported_connector_action("jira", action),
        },
        GatewayConnectorConfig::Feishu { api_base, app } => {
            enterprise_message_request(api_base, token, app, "feishu", action)
        }
        GatewayConnectorConfig::WeCom { api_base, corp_id } => {
            enterprise_message_request(api_base, token, corp_id, "wecom", action)
        }
        GatewayConnectorConfig::DingTalk { api_base, tenant } => {
            enterprise_message_request(api_base, token, tenant, "dingtalk", action)
        }
        GatewayConnectorConfig::Gitee {
            api_base,
            repository,
        } => match (action.tool.as_str(), action.operation.as_str()) {
            ("gitee", "read_issue") => {
                let number = plain_u64_parameter(action, "number")?;
                Ok(json_request(
                    "GET",
                    &format!("{api_base}/repos/{repository}/issues/{number}"),
                    token,
                    serde_json::Value::Null,
                ))
            }
            ("gitee", "create_pr") => {
                let body = serde_json::json!({
                    "title": bounded_plain_parameter(action, "title", 512)?,
                    "head": safe_git_ref_parameter(action, "head")?,
                    "base": safe_git_ref_parameter(action, "base")?,
                    "body": optional_bounded_plain_parameter(action, "body", 65_536)?.unwrap_or_default(),
                });
                Ok(json_request(
                    "POST",
                    &format!("{api_base}/repos/{repository}/pulls"),
                    token,
                    body,
                ))
            }
            ("gitee", "comment_issue") => {
                let number = plain_u64_parameter(action, "number")?;
                let body = serde_json::json!({
                    "body": bounded_plain_parameter(action, "body", 65_536)?,
                });
                Ok(json_request(
                    "POST",
                    &format!("{api_base}/repos/{repository}/issues/{number}/comments"),
                    token,
                    body,
                ))
            }
            _ => unsupported_connector_action("gitee", action),
        },
        GatewayConnectorConfig::Coding { api_base, project } => {
            let encoded_project = path_encode(project);
            match (action.tool.as_str(), action.operation.as_str()) {
                ("coding", "read_issue") => {
                    let number = plain_u64_parameter(action, "number")?;
                    Ok(json_request(
                        "GET",
                        &format!("{api_base}/projects/{encoded_project}/issues/{number}"),
                        token,
                        serde_json::Value::Null,
                    ))
                }
                ("coding", "create_merge_request") => {
                    let body = serde_json::json!({
                        "title": bounded_plain_parameter(action, "title", 512)?,
                        "source_branch": safe_git_ref_parameter(action, "source_branch")?,
                        "target_branch": safe_git_ref_parameter(action, "target_branch")?,
                        "description": optional_bounded_plain_parameter(action, "description", 65_536)?.unwrap_or_default(),
                    });
                    Ok(json_request(
                        "POST",
                        &format!("{api_base}/projects/{encoded_project}/merge_requests"),
                        token,
                        body,
                    ))
                }
                ("coding", "comment_issue") => {
                    let number = plain_u64_parameter(action, "number")?;
                    let body = serde_json::json!({
                        "body": bounded_plain_parameter(action, "body", 65_536)?,
                    });
                    Ok(json_request(
                        "POST",
                        &format!("{api_base}/projects/{encoded_project}/issues/{number}/comments"),
                        token,
                        body,
                    ))
                }
                _ => unsupported_connector_action("coding", action),
            }
        }
        GatewayConnectorConfig::InternalHttp { api_base, service } => {
            if action.tool != "internal_http" || action.operation != "call" {
                return unsupported_connector_action("internal_http", action);
            }
            let path = safe_http_path_parameter(action, "path")?;
            let method =
                optional_http_method_parameter(action, "method")?.unwrap_or_else(|| "POST".into());
            let body = optional_bounded_plain_parameter(action, "body", 64 * 1024)?
                .map(|body| serde_json::json!({ "body": body }))
                .unwrap_or_else(|| serde_json::json!({}));
            Ok(json_request(
                &method,
                &format!("{api_base}/{service}/{path}"),
                token,
                body,
            ))
        }
        GatewayConnectorConfig::SiemExport { api_base, sink } => {
            if action.tool != "siem" || action.operation != "export_events" {
                return unsupported_connector_action("siem_export", action);
            }
            let events_json = bounded_plain_parameter(action, "events_json", 256 * 1024)?;
            let events: serde_json::Value =
                serde_json::from_str(&events_json).map_err(|err| ToolExecutionError {
                    kind: ToolExecutionErrorKind::InvalidParameters,
                    message: format!("siem.export_events events_json must be valid JSON: {err}"),
                })?;
            let body = serde_json::json!({
                "sink": sink,
                "task_id": bounded_plain_parameter(action, "task_id", 256)?,
                "events": events,
                "summary": optional_bounded_plain_parameter(action, "summary", 16 * 1024)?.unwrap_or_default(),
            });
            Ok(json_request(
                "POST",
                &format!("{api_base}/sinks/{}/events", path_encode(sink)),
                token,
                body,
            ))
        }
        _ => Err(ToolExecutionError {
            kind: ToolExecutionErrorKind::UnsupportedTool,
            message: format!(
                "{} connector is not handled by the enterprise HTTP adapter",
                connector_kind(connector)
            ),
        }),
    }
}

fn enterprise_http_result(
    connector: &GatewayConnectorConfig,
    action: &ToolAction,
    request: &EnterpriseHttpRequest,
    response: EnterpriseHttpResponse,
) -> std::result::Result<ToolResult, ToolExecutionError> {
    let connector_kind = connector_kind(connector);
    let resource_id = response
        .body
        .get("id")
        .or_else(|| response.body.get("iid"))
        .or_else(|| response.body.get("key"))
        .or_else(|| response.body.get("number"))
        .map(|value| value.to_string())
        .unwrap_or_else(|| "-".into());
    Ok(ToolResult {
        summary: format!(
            "executed {}.{} through {} connector",
            action.tool, action.operation, connector_kind
        ),
        values: BTreeMap::from([
            ("connector".into(), RedactedValue::Plain(connector_kind)),
            (
                "method".into(),
                RedactedValue::Plain(request.method.clone()),
            ),
            (
                "url".into(),
                RedactedValue::Plain(redact_secret_like_text(&request.url)),
            ),
            (
                "status".into(),
                RedactedValue::Plain(response.status.to_string()),
            ),
            (
                "resource_id".into(),
                RedactedValue::Plain(redact_secret_like_text(&resource_id)),
            ),
            (
                "response".into(),
                RedactedValue::Plain(redact_secret_like_text(&response.body.to_string())),
            ),
        ]),
        artifacts: Vec::new(),
        usage: Vec::new(),
    })
}

fn enterprise_message_request(
    api_base: &str,
    token: &str,
    tenant: &str,
    tool: &str,
    action: &ToolAction,
) -> std::result::Result<EnterpriseHttpRequest, ToolExecutionError> {
    match (action.tool.as_str(), action.operation.as_str()) {
        (_, "send_message") if action.tool == tool => {
            let channel = safe_connector_key_parameter(action, "channel")?;
            let body = serde_json::json!({
                "tenant": tenant,
                "channel": channel,
                "text": bounded_plain_parameter(action, "text", 16 * 1024)?,
            });
            Ok(json_request(
                "POST",
                &format!("{api_base}/{tool}/messages"),
                token,
                body,
            ))
        }
        ("feishu", "create_doc") if tool == "feishu" => {
            let body = serde_json::json!({
                "app": tenant,
                "title": bounded_plain_parameter(action, "title", 512)?,
                "content": bounded_plain_parameter(action, "content", 128 * 1024)?,
            });
            Ok(json_request(
                "POST",
                &format!("{api_base}/feishu/docs"),
                token,
                body,
            ))
        }
        _ => unsupported_connector_action(tool, action),
    }
}

fn json_request(
    method: &str,
    url: &str,
    token: &str,
    body: serde_json::Value,
) -> EnterpriseHttpRequest {
    EnterpriseHttpRequest {
        method: method.into(),
        url: url.into(),
        bearer_token: token.into(),
        body,
    }
}

fn unsupported_connector_action<T>(
    connector: &str,
    action: &ToolAction,
) -> std::result::Result<T, ToolExecutionError> {
    Err(ToolExecutionError {
        kind: ToolExecutionErrorKind::UnsupportedTool,
        message: format!(
            "{connector} connector does not support {}.{}",
            action.tool, action.operation
        ),
    })
}

fn execute_github_read_issue(
    path: &camino::Utf8Path,
    action: &ToolAction,
) -> std::result::Result<ToolResult, ToolExecutionError> {
    let fixture = read_fixture_json(path)?;
    let repository = fixture
        .get("repository")
        .and_then(serde_json::Value::as_str)
        .unwrap_or("unknown/repository")
        .to_owned();
    let repository = redact_secret_like_text(&repository);
    let number = plain_u64_parameter(action, "number")?;
    let issue = fixture
        .get("issues")
        .and_then(serde_json::Value::as_array)
        .and_then(|issues| {
            issues.iter().find(|issue| {
                issue.get("number").and_then(serde_json::Value::as_u64) == Some(number)
            })
        })
        .ok_or_else(|| ToolExecutionError {
            kind: ToolExecutionErrorKind::InvalidParameters,
            message: format!("fixture issue number {number} was not found"),
        })?;
    let title = redact_secret_like_text(&json_string_field(issue, "title"));
    let state = redact_secret_like_text(&json_string_field(issue, "state"));
    let body = redact_secret_like_text(&json_string_field(issue, "body"));

    Ok(ToolResult {
        summary: format!("read fixture issue #{number} from {repository}"),
        values: BTreeMap::from([
            ("repository".into(), RedactedValue::Plain(repository)),
            ("number".into(), RedactedValue::Plain(number.to_string())),
            ("title".into(), RedactedValue::Plain(title)),
            ("state".into(), RedactedValue::Plain(state)),
            ("body".into(), RedactedValue::Plain(body)),
        ]),
        artifacts: Vec::new(),
        usage: Vec::new(),
    })
}

fn execute_github_create_pr(
    path: &camino::Utf8Path,
    action: &ToolAction,
    context: &ToolExecutionContext,
) -> std::result::Result<ToolResult, ToolExecutionError> {
    let fixture = read_fixture_json(path)?;
    let repository = fixture
        .get("repository")
        .and_then(serde_json::Value::as_str)
        .unwrap_or("unknown/repository")
        .to_owned();
    let repository = redact_secret_like_text(&repository);
    let base = plain_optional_parameter(action, "base").unwrap_or_else(|| {
        fixture
            .get("default_branch")
            .and_then(serde_json::Value::as_str)
            .unwrap_or("main")
            .to_owned()
    });
    let head = plain_optional_parameter(action, "head")
        .unwrap_or_else(|| "taskfence/local-fixture".into());
    let base = redact_secret_like_text(&base);
    let head = redact_secret_like_text(&head);
    let title = redact_secret_like_text(plain_required_parameter(action, "title")?);
    let body =
        redact_secret_like_text(&plain_optional_parameter(action, "body").unwrap_or_default());
    let artifact_dir = context
        .artifact_dir
        .as_ref()
        .ok_or_else(|| ToolExecutionError {
            kind: ToolExecutionErrorKind::AdapterFailed,
            message: "local fixture create_pr requires an artifact directory".into(),
        })?;
    fs::create_dir_all(artifact_dir.as_std_path()).map_err(|err| ToolExecutionError {
        kind: ToolExecutionErrorKind::AdapterFailed,
        message: format!("failed to create fixture artifact directory {artifact_dir}: {err}"),
    })?;
    let path = artifact_dir.join("github-pr-proposal.json");
    let proposal = serde_json::json!({
        "fixture": true,
        "provider": "github",
        "repository": repository,
        "title": title,
        "body": body,
        "base": base,
        "head": head,
    });
    let bytes = serde_json::to_vec_pretty(&proposal).map_err(|err| ToolExecutionError {
        kind: ToolExecutionErrorKind::AdapterFailed,
        message: format!("failed to serialize fixture PR proposal: {err}"),
    })?;
    fs::write(path.as_std_path(), bytes).map_err(|err| ToolExecutionError {
        kind: ToolExecutionErrorKind::AdapterFailed,
        message: format!("failed to write fixture PR proposal {path}: {err}"),
    })?;

    Ok(ToolResult {
        summary: format!("wrote local GitHub PR proposal for {repository}"),
        values: BTreeMap::from([
            ("repository".into(), RedactedValue::Plain(repository)),
            ("title".into(), RedactedValue::Plain(title)),
            ("base".into(), RedactedValue::Plain(base)),
            ("head".into(), RedactedValue::Plain(head)),
        ]),
        artifacts: vec![path],
        usage: Vec::new(),
    })
}

fn read_fixture_json(
    path: &camino::Utf8Path,
) -> std::result::Result<serde_json::Value, ToolExecutionError> {
    let contents = fs::read_to_string(path.as_std_path()).map_err(|err| ToolExecutionError {
        kind: ToolExecutionErrorKind::AdapterFailed,
        message: format!("failed to read fixture {path}: {err}"),
    })?;
    serde_json::from_str(&contents).map_err(|err| ToolExecutionError {
        kind: ToolExecutionErrorKind::AdapterFailed,
        message: format!("failed to parse fixture {path}: {err}"),
    })
}

fn plain_u64_parameter(
    action: &ToolAction,
    key: &str,
) -> std::result::Result<u64, ToolExecutionError> {
    let value = plain_required_parameter(action, key)?;
    value.parse::<u64>().map_err(|err| ToolExecutionError {
        kind: ToolExecutionErrorKind::InvalidParameters,
        message: format!("parameter {key} must be an integer: {err}"),
    })
}

fn optional_u32_parameter(
    action: &ToolAction,
    key: &str,
) -> std::result::Result<Option<u32>, ToolExecutionError> {
    plain_optional_parameter(action, key)
        .map(|value| {
            value.parse::<u32>().map_err(|err| ToolExecutionError {
                kind: ToolExecutionErrorKind::InvalidParameters,
                message: format!("parameter {key} must be an integer: {err}"),
            })
        })
        .transpose()
}

fn plain_required_parameter<'a>(
    action: &'a ToolAction,
    key: &str,
) -> std::result::Result<&'a str, ToolExecutionError> {
    match action.parameters.get(key) {
        Some(RedactedValue::Plain(value)) if !value.trim().is_empty() => Ok(value.trim()),
        Some(RedactedValue::Plain(_)) => Err(ToolExecutionError {
            kind: ToolExecutionErrorKind::InvalidParameters,
            message: format!("parameter {key} must not be empty"),
        }),
        Some(RedactedValue::Redacted { .. }) => Err(ToolExecutionError {
            kind: ToolExecutionErrorKind::InvalidParameters,
            message: format!("parameter {key} cannot be redacted"),
        }),
        None => Err(ToolExecutionError {
            kind: ToolExecutionErrorKind::InvalidParameters,
            message: format!("missing required parameter {key}"),
        }),
    }
}

fn plain_optional_parameter(action: &ToolAction, key: &str) -> Option<String> {
    match action.parameters.get(key) {
        Some(RedactedValue::Plain(value)) if !value.trim().is_empty() => {
            Some(value.trim().to_owned())
        }
        _ => None,
    }
}

fn bounded_plain_parameter(
    action: &ToolAction,
    key: &str,
    max_bytes: usize,
) -> std::result::Result<String, ToolExecutionError> {
    let value = plain_required_parameter(action, key)?;
    if value.len() > max_bytes {
        return Err(ToolExecutionError {
            kind: ToolExecutionErrorKind::InvalidParameters,
            message: format!("parameter {key} exceeds {max_bytes} bytes"),
        });
    }
    Ok(value.to_owned())
}

fn optional_bounded_plain_parameter(
    action: &ToolAction,
    key: &str,
    max_bytes: usize,
) -> std::result::Result<Option<String>, ToolExecutionError> {
    plain_optional_parameter(action, key)
        .map(|value| {
            if value.len() > max_bytes {
                Err(ToolExecutionError {
                    kind: ToolExecutionErrorKind::InvalidParameters,
                    message: format!("parameter {key} exceeds {max_bytes} bytes"),
                })
            } else {
                Ok(value)
            }
        })
        .transpose()
}

fn safe_git_ref_parameter(
    action: &ToolAction,
    key: &str,
) -> std::result::Result<String, ToolExecutionError> {
    validate_safe_git_ref(key, plain_required_parameter(action, key)?)
}

fn optional_safe_git_ref_parameter(
    action: &ToolAction,
    key: &str,
) -> std::result::Result<Option<String>, ToolExecutionError> {
    plain_optional_parameter(action, key)
        .map(|value| validate_safe_git_ref(key, &value))
        .transpose()
}

fn validate_safe_git_ref(
    key: &str,
    value: &str,
) -> std::result::Result<String, ToolExecutionError> {
    let value = value.trim();
    if value.is_empty()
        || value.starts_with('/')
        || value.ends_with('/')
        || value.starts_with('-')
        || value.contains("..")
        || value.contains('@')
        || value.contains('\\')
        || value.chars().any(char::is_whitespace)
        || value
            .split('/')
            .any(|segment| segment.is_empty() || segment == "." || segment == "..")
        || !value
            .chars()
            .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '.' | '_' | '-' | '/'))
    {
        return Err(ToolExecutionError {
            kind: ToolExecutionErrorKind::InvalidParameters,
            message: format!("parameter {key} must be a safe Git ref"),
        });
    }
    Ok(value.to_owned())
}

fn safe_github_file_path_parameter(
    action: &ToolAction,
    key: &str,
) -> std::result::Result<String, ToolExecutionError> {
    let value = plain_required_parameter(action, key)?;
    if value.starts_with('/')
        || value.contains('\\')
        || value.contains('\0')
        || value.contains("..")
        || value.chars().any(char::is_whitespace)
        || value.split('/').any(|segment| {
            segment.is_empty()
                || segment == "."
                || segment == ".."
                || segment.chars().any(char::is_control)
        })
        || !value
            .chars()
            .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '.' | '_' | '-' | '/'))
    {
        return Err(ToolExecutionError {
            kind: ToolExecutionErrorKind::InvalidParameters,
            message: format!("parameter {key} must be a safe repository-relative file path"),
        });
    }
    Ok(value.to_owned())
}

fn optional_safe_sha_parameter(
    action: &ToolAction,
    key: &str,
) -> std::result::Result<Option<String>, ToolExecutionError> {
    plain_optional_parameter(action, key)
        .map(|value| {
            if value.len() < 7
                || value.len() > 64
                || !value.chars().all(|ch| ch.is_ascii_hexdigit())
            {
                Err(ToolExecutionError {
                    kind: ToolExecutionErrorKind::InvalidParameters,
                    message: format!("parameter {key} must be a Git object sha"),
                })
            } else {
                Ok(value)
            }
        })
        .transpose()
}

fn optional_pr_state_parameter(
    action: &ToolAction,
    key: &str,
) -> std::result::Result<Option<String>, ToolExecutionError> {
    plain_optional_parameter(action, key)
        .map(|value| {
            let normalized = value.to_ascii_lowercase();
            if matches!(normalized.as_str(), "open" | "closed") {
                Ok(normalized)
            } else {
                Err(ToolExecutionError {
                    kind: ToolExecutionErrorKind::InvalidParameters,
                    message: format!("parameter {key} must be open or closed"),
                })
            }
        })
        .transpose()
}

fn safe_connector_key_parameter(
    action: &ToolAction,
    key: &str,
) -> std::result::Result<String, ToolExecutionError> {
    let value = plain_required_parameter(action, key)?;
    if value.is_empty()
        || value.contains("..")
        || value.contains('/')
        || value.contains('\\')
        || value.chars().any(char::is_whitespace)
        || !value
            .chars()
            .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '.' | '_' | '-'))
    {
        return Err(ToolExecutionError {
            kind: ToolExecutionErrorKind::InvalidParameters,
            message: format!("parameter {key} must be a safe connector identifier"),
        });
    }
    Ok(value.to_owned())
}

fn safe_http_path_parameter(
    action: &ToolAction,
    key: &str,
) -> std::result::Result<String, ToolExecutionError> {
    let value = plain_required_parameter(action, key)?.trim_matches('/');
    if value.is_empty()
        || value.contains("..")
        || value.contains('\\')
        || value.contains('?')
        || value.contains('#')
        || value.chars().any(char::is_whitespace)
        || value.split('/').any(|segment| {
            segment.is_empty()
                || !segment
                    .chars()
                    .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '.' | '_' | '-'))
        })
    {
        return Err(ToolExecutionError {
            kind: ToolExecutionErrorKind::InvalidParameters,
            message: format!("parameter {key} must be a safe relative HTTP path"),
        });
    }
    Ok(value.to_owned())
}

fn optional_http_method_parameter(
    action: &ToolAction,
    key: &str,
) -> std::result::Result<Option<String>, ToolExecutionError> {
    plain_optional_parameter(action, key)
        .map(|value| {
            let method = value.to_ascii_uppercase();
            if matches!(method.as_str(), "GET" | "POST" | "PATCH") {
                Ok(method)
            } else {
                Err(ToolExecutionError {
                    kind: ToolExecutionErrorKind::InvalidParameters,
                    message: format!("parameter {key} must be GET, POST, or PATCH"),
                })
            }
        })
        .transpose()
}

fn validate_sql_statement(
    sql: &str,
    read_only: bool,
) -> std::result::Result<(), ToolExecutionError> {
    let trimmed = sql.trim();
    let lower = trimmed.to_ascii_lowercase();
    if trimmed.is_empty()
        || trimmed.contains('\0')
        || trimmed.contains("--")
        || trimmed.contains("/*")
        || trimmed.contains("*/")
        || trimmed.matches(';').count() > usize::from(trimmed.ends_with(';'))
    {
        return Err(ToolExecutionError {
            kind: ToolExecutionErrorKind::InvalidParameters,
            message: "database query must be one bounded statement without comments".into(),
        });
    }
    if read_only {
        if !(lower.starts_with("select ") || lower.starts_with("with ")) {
            return Err(ToolExecutionError {
                kind: ToolExecutionErrorKind::InvalidParameters,
                message: "database.read supports only SELECT or WITH statements".into(),
            });
        }
        for forbidden in [
            " insert ",
            " update ",
            " delete ",
            " drop ",
            " alter ",
            " create ",
            " truncate ",
            " grant ",
            " revoke ",
            " copy ",
        ] {
            if lower.contains(forbidden) {
                return Err(ToolExecutionError {
                    kind: ToolExecutionErrorKind::InvalidParameters,
                    message: "database.read query contains a mutating keyword".into(),
                });
            }
        }
    } else if lower.starts_with("drop ")
        || lower.starts_with("alter ")
        || lower.starts_with("truncate ")
        || lower.starts_with("grant ")
        || lower.starts_with("revoke ")
        || lower.starts_with("copy ")
    {
        return Err(ToolExecutionError {
            kind: ToolExecutionErrorKind::InvalidParameters,
            message: "database.write does not allow schema or privilege operations".into(),
        });
    }
    Ok(())
}

fn path_encode(value: &str) -> String {
    value.replace('/', "%2F")
}

fn jira_doc_text(text: String) -> serde_json::Value {
    serde_json::json!({
        "type": "doc",
        "version": 1,
        "content": [{
            "type": "paragraph",
            "content": [{
                "type": "text",
                "text": redact_secret_like_text(&text),
            }],
        }],
    })
}

fn report_comment_body(action: &ToolAction) -> std::result::Result<String, ToolExecutionError> {
    let title =
        plain_optional_parameter(action, "title").unwrap_or_else(|| "TaskFence report".into());
    let status = plain_optional_parameter(action, "status").unwrap_or_else(|| "unknown".into());
    let report_url = plain_optional_parameter(action, "report_url");
    let summary = optional_bounded_plain_parameter(action, "summary", 16 * 1024)?
        .unwrap_or_else(|| "No summary provided.".into());
    let mut body = format!(
        "## {}\n\nStatus: `{}`\n\n{}",
        redact_secret_like_text(&title),
        redact_secret_like_text(&status),
        redact_secret_like_text(&summary)
    );
    if let Some(report_url) = report_url {
        validate_report_url(&report_url)?;
        body.push_str("\n\nReport: ");
        body.push_str(&redact_secret_like_text(&report_url));
    }
    Ok(body)
}

fn validate_report_url(value: &str) -> std::result::Result<(), ToolExecutionError> {
    let lower = value.to_ascii_lowercase();
    if !(lower.starts_with("https://") || lower.starts_with("http://"))
        || value.contains('@')
        || value.contains('#')
        || value.chars().any(char::is_whitespace)
        || secret_like_url(value)
    {
        return Err(ToolExecutionError {
            kind: ToolExecutionErrorKind::InvalidParameters,
            message: "report_url must be a non-secret http(s) URL without userinfo, fragments, or whitespace".into(),
        });
    }
    Ok(())
}

fn json_string_field(value: &serde_json::Value, key: &str) -> String {
    value
        .get(key)
        .and_then(serde_json::Value::as_str)
        .unwrap_or("")
        .to_owned()
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

impl<'a> GatewayMediator<'a> {
    pub fn new(policy: &'a dyn PolicyEngine, audit: &'a dyn AuditLogger) -> Self {
        Self {
            policy,
            audit,
            approval: None,
            registry: None,
            supported_protocols: BTreeSet::from(["mcp".into()]),
        }
    }

    pub fn with_approval(mut self, approval: &'a dyn ApprovalEngine) -> Self {
        self.approval = Some(approval);
        self
    }

    pub fn with_tool_registry(mut self, registry: &'a dyn ToolRegistry) -> Self {
        self.registry = Some(registry);
        self
    }

    pub fn with_supported_protocols<I, S>(mut self, protocols: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        self.supported_protocols = protocols
            .into_iter()
            .map(|protocol| protocol.into().trim().to_ascii_lowercase())
            .filter(|protocol| !protocol.is_empty())
            .collect();
        self
    }

    pub fn mediate_tool_action(
        &self,
        task: &ResolvedTask,
        action: ToolAction,
    ) -> taskfence_core::Result<GatewayMediation> {
        let action = normalize_tool_action(action)?;
        if !self.supported_protocols.contains(&action.protocol) {
            let message = format!("gateway protocol '{}' is not supported", action.protocol);
            self.audit.record(AuditEvent::Error {
                task_id: task.id.clone(),
                at: OffsetDateTime::now_utc(),
                message: message.clone(),
            })?;
            return Err(TaskFenceError::Unsupported(message));
        }

        if let Some(registry) = self.registry {
            if !registry.contains(&action)? {
                let key = ToolKey::from_action(&action)?;
                let message = format!(
                    "gateway tool action is not registered: {}",
                    key.display_name()
                );
                self.audit.record(AuditEvent::Error {
                    task_id: task.id.clone(),
                    at: OffsetDateTime::now_utc(),
                    message: message.clone(),
                })?;
                return Err(TaskFenceError::Gateway(message));
            }
        }

        let wrapped = Action::ToolCall(action.clone());
        let decision = self.policy.evaluate(task, &wrapped)?;
        self.audit.record(AuditEvent::PolicyDecision {
            task_id: task.id.clone(),
            at: OffsetDateTime::now_utc(),
            action: wrapped.clone(),
            decision: decision.clone(),
        })?;

        let approval = match decision {
            ActionDecision::Allow { .. } | ActionDecision::Deny { .. } => None,
            ActionDecision::RequireApproval { .. } => match self.approval {
                Some(_) => Some(self.request_tool_approval(task, wrapped, decision.clone())?),
                // Policy-only mediation is still useful for evidence and compatibility.
                None => None,
            },
        };

        Ok(GatewayMediation {
            action,
            decision,
            approval,
        })
    }

    fn request_tool_approval(
        &self,
        task: &ResolvedTask,
        action: Action,
        decision: ActionDecision,
    ) -> taskfence_core::Result<ApprovalRecord> {
        let approval = self.approval.ok_or_else(|| {
            TaskFenceError::Approval("gateway approval engine is not configured".into())
        })?;
        let requested = approval.request(task, action, decision)?;
        self.audit.record(AuditEvent::ApprovalRequested {
            record: requested.clone(),
        })?;
        let resolved = approval.wait(&requested.id)?;
        self.audit.record(AuditEvent::ApprovalResolved {
            record: resolved.clone(),
        })?;

        match resolved.decision {
            Some(ApprovalDecision::Approved) => Ok(resolved),
            Some(ApprovalDecision::Denied) | Some(ApprovalDecision::TimedOut) | None => Err(
                TaskFenceError::Approval("gateway tool approval denied or timed out".into()),
            ),
        }
    }
}

pub fn normalize_tool_action(action: ToolAction) -> taskfence_core::Result<ToolAction> {
    let protocol = normalize_required_segment("protocol", action.protocol)?;
    let tool = normalize_required_segment("tool", action.tool)?;
    let operation = normalize_required_segment("operation", action.operation)?;
    let parameters = normalize_parameters(action.parameters)?;

    Ok(ToolAction {
        protocol,
        tool,
        operation,
        parameters,
    })
}

pub fn gateway_secret_reference(
    task: &ResolvedTask,
    broker: &dyn SecretBroker,
    name: impl Into<String>,
    scope: impl Into<String>,
) -> taskfence_core::Result<SecretReference> {
    let name = normalize_required_segment("secret name", name.into())?;
    let scope = normalize_required_segment("secret scope", scope.into())?;
    ensure_secret_grant(task, &name, &scope)?;
    broker.issue_reference(task, &name, &scope)
}

pub fn attach_secret_reference(
    action: ToolAction,
    parameter_name: impl Into<String>,
    reference: &SecretReference,
) -> taskfence_core::Result<ToolAction> {
    let mut action = normalize_tool_action(action)?;
    let parameter_name = parameter_name.into().trim().to_owned();
    if parameter_name.is_empty() {
        return Err(TaskFenceError::Gateway(
            "secret reference parameter name must not be empty".into(),
        ));
    }
    action
        .parameters
        .insert(parameter_name, reference.as_redacted_value());
    Ok(action)
}

fn ensure_secret_grant(task: &ResolvedTask, name: &str, scope: &str) -> taskfence_core::Result<()> {
    if task.secrets.expose_to_agent {
        return Err(TaskFenceError::Gateway(
            "gateway secret references require secrets to stay out of the agent".into(),
        ));
    }

    if task
        .secrets
        .available_to_gateway
        .iter()
        .any(|grant| grant.name == name && grant.use_for.iter().any(|allowed| allowed == scope))
    {
        Ok(())
    } else {
        Err(TaskFenceError::Gateway(format!(
            "secret {name} is not available to gateway scope {scope}"
        )))
    }
}

fn normalize_required_segment(name: &str, value: String) -> taskfence_core::Result<String> {
    let normalized = value.trim().to_ascii_lowercase();
    if normalized.is_empty() {
        return Err(TaskFenceError::Gateway(format!(
            "tool action {name} must not be empty"
        )));
    }
    Ok(normalized)
}

fn normalize_parameters(
    parameters: BTreeMap<String, RedactedValue>,
) -> taskfence_core::Result<BTreeMap<String, RedactedValue>> {
    let mut normalized = BTreeMap::new();
    for (key, value) in parameters {
        let key = key.trim().to_owned();
        if key.is_empty() {
            return Err(TaskFenceError::Gateway(
                "tool action parameter names must not be empty".into(),
            ));
        }
        normalized.insert(key, value);
    }
    Ok(normalized)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Arc, Mutex};
    use taskfence_core::{
        AgentConfig, AgentKind, ApprovalConfig, ApprovalId, AuditConfig, BudgetLimit,
        BudgetPermissions, LimitConfig, PermissionConfig, SandboxConfig, SandboxKind, SecretConfig,
        SecretGrant, TaskId, ToolPermissions,
    };
    use taskfence_policy::BuiltInPolicyEngine;

    #[derive(Debug)]
    struct StaticPolicy {
        decision: ActionDecision,
        seen_actions: Mutex<Vec<Action>>,
    }

    impl StaticPolicy {
        fn new(decision: ActionDecision) -> Self {
            Self {
                decision,
                seen_actions: Mutex::new(Vec::new()),
            }
        }
    }

    impl PolicyEngine for StaticPolicy {
        fn evaluate(
            &self,
            _task: &ResolvedTask,
            action: &Action,
        ) -> taskfence_core::Result<ActionDecision> {
            self.seen_actions.lock().unwrap().push(action.clone());
            Ok(self.decision.clone())
        }
    }

    #[derive(Debug, Default)]
    struct RecordingAudit {
        events: Mutex<Vec<AuditEvent>>,
    }

    impl AuditLogger for RecordingAudit {
        fn record(&self, event: AuditEvent) -> taskfence_core::Result<()> {
            self.events.lock().unwrap().push(event);
            Ok(())
        }
    }

    #[derive(Debug)]
    struct StaticApproval {
        decision: ApprovalDecision,
        requested: Mutex<Vec<ApprovalRecord>>,
    }

    impl StaticApproval {
        fn new(decision: ApprovalDecision) -> Self {
            Self {
                decision,
                requested: Mutex::new(Vec::new()),
            }
        }
    }

    impl ApprovalEngine for StaticApproval {
        fn request(
            &self,
            task: &ResolvedTask,
            action: Action,
            policy_decision: ActionDecision,
        ) -> taskfence_core::Result<ApprovalRecord> {
            let record = ApprovalRecord {
                id: ApprovalId("approval-tool-1".into()),
                task_id: task.id.clone(),
                actor: "gateway-test".into(),
                source: Some("gateway".into()),
                requested_at: OffsetDateTime::now_utc(),
                resolved_at: None,
                action,
                policy_decision,
                decision: None,
            };
            self.requested.lock().unwrap().push(record.clone());
            Ok(record)
        }

        fn wait(&self, approval_id: &ApprovalId) -> taskfence_core::Result<ApprovalRecord> {
            let mut record = self.requested.lock().unwrap()[0].clone();
            record.id = approval_id.clone();
            record.resolved_at = Some(OffsetDateTime::now_utc());
            record.decision = Some(self.decision.clone());
            Ok(record)
        }
    }

    #[derive(Debug, Default)]
    struct StaticSecretBroker {
        issued: Mutex<Vec<(String, String)>>,
    }

    impl SecretBroker for StaticSecretBroker {
        fn issue_reference(
            &self,
            task: &ResolvedTask,
            name: &str,
            scope: &str,
        ) -> taskfence_core::Result<SecretReference> {
            self.issued
                .lock()
                .unwrap()
                .push((name.into(), scope.into()));
            Ok(SecretReference {
                name: name.into(),
                scope: scope.into(),
                handle: format!("taskfence://{}/{name}/{scope}", task.id.0),
            })
        }
    }

    #[derive(Debug)]
    struct StaticLiveSecretBroker {
        issued: Mutex<Vec<(String, String)>>,
        token: String,
    }

    impl StaticLiveSecretBroker {
        fn new(token: impl Into<String>) -> Self {
            Self {
                issued: Mutex::new(Vec::new()),
                token: token.into(),
            }
        }
    }

    impl SecretBroker for StaticLiveSecretBroker {
        fn issue_reference(
            &self,
            _task: &ResolvedTask,
            name: &str,
            scope: &str,
        ) -> taskfence_core::Result<SecretReference> {
            self.issued
                .lock()
                .unwrap()
                .push((name.into(), scope.into()));
            Ok(SecretReference {
                name: name.into(),
                scope: scope.into(),
                handle: self.token.clone(),
            })
        }
    }

    #[derive(Clone, Debug, PartialEq, Eq)]
    enum GitHubClientCall {
        ReadIssue {
            api_base: String,
            repository: String,
            token: String,
            number: u64,
        },
        CreatePullRequest {
            api_base: String,
            repository: String,
            token: String,
            input: GitHubPullRequestInput,
        },
        CreateIssueComment {
            api_base: String,
            repository: String,
            token: String,
            number: u64,
            body: String,
        },
        CreateBranch {
            api_base: String,
            repository: String,
            token: String,
            branch: String,
            from_ref: String,
        },
        CommitFile {
            api_base: String,
            repository: String,
            token: String,
            input: GitHubFileCommitInput,
        },
        UpdatePullRequest {
            api_base: String,
            repository: String,
            token: String,
            input: GitHubPullRequestUpdateInput,
        },
    }

    #[derive(Clone, Debug, Default)]
    struct RecordingGitHubClient {
        calls: Arc<Mutex<Vec<GitHubClientCall>>>,
    }

    impl GitHubApiClient for RecordingGitHubClient {
        fn read_issue(
            &self,
            api_base: &str,
            repository: &str,
            token: &str,
            number: u64,
        ) -> std::result::Result<GitHubIssue, ToolExecutionError> {
            self.calls
                .lock()
                .unwrap()
                .push(GitHubClientCall::ReadIssue {
                    api_base: api_base.into(),
                    repository: repository.into(),
                    token: token.into(),
                    number,
                });
            Ok(GitHubIssue {
                number,
                title: "Needs review".into(),
                state: "open".into(),
                body: "No credentials here".into(),
                html_url: format!("https://github.example/{repository}/issues/{number}"),
            })
        }

        fn create_pull_request(
            &self,
            api_base: &str,
            repository: &str,
            token: &str,
            input: GitHubPullRequestInput,
        ) -> std::result::Result<GitHubPullRequest, ToolExecutionError> {
            self.calls
                .lock()
                .unwrap()
                .push(GitHubClientCall::CreatePullRequest {
                    api_base: api_base.into(),
                    repository: repository.into(),
                    token: token.into(),
                    input,
                });
            Ok(GitHubPullRequest {
                number: 7,
                title: "Ship bounded connector".into(),
                state: "open".into(),
                html_url: format!("https://github.example/{repository}/pull/7"),
            })
        }

        fn create_issue_comment(
            &self,
            api_base: &str,
            repository: &str,
            token: &str,
            number: u64,
            body: String,
        ) -> std::result::Result<GitHubIssueComment, ToolExecutionError> {
            self.calls
                .lock()
                .unwrap()
                .push(GitHubClientCall::CreateIssueComment {
                    api_base: api_base.into(),
                    repository: repository.into(),
                    token: token.into(),
                    number,
                    body,
                });
            Ok(GitHubIssueComment {
                id: 99,
                body: "Comment recorded".into(),
                html_url: format!("https://github.example/{repository}/issues/{number}#comment-99"),
            })
        }

        fn create_branch(
            &self,
            api_base: &str,
            repository: &str,
            token: &str,
            branch: &str,
            from_ref: &str,
        ) -> std::result::Result<GitHubBranch, ToolExecutionError> {
            self.calls
                .lock()
                .unwrap()
                .push(GitHubClientCall::CreateBranch {
                    api_base: api_base.into(),
                    repository: repository.into(),
                    token: token.into(),
                    branch: branch.into(),
                    from_ref: from_ref.into(),
                });
            Ok(GitHubBranch {
                name: branch.into(),
                sha: "abc123def456".into(),
            })
        }

        fn commit_file(
            &self,
            api_base: &str,
            repository: &str,
            token: &str,
            input: GitHubFileCommitInput,
        ) -> std::result::Result<GitHubFileCommit, ToolExecutionError> {
            self.calls
                .lock()
                .unwrap()
                .push(GitHubClientCall::CommitFile {
                    api_base: api_base.into(),
                    repository: repository.into(),
                    token: token.into(),
                    input: input.clone(),
                });
            Ok(GitHubFileCommit {
                path: input.path,
                sha: "feedface1234".into(),
                html_url: format!(
                    "https://github.example/{repository}/blob/{}/README.md",
                    input.branch
                ),
            })
        }

        fn update_pull_request(
            &self,
            api_base: &str,
            repository: &str,
            token: &str,
            input: GitHubPullRequestUpdateInput,
        ) -> std::result::Result<GitHubPullRequest, ToolExecutionError> {
            self.calls
                .lock()
                .unwrap()
                .push(GitHubClientCall::UpdatePullRequest {
                    api_base: api_base.into(),
                    repository: repository.into(),
                    token: token.into(),
                    input: input.clone(),
                });
            Ok(GitHubPullRequest {
                number: input.number,
                title: input.title.unwrap_or_else(|| "Updated PR".into()),
                state: input.state.unwrap_or_else(|| "open".into()),
                html_url: format!("https://github.example/{repository}/pull/{}", input.number),
            })
        }
    }

    #[derive(Clone, Debug, Default)]
    struct RecordingEgressClient {
        calls: Arc<Mutex<Vec<GatewayEgressRequest>>>,
    }

    impl GatewayEgressClient for RecordingEgressClient {
        fn fetch(
            &self,
            request: &GatewayEgressRequest,
        ) -> std::result::Result<GatewayEgressResponse, ToolExecutionError> {
            self.calls.lock().unwrap().push(request.clone());
            Ok(GatewayEgressResponse {
                status: 200,
                body: "ok token=secret-value".into(),
            })
        }
    }

    #[derive(Clone, Debug, Default)]
    struct RecordingEnterpriseHttpClient {
        calls: Arc<Mutex<Vec<EnterpriseHttpRequest>>>,
    }

    impl EnterpriseHttpClient for RecordingEnterpriseHttpClient {
        fn execute(
            &self,
            request: &EnterpriseHttpRequest,
        ) -> std::result::Result<EnterpriseHttpResponse, ToolExecutionError> {
            self.calls.lock().unwrap().push(request.clone());
            Ok(EnterpriseHttpResponse {
                status: 200,
                body: serde_json::json!({
                    "id": 123,
                    "title": "created token=secret-value",
                    "url": "https://enterprise.example/result",
                }),
            })
        }
    }

    #[derive(Clone, Debug, Default)]
    struct RecordingDatabaseClient {
        calls: Arc<Mutex<Vec<(String, DatabaseStatement)>>>,
    }

    impl DatabaseConnectorClient for RecordingDatabaseClient {
        fn execute(
            &self,
            credential: &str,
            statement: DatabaseStatement,
        ) -> std::result::Result<DatabaseStatementResult, ToolExecutionError> {
            self.calls
                .lock()
                .unwrap()
                .push((credential.into(), statement));
            Ok(DatabaseStatementResult {
                rows_affected: 1,
                rows: vec![BTreeMap::from([(
                    "status".into(),
                    "ok token=secret-value".into(),
                )])],
            })
        }
    }

    #[derive(Debug)]
    struct StaticToolAdapter {
        outcome: Mutex<std::result::Result<ToolResult, ToolExecutionError>>,
        planned_usage: Mutex<std::result::Result<Vec<BudgetUsage>, ToolExecutionError>>,
        calls: Mutex<Vec<ToolAction>>,
        secret_refs: Vec<GatewaySecretReferenceConfig>,
    }

    impl StaticToolAdapter {
        fn succeeding(summary: &str) -> Self {
            Self {
                outcome: Mutex::new(Ok(ToolResult {
                    summary: summary.into(),
                    values: BTreeMap::from([(
                        "status".into(),
                        RedactedValue::Plain("fixture-ok".into()),
                    )]),
                    artifacts: Vec::new(),
                    usage: Vec::new(),
                })),
                planned_usage: Mutex::new(Ok(Vec::new())),
                calls: Mutex::new(Vec::new()),
                secret_refs: Vec::new(),
            }
        }

        fn failing(message: &str) -> Self {
            Self {
                outcome: Mutex::new(Err(ToolExecutionError {
                    kind: ToolExecutionErrorKind::AdapterFailed,
                    message: message.into(),
                })),
                planned_usage: Mutex::new(Ok(Vec::new())),
                calls: Mutex::new(Vec::new()),
                secret_refs: Vec::new(),
            }
        }

        fn with_planned_usage(mut self, usage: Vec<BudgetUsage>) -> Self {
            self.planned_usage = Mutex::new(Ok(usage));
            self
        }

        fn with_result_usage(self, usage: Vec<BudgetUsage>) -> Self {
            if let Ok(result) = self.outcome.lock().unwrap().as_mut() {
                result.usage = usage;
            }
            self
        }

        fn with_secret_ref(mut self, secret_ref: GatewaySecretReferenceConfig) -> Self {
            self.secret_refs.push(secret_ref);
            self
        }

        fn call_count(&self) -> usize {
            self.calls.lock().unwrap().len()
        }
    }

    impl ToolAdapter for StaticToolAdapter {
        fn identity(&self) -> ToolAdapterIdentity {
            ToolAdapterIdentity {
                kind: "local_fixture".into(),
                name: "static-test".into(),
            }
        }

        fn secret_references(&self) -> &[GatewaySecretReferenceConfig] {
            &self.secret_refs
        }

        fn planned_budget_usage(
            &self,
            _task: &ResolvedTask,
            _action: &ToolAction,
            _context: &ToolExecutionContext,
        ) -> std::result::Result<Vec<BudgetUsage>, ToolExecutionError> {
            self.planned_usage.lock().unwrap().clone()
        }

        fn execute(
            &self,
            _task: &ResolvedTask,
            action: &ToolAction,
            _context: &ToolExecutionContext,
        ) -> std::result::Result<ToolResult, ToolExecutionError> {
            self.calls.lock().unwrap().push(action.clone());
            self.outcome.lock().unwrap().clone()
        }
    }

    fn allow() -> ActionDecision {
        ActionDecision::Allow {
            rule_id: Some("tools.allow".into()),
            reason: "allowed by test".into(),
        }
    }

    fn deny() -> ActionDecision {
        ActionDecision::Deny {
            rule_id: Some("tools.deny".into()),
            reason: "denied by test".into(),
        }
    }

    fn task() -> ResolvedTask {
        ResolvedTask {
            id: TaskId("task-1".into()),
            task_file: "/tmp/task.yaml".into(),
            goal: "test gateway".into(),
            workspace_host_path: "/tmp/repo".into(),
            workspace_container_path: "/workspace".into(),
            agent: AgentConfig {
                kind: AgentKind::Generic,
                command: "codex".into(),
                args: Vec::new(),
            },
            sandbox: SandboxConfig {
                kind: SandboxKind::Docker,
                image: Some("taskfence/runner:latest".into()),
                ssh: None,
                limits: LimitConfig::default(),
            },
            permissions: PermissionConfig::default(),
            secrets: SecretConfig::default(),
            approval: ApprovalConfig::default(),
            gateway: Default::default(),
            audit: AuditConfig::default(),
        }
    }

    fn task_with_budget(kind: &str, max_amount: u64) -> ResolvedTask {
        let mut task = task();
        task.permissions.tools.allow = vec!["github.create_pr".into()];
        task.permissions.budget = BudgetPermissions {
            allow: vec![BudgetLimit {
                kind: kind.into(),
                max_amount,
            }],
        };
        task
    }

    fn tool_action(protocol: &str) -> ToolAction {
        ToolAction {
            protocol: protocol.into(),
            tool: " GitHub ".into(),
            operation: " CREATE_PR ".into(),
            parameters: BTreeMap::from([(
                " title ".into(),
                RedactedValue::Plain("ship bounded slice".into()),
            )]),
        }
    }

    #[test]
    fn mcp_adapter_normalizes_request_to_tool_action() {
        let adapter = McpGatewayAdapter;

        let action = adapter
            .to_tool_action(McpToolRequest {
                server: " GitHub ".into(),
                tool: " Create_PR ".into(),
                arguments: BTreeMap::from([(
                    " token ".into(),
                    RedactedValue::Redacted {
                        reason: "test secret reference".into(),
                    },
                )]),
            })
            .unwrap();

        assert_eq!(action.protocol, "mcp");
        assert_eq!(action.tool, "github");
        assert_eq!(action.operation, "create_pr");
        assert!(matches!(
            action.parameters.get("token"),
            Some(RedactedValue::Redacted { reason }) if reason == "test secret reference"
        ));
    }

    #[test]
    fn mcp_adapter_executes_through_gateway_executor() {
        let adapter = McpGatewayAdapter;
        let policy = StaticPolicy::new(allow());
        let audit = RecordingAudit::default();
        let tool_adapter = StaticToolAdapter::succeeding("mcp execution complete");
        let registry =
            InMemoryToolRegistry::new([RegisteredTool::new("mcp", "github", "create_pr").unwrap()]);
        let mediator = GatewayMediator::new(&policy, &audit)
            .with_tool_registry(&registry)
            .with_supported_protocols(["mcp"]);
        let executor = GatewayExecutor::new(mediator, &audit, &tool_adapter);

        let execution = adapter
            .execute(
                &task(),
                McpToolRequest {
                    server: "github".into(),
                    tool: "create_pr".into(),
                    arguments: BTreeMap::from([(
                        "title".into(),
                        RedactedValue::Plain("Ship connector".into()),
                    )]),
                },
                &executor,
                ToolExecutionContext::default(),
            )
            .unwrap();

        assert!(
            execution.error.is_none(),
            "unexpected execution error: {:?}",
            execution.error
        );
        assert!(matches!(
            execution.result,
            Some(ToolResult { summary, .. }) if summary == "mcp execution complete"
        ));
        assert_eq!(tool_adapter.call_count(), 1);
    }

    #[test]
    fn http_adapter_normalizes_request_to_tool_action() {
        let adapter = HttpGatewayAdapter;

        let action = adapter
            .to_tool_action(HttpToolRequest {
                connector: " Linear ".into(),
                operation: " Create_Issue ".into(),
                parameters: BTreeMap::from([(
                    "title".into(),
                    RedactedValue::Plain("Create launch task".into()),
                )]),
            })
            .unwrap();

        assert_eq!(action.protocol, "http");
        assert_eq!(action.tool, "linear");
        assert_eq!(action.operation, "create_issue");
        assert!(matches!(
            action.parameters.get("title"),
            Some(RedactedValue::Plain(title)) if title == "Create launch task"
        ));
    }

    #[test]
    fn http_adapter_executes_through_gateway_executor() {
        let adapter = HttpGatewayAdapter;
        let policy = StaticPolicy::new(allow());
        let audit = RecordingAudit::default();
        let tool_adapter = StaticToolAdapter::succeeding("http execution complete");
        let registry =
            InMemoryToolRegistry::new(
                [RegisteredTool::new("http", "github", "create_pr").unwrap()],
            );
        let mediator = GatewayMediator::new(&policy, &audit)
            .with_tool_registry(&registry)
            .with_supported_protocols(["http"]);
        let executor = GatewayExecutor::new(mediator, &audit, &tool_adapter);

        let execution = adapter
            .execute(
                &task(),
                HttpToolRequest {
                    connector: "github".into(),
                    operation: "create_pr".into(),
                    parameters: BTreeMap::from([(
                        "title".into(),
                        RedactedValue::Plain("Ship connector".into()),
                    )]),
                },
                &executor,
                ToolExecutionContext::default(),
            )
            .unwrap();

        assert!(
            execution.error.is_none(),
            "unexpected execution error: {:?}",
            execution.error
        );
        assert!(matches!(
            execution.result,
            Some(ToolResult { summary, .. }) if summary == "http execution complete"
        ));
        assert_eq!(tool_adapter.call_count(), 1);
    }

    #[test]
    fn gateway_egress_adapter_enforces_network_policy_and_redacts_response() {
        let mut task = task_with_budget("gateway_calls", 2);
        task.permissions.network.allow_domains = vec!["api.github.com".into()];
        task.permissions.tools.allow = vec!["egress.fetch".into()];
        let policy = BuiltInPolicyEngine;
        let audit = RecordingAudit::default();
        let registry =
            InMemoryToolRegistry::new([RegisteredTool::new("http", "egress", "fetch").unwrap()]);
        let client = RecordingEgressClient::default();
        let calls = client.calls.clone();
        let adapter = GatewayEgressAdapter::new(client);
        let mediator = GatewayMediator::new(&policy, &audit)
            .with_tool_registry(&registry)
            .with_supported_protocols(["http"]);
        let executor = GatewayExecutor::new(mediator, &audit, &adapter);

        let execution = executor
            .execute_tool_action(
                &task,
                ToolAction {
                    protocol: "http".into(),
                    tool: "egress".into(),
                    operation: "fetch".into(),
                    parameters: BTreeMap::from([(
                        "url".into(),
                        RedactedValue::Plain(
                            "https://api.github.com/repos/taskfence/example".into(),
                        ),
                    )]),
                },
                ToolExecutionContext::default(),
            )
            .unwrap();

        assert!(execution.error.is_none());
        assert_eq!(calls.lock().unwrap().len(), 1);
        let result = execution.result.unwrap();
        assert!(matches!(
            result.values.get("body"),
            Some(RedactedValue::Plain(body)) if body.contains("[redacted]")
        ));
        assert!(audit.events.lock().unwrap().iter().any(|event| {
            matches!(
                event,
                AuditEvent::PolicyDecision {
                    action: Action::Network { host, .. },
                    decision: ActionDecision::Allow { .. },
                    ..
                } if host == "api.github.com"
            )
        }));
    }

    #[test]
    fn gateway_egress_denies_non_allowlisted_domain_before_client_call() {
        let mut task = task_with_budget("gateway_calls", 2);
        task.permissions.network.allow_domains = vec!["api.github.com".into()];
        task.permissions.tools.allow = vec!["egress.fetch".into()];
        let policy = BuiltInPolicyEngine;
        let audit = RecordingAudit::default();
        let registry =
            InMemoryToolRegistry::new([RegisteredTool::new("http", "egress", "fetch").unwrap()]);
        let client = RecordingEgressClient::default();
        let calls = client.calls.clone();
        let adapter = GatewayEgressAdapter::new(client);
        let mediator = GatewayMediator::new(&policy, &audit)
            .with_tool_registry(&registry)
            .with_supported_protocols(["http"]);
        let executor = GatewayExecutor::new(mediator, &audit, &adapter);

        let execution = executor
            .execute_tool_action(
                &task,
                ToolAction {
                    protocol: "http".into(),
                    tool: "egress".into(),
                    operation: "fetch".into(),
                    parameters: BTreeMap::from([(
                        "url".into(),
                        RedactedValue::Plain("https://example.com/".into()),
                    )]),
                },
                ToolExecutionContext::default(),
            )
            .unwrap();

        assert!(matches!(
            execution.error,
            Some(ToolExecutionError {
                kind: ToolExecutionErrorKind::PolicyDenied,
                ..
            })
        ));
        assert!(calls.lock().unwrap().is_empty());
    }

    #[test]
    fn gateway_egress_rejects_unsafe_urls() {
        for url in [
            "http://api.github.com/",
            "https://token@api.github.com/",
            "https://api.github.com/../secret",
            "https://api.github.com/repos?token=secret",
        ] {
            let action = ToolAction {
                protocol: "http".into(),
                tool: "egress".into(),
                operation: "fetch".into(),
                parameters: BTreeMap::from([("url".into(), RedactedValue::Plain(url.into()))]),
            };

            let err = gateway_egress_destination(&action).unwrap_err();

            assert_eq!(err.kind, ToolExecutionErrorKind::InvalidParameters);
        }
    }

    #[test]
    fn registered_tool_normalizes_key_segments() {
        let tool = RegisteredTool::new(" MCP ", " GitHub ", " Read_Issue ").unwrap();

        assert_eq!(tool.key.protocol, "mcp");
        assert_eq!(tool.key.tool, "github");
        assert_eq!(tool.key.operation, "read_issue");
        assert_eq!(tool.key.display_name(), "mcp github.read_issue");
    }

    #[test]
    fn registered_tool_rejects_empty_segments() {
        let err = RegisteredTool::new("mcp", " ", "read_issue").unwrap_err();

        assert!(matches!(err, TaskFenceError::Gateway(message) if message.contains("tool")));
    }

    #[test]
    fn in_memory_registry_matches_normalized_actions() {
        let registry =
            InMemoryToolRegistry::new([RegisteredTool::new("mcp", "github", "create_pr").unwrap()]);

        assert!(!registry.is_empty());
        assert!(registry.contains(&tool_action(" MCP ")).unwrap());
        assert!(!registry
            .contains(&ToolAction {
                protocol: "mcp".into(),
                tool: "github".into(),
                operation: "delete_repo".into(),
                parameters: BTreeMap::new(),
            })
            .unwrap());
    }

    fn task_with_gateway_secret() -> ResolvedTask {
        task_with_gateway_secret_scope("github.create_pr")
    }

    fn task_with_gateway_secret_scope(scope: &str) -> ResolvedTask {
        let mut task = task();
        task.secrets.available_to_gateway = vec![SecretGrant {
            name: "github_token".into(),
            use_for: vec![scope.into()],
        }];
        task
    }

    fn github_rest_tool(operation: &str) -> GatewayToolConfig {
        GatewayToolConfig {
            protocol: "mcp".into(),
            tool: "github".into(),
            operation: operation.into(),
            connector: GatewayConnectorConfig::GitHubRest {
                api_base: "https://api.github.test".into(),
                repository: "taskfence/example".into(),
            },
            secret_refs: vec![GatewaySecretReferenceConfig {
                name: "github_token".into(),
                parameter: "authorization".into(),
                scope: format!("github.{operation}"),
            }],
        }
    }

    fn contract_connector_tool(
        tool_name: &str,
        operation: &str,
        connector: GatewayConnectorConfig,
    ) -> GatewayToolConfig {
        GatewayToolConfig {
            protocol: "mcp".into(),
            tool: tool_name.into(),
            operation: operation.into(),
            connector,
            secret_refs: vec![GatewaySecretReferenceConfig {
                name: format!("{tool_name}_token"),
                parameter: "authorization".into(),
                scope: format!("{tool_name}.{operation}"),
            }],
        }
    }

    fn enterprise_tool(
        tool_name: &str,
        operation: &str,
        connector: GatewayConnectorConfig,
    ) -> GatewayToolConfig {
        contract_connector_tool(tool_name, operation, connector)
    }

    fn database_tool(operation: &str) -> GatewayToolConfig {
        GatewayToolConfig {
            protocol: "mcp".into(),
            tool: "database".into(),
            operation: operation.into(),
            connector: GatewayConnectorConfig::Database {
                engine: "postgres".into(),
                database_ref: "taskfence_reporting".into(),
            },
            secret_refs: vec![GatewaySecretReferenceConfig {
                name: "database_credential".into(),
                parameter: "credential".into(),
                scope: format!("database.{operation}"),
            }],
        }
    }

    #[test]
    fn normalizes_tool_action_before_policy_and_audit() {
        let policy = StaticPolicy::new(allow());
        let audit = RecordingAudit::default();
        let mediator = GatewayMediator::new(&policy, &audit);

        let result = mediator
            .mediate_tool_action(&task(), tool_action(" MCP "))
            .unwrap();

        assert_eq!(result.action.protocol, "mcp");
        assert_eq!(result.action.tool, "github");
        assert_eq!(result.action.operation, "create_pr");
        assert!(result.action.parameters.contains_key("title"));
        assert!(result.approval.is_none());

        let seen = policy.seen_actions.lock().unwrap();
        assert_eq!(seen.len(), 1);
        assert!(matches!(
            &seen[0],
            Action::ToolCall(action)
                if action.tool == "github" && action.operation == "create_pr"
        ));

        let events = audit.events.lock().unwrap();
        assert_eq!(events.len(), 1);
        assert!(matches!(
            events.first(),
            Some(AuditEvent::PolicyDecision { .. })
        ));
    }

    #[test]
    fn registered_gateway_tool_continues_to_policy_evaluation() {
        let policy = StaticPolicy::new(allow());
        let audit = RecordingAudit::default();
        let registry =
            InMemoryToolRegistry::new([RegisteredTool::new("mcp", "github", "create_pr").unwrap()]);
        let mediator = GatewayMediator::new(&policy, &audit).with_tool_registry(&registry);

        let result = mediator
            .mediate_tool_action(&task(), tool_action("mcp"))
            .unwrap();

        assert!(matches!(result.decision, ActionDecision::Allow { .. }));
        assert_eq!(policy.seen_actions.lock().unwrap().len(), 1);
        let events = audit.events.lock().unwrap();
        assert_eq!(events.len(), 1);
        assert!(matches!(
            events.first(),
            Some(AuditEvent::PolicyDecision { .. })
        ));
    }

    #[test]
    fn unregistered_gateway_tool_fails_before_policy_evaluation() {
        let policy = StaticPolicy::new(allow());
        let audit = RecordingAudit::default();
        let registry =
            InMemoryToolRegistry::new(
                [RegisteredTool::new("mcp", "github", "read_issue").unwrap()],
            );
        let mediator = GatewayMediator::new(&policy, &audit).with_tool_registry(&registry);

        let err = mediator
            .mediate_tool_action(&task(), tool_action("mcp"))
            .unwrap_err();

        assert!(matches!(
            err,
            TaskFenceError::Gateway(message)
                if message.contains("not registered")
                    && message.contains("mcp github.create_pr")
        ));
        assert!(policy.seen_actions.lock().unwrap().is_empty());
        let events = audit.events.lock().unwrap();
        assert_eq!(events.len(), 1);
        assert!(matches!(
            events.first(),
            Some(AuditEvent::Error { message, .. })
                if message.contains("mcp github.create_pr")
        ));
    }

    #[test]
    fn returns_policy_decision_without_executing_gateway_action() {
        let policy = StaticPolicy::new(deny());
        let audit = RecordingAudit::default();
        let mediator = GatewayMediator::new(&policy, &audit);

        let result = mediator
            .mediate_tool_action(&task(), tool_action("mcp"))
            .unwrap();

        assert!(matches!(result.decision, ActionDecision::Deny { .. }));
        assert!(result.approval.is_none());
    }

    #[test]
    fn records_configured_tool_policy_decision_without_execution() {
        let policy = BuiltInPolicyEngine;
        let audit = RecordingAudit::default();
        let mediator = GatewayMediator::new(&policy, &audit);
        let mut task = task();
        task.permissions.tools = ToolPermissions {
            allow: vec!["github.read_issue".into()],
            approval_required: vec!["github.create_pr".into()],
            deny: vec!["github.delete_repo".into()],
        };

        let result = mediator
            .mediate_tool_action(&task, tool_action("mcp"))
            .unwrap();

        assert!(matches!(
            result.decision,
            ActionDecision::RequireApproval {
                approval_kind,
                ..
            } if approval_kind == "tool_call"
        ));
        assert!(result.approval.is_none());
        let events = audit.events.lock().unwrap();
        assert_eq!(events.len(), 1);
        assert!(matches!(
            events.first(),
            Some(AuditEvent::PolicyDecision {
                action: Action::ToolCall(action),
                decision: ActionDecision::RequireApproval { .. },
                ..
            }) if action.tool == "github" && action.operation == "create_pr"
        ));
    }

    #[test]
    fn approved_tool_call_records_approval_events_without_execution() {
        let policy = BuiltInPolicyEngine;
        let approval = StaticApproval::new(ApprovalDecision::Approved);
        let audit = RecordingAudit::default();
        let mediator = GatewayMediator::new(&policy, &audit).with_approval(&approval);
        let mut task = task();
        task.permissions.tools.approval_required = vec!["github.create_pr".into()];

        let result = mediator
            .mediate_tool_action(&task, tool_action("mcp"))
            .unwrap();

        assert!(matches!(
            result.decision,
            ActionDecision::RequireApproval { .. }
        ));
        assert!(matches!(
            result.approval,
            Some(ApprovalRecord {
                decision: Some(ApprovalDecision::Approved),
                ..
            })
        ));
        let events = audit.events.lock().unwrap();
        assert_eq!(events.len(), 3);
        assert!(matches!(
            events.as_slice(),
            [
                AuditEvent::PolicyDecision {
                    action: Action::ToolCall(_),
                    decision: ActionDecision::RequireApproval { .. },
                    ..
                },
                AuditEvent::ApprovalRequested { record: requested },
                AuditEvent::ApprovalResolved { record: resolved },
            ] if requested.decision.is_none()
                && resolved.decision == Some(ApprovalDecision::Approved)
        ));
    }

    #[test]
    fn denied_tool_approval_fails_closed_after_audit_resolution() {
        let policy = BuiltInPolicyEngine;
        let approval = StaticApproval::new(ApprovalDecision::Denied);
        let audit = RecordingAudit::default();
        let mediator = GatewayMediator::new(&policy, &audit).with_approval(&approval);
        let mut task = task();
        task.permissions.tools.approval_required = vec!["github.create_pr".into()];

        let err = mediator
            .mediate_tool_action(&task, tool_action("mcp"))
            .unwrap_err();

        assert!(matches!(
            err,
            TaskFenceError::Approval(message) if message.contains("denied or timed out")
        ));
        let events = audit.events.lock().unwrap();
        assert_eq!(events.len(), 3);
        assert!(matches!(
            events.last(),
            Some(AuditEvent::ApprovalResolved { record })
                if record.decision == Some(ApprovalDecision::Denied)
        ));
    }

    #[test]
    fn timed_out_tool_approval_fails_closed_after_audit_resolution() {
        let policy = BuiltInPolicyEngine;
        let approval = StaticApproval::new(ApprovalDecision::TimedOut);
        let audit = RecordingAudit::default();
        let mediator = GatewayMediator::new(&policy, &audit).with_approval(&approval);
        let mut task = task();
        task.permissions.tools.approval_required = vec!["github.create_pr".into()];

        let err = mediator
            .mediate_tool_action(&task, tool_action("mcp"))
            .unwrap_err();

        assert!(matches!(err, TaskFenceError::Approval(_)));
        let events = audit.events.lock().unwrap();
        assert!(matches!(
            events.last(),
            Some(AuditEvent::ApprovalResolved { record })
                if record.decision == Some(ApprovalDecision::TimedOut)
        ));
    }

    #[test]
    fn issues_redacted_gateway_secret_reference_for_allowed_scope() {
        let task = task_with_gateway_secret();
        let broker = StaticSecretBroker::default();

        let reference =
            gateway_secret_reference(&task, &broker, " GitHub_Token ", " GitHub.Create_Pr ")
                .unwrap();

        assert_eq!(reference.name, "github_token");
        assert_eq!(reference.scope, "github.create_pr");
        assert_eq!(
            broker.issued.lock().unwrap().as_slice(),
            &[("github_token".into(), "github.create_pr".into())]
        );
        assert!(matches!(
            reference.as_redacted_value(),
            RedactedValue::Redacted { reason } if reason.contains("github_token")
        ));
    }

    #[test]
    fn gateway_secret_reference_denies_unavailable_secret_or_scope() {
        let task = task_with_gateway_secret();
        let broker = StaticSecretBroker::default();

        let missing = gateway_secret_reference(&task, &broker, "slack_token", "github.create_pr")
            .unwrap_err();
        let wrong_scope =
            gateway_secret_reference(&task, &broker, "github_token", "github.delete_repo")
                .unwrap_err();

        assert!(
            matches!(missing, TaskFenceError::Gateway(message) if message.contains("slack_token"))
        );
        assert!(
            matches!(wrong_scope, TaskFenceError::Gateway(message) if message.contains("github.delete_repo"))
        );
        assert!(broker.issued.lock().unwrap().is_empty());
    }

    #[test]
    fn gateway_secret_reference_requires_secrets_to_stay_out_of_agent() {
        let mut task = task_with_gateway_secret();
        task.secrets.expose_to_agent = true;
        let broker = StaticSecretBroker::default();

        let err = gateway_secret_reference(&task, &broker, "github_token", "github.create_pr")
            .unwrap_err();

        assert!(matches!(
            err,
            TaskFenceError::Gateway(message) if message.contains("stay out of the agent")
        ));
        assert!(broker.issued.lock().unwrap().is_empty());
    }

    #[test]
    fn attaches_secret_reference_without_raw_secret_parameter_value() {
        let task = task_with_gateway_secret();
        let broker = StaticSecretBroker::default();
        let reference =
            gateway_secret_reference(&task, &broker, "github_token", "github.create_pr").unwrap();

        let action =
            attach_secret_reference(tool_action("mcp"), " authorization ", &reference).unwrap();

        assert!(matches!(
            action.parameters.get("authorization"),
            Some(RedactedValue::Redacted { reason })
                if reason == "gateway secret reference for github_token"
        ));
        assert!(!format!("{:?}", action.parameters).contains(&reference.handle));
        assert!(!format!("{:?}", action.parameters).contains("raw"));
    }

    #[test]
    fn unsupported_protocol_returns_explicit_error_and_audit_event() {
        let policy = StaticPolicy::new(allow());
        let audit = RecordingAudit::default();
        let mediator = GatewayMediator::new(&policy, &audit);

        let err = mediator
            .mediate_tool_action(&task(), tool_action("http"))
            .unwrap_err();

        assert!(matches!(
            err,
            TaskFenceError::Unsupported(message) if message.contains("http")
        ));
        assert!(policy.seen_actions.lock().unwrap().is_empty());
        let events = audit.events.lock().unwrap();
        assert_eq!(events.len(), 1);
        assert!(matches!(events.first(), Some(AuditEvent::Error { .. })));
    }

    #[test]
    fn empty_tool_segment_is_gateway_error() {
        let err = normalize_tool_action(ToolAction {
            protocol: "mcp".into(),
            tool: " ".into(),
            operation: "read_issue".into(),
            parameters: BTreeMap::new(),
        })
        .unwrap_err();

        assert!(matches!(err, TaskFenceError::Gateway(message) if message.contains("tool")));
    }

    #[test]
    fn executor_records_started_and_finished_events_for_allowed_execution() {
        let policy = StaticPolicy::new(allow());
        let audit = RecordingAudit::default();
        let adapter = StaticToolAdapter::succeeding("fixture read_issue complete");
        let mediator = GatewayMediator::new(&policy, &audit);
        let executor = GatewayExecutor::new(mediator, &audit, &adapter);

        let execution = executor
            .execute_tool_action(&task(), tool_action("mcp"), ToolExecutionContext::default())
            .unwrap();

        assert!(matches!(
            execution.result,
            Some(ToolResult { summary, .. }) if summary == "fixture read_issue complete"
        ));
        assert!(execution.error.is_none());
        assert_eq!(adapter.call_count(), 1);

        let events = audit.events.lock().unwrap();
        assert_eq!(events.len(), 3);
        assert!(matches!(events[0], AuditEvent::PolicyDecision { .. }));
        assert!(matches!(
            &events[1],
            AuditEvent::ToolExecutionStarted { request, .. }
                if request.action.tool == "github"
                    && request.action.operation == "create_pr"
                    && matches!(
                        request.adapter.as_ref(),
                        Some(ToolAdapterIdentity { kind, name })
                            if kind == "local_fixture" && name == "static-test"
                    )
        ));
        assert!(matches!(
            &events[2],
            AuditEvent::ToolExecutionFinished { execution, .. }
                if execution.result.is_some() && execution.error.is_none()
        ));
    }

    #[test]
    fn executor_records_allowed_planned_budget_usage_before_execution() {
        let policy = BuiltInPolicyEngine;
        let audit = RecordingAudit::default();
        let adapter = StaticToolAdapter::succeeding("fixture read_issue complete")
            .with_planned_usage(vec![BudgetUsage {
                kind: " Tokens ".into(),
                amount: 50,
                provider: Some(" FixtureAI ".into()),
                model: Some("demo-model".into()),
                operation: Some("github.create_pr".into()),
                metadata: BTreeMap::from([(
                    " request_class ".into(),
                    RedactedValue::Plain("planned".into()),
                )]),
            }]);
        let mediator = GatewayMediator::new(&policy, &audit);
        let executor = GatewayExecutor::new(mediator, &audit, &adapter);

        let execution = executor
            .execute_tool_action(
                &task_with_budget("tokens", 100),
                tool_action("mcp"),
                ToolExecutionContext::default(),
            )
            .unwrap();

        assert!(execution.error.is_none());
        assert_eq!(adapter.call_count(), 1);
        let events = audit.events.lock().unwrap();
        assert!(matches!(
            events.as_slice(),
            [
                AuditEvent::PolicyDecision {
                    action: Action::ToolCall(_),
                    decision: ActionDecision::Allow { .. },
                    ..
                },
                AuditEvent::PolicyDecision {
                    action: Action::Budget { kind, amount },
                    decision: ActionDecision::Allow { .. },
                    ..
                },
                AuditEvent::BudgetUsageRecorded { record, .. },
                AuditEvent::ToolExecutionStarted { .. },
                AuditEvent::ToolExecutionFinished { execution, .. },
            ] if kind == "tokens"
                && *amount == 50
                && record.usage.kind == "tokens"
                && record.usage.amount == 50
                && record.usage.provider.as_deref() == Some("FixtureAI")
                && record.limit.as_ref().map(|limit| limit.max_amount) == Some(100)
                && matches!(record.decision, ActionDecision::Allow { .. })
                && execution.error.is_none()
        ));
    }

    #[test]
    fn executor_denies_planned_budget_usage_before_secret_or_adapter_execution() {
        let policy = BuiltInPolicyEngine;
        let audit = RecordingAudit::default();
        let adapter = StaticToolAdapter::succeeding("must not run")
            .with_planned_usage(vec![BudgetUsage {
                kind: "gateway_calls".into(),
                amount: 2,
                provider: Some("github".into()),
                model: None,
                operation: Some("github.create_pr".into()),
                metadata: BTreeMap::new(),
            }])
            .with_secret_ref(GatewaySecretReferenceConfig {
                name: "github_token".into(),
                parameter: "authorization".into(),
                scope: "github.create_pr".into(),
            });
        let broker = StaticSecretBroker::default();
        let mediator = GatewayMediator::new(&policy, &audit);
        let executor = GatewayExecutor::new(mediator, &audit, &adapter).with_secret_broker(&broker);
        let mut task = task_with_gateway_secret();
        task.permissions.tools.allow = vec!["github.create_pr".into()];
        task.permissions.budget = BudgetPermissions {
            allow: vec![BudgetLimit {
                kind: "gateway_calls".into(),
                max_amount: 1,
            }],
        };

        let execution = executor
            .execute_tool_action(&task, tool_action("mcp"), ToolExecutionContext::default())
            .unwrap();

        assert!(matches!(
            execution.error,
            Some(ToolExecutionError {
                kind: ToolExecutionErrorKind::BudgetExceeded,
                message,
            }) if message == "budget amount exceeds configured limit"
        ));
        assert_eq!(adapter.call_count(), 0);
        assert!(broker.issued.lock().unwrap().is_empty());
        let events = audit.events.lock().unwrap();
        assert!(matches!(
            events.as_slice(),
            [
                AuditEvent::PolicyDecision {
                    action: Action::ToolCall(_),
                    decision: ActionDecision::Allow { .. },
                    ..
                },
                AuditEvent::PolicyDecision {
                    action: Action::Budget { kind, amount },
                    decision: ActionDecision::Deny { .. },
                    ..
                },
                AuditEvent::BudgetUsageRecorded { record, .. },
                AuditEvent::ToolExecutionFinished { execution, .. },
            ] if kind == "gateway_calls"
                && *amount == 2
                && record.usage.operation.as_deref() == Some("github.create_pr")
                && matches!(record.decision, ActionDecision::Deny { .. })
                && matches!(
                    execution.error,
                    Some(ToolExecutionError {
                        kind: ToolExecutionErrorKind::BudgetExceeded,
                        ..
                    })
                )
        ));
    }

    #[test]
    fn executor_records_result_budget_usage_and_marks_over_limit_partial_result() {
        let policy = BuiltInPolicyEngine;
        let audit = RecordingAudit::default();
        let adapter = StaticToolAdapter::succeeding("fixture completed with observed usage")
            .with_result_usage(vec![BudgetUsage {
                kind: "usd_cents".into(),
                amount: 25,
                provider: Some("fixture-provider".into()),
                model: None,
                operation: Some("fixture.complete".into()),
                metadata: BTreeMap::new(),
            }]);
        let mediator = GatewayMediator::new(&policy, &audit);
        let executor = GatewayExecutor::new(mediator, &audit, &adapter);

        let execution = executor
            .execute_tool_action(
                &task_with_budget("usd_cents", 10),
                tool_action("mcp"),
                ToolExecutionContext::default(),
            )
            .unwrap();

        assert!(execution.result.is_some());
        assert!(matches!(
            execution.error,
            Some(ToolExecutionError {
                kind: ToolExecutionErrorKind::BudgetExceeded,
                ..
            })
        ));
        let events = audit.events.lock().unwrap();
        assert!(matches!(
            events.as_slice(),
            [
                AuditEvent::PolicyDecision {
                    action: Action::ToolCall(_),
                    decision: ActionDecision::Allow { .. },
                    ..
                },
                AuditEvent::ToolExecutionStarted { .. },
                AuditEvent::PolicyDecision {
                    action: Action::Budget { kind, amount },
                    decision: ActionDecision::Deny { .. },
                    ..
                },
                AuditEvent::BudgetUsageRecorded { record, .. },
                AuditEvent::ToolExecutionFinished { execution, .. },
            ] if kind == "usd_cents"
                && *amount == 25
                && record.limit.as_ref().map(|limit| limit.max_amount) == Some(10)
                && execution.result.is_some()
                && matches!(
                    execution.error,
                    Some(ToolExecutionError {
                        kind: ToolExecutionErrorKind::BudgetExceeded,
                        ..
                    })
                )
        ));
    }

    #[test]
    fn executor_attaches_redacted_secret_reference_after_approval_before_execution() {
        let policy = BuiltInPolicyEngine;
        let approval = StaticApproval::new(ApprovalDecision::Approved);
        let audit = RecordingAudit::default();
        let adapter = StaticToolAdapter::succeeding("fixture create_pr complete").with_secret_ref(
            GatewaySecretReferenceConfig {
                name: "github_token".into(),
                parameter: "authorization".into(),
                scope: "github.create_pr".into(),
            },
        );
        let broker = StaticSecretBroker::default();
        let mut task = task_with_gateway_secret();
        task.permissions.tools.approval_required = vec!["github.create_pr".into()];
        let mediator = GatewayMediator::new(&policy, &audit).with_approval(&approval);
        let executor = GatewayExecutor::new(mediator, &audit, &adapter).with_secret_broker(&broker);

        let execution = executor
            .execute_tool_action(&task, tool_action("mcp"), ToolExecutionContext::default())
            .unwrap();

        assert!(execution.error.is_none());
        assert_eq!(
            broker.issued.lock().unwrap().as_slice(),
            &[("github_token".into(), "github.create_pr".into())]
        );
        let calls = adapter.calls.lock().unwrap();
        assert_eq!(calls.len(), 1);
        assert!(matches!(
            calls[0].parameters.get("authorization"),
            Some(RedactedValue::Redacted { reason })
                if reason == "gateway secret reference for github_token"
        ));
        drop(calls);

        let events = audit.events.lock().unwrap();
        assert!(matches!(
            events.as_slice(),
            [
                AuditEvent::PolicyDecision {
                    action: Action::ToolCall(policy_action),
                    decision: ActionDecision::RequireApproval { .. },
                    ..
                },
                AuditEvent::ApprovalRequested { .. },
                AuditEvent::ApprovalResolved { record },
                AuditEvent::ToolExecutionStarted { request, .. },
                AuditEvent::ToolExecutionFinished { execution, .. },
            ] if !policy_action.parameters.contains_key("authorization")
                && record.decision == Some(ApprovalDecision::Approved)
                && matches!(
                    request.action.parameters.get("authorization"),
                    Some(RedactedValue::Redacted { reason })
                        if reason == "gateway secret reference for github_token"
                )
                && execution.error.is_none()
        ));
    }

    #[test]
    fn executor_denied_approval_does_not_attach_secret_or_execute_adapter() {
        let policy = BuiltInPolicyEngine;
        let approval = StaticApproval::new(ApprovalDecision::Denied);
        let audit = RecordingAudit::default();
        let adapter = StaticToolAdapter::succeeding("must not run").with_secret_ref(
            GatewaySecretReferenceConfig {
                name: "github_token".into(),
                parameter: "authorization".into(),
                scope: "github.create_pr".into(),
            },
        );
        let broker = StaticSecretBroker::default();
        let mut task = task_with_gateway_secret();
        task.permissions.tools.approval_required = vec!["github.create_pr".into()];
        let mediator = GatewayMediator::new(&policy, &audit).with_approval(&approval);
        let executor = GatewayExecutor::new(mediator, &audit, &adapter).with_secret_broker(&broker);

        let execution = executor
            .execute_tool_action(&task, tool_action("mcp"), ToolExecutionContext::default())
            .unwrap();

        assert!(matches!(
            execution.error,
            Some(ToolExecutionError {
                kind: ToolExecutionErrorKind::ApprovalDeniedOrTimedOut,
                ..
            })
        ));
        assert_eq!(adapter.call_count(), 0);
        assert!(broker.issued.lock().unwrap().is_empty());
        let events = audit.events.lock().unwrap();
        assert!(matches!(
            events.as_slice(),
            [
                AuditEvent::PolicyDecision { .. },
                AuditEvent::ApprovalRequested { .. },
                AuditEvent::ApprovalResolved { record },
                AuditEvent::ToolExecutionFinished { execution, .. },
            ] if record.decision == Some(ApprovalDecision::Denied)
                && matches!(
                    execution.error,
                    Some(ToolExecutionError {
                        kind: ToolExecutionErrorKind::ApprovalDeniedOrTimedOut,
                        ..
                    })
                )
        ));
    }

    #[test]
    fn executor_turns_policy_denial_into_structured_failure_without_adapter_call() {
        let policy = StaticPolicy::new(deny());
        let audit = RecordingAudit::default();
        let adapter = StaticToolAdapter::succeeding("must not run");
        let mediator = GatewayMediator::new(&policy, &audit);
        let executor = GatewayExecutor::new(mediator, &audit, &adapter);

        let execution = executor
            .execute_tool_action(&task(), tool_action("mcp"), ToolExecutionContext::default())
            .unwrap();

        assert!(matches!(
            execution.error,
            Some(ToolExecutionError {
                kind: ToolExecutionErrorKind::PolicyDenied,
                ..
            })
        ));
        assert_eq!(adapter.call_count(), 0);
        let events = audit.events.lock().unwrap();
        assert_eq!(events.len(), 2);
        assert!(matches!(events[0], AuditEvent::PolicyDecision { .. }));
        assert!(matches!(
            events[1],
            AuditEvent::ToolExecutionFinished { .. }
        ));
    }

    #[test]
    fn executor_turns_unregistered_tool_into_structured_failure() {
        let policy = StaticPolicy::new(allow());
        let audit = RecordingAudit::default();
        let adapter = StaticToolAdapter::succeeding("must not run");
        let registry =
            InMemoryToolRegistry::new(
                [RegisteredTool::new("mcp", "github", "read_issue").unwrap()],
            );
        let mediator = GatewayMediator::new(&policy, &audit).with_tool_registry(&registry);
        let executor = GatewayExecutor::new(mediator, &audit, &adapter);

        let execution = executor
            .execute_tool_action(&task(), tool_action("mcp"), ToolExecutionContext::default())
            .unwrap();

        assert!(matches!(
            execution.error,
            Some(ToolExecutionError {
                kind: ToolExecutionErrorKind::UnregisteredTool,
                ..
            })
        ));
        assert_eq!(adapter.call_count(), 0);
        let events = audit.events.lock().unwrap();
        assert_eq!(events.len(), 2);
        assert!(matches!(events[0], AuditEvent::Error { .. }));
        assert!(matches!(
            events[1],
            AuditEvent::ToolExecutionFinished { .. }
        ));
    }

    #[test]
    fn executor_turns_unsupported_protocol_into_structured_failure() {
        let policy = StaticPolicy::new(allow());
        let audit = RecordingAudit::default();
        let adapter = StaticToolAdapter::succeeding("must not run");
        let mediator = GatewayMediator::new(&policy, &audit);
        let executor = GatewayExecutor::new(mediator, &audit, &adapter);

        let execution = executor
            .execute_tool_action(
                &task(),
                tool_action("http"),
                ToolExecutionContext::default(),
            )
            .unwrap();

        assert!(matches!(
            execution.error,
            Some(ToolExecutionError {
                kind: ToolExecutionErrorKind::UnsupportedProtocol,
                ..
            })
        ));
        assert_eq!(adapter.call_count(), 0);
    }

    #[test]
    fn executor_records_adapter_failure_as_finished_execution() {
        let policy = StaticPolicy::new(allow());
        let audit = RecordingAudit::default();
        let adapter = StaticToolAdapter::failing("fixture failed");
        let mediator = GatewayMediator::new(&policy, &audit);
        let executor = GatewayExecutor::new(mediator, &audit, &adapter);

        let execution = executor
            .execute_tool_action(&task(), tool_action("mcp"), ToolExecutionContext::default())
            .unwrap();

        assert!(matches!(
            execution.error,
            Some(ToolExecutionError {
                kind: ToolExecutionErrorKind::AdapterFailed,
                message,
            }) if message == "fixture failed"
        ));
        assert_eq!(adapter.call_count(), 1);
        let events = audit.events.lock().unwrap();
        assert!(matches!(
            events.as_slice(),
            [
                AuditEvent::PolicyDecision { .. },
                AuditEvent::ToolExecutionStarted { .. },
                AuditEvent::ToolExecutionFinished { execution, .. },
            ] if matches!(
                execution.error,
                Some(ToolExecutionError {
                    kind: ToolExecutionErrorKind::AdapterFailed,
                    ..
                })
            )
        ));
    }

    #[test]
    fn github_rest_read_issue_uses_live_token_without_auditing_it() {
        let token = "ghp_live_test_token";
        let client = RecordingGitHubClient::default();
        let adapter = GitHubRestAdapter::new(github_rest_tool("read_issue"), client.clone());
        let broker = StaticLiveSecretBroker::new(token);
        let policy = BuiltInPolicyEngine;
        let audit = RecordingAudit::default();
        let registry =
            InMemoryToolRegistry::new(
                [RegisteredTool::new("mcp", "github", "read_issue").unwrap()],
            );
        let mediator = GatewayMediator::new(&policy, &audit).with_tool_registry(&registry);
        let executor = GatewayExecutor::new(mediator, &audit, &adapter).with_secret_broker(&broker);
        let mut task = task_with_gateway_secret_scope("github.read_issue");
        task.permissions.tools.allow = vec!["github.read_issue".into()];
        task.permissions.budget = BudgetPermissions {
            allow: vec![BudgetLimit {
                kind: "gateway_calls".into(),
                max_amount: 1,
            }],
        };

        let execution = executor
            .execute_tool_action(
                &task,
                ToolAction {
                    protocol: "mcp".into(),
                    tool: "github".into(),
                    operation: "read_issue".into(),
                    parameters: BTreeMap::from([(
                        "number".into(),
                        RedactedValue::Plain("42".into()),
                    )]),
                },
                ToolExecutionContext::default(),
            )
            .unwrap();

        assert!(execution.error.is_none());
        assert!(matches!(
            execution.result,
            Some(ToolResult { summary, values, .. })
                if summary == "read GitHub issue #42 from taskfence/example"
                    && matches!(
                        values.get("title"),
                        Some(RedactedValue::Plain(title)) if title == "Needs review"
                    )
        ));
        assert_eq!(
            client.calls.lock().unwrap().as_slice(),
            &[GitHubClientCall::ReadIssue {
                api_base: "https://api.github.test".into(),
                repository: "taskfence/example".into(),
                token: token.into(),
                number: 42,
            }]
        );
        assert_eq!(
            broker.issued.lock().unwrap().as_slice(),
            &[("github_token".into(), "github.read_issue".into())]
        );

        let events = audit.events.lock().unwrap();
        assert!(!format!("{events:?}").contains(token));
        assert!(events.iter().any(|event| {
            matches!(
                event,
                AuditEvent::ToolExecutionStarted { request, .. }
                    if matches!(
                        request.action.parameters.get("authorization"),
                        Some(RedactedValue::Redacted { reason })
                            if reason == "gateway secret reference for github_token"
                    )
            )
        }));
    }

    #[test]
    fn github_rest_create_pr_runs_after_approval() {
        let token = "ghp_live_test_token";
        let client = RecordingGitHubClient::default();
        let adapter = GitHubRestAdapter::new(github_rest_tool("create_pr"), client.clone());
        let broker = StaticLiveSecretBroker::new(token);
        let policy = BuiltInPolicyEngine;
        let approval = StaticApproval::new(ApprovalDecision::Approved);
        let audit = RecordingAudit::default();
        let registry =
            InMemoryToolRegistry::new([RegisteredTool::new("mcp", "github", "create_pr").unwrap()]);
        let mediator = GatewayMediator::new(&policy, &audit)
            .with_tool_registry(&registry)
            .with_approval(&approval);
        let executor = GatewayExecutor::new(mediator, &audit, &adapter).with_secret_broker(&broker);
        let mut task = task_with_gateway_secret();
        task.permissions.tools.approval_required = vec!["github.create_pr".into()];
        task.permissions.budget = BudgetPermissions {
            allow: vec![BudgetLimit {
                kind: "gateway_calls".into(),
                max_amount: 1,
            }],
        };

        let execution = executor
            .execute_tool_action(
                &task,
                ToolAction {
                    protocol: "mcp".into(),
                    tool: "github".into(),
                    operation: "create_pr".into(),
                    parameters: BTreeMap::from([
                        (
                            "title".into(),
                            RedactedValue::Plain("Ship connector".into()),
                        ),
                        (
                            "head".into(),
                            RedactedValue::Plain("codex/connector".into()),
                        ),
                        ("base".into(), RedactedValue::Plain("main".into())),
                        (
                            "body".into(),
                            RedactedValue::Plain("Bounded GitHub REST PR".into()),
                        ),
                    ]),
                },
                ToolExecutionContext::default(),
            )
            .unwrap();

        assert!(execution.error.is_none());
        assert!(matches!(
            execution.result,
            Some(ToolResult { summary, values, .. })
                if summary == "created GitHub pull request #7 in taskfence/example"
                    && matches!(
                        values.get("html_url"),
                        Some(RedactedValue::Plain(url)) if url.ends_with("/pull/7")
                    )
        ));
        assert_eq!(
            client.calls.lock().unwrap().as_slice(),
            &[GitHubClientCall::CreatePullRequest {
                api_base: "https://api.github.test".into(),
                repository: "taskfence/example".into(),
                token: token.into(),
                input: GitHubPullRequestInput {
                    title: "Ship connector".into(),
                    head: "codex/connector".into(),
                    base: "main".into(),
                    body: "Bounded GitHub REST PR".into(),
                },
            }]
        );

        let events = audit.events.lock().unwrap();
        assert!(!format!("{events:?}").contains(token));
        assert!(matches!(
            events.as_slice(),
            [
                AuditEvent::PolicyDecision {
                    decision: ActionDecision::RequireApproval { .. },
                    ..
                },
                AuditEvent::ApprovalRequested { .. },
                AuditEvent::ApprovalResolved { record },
                AuditEvent::PolicyDecision {
                    action: Action::Budget { kind, amount },
                    decision: ActionDecision::Allow { .. },
                    ..
                },
                AuditEvent::BudgetUsageRecorded { record: budget, .. },
                AuditEvent::ToolExecutionStarted { request, .. },
                AuditEvent::ToolExecutionFinished { execution, .. },
            ] if record.decision == Some(ApprovalDecision::Approved)
                && kind == "gateway_calls"
                && *amount == 1
                && budget.usage.provider.as_deref() == Some("github")
                && matches!(
                    request.action.parameters.get("authorization"),
                    Some(RedactedValue::Redacted { .. })
                )
                && execution.error.is_none()
        ));
    }

    #[test]
    fn github_rest_comment_issue_uses_mocked_api_client() {
        let token = "ghp_live_test_token";
        let client = RecordingGitHubClient::default();
        let adapter = GitHubRestAdapter::new(github_rest_tool("comment_issue"), client.clone());
        let broker = StaticLiveSecretBroker::new(token);
        let policy = BuiltInPolicyEngine;
        let audit = RecordingAudit::default();
        let registry =
            InMemoryToolRegistry::new([
                RegisteredTool::new("mcp", "github", "comment_issue").unwrap()
            ]);
        let mediator = GatewayMediator::new(&policy, &audit).with_tool_registry(&registry);
        let executor = GatewayExecutor::new(mediator, &audit, &adapter).with_secret_broker(&broker);
        let mut task = task_with_gateway_secret_scope("github.comment_issue");
        task.permissions.tools.allow = vec!["github.comment_issue".into()];
        task.permissions.budget = BudgetPermissions {
            allow: vec![BudgetLimit {
                kind: "gateway_calls".into(),
                max_amount: 1,
            }],
        };

        let execution = executor
            .execute_tool_action(
                &task,
                ToolAction {
                    protocol: "mcp".into(),
                    tool: "github".into(),
                    operation: "comment_issue".into(),
                    parameters: BTreeMap::from([
                        ("number".into(), RedactedValue::Plain("42".into())),
                        ("body".into(), RedactedValue::Plain("Looks good".into())),
                    ]),
                },
                ToolExecutionContext::default(),
            )
            .unwrap();

        assert!(execution.error.is_none());
        assert!(matches!(
            execution.result,
            Some(ToolResult { summary, values, .. })
                if summary == "created GitHub issue comment 99 on taskfence/example#42"
                    && matches!(
                        values.get("comment_id"),
                        Some(RedactedValue::Plain(id)) if id == "99"
                    )
        ));
        assert_eq!(
            client.calls.lock().unwrap().as_slice(),
            &[GitHubClientCall::CreateIssueComment {
                api_base: "https://api.github.test".into(),
                repository: "taskfence/example".into(),
                token: token.into(),
                number: 42,
                body: "Looks good".into(),
            }]
        );
        assert!(!format!("{:?}", audit.events.lock().unwrap()).contains(token));
    }

    #[test]
    fn github_rest_create_branch_runs_after_approval() {
        let token = "ghp_live_test_token";
        let client = RecordingGitHubClient::default();
        let adapter = GitHubRestAdapter::new(github_rest_tool("create_branch"), client.clone());
        let broker = StaticLiveSecretBroker::new(token);
        let policy = BuiltInPolicyEngine;
        let approval = StaticApproval::new(ApprovalDecision::Approved);
        let audit = RecordingAudit::default();
        let registry =
            InMemoryToolRegistry::new([
                RegisteredTool::new("mcp", "github", "create_branch").unwrap()
            ]);
        let mediator = GatewayMediator::new(&policy, &audit)
            .with_tool_registry(&registry)
            .with_approval(&approval);
        let executor = GatewayExecutor::new(mediator, &audit, &adapter).with_secret_broker(&broker);
        let mut task = task_with_gateway_secret_scope("github.create_branch");
        task.permissions.tools.approval_required = vec!["github.create_branch".into()];
        task.permissions.budget = BudgetPermissions {
            allow: vec![BudgetLimit {
                kind: "gateway_calls".into(),
                max_amount: 1,
            }],
        };

        let execution = executor
            .execute_tool_action(
                &task,
                ToolAction {
                    protocol: "mcp".into(),
                    tool: "github".into(),
                    operation: "create_branch".into(),
                    parameters: BTreeMap::from([
                        (
                            "branch".into(),
                            RedactedValue::Plain("codex/connector".into()),
                        ),
                        ("from_ref".into(), RedactedValue::Plain("heads/main".into())),
                    ]),
                },
                ToolExecutionContext::default(),
            )
            .unwrap();

        assert!(execution.error.is_none());
        assert!(matches!(
            execution.result,
            Some(ToolResult { summary, values, .. })
                if summary == "created GitHub branch codex/connector in taskfence/example"
                    && matches!(
                        values.get("branch"),
                        Some(RedactedValue::Plain(branch)) if branch == "codex/connector"
                    )
        ));
        assert_eq!(
            client.calls.lock().unwrap().as_slice(),
            &[GitHubClientCall::CreateBranch {
                api_base: "https://api.github.test".into(),
                repository: "taskfence/example".into(),
                token: token.into(),
                branch: "codex/connector".into(),
                from_ref: "heads/main".into(),
            }]
        );
        assert!(!format!("{:?}", audit.events.lock().unwrap()).contains(token));
    }

    #[test]
    fn github_rest_commit_file_uses_safe_path_branch_and_sha() {
        let token = "ghp_live_test_token";
        let client = RecordingGitHubClient::default();
        let adapter = GitHubRestAdapter::new(github_rest_tool("commit_file"), client.clone());
        let broker = StaticLiveSecretBroker::new(token);
        let policy = BuiltInPolicyEngine;
        let approval = StaticApproval::new(ApprovalDecision::Approved);
        let audit = RecordingAudit::default();
        let registry =
            InMemoryToolRegistry::new([
                RegisteredTool::new("mcp", "github", "commit_file").unwrap()
            ]);
        let mediator = GatewayMediator::new(&policy, &audit)
            .with_tool_registry(&registry)
            .with_approval(&approval);
        let executor = GatewayExecutor::new(mediator, &audit, &adapter).with_secret_broker(&broker);
        let mut task = task_with_gateway_secret_scope("github.commit_file");
        task.permissions.tools.approval_required = vec!["github.commit_file".into()];
        task.permissions.budget = BudgetPermissions {
            allow: vec![BudgetLimit {
                kind: "gateway_calls".into(),
                max_amount: 1,
            }],
        };

        let execution = executor
            .execute_tool_action(
                &task,
                ToolAction {
                    protocol: "mcp".into(),
                    tool: "github".into(),
                    operation: "commit_file".into(),
                    parameters: BTreeMap::from([
                        ("path".into(), RedactedValue::Plain("docs/report.md".into())),
                        (
                            "message".into(),
                            RedactedValue::Plain("Add TaskFence report".into()),
                        ),
                        (
                            "content".into(),
                            RedactedValue::Plain("# TaskFence\n\nReport".into()),
                        ),
                        (
                            "branch".into(),
                            RedactedValue::Plain("codex/connector".into()),
                        ),
                        ("sha".into(), RedactedValue::Plain("abc123def456".into())),
                    ]),
                },
                ToolExecutionContext::default(),
            )
            .unwrap();

        assert!(execution.error.is_none());
        assert!(matches!(
            execution.result,
            Some(ToolResult { summary, values, .. })
                if summary == "committed file docs/report.md in taskfence/example"
                    && matches!(
                        values.get("sha"),
                        Some(RedactedValue::Plain(sha)) if sha == "feedface1234"
                    )
        ));
        assert_eq!(
            client.calls.lock().unwrap().as_slice(),
            &[GitHubClientCall::CommitFile {
                api_base: "https://api.github.test".into(),
                repository: "taskfence/example".into(),
                token: token.into(),
                input: GitHubFileCommitInput {
                    path: "docs/report.md".into(),
                    message: "Add TaskFence report".into(),
                    content: "# TaskFence\n\nReport".into(),
                    branch: "codex/connector".into(),
                    sha: Some("abc123def456".into()),
                },
            }]
        );
    }

    #[test]
    fn github_rest_update_pr_requires_update_fields_and_safe_state() {
        let token = "ghp_live_test_token";
        let client = RecordingGitHubClient::default();
        let adapter = GitHubRestAdapter::new(github_rest_tool("update_pr"), client.clone());
        let broker = StaticLiveSecretBroker::new(token);
        let policy = BuiltInPolicyEngine;
        let audit = RecordingAudit::default();
        let registry =
            InMemoryToolRegistry::new([RegisteredTool::new("mcp", "github", "update_pr").unwrap()]);
        let mediator = GatewayMediator::new(&policy, &audit).with_tool_registry(&registry);
        let executor = GatewayExecutor::new(mediator, &audit, &adapter).with_secret_broker(&broker);
        let mut task = task_with_gateway_secret_scope("github.update_pr");
        task.permissions.tools.allow = vec!["github.update_pr".into()];
        task.permissions.budget = BudgetPermissions {
            allow: vec![BudgetLimit {
                kind: "gateway_calls".into(),
                max_amount: 2,
            }],
        };

        let missing_update = executor
            .execute_tool_action(
                &task,
                ToolAction {
                    protocol: "mcp".into(),
                    tool: "github".into(),
                    operation: "update_pr".into(),
                    parameters: BTreeMap::from([(
                        "number".into(),
                        RedactedValue::Plain("7".into()),
                    )]),
                },
                ToolExecutionContext::default(),
            )
            .unwrap();

        assert!(matches!(
            missing_update.error,
            Some(ToolExecutionError {
                kind: ToolExecutionErrorKind::InvalidParameters,
                ..
            })
        ));
        assert!(client.calls.lock().unwrap().is_empty());

        let execution = executor
            .execute_tool_action(
                &task,
                ToolAction {
                    protocol: "mcp".into(),
                    tool: "github".into(),
                    operation: "update_pr".into(),
                    parameters: BTreeMap::from([
                        ("number".into(), RedactedValue::Plain("7".into())),
                        (
                            "title".into(),
                            RedactedValue::Plain("Ship connector workflow".into()),
                        ),
                        ("state".into(), RedactedValue::Plain("open".into())),
                    ]),
                },
                ToolExecutionContext::default(),
            )
            .unwrap();

        assert!(execution.error.is_none());
        assert_eq!(
            client.calls.lock().unwrap().as_slice(),
            &[GitHubClientCall::UpdatePullRequest {
                api_base: "https://api.github.test".into(),
                repository: "taskfence/example".into(),
                token: token.into(),
                input: GitHubPullRequestUpdateInput {
                    number: 7,
                    title: Some("Ship connector workflow".into()),
                    body: None,
                    state: Some("open".into()),
                    base: None,
                },
            }]
        );
    }

    #[test]
    fn github_rest_comment_report_posts_structured_pr_comment() {
        let token = "ghp_live_test_token";
        let client = RecordingGitHubClient::default();
        let adapter = GitHubRestAdapter::new(github_rest_tool("comment_report"), client.clone());
        let broker = StaticLiveSecretBroker::new(token);
        let policy = BuiltInPolicyEngine;
        let approval = StaticApproval::new(ApprovalDecision::Approved);
        let audit = RecordingAudit::default();
        let registry =
            InMemoryToolRegistry::new([
                RegisteredTool::new("mcp", "github", "comment_report").unwrap()
            ]);
        let mediator = GatewayMediator::new(&policy, &audit)
            .with_tool_registry(&registry)
            .with_approval(&approval);
        let executor = GatewayExecutor::new(mediator, &audit, &adapter).with_secret_broker(&broker);
        let mut task = task_with_gateway_secret_scope("github.comment_report");
        task.permissions.tools.approval_required = vec!["github.comment_report".into()];
        task.permissions.budget = BudgetPermissions {
            allow: vec![BudgetLimit {
                kind: "gateway_calls".into(),
                max_amount: 1,
            }],
        };

        let execution = executor
            .execute_tool_action(
                &task,
                ToolAction {
                    protocol: "mcp".into(),
                    tool: "github".into(),
                    operation: "comment_report".into(),
                    parameters: BTreeMap::from([
                        ("number".into(), RedactedValue::Plain("7".into())),
                        ("title".into(), RedactedValue::Plain("TaskFence run".into())),
                        ("status".into(), RedactedValue::Plain("passed".into())),
                        (
                            "summary".into(),
                            RedactedValue::Plain("All checks passed token=secret".into()),
                        ),
                        (
                            "report_url".into(),
                            RedactedValue::Plain("https://example.invalid/report".into()),
                        ),
                    ]),
                },
                ToolExecutionContext::default(),
            )
            .unwrap();

        assert!(execution.error.is_none());
        let calls = client.calls.lock().unwrap();
        let [GitHubClientCall::CreateIssueComment { number, body, .. }] = calls.as_slice() else {
            panic!("expected report comment call, got {calls:?}");
        };
        assert_eq!(*number, 7);
        assert!(body.contains("## TaskFence run"));
        assert!(body.contains("Status: `passed`"));
        assert!(body.contains("[redacted]"));
        assert!(body.contains("https://example.invalid/report"));
    }

    #[test]
    fn github_rest_commit_file_rejects_path_escape_before_client_call() {
        let client = RecordingGitHubClient::default();
        let adapter = GitHubRestAdapter::new(github_rest_tool("commit_file"), client.clone());
        let broker = StaticLiveSecretBroker::new("ghp_live_test_token");
        let policy = BuiltInPolicyEngine;
        let audit = RecordingAudit::default();
        let registry =
            InMemoryToolRegistry::new([
                RegisteredTool::new("mcp", "github", "commit_file").unwrap()
            ]);
        let mediator = GatewayMediator::new(&policy, &audit).with_tool_registry(&registry);
        let executor = GatewayExecutor::new(mediator, &audit, &adapter).with_secret_broker(&broker);
        let mut task = task_with_gateway_secret_scope("github.commit_file");
        task.permissions.tools.allow = vec!["github.commit_file".into()];
        task.permissions.budget = BudgetPermissions {
            allow: vec![BudgetLimit {
                kind: "gateway_calls".into(),
                max_amount: 1,
            }],
        };

        let execution = executor
            .execute_tool_action(
                &task,
                ToolAction {
                    protocol: "mcp".into(),
                    tool: "github".into(),
                    operation: "commit_file".into(),
                    parameters: BTreeMap::from([
                        ("path".into(), RedactedValue::Plain("../secret".into())),
                        ("message".into(), RedactedValue::Plain("bad".into())),
                        ("content".into(), RedactedValue::Plain("bad".into())),
                        ("branch".into(), RedactedValue::Plain("codex/bad".into())),
                    ]),
                },
                ToolExecutionContext::default(),
            )
            .unwrap();

        assert!(matches!(
            execution.error,
            Some(ToolExecutionError {
                kind: ToolExecutionErrorKind::InvalidParameters,
                ..
            })
        ));
        assert!(client.calls.lock().unwrap().is_empty());
    }

    #[test]
    fn github_rest_unsupported_operation_fails_closed() {
        let client = RecordingGitHubClient::default();
        let adapter = GitHubRestAdapter::new(github_rest_tool("close_issue"), client.clone());
        let broker = StaticLiveSecretBroker::new("ghp_live_test_token");
        let policy = BuiltInPolicyEngine;
        let audit = RecordingAudit::default();
        let registry =
            InMemoryToolRegistry::new([
                RegisteredTool::new("mcp", "github", "close_issue").unwrap()
            ]);
        let mediator = GatewayMediator::new(&policy, &audit).with_tool_registry(&registry);
        let executor = GatewayExecutor::new(mediator, &audit, &adapter).with_secret_broker(&broker);
        let mut task = task_with_gateway_secret_scope("github.close_issue");
        task.permissions.tools.allow = vec!["github.close_issue".into()];
        task.permissions.budget = BudgetPermissions {
            allow: vec![BudgetLimit {
                kind: "gateway_calls".into(),
                max_amount: 1,
            }],
        };

        let execution = executor
            .execute_tool_action(
                &task,
                ToolAction {
                    protocol: "mcp".into(),
                    tool: "github".into(),
                    operation: "close_issue".into(),
                    parameters: BTreeMap::from([(
                        "number".into(),
                        RedactedValue::Plain("42".into()),
                    )]),
                },
                ToolExecutionContext::default(),
            )
            .unwrap();

        assert!(matches!(
            execution.error,
            Some(ToolExecutionError {
                kind: ToolExecutionErrorKind::UnsupportedTool,
                message,
            }) if message.contains("github.close_issue")
        ));
        assert!(client.calls.lock().unwrap().is_empty());
    }

    #[test]
    fn github_enterprise_rest_reuses_bounded_github_adapter_without_auditing_token() {
        let token = "ghp_enterprise_live_secret";
        let client = RecordingGitHubClient::default();
        let mut tool = github_rest_tool("read_issue");
        tool.connector = GatewayConnectorConfig::GitHubEnterpriseRest {
            api_base: "https://github.enterprise.example/api/v3".into(),
            repository: "taskfence/example".into(),
        };
        let adapter = GitHubRestAdapter::new(tool, client.clone());
        let broker = StaticLiveSecretBroker::new(token);
        let policy = StaticPolicy::new(allow());
        let audit = RecordingAudit::default();
        let task = task_with_gateway_secret_scope("github.read_issue");
        let registry =
            InMemoryToolRegistry::new(
                [RegisteredTool::new("mcp", "github", "read_issue").unwrap()],
            );
        let mediator = GatewayMediator::new(&policy, &audit)
            .with_tool_registry(&registry)
            .with_supported_protocols(["mcp"]);
        let executor = GatewayExecutor::new(mediator, &audit, &adapter).with_secret_broker(&broker);

        let execution = executor
            .execute_tool_action(
                &task,
                ToolAction {
                    protocol: "mcp".into(),
                    tool: "github".into(),
                    operation: "read_issue".into(),
                    parameters: BTreeMap::from([(
                        "number".into(),
                        RedactedValue::Plain("42".into()),
                    )]),
                },
                ToolExecutionContext::default(),
            )
            .unwrap();

        assert!(execution.error.is_none());
        assert_eq!(
            client.calls.lock().unwrap().as_slice(),
            &[GitHubClientCall::ReadIssue {
                api_base: "https://github.enterprise.example/api/v3".into(),
                repository: "taskfence/example".into(),
                token: token.into(),
                number: 42,
            }]
        );
        assert_eq!(adapter.identity().kind, "github_enterprise_rest");
        let serialized = serde_json::to_string(&audit.events.lock().unwrap().clone()).unwrap();
        assert!(!serialized.contains(token));
    }

    #[test]
    fn enterprise_connector_templates_define_approval_and_secret_boundaries() {
        let template = connector_policy_template("gitlab").unwrap();

        assert!(template
            .supported_operations
            .contains(&"gitlab.create_merge_request".into()));
        assert!(template
            .approval_required_operations
            .contains(&"gitlab.create_merge_request".into()));
        assert!(template
            .secret_scopes
            .contains(&"gitlab.comment_issue".into()));
        assert!(connector_supports_operation(
            &GatewayConnectorConfig::GitLab {
                api_base: "https://gitlab.example/api/v4".into(),
                project: "group/project".into(),
            },
            "gitlab",
            "create_merge_request"
        ));
        assert!(!connector_supports_operation(
            &GatewayConnectorConfig::GitLab {
                api_base: "https://gitlab.example/api/v4".into(),
                project: "group/project".into(),
            },
            "gitlab",
            "delete_project"
        ));
    }

    #[test]
    fn gitlab_live_connector_builds_bounded_merge_request_without_auditing_token() {
        let token = "gitlab-live-token";
        let client = RecordingEnterpriseHttpClient::default();
        let calls = client.calls.clone();
        let adapter = EnterpriseConnectorAdapter::new(
            enterprise_tool(
                "gitlab",
                "create_merge_request",
                GatewayConnectorConfig::GitLab {
                    api_base: "https://gitlab.example/api/v4".into(),
                    project: "group/project".into(),
                },
            ),
            client,
        );
        let broker = StaticLiveSecretBroker::new(token);
        let policy = BuiltInPolicyEngine;
        let approval = StaticApproval::new(ApprovalDecision::Approved);
        let audit = RecordingAudit::default();
        let registry = InMemoryToolRegistry::new([RegisteredTool::new(
            "mcp",
            "gitlab",
            "create_merge_request",
        )
        .unwrap()]);
        let mediator = GatewayMediator::new(&policy, &audit)
            .with_tool_registry(&registry)
            .with_approval(&approval);
        let executor = GatewayExecutor::new(mediator, &audit, &adapter).with_secret_broker(&broker);
        let mut task = task();
        task.secrets.available_to_gateway = vec![SecretGrant {
            name: "gitlab_token".into(),
            use_for: vec!["gitlab.create_merge_request".into()],
        }];
        task.permissions.tools.approval_required = vec!["gitlab.create_merge_request".into()];
        task.permissions.budget = BudgetPermissions {
            allow: vec![BudgetLimit {
                kind: "gateway_calls".into(),
                max_amount: 1,
            }],
        };

        let execution = executor
            .execute_tool_action(
                &task,
                ToolAction {
                    protocol: "mcp".into(),
                    tool: "gitlab".into(),
                    operation: "create_merge_request".into(),
                    parameters: BTreeMap::from([
                        ("title".into(), RedactedValue::Plain("Ship MR".into())),
                        (
                            "source_branch".into(),
                            RedactedValue::Plain("codex/phase-7".into()),
                        ),
                        ("target_branch".into(), RedactedValue::Plain("main".into())),
                    ]),
                },
                ToolExecutionContext::default(),
            )
            .unwrap();

        assert!(
            execution.error.is_none(),
            "unexpected execution error: {:?}",
            execution.error
        );
        let calls = calls.lock().unwrap();
        let [call] = calls.as_slice() else {
            panic!("expected one GitLab call, got {calls:?}");
        };
        assert_eq!(call.method, "POST");
        assert_eq!(
            call.url,
            "https://gitlab.example/api/v4/projects/group%2Fproject/merge_requests"
        );
        assert_eq!(call.bearer_token, token);
        assert_eq!(call.body["title"], "Ship MR");
        let serialized_events =
            serde_json::to_string(&audit.events.lock().unwrap().clone()).unwrap();
        assert!(!serialized_events.contains(token));
        assert!(serialized_events.contains("gateway secret reference for gitlab_token"));
    }

    #[test]
    fn enterprise_http_connector_rejects_unsafe_parameters_before_client_call() {
        let client = RecordingEnterpriseHttpClient::default();
        let calls = client.calls.clone();
        let adapter = EnterpriseConnectorAdapter::new(
            GatewayToolConfig {
                protocol: "http".into(),
                tool: "internal_http".into(),
                operation: "call".into(),
                connector: GatewayConnectorConfig::InternalHttp {
                    api_base: "https://internal.example/api".into(),
                    service: "ticket-router".into(),
                },
                secret_refs: vec![GatewaySecretReferenceConfig {
                    name: "internal_http_token".into(),
                    parameter: "authorization".into(),
                    scope: "internal_http.call".into(),
                }],
            },
            client,
        );
        let broker = StaticLiveSecretBroker::new("internal-token");
        let policy = BuiltInPolicyEngine;
        let audit = RecordingAudit::default();
        let registry =
            InMemoryToolRegistry::new([
                RegisteredTool::new("http", "internal_http", "call").unwrap()
            ]);
        let mediator = GatewayMediator::new(&policy, &audit)
            .with_tool_registry(&registry)
            .with_supported_protocols(["http"]);
        let executor = GatewayExecutor::new(mediator, &audit, &adapter).with_secret_broker(&broker);
        let mut task = task();
        task.secrets.available_to_gateway = vec![SecretGrant {
            name: "internal_http_token".into(),
            use_for: vec!["internal_http.call".into()],
        }];
        task.permissions.tools.allow = vec!["internal_http.call".into()];
        task.permissions.budget = BudgetPermissions {
            allow: vec![BudgetLimit {
                kind: "gateway_calls".into(),
                max_amount: 1,
            }],
        };

        let execution = executor
            .execute_tool_action(
                &task,
                ToolAction {
                    protocol: "http".into(),
                    tool: "internal_http".into(),
                    operation: "call".into(),
                    parameters: BTreeMap::from([
                        ("path".into(), RedactedValue::Plain("../admin".into())),
                        ("method".into(), RedactedValue::Plain("POST".into())),
                    ]),
                },
                ToolExecutionContext::default(),
            )
            .unwrap();

        assert!(matches!(
            execution.error,
            Some(ToolExecutionError {
                kind: ToolExecutionErrorKind::InvalidParameters,
                ..
            })
        ));
        assert!(calls.lock().unwrap().is_empty());
    }

    #[test]
    fn database_live_connector_enforces_sql_boundary_and_redacts_rows() {
        let client = RecordingDatabaseClient::default();
        let calls = client.calls.clone();
        let adapter = DatabaseConnectorAdapter::new(database_tool("read"), client);
        let broker = StaticLiveSecretBroker::new("postgres://user:secret@db/taskfence");
        let policy = BuiltInPolicyEngine;
        let audit = RecordingAudit::default();
        let registry =
            InMemoryToolRegistry::new([RegisteredTool::new("mcp", "database", "read").unwrap()]);
        let mediator = GatewayMediator::new(&policy, &audit).with_tool_registry(&registry);
        let executor = GatewayExecutor::new(mediator, &audit, &adapter).with_secret_broker(&broker);
        let mut task = task();
        task.secrets.available_to_gateway = vec![SecretGrant {
            name: "database_credential".into(),
            use_for: vec!["database.read".into()],
        }];
        task.permissions.tools.allow = vec!["database.read".into()];
        task.permissions.budget = BudgetPermissions {
            allow: vec![BudgetLimit {
                kind: "gateway_calls".into(),
                max_amount: 1,
            }],
        };

        let execution = executor
            .execute_tool_action(
                &task,
                ToolAction {
                    protocol: "mcp".into(),
                    tool: "database".into(),
                    operation: "read".into(),
                    parameters: BTreeMap::from([
                        (
                            "query".into(),
                            RedactedValue::Plain("select status from task_runs".into()),
                        ),
                        ("max_rows".into(), RedactedValue::Plain("10".into())),
                    ]),
                },
                ToolExecutionContext::default(),
            )
            .unwrap();

        assert!(execution.error.is_none());
        let result = execution.result.unwrap();
        assert!(matches!(
            result.values.get("rows"),
            Some(RedactedValue::Plain(rows)) if rows.contains("[redacted]")
        ));
        let calls = calls.lock().unwrap();
        let [(credential, statement)] = calls.as_slice() else {
            panic!("expected database call, got {calls:?}");
        };
        assert!(credential.starts_with("postgres://"));
        assert!(statement.read_only);
        assert_eq!(statement.max_rows, 10);
        let serialized_events =
            serde_json::to_string(&audit.events.lock().unwrap().clone()).unwrap();
        assert!(!serialized_events.contains("postgres://user:secret"));
    }

    #[test]
    fn database_read_rejects_mutating_statement_before_client_call() {
        let client = RecordingDatabaseClient::default();
        let calls = client.calls.clone();
        let adapter = DatabaseConnectorAdapter::new(database_tool("read"), client);
        let broker = StaticLiveSecretBroker::new("postgres://user:secret@db/taskfence");
        let policy = BuiltInPolicyEngine;
        let audit = RecordingAudit::default();
        let registry =
            InMemoryToolRegistry::new([RegisteredTool::new("mcp", "database", "read").unwrap()]);
        let mediator = GatewayMediator::new(&policy, &audit).with_tool_registry(&registry);
        let executor = GatewayExecutor::new(mediator, &audit, &adapter).with_secret_broker(&broker);
        let mut task = task();
        task.secrets.available_to_gateway = vec![SecretGrant {
            name: "database_credential".into(),
            use_for: vec!["database.read".into()],
        }];
        task.permissions.tools.allow = vec!["database.read".into()];
        task.permissions.budget = BudgetPermissions {
            allow: vec![BudgetLimit {
                kind: "gateway_calls".into(),
                max_amount: 1,
            }],
        };

        let execution = executor
            .execute_tool_action(
                &task,
                ToolAction {
                    protocol: "mcp".into(),
                    tool: "database".into(),
                    operation: "read".into(),
                    parameters: BTreeMap::from([(
                        "query".into(),
                        RedactedValue::Plain("delete from task_runs".into()),
                    )]),
                },
                ToolExecutionContext::default(),
            )
            .unwrap();

        assert!(matches!(
            execution.error,
            Some(ToolExecutionError {
                kind: ToolExecutionErrorKind::InvalidParameters,
                ..
            })
        ));
        assert!(calls.lock().unwrap().is_empty());
    }

    #[test]
    fn production_gateway_transport_contract_prioritizes_mcp_before_other_surfaces() {
        let transports = GatewayTransportHardening::production_priority_order();

        assert_eq!(transports[0].kind, GatewayTransportKind::McpServer);
        assert_eq!(
            transports[0].status,
            GatewayTransportStatus::ReadyForImplementation
        );
        assert_eq!(transports[1].kind, GatewayTransportKind::BoundedHttpAdapter);
        assert_eq!(
            transports[2].kind,
            GatewayTransportKind::SdkWebhookEntryPoint
        );
        for transport in &transports[..3] {
            assert!(transport.request_authentication);
            assert!(transport.structured_error_schema);
            assert!(transport.response_redaction);
            assert!(transport.destination_policy_required);
            assert!(transport.secret_broker_required);
            assert!(transport.limits.max_request_bytes > 0);
            assert!(transport
                .compatible_local_surfaces
                .iter()
                .any(|surface| surface == "taskfence gateway call"));
            assert!(transport
                .compatible_local_surfaces
                .iter()
                .any(|surface| surface == "taskfence gateway listen"));
            assert!(transport
                .compatible_local_surfaces
                .iter()
                .any(|surface| surface == "taskfence gateway spool process"));
        }
    }

    #[test]
    fn arbitrary_http_proxy_transport_is_explicitly_unsupported() {
        let transports = GatewayTransportHardening::production_priority_order();
        let proxy = transports
            .iter()
            .find(|transport| transport.kind == GatewayTransportKind::ArbitraryHttpProxy)
            .unwrap();

        assert_eq!(proxy.status, GatewayTransportStatus::Unsupported);
        assert!(matches!(
            proxy.unsupported_start_error(),
            Some(TaskFenceError::Unsupported(message))
                if message.contains("arbitrary HTTP proxying is unsupported")
                    && message.contains("destination policy")
                    && message.contains("bypass controls")
        ));
    }

    #[test]
    fn siem_export_connector_posts_structured_events_to_sink() {
        let token = "siem-live-token";
        let client = RecordingEnterpriseHttpClient::default();
        let calls = client.calls.clone();
        let adapter = EnterpriseConnectorAdapter::new(
            enterprise_tool(
                "siem",
                "export_events",
                GatewayConnectorConfig::SiemExport {
                    api_base: "https://siem.example/api".into(),
                    sink: "soc-pipeline".into(),
                },
            ),
            client,
        );
        let broker = StaticLiveSecretBroker::new(token);
        let policy = BuiltInPolicyEngine;
        let approval = StaticApproval::new(ApprovalDecision::Approved);
        let audit = RecordingAudit::default();
        let registry =
            InMemoryToolRegistry::new([
                RegisteredTool::new("mcp", "siem", "export_events").unwrap()
            ]);
        let mediator = GatewayMediator::new(&policy, &audit)
            .with_tool_registry(&registry)
            .with_approval(&approval);
        let executor = GatewayExecutor::new(mediator, &audit, &adapter).with_secret_broker(&broker);
        let mut task = task();
        task.secrets.available_to_gateway = vec![SecretGrant {
            name: "siem_token".into(),
            use_for: vec!["siem.export_events".into()],
        }];
        task.permissions.tools.approval_required = vec!["siem.export_events".into()];
        task.permissions.budget = BudgetPermissions {
            allow: vec![BudgetLimit {
                kind: "gateway_calls".into(),
                max_amount: 1,
            }],
        };

        let execution = executor
            .execute_tool_action(
                &task,
                ToolAction {
                    protocol: "mcp".into(),
                    tool: "siem".into(),
                    operation: "export_events".into(),
                    parameters: BTreeMap::from([
                        ("task_id".into(), RedactedValue::Plain("task-7".into())),
                        (
                            "events_json".into(),
                            RedactedValue::Plain("[{\"kind\":\"policy\"}]".into()),
                        ),
                        (
                            "summary".into(),
                            RedactedValue::Plain("policy export token=secret".into()),
                        ),
                    ]),
                },
                ToolExecutionContext::default(),
            )
            .unwrap();

        assert!(
            execution.error.is_none(),
            "unexpected execution error: {:?}",
            execution.error
        );
        let calls = calls.lock().unwrap();
        let [call] = calls.as_slice() else {
            panic!("expected SIEM call, got {calls:?}");
        };
        assert_eq!(
            call.url,
            "https://siem.example/api/sinks/soc-pipeline/events"
        );
        assert_eq!(call.body["sink"], "soc-pipeline");
        assert_eq!(call.body["events"][0]["kind"], "policy");
        assert_eq!(call.bearer_token, token);
        assert!(
            !serde_json::to_string(&audit.events.lock().unwrap().clone())
                .unwrap()
                .contains(token)
        );
    }

    #[test]
    fn enterprise_connector_contract_fails_closed_after_policy_and_secret_reference() {
        let tool = contract_connector_tool(
            "gitlab",
            "create_merge_request",
            GatewayConnectorConfig::GitLab {
                api_base: "https://gitlab.example/api/v4".into(),
                project: "group/project".into(),
            },
        );
        let adapter = UnsupportedGatewayAdapter::for_contract_tool(tool);
        let broker = StaticSecretBroker::default();
        let mut task = task();
        task.secrets.available_to_gateway = vec![SecretGrant {
            name: "gitlab_token".into(),
            use_for: vec!["gitlab.create_merge_request".into()],
        }];
        task.permissions.tools.allow = vec!["gitlab.create_merge_request".into()];
        let policy = StaticPolicy::new(allow());
        let audit = RecordingAudit::default();
        let registry = InMemoryToolRegistry::new([RegisteredTool::new(
            "mcp",
            "gitlab",
            "create_merge_request",
        )
        .unwrap()]);
        let mediator = GatewayMediator::new(&policy, &audit)
            .with_tool_registry(&registry)
            .with_supported_protocols(["mcp"]);
        let executor = GatewayExecutor::new(mediator, &audit, &adapter).with_secret_broker(&broker);

        let execution = executor
            .execute_tool_action(
                &task,
                ToolAction {
                    protocol: "mcp".into(),
                    tool: "gitlab".into(),
                    operation: "create_merge_request".into(),
                    parameters: BTreeMap::from([(
                        "title".into(),
                        RedactedValue::Plain("Ship MR".into()),
                    )]),
                },
                ToolExecutionContext::default(),
            )
            .unwrap();

        assert!(matches!(
            execution.error,
            Some(ToolExecutionError {
                kind: ToolExecutionErrorKind::UnsupportedTool,
                message,
            }) if message.contains("contract-only")
        ));
        assert!(matches!(
            execution.request.action.parameters.get("authorization"),
            Some(RedactedValue::Redacted { .. })
        ));
        assert!(audit.events.lock().unwrap().iter().any(|event| {
            matches!(event, AuditEvent::ToolExecutionStarted { request, .. }
            if matches!(
                request.action.parameters.get("authorization"),
                Some(RedactedValue::Redacted { .. })
            ))
        }));
    }

    #[test]
    fn enterprise_connector_contract_rejects_template_unsupported_operation() {
        let tool = contract_connector_tool(
            "gitlab",
            "delete_project",
            GatewayConnectorConfig::GitLab {
                api_base: "https://gitlab.example/api/v4".into(),
                project: "group/project".into(),
            },
        );
        let adapter = UnsupportedGatewayAdapter::for_contract_tool(tool);
        let broker = StaticSecretBroker::default();
        let mut task = task();
        task.secrets.available_to_gateway = vec![SecretGrant {
            name: "gitlab_token".into(),
            use_for: vec!["gitlab.delete_project".into()],
        }];
        task.permissions.tools.allow = vec!["gitlab.delete_project".into()];
        let policy = StaticPolicy::new(allow());
        let audit = RecordingAudit::default();
        let registry =
            InMemoryToolRegistry::new([
                RegisteredTool::new("mcp", "gitlab", "delete_project").unwrap()
            ]);
        let mediator = GatewayMediator::new(&policy, &audit)
            .with_tool_registry(&registry)
            .with_supported_protocols(["mcp"]);
        let executor = GatewayExecutor::new(mediator, &audit, &adapter).with_secret_broker(&broker);

        let execution = executor
            .execute_tool_action(
                &task,
                ToolAction {
                    protocol: "mcp".into(),
                    tool: "gitlab".into(),
                    operation: "delete_project".into(),
                    parameters: BTreeMap::new(),
                },
                ToolExecutionContext::default(),
            )
            .unwrap();

        assert!(matches!(
            execution.error,
            Some(ToolExecutionError {
                kind: ToolExecutionErrorKind::UnsupportedTool,
                message,
            }) if message.contains("template does not support")
        ));
    }

    #[test]
    fn github_rest_missing_live_token_fails_before_api_client_call() {
        let client = RecordingGitHubClient::default();
        let adapter = GitHubRestAdapter::new(github_rest_tool("read_issue"), client.clone());
        let broker = StaticSecretBroker::default();
        let policy = BuiltInPolicyEngine;
        let audit = RecordingAudit::default();
        let registry =
            InMemoryToolRegistry::new(
                [RegisteredTool::new("mcp", "github", "read_issue").unwrap()],
            );
        let mediator = GatewayMediator::new(&policy, &audit).with_tool_registry(&registry);
        let executor = GatewayExecutor::new(mediator, &audit, &adapter).with_secret_broker(&broker);
        let mut task = task_with_gateway_secret_scope("github.read_issue");
        task.permissions.tools.allow = vec!["github.read_issue".into()];
        task.permissions.budget = BudgetPermissions {
            allow: vec![BudgetLimit {
                kind: "gateway_calls".into(),
                max_amount: 1,
            }],
        };

        let execution = executor
            .execute_tool_action(
                &task,
                ToolAction {
                    protocol: "mcp".into(),
                    tool: "github".into(),
                    operation: "read_issue".into(),
                    parameters: BTreeMap::from([(
                        "number".into(),
                        RedactedValue::Plain("42".into()),
                    )]),
                },
                ToolExecutionContext::default(),
            )
            .unwrap();

        assert!(matches!(
            execution.error,
            Some(ToolExecutionError {
                kind: ToolExecutionErrorKind::SecretUnavailable,
                message,
            }) if message.contains("not backed by a live credential")
        ));
        assert!(client.calls.lock().unwrap().is_empty());
    }

    #[test]
    fn spool_request_round_trips_and_writes_response_under_task_root() {
        let temp = tempfile::tempdir().unwrap();
        let spool_root = Utf8PathBuf::from_path_buf(temp.path().join("gateway-spool")).unwrap();
        let paths = GatewaySpoolPaths::new(spool_root).unwrap();
        fs::create_dir_all(&paths.requests_dir).unwrap();
        fs::create_dir_all(&paths.responses_dir).unwrap();
        let request_path = paths.request_path("request-1").unwrap();
        let request = GatewaySpoolRequest {
            request_id: "request-1".into(),
            action: tool_action("mcp"),
            timeout_seconds: Some(30),
            cancel: false,
        };
        fs::write(&request_path, serde_json::to_vec_pretty(&request).unwrap()).unwrap();

        let parsed = read_gateway_spool_request(&paths, &request_path).unwrap();

        assert_eq!(parsed.request_id, "request-1");
        assert_eq!(parsed.action.protocol, "mcp");
        assert_eq!(parsed.action.tool, "github");
        assert_eq!(parsed.action.operation, "create_pr");
        assert_eq!(parsed.timeout_seconds, Some(30));

        let response = GatewaySpoolResponse::error(
            "request-1",
            GatewaySpoolResponseState::Cancelled,
            ToolExecutionErrorKind::AdapterFailed,
            "cancelled by test",
        );
        let response_path = write_gateway_spool_response(&paths, &response).unwrap();
        let written =
            serde_json::from_slice::<GatewaySpoolResponse>(&fs::read(&response_path).unwrap())
                .unwrap();

        assert_eq!(response_path, paths.response_path("request-1").unwrap());
        assert_eq!(written.state, GatewaySpoolResponseState::Cancelled);
        assert_eq!(
            written.error.as_ref().map(|error| &error.kind),
            Some(&ToolExecutionErrorKind::AdapterFailed)
        );
        assert!(write_gateway_spool_response(&paths, &response)
            .unwrap_err()
            .to_string()
            .contains("already exists"));
    }

    #[test]
    fn spool_request_id_must_match_request_file_name() {
        let temp = tempfile::tempdir().unwrap();
        let spool_root = Utf8PathBuf::from_path_buf(temp.path().join("gateway-spool")).unwrap();
        let paths = GatewaySpoolPaths::new(spool_root).unwrap();
        fs::create_dir_all(&paths.requests_dir).unwrap();
        let request_path = paths.request_path("file-id").unwrap();
        let request = GatewaySpoolRequest {
            request_id: "body-id".into(),
            action: tool_action("mcp"),
            timeout_seconds: None,
            cancel: false,
        };
        fs::write(&request_path, serde_json::to_vec_pretty(&request).unwrap()).unwrap();

        let err = read_gateway_spool_request(&paths, &request_path).unwrap_err();

        assert!(err.to_string().contains("does not match request file"));
    }

    #[test]
    fn spool_request_rejects_parent_components() {
        let temp = tempfile::tempdir().unwrap();
        let spool_root = Utf8PathBuf::from_path_buf(temp.path().join("gateway-spool")).unwrap();
        let paths = GatewaySpoolPaths::new(spool_root).unwrap();
        fs::create_dir_all(&paths.requests_dir).unwrap();
        let escape = paths.requests_dir.join("../escape.json");

        let err = read_gateway_spool_request(&paths, &escape).unwrap_err();

        assert!(err.to_string().contains("must not contain '..'"));
    }

    #[cfg(unix)]
    #[test]
    fn spool_request_rejects_symlinked_request_file() {
        let temp = tempfile::tempdir().unwrap();
        let spool_root = Utf8PathBuf::from_path_buf(temp.path().join("gateway-spool")).unwrap();
        let paths = GatewaySpoolPaths::new(spool_root).unwrap();
        fs::create_dir_all(&paths.requests_dir).unwrap();
        let outside = Utf8PathBuf::from_path_buf(temp.path().join("outside.json")).unwrap();
        fs::write(&outside, "{}").unwrap();
        let request_path = paths.request_path("linked").unwrap();
        std::os::unix::fs::symlink(&outside, &request_path).unwrap();

        let err = read_gateway_spool_request(&paths, &request_path).unwrap_err();

        assert!(err.to_string().contains("must not be a symlink"));
    }
}
