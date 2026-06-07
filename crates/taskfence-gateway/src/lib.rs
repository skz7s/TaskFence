use camino::{Utf8Path, Utf8PathBuf};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use std::fs::{self, OpenOptions};

use taskfence_core::{
    Action, ActionDecision, ApprovalDecision, ApprovalEngine, ApprovalRecord, AuditEvent,
    AuditLogger, GatewayConnectorConfig, GatewaySecretReferenceConfig, GatewayToolConfig,
    PolicyEngine, RedactedValue, ResolvedTask, TaskFenceError, ToolAction, ToolAdapterIdentity,
    ToolExecution, ToolExecutionContext, ToolExecutionError, ToolExecutionErrorKind, ToolRequest,
    ToolResult, GATEWAY_SPOOL_DIR_NAME, GATEWAY_SPOOL_REQUESTS_DIR_NAME,
    GATEWAY_SPOOL_RESPONSES_DIR_NAME,
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
                | ToolExecutionErrorKind::ApprovalDeniedOrTimedOut,
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
        task: &ResolvedTask,
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
        request: McpToolRequest,
    ) -> taskfence_core::Result<UnsupportedToolExecution> {
        let action = self.to_tool_action(request)?;
        Err(TaskFenceError::Unsupported(format!(
            "mcp gateway execution is not implemented for {}.{}",
            action.tool, action.operation
        )))
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
        request: HttpToolRequest,
    ) -> taskfence_core::Result<UnsupportedToolExecution> {
        let action = self.to_tool_action(request)?;
        Err(TaskFenceError::Unsupported(format!(
            "http gateway execution is not implemented for {}.{}",
            action.tool, action.operation
        )))
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

        let request = match self.attach_configured_secret_references(task, request.clone()) {
            Ok(request) => request,
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

        let execution = match self.adapter.execute(task, &request.action, &context) {
            Ok(result) => ToolExecution {
                request,
                result: Some(result),
                error: None,
            },
            Err(error) => ToolExecution {
                request,
                result: None,
                error: Some(error),
            },
        };
        self.record_finished(task, execution)
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
    ) -> std::result::Result<ToolRequest, ToolExecutionError> {
        if self.adapter.secret_references().is_empty() {
            return Ok(request);
        }

        let broker = self.secret_broker.ok_or_else(|| ToolExecutionError {
            kind: ToolExecutionErrorKind::SecretUnavailable,
            message: "gateway secret reference broker is not configured".into(),
        })?;

        let mut action = request.action;
        for secret_ref in self.adapter.secret_references() {
            let reference =
                gateway_secret_reference(task, broker, &secret_ref.name, &secret_ref.scope)
                    .map_err(|err| tool_execution_error_from_taskfence(&err))?;
            action = attach_secret_reference(action, &secret_ref.parameter, &reference)
                .map_err(|err| tool_execution_error_from_taskfence(&err))?;
        }

        Ok(ToolRequest {
            action,
            adapter: request.adapter,
        })
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
pub struct UnsupportedGatewayAdapter {
    identity: ToolAdapterIdentity,
}

impl UnsupportedGatewayAdapter {
    pub fn new(kind: impl Into<String>, name: impl Into<String>) -> Self {
        Self {
            identity: ToolAdapterIdentity {
                kind: kind.into(),
                name: name.into(),
            },
        }
    }
}

impl ToolAdapter for UnsupportedGatewayAdapter {
    fn identity(&self) -> ToolAdapterIdentity {
        self.identity.clone()
    }

    fn execute(
        &self,
        _task: &ResolvedTask,
        action: &ToolAction,
        _context: &ToolExecutionContext,
    ) -> std::result::Result<ToolResult, ToolExecutionError> {
        Err(ToolExecutionError {
            kind: ToolExecutionErrorKind::UnsupportedTool,
            message: format!(
                "{} adapter does not support {}.{}",
                self.identity.name, action.tool, action.operation
            ),
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
        match &self.tool.connector {
            GatewayConnectorConfig::LocalFixture { kind, .. } => ToolAdapterIdentity {
                kind: "local_fixture".into(),
                name: kind.clone(),
            },
            GatewayConnectorConfig::Unsupported { kind } => ToolAdapterIdentity {
                kind: "unsupported".into(),
                name: kind.clone(),
            },
        }
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
    use std::sync::Mutex;
    use taskfence_core::{
        AgentConfig, AgentKind, ApprovalConfig, ApprovalId, AuditConfig, LimitConfig,
        PermissionConfig, SandboxConfig, SandboxKind, SecretConfig, SecretGrant, TaskId,
        ToolPermissions,
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
    struct StaticToolAdapter {
        outcome: Mutex<std::result::Result<ToolResult, ToolExecutionError>>,
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
                })),
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
                calls: Mutex::new(Vec::new()),
                secret_refs: Vec::new(),
            }
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
                limits: LimitConfig::default(),
            },
            permissions: PermissionConfig::default(),
            secrets: SecretConfig::default(),
            approval: ApprovalConfig::default(),
            gateway: Default::default(),
            audit: AuditConfig::default(),
        }
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
    fn mcp_adapter_execution_is_explicitly_unsupported() {
        let adapter = McpGatewayAdapter;

        let err = adapter
            .execute(McpToolRequest {
                server: "github".into(),
                tool: "read_issue".into(),
                arguments: BTreeMap::new(),
            })
            .unwrap_err();

        assert!(matches!(
            err,
            TaskFenceError::Unsupported(message)
                if message.contains("mcp gateway execution is not implemented")
                    && message.contains("github.read_issue")
        ));
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
    fn http_adapter_execution_is_explicitly_unsupported() {
        let adapter = HttpGatewayAdapter;

        let err = adapter
            .execute(HttpToolRequest {
                connector: "github".into(),
                operation: "create_pr".into(),
                parameters: BTreeMap::new(),
            })
            .unwrap_err();

        assert!(matches!(
            err,
            TaskFenceError::Unsupported(message)
                if message.contains("http gateway execution is not implemented")
                    && message.contains("github.create_pr")
        ));
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
        let mut task = task();
        task.secrets.available_to_gateway = vec![SecretGrant {
            name: "github_token".into(),
            use_for: vec!["github.create_pr".into()],
        }];
        task
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
