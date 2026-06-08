use camino::{Utf8Path, Utf8PathBuf};
use serde::Deserialize;
use std::fs;
use std::io;
use taskfence_core::{
    AgentConfig, AgentKind, ApprovalConfig, AuditConfig, BudgetLimit, BudgetPermissions,
    CaptureConfig, CommandPermissions, EnvPermissions, GatewayConfig, GatewayConnectorConfig,
    GatewayEgressConfig, GatewayMode, GatewaySecretReferenceConfig, GatewayToolConfig, LimitConfig,
    NetworkDefault, NetworkPermissions, PathPermissions, PermissionConfig, ReportConfig,
    ReportFormat, ResolvedTask, SandboxConfig, SandboxKind, SecretConfig, SecretGrant,
    SshNetworkPolicy, SshSandboxConfig, TaskFenceError, TaskId, ToolPermissions,
};

pub fn load_task_file(path: impl AsRef<Utf8Path>) -> taskfence_core::Result<ResolvedTask> {
    let task_file = path.as_ref();
    let contents = fs::read_to_string(task_file)
        .map_err(|err| TaskFenceError::Config(format!("failed to read {task_file}: {err}")))?;
    parse_task_file(task_file, &contents)
}

pub fn parse_task_file(
    task_file: impl AsRef<Utf8Path>,
    contents: &str,
) -> taskfence_core::Result<ResolvedTask> {
    let task_file = make_absolute(task_file.as_ref())?;
    let raw: RawTask = serde_yaml::from_str(contents)
        .map_err(|err| TaskFenceError::Config(format!("invalid task yaml: {err}")))?;
    raw.resolve(&task_file)
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct RawTask {
    id: Option<String>,
    goal: String,
    workspace: Utf8PathBuf,
    agent: RawAgent,
    sandbox: RawSandbox,
    #[serde(default)]
    permissions: RawPermissions,
    #[serde(default)]
    secrets: RawSecrets,
    #[serde(default)]
    approval: RawApproval,
    #[serde(default)]
    gateway: RawGateway,
    #[serde(default)]
    audit: RawAudit,
}

impl RawTask {
    fn resolve(self, task_file: &Utf8Path) -> taskfence_core::Result<ResolvedTask> {
        if self.goal.trim().is_empty() {
            return Err(TaskFenceError::Config("goal must not be empty".into()));
        }

        let base_dir = task_file.parent().unwrap_or_else(|| Utf8Path::new("."));
        if escapes_with_parent(&self.workspace) {
            return Err(TaskFenceError::Config(
                "workspace must not contain '..'".into(),
            ));
        }
        let workspace = resolve_existing_workspace(base_dir, &self.workspace)?;

        let permissions = self.permissions.resolve(base_dir, &workspace)?;
        if self.secrets.expose_to_agent {
            return Err(TaskFenceError::Config(
                "secrets.expose_to_agent=true is not allowed without an explicit high-risk override"
                    .into(),
            ));
        }

        let gateway = self.gateway.resolve(base_dir, &workspace)?;

        Ok(ResolvedTask {
            id: self.id.map(TaskId).unwrap_or_else(|| TaskId::new("task")),
            task_file: task_file.to_path_buf(),
            goal: self.goal,
            workspace_host_path: workspace,
            workspace_container_path: Utf8PathBuf::from("/workspace"),
            agent: self.agent.resolve()?,
            sandbox: self.sandbox.resolve()?,
            permissions,
            secrets: self.secrets.resolve(),
            approval: self.approval.resolve()?,
            gateway,
            audit: self.audit.resolve()?,
        })
    }
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct RawAgent {
    #[serde(default = "default_agent_type", rename = "type")]
    kind: String,
    command: String,
    #[serde(default)]
    args: Vec<String>,
}

impl RawAgent {
    fn resolve(self) -> taskfence_core::Result<AgentConfig> {
        if self.command.trim().is_empty() {
            return Err(TaskFenceError::Config(
                "agent.command must not be empty".into(),
            ));
        }

        Ok(AgentConfig {
            kind: match self.kind.as_str() {
                "generic" => AgentKind::Generic,
                other => AgentKind::Specialized(other.to_owned()),
            },
            command: self.command,
            args: self.args,
        })
    }
}

fn default_agent_type() -> String {
    "generic".into()
}

#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
struct RawSandbox {
    #[serde(rename = "type")]
    kind: Option<String>,
    image: Option<String>,
    ssh: Option<RawSshSandbox>,
    #[serde(default)]
    limits: LimitConfig,
}

impl RawSandbox {
    fn resolve(self) -> taskfence_core::Result<SandboxConfig> {
        let kind = match self.kind.as_deref() {
            Some("docker") => SandboxKind::Docker,
            Some("remote_ssh") => SandboxKind::RemoteSsh,
            Some("kubernetes_job") => SandboxKind::KubernetesJob,
            Some("microvm") => SandboxKind::MicroVm,
            Some("managed_cloud") => SandboxKind::ManagedCloud,
            Some(other) => SandboxKind::Unsupported(other.to_owned()),
            None => return Err(TaskFenceError::Config("sandbox.type is required".into())),
        };

        if self.limits.timeout_minutes == Some(0) {
            return Err(TaskFenceError::Config(
                "sandbox.limits.timeout_minutes must be positive".into(),
            ));
        }
        let ssh = match (kind.clone(), self.ssh) {
            (SandboxKind::RemoteSsh, Some(ssh)) => Some(ssh.resolve()?),
            (SandboxKind::RemoteSsh, None) => {
                return Err(TaskFenceError::Config(
                    "sandbox.ssh is required for remote_ssh tasks".into(),
                ));
            }
            (_, Some(_)) => {
                return Err(TaskFenceError::Config(
                    "sandbox.ssh is only supported when sandbox.type is remote_ssh".into(),
                ));
            }
            (_, None) => None,
        };

        Ok(SandboxConfig {
            kind,
            image: self.image,
            ssh,
            limits: self.limits,
        })
    }
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct RawSshSandbox {
    host: String,
    #[serde(default)]
    user: Option<String>,
    #[serde(default)]
    port: Option<u16>,
    #[serde(default)]
    workspace: Option<Utf8PathBuf>,
    #[serde(default)]
    identity_file: Option<Utf8PathBuf>,
    #[serde(default)]
    known_hosts_file: Option<Utf8PathBuf>,
    #[serde(default)]
    isolated_workspace: bool,
    #[serde(default)]
    isolated_secrets: bool,
    #[serde(default)]
    terminates_remote_processes: bool,
    #[serde(default)]
    enforces_resource_limits: bool,
    #[serde(default)]
    network_policy: Option<RawSshNetworkPolicy>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "snake_case")]
enum RawSshNetworkPolicy {
    UncontrolledAllow,
}

impl RawSshSandbox {
    fn resolve(self) -> taskfence_core::Result<SshSandboxConfig> {
        let host = normalize_ssh_segment("sandbox.ssh.host", self.host)?;
        let user = self
            .user
            .map(|value| normalize_ssh_segment("sandbox.ssh.user", value))
            .transpose()?;
        if self.port == Some(0) {
            return Err(TaskFenceError::Config(
                "sandbox.ssh.port must be positive".into(),
            ));
        }
        if let Some(path) = &self.workspace {
            reject_absolute_path_escape("sandbox.ssh.workspace", path)?;
        }
        if let Some(path) = &self.identity_file {
            reject_absolute_path_escape("sandbox.ssh.identity_file", path)?;
        }
        if let Some(path) = &self.known_hosts_file {
            reject_absolute_path_escape("sandbox.ssh.known_hosts_file", path)?;
        }
        Ok(SshSandboxConfig {
            host,
            user,
            port: self.port,
            workspace: self.workspace,
            identity_file: self.identity_file,
            known_hosts_file: self.known_hosts_file,
            isolated_workspace: self.isolated_workspace,
            isolated_secrets: self.isolated_secrets,
            terminates_remote_processes: self.terminates_remote_processes,
            enforces_resource_limits: self.enforces_resource_limits,
            network_policy: self.network_policy.map(|policy| match policy {
                RawSshNetworkPolicy::UncontrolledAllow => SshNetworkPolicy::UncontrolledAllow,
            }),
        })
    }
}

fn normalize_ssh_segment(field: &str, value: String) -> taskfence_core::Result<String> {
    let value = value.trim();
    if value.is_empty()
        || value.contains('\0')
        || value.contains('@')
        || value.chars().any(char::is_control)
    {
        return Err(TaskFenceError::Config(format!(
            "{field} must be a non-empty SSH segment without control characters or '@'"
        )));
    }
    Ok(value.into())
}

fn reject_absolute_path_escape(field: &str, path: &Utf8Path) -> taskfence_core::Result<()> {
    if !path.is_absolute() || escapes_with_parent(path) || path.as_str().contains('\0') {
        return Err(TaskFenceError::Config(format!(
            "{field} must be an absolute path without '..' or NUL"
        )));
    }
    Ok(())
}

#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
struct RawPermissions {
    #[serde(default)]
    paths: RawPathPermissions,
    #[serde(default)]
    commands: CommandPermissions,
    #[serde(default)]
    network: RawNetworkPermissions,
    #[serde(default)]
    env: EnvPermissions,
    #[serde(default)]
    tools: ToolPermissions,
    #[serde(default)]
    budget: RawBudgetPermissions,
}

impl RawPermissions {
    fn resolve(
        self,
        base_dir: &Utf8Path,
        workspace: &Utf8Path,
    ) -> taskfence_core::Result<PermissionConfig> {
        Ok(PermissionConfig {
            paths: self.paths.resolve(base_dir, workspace)?,
            commands: self.commands,
            network: self.network.resolve()?,
            env: self.env,
            tools: self.tools,
            budget: self.budget.resolve()?,
        })
    }
}

#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
struct RawPathPermissions {
    #[serde(default)]
    read: Vec<Utf8PathBuf>,
    #[serde(default)]
    write: Vec<Utf8PathBuf>,
}

impl RawPathPermissions {
    fn resolve(
        self,
        base_dir: &Utf8Path,
        workspace: &Utf8Path,
    ) -> taskfence_core::Result<PathPermissions> {
        Ok(PathPermissions {
            read: resolve_paths(base_dir, workspace, self.read)?,
            write: resolve_paths(base_dir, workspace, self.write)?,
        })
    }
}

#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
struct RawNetworkPermissions {
    default: Option<String>,
    #[serde(default)]
    allow_domains: Vec<String>,
}

impl RawNetworkPermissions {
    fn resolve(self) -> taskfence_core::Result<NetworkPermissions> {
        let default = match self.default.as_deref() {
            Some("allow") => NetworkDefault::Allow,
            Some("disabled") => NetworkDefault::Disabled,
            Some("deny") | None => NetworkDefault::Deny,
            Some(other) => {
                return Err(TaskFenceError::Config(format!(
                    "permissions.network.default must be allow, deny, or disabled, got {other}"
                )));
            }
        };
        Ok(NetworkPermissions {
            default,
            allow_domains: self
                .allow_domains
                .into_iter()
                .map(validate_domain)
                .collect::<taskfence_core::Result<Vec<_>>>()?,
        })
    }
}

#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
struct RawBudgetPermissions {
    #[serde(default)]
    allow: Vec<RawBudgetLimit>,
}

impl RawBudgetPermissions {
    fn resolve(self) -> taskfence_core::Result<BudgetPermissions> {
        Ok(BudgetPermissions {
            allow: self
                .allow
                .into_iter()
                .map(RawBudgetLimit::resolve)
                .collect::<taskfence_core::Result<Vec<_>>>()?,
        })
    }
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct RawBudgetLimit {
    kind: String,
    max_amount: u64,
}

impl RawBudgetLimit {
    fn resolve(self) -> taskfence_core::Result<BudgetLimit> {
        let kind = normalize_budget_kind(&self.kind)?;
        if self.max_amount == 0 {
            return Err(TaskFenceError::Config(
                "permissions.budget.allow max_amount must be positive".into(),
            ));
        }
        Ok(BudgetLimit {
            kind,
            max_amount: self.max_amount,
        })
    }
}

#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
struct RawSecrets {
    #[serde(default)]
    expose_to_agent: bool,
    #[serde(default)]
    available_to_gateway: Vec<SecretGrant>,
}

impl RawSecrets {
    fn resolve(self) -> SecretConfig {
        SecretConfig {
            expose_to_agent: self.expose_to_agent,
            available_to_gateway: self.available_to_gateway,
        }
    }
}

#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
struct RawApproval {
    #[serde(default)]
    require_for: Vec<String>,
    timeout_minutes: Option<u64>,
}

impl RawApproval {
    fn resolve(self) -> taskfence_core::Result<ApprovalConfig> {
        if self.timeout_minutes == Some(0) {
            return Err(TaskFenceError::Config(
                "approval.timeout_minutes must be positive".into(),
            ));
        }
        Ok(ApprovalConfig {
            require_for: self.require_for,
            timeout_minutes: self.timeout_minutes.or(Some(60)),
        })
    }
}

#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
struct RawGateway {
    #[serde(default)]
    mode: Option<String>,
    #[serde(default)]
    egress: RawGatewayEgress,
    #[serde(default)]
    tools: Vec<RawGatewayTool>,
}

impl RawGateway {
    fn resolve(
        self,
        base_dir: &Utf8Path,
        workspace: &Utf8Path,
    ) -> taskfence_core::Result<GatewayConfig> {
        let mode = match self.mode.as_deref() {
            Some("spool_only") | None => GatewayMode::SpoolOnly,
            Some("local_listener") => GatewayMode::LocalListener,
            Some(other) => {
                return Err(TaskFenceError::Config(format!(
                    "gateway.mode must be spool_only or local_listener, got {other}"
                )));
            }
        };
        let egress = self.egress.resolve();
        if egress.allow_domains && mode != GatewayMode::LocalListener {
            return Err(TaskFenceError::Config(
                "gateway.egress.allow_domains requires gateway.mode: local_listener".into(),
            ));
        }
        Ok(GatewayConfig {
            mode,
            egress,
            tools: self
                .tools
                .into_iter()
                .map(|tool| tool.resolve(base_dir, workspace))
                .collect::<taskfence_core::Result<Vec<_>>>()?,
        })
    }
}

#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
struct RawGatewayEgress {
    #[serde(default)]
    allow_domains: bool,
}

impl RawGatewayEgress {
    fn resolve(self) -> GatewayEgressConfig {
        GatewayEgressConfig {
            allow_domains: self.allow_domains,
        }
    }
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct RawGatewayTool {
    protocol: String,
    tool: String,
    operation: String,
    connector: RawGatewayConnector,
    #[serde(default)]
    secret_refs: Vec<RawGatewaySecretReference>,
}

impl RawGatewayTool {
    fn resolve(
        self,
        base_dir: &Utf8Path,
        workspace: &Utf8Path,
    ) -> taskfence_core::Result<GatewayToolConfig> {
        Ok(GatewayToolConfig {
            protocol: normalize_gateway_segment("gateway.tools.protocol", &self.protocol)?,
            tool: normalize_gateway_segment("gateway.tools.tool", &self.tool)?,
            operation: normalize_gateway_segment("gateway.tools.operation", &self.operation)?,
            connector: self.connector.resolve(base_dir, workspace)?,
            secret_refs: self
                .secret_refs
                .into_iter()
                .map(RawGatewaySecretReference::resolve)
                .collect::<taskfence_core::Result<Vec<_>>>()?,
        })
    }
}

#[derive(Debug, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
enum RawGatewayConnector {
    LocalFixture {
        kind: String,
        path: Utf8PathBuf,
    },
    #[serde(rename = "github_rest")]
    GitHubRest {
        repository: String,
        #[serde(default = "default_github_api_base")]
        api_base: String,
    },
    #[serde(rename = "github_enterprise_rest")]
    GitHubEnterpriseRest {
        repository: String,
        api_base: String,
    },
    #[serde(rename = "gitlab")]
    GitLab {
        api_base: String,
        project: String,
    },
    #[serde(rename = "jira")]
    Jira {
        api_base: String,
        project_key: String,
    },
    #[serde(rename = "feishu")]
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
    #[serde(rename = "gitee")]
    Gitee {
        api_base: String,
        repository: String,
    },
    #[serde(rename = "coding")]
    Coding {
        api_base: String,
        project: String,
    },
    #[serde(rename = "database")]
    Database {
        engine: String,
        database_ref: String,
    },
    #[serde(rename = "internal_http")]
    InternalHttp {
        api_base: String,
        service: String,
    },
    #[serde(rename = "siem_export")]
    SiemExport {
        api_base: String,
        sink: String,
    },
    Unsupported {
        kind: String,
    },
}

impl RawGatewayConnector {
    fn resolve(
        self,
        base_dir: &Utf8Path,
        workspace: &Utf8Path,
    ) -> taskfence_core::Result<GatewayConnectorConfig> {
        match self {
            Self::LocalFixture { kind, path } => {
                let kind = normalize_gateway_segment("gateway.tools.connector.kind", &kind)?;
                if path.as_str().trim().is_empty() {
                    return Err(TaskFenceError::Config(
                        "gateway.tools.connector.path must not be empty".into(),
                    ));
                }
                if escapes_with_parent(&path) {
                    return Err(TaskFenceError::Config(format!(
                        "gateway fixture path must not contain '..': {path}"
                    )));
                }
                let resolved = resolve_relative_path(base_dir, &path)?;
                let canonical = fs::canonicalize(resolved.as_std_path()).map_err(|err| {
                    TaskFenceError::Config(format!(
                        "failed to resolve gateway fixture {path}: {err}"
                    ))
                })?;
                let canonical = Utf8PathBuf::from_path_buf(canonical).map_err(|path| {
                    TaskFenceError::Config(format!(
                        "gateway fixture path is not valid UTF-8: {}",
                        path.display()
                    ))
                })?;
                if !canonical.starts_with(workspace) {
                    return Err(TaskFenceError::Config(format!(
                        "gateway fixture path must stay inside workspace {workspace}: {canonical}"
                    )));
                }
                Ok(GatewayConnectorConfig::LocalFixture {
                    kind,
                    path: canonical,
                })
            }
            Self::GitHubRest {
                api_base,
                repository,
            } => Ok(GatewayConnectorConfig::GitHubRest {
                api_base: normalize_gateway_api_base(&api_base)?,
                repository: normalize_github_repository(&repository)?,
            }),
            Self::GitHubEnterpriseRest {
                api_base,
                repository,
            } => Ok(GatewayConnectorConfig::GitHubEnterpriseRest {
                api_base: normalize_gateway_api_base(&api_base)?,
                repository: normalize_github_repository(&repository)?,
            }),
            Self::GitLab { api_base, project } => Ok(GatewayConnectorConfig::GitLab {
                api_base: normalize_gateway_api_base(&api_base)?,
                project: normalize_connector_path("gateway.tools.connector.project", &project)?,
            }),
            Self::Jira {
                api_base,
                project_key,
            } => Ok(GatewayConnectorConfig::Jira {
                api_base: normalize_gateway_api_base(&api_base)?,
                project_key: normalize_connector_token(
                    "gateway.tools.connector.project_key",
                    &project_key,
                )?,
            }),
            Self::Feishu { api_base, app } => Ok(GatewayConnectorConfig::Feishu {
                api_base: normalize_gateway_api_base(&api_base)?,
                app: normalize_connector_token("gateway.tools.connector.app", &app)?,
            }),
            Self::WeCom { api_base, corp_id } => Ok(GatewayConnectorConfig::WeCom {
                api_base: normalize_gateway_api_base(&api_base)?,
                corp_id: normalize_connector_token("gateway.tools.connector.corp_id", &corp_id)?,
            }),
            Self::DingTalk { api_base, tenant } => Ok(GatewayConnectorConfig::DingTalk {
                api_base: normalize_gateway_api_base(&api_base)?,
                tenant: normalize_connector_token("gateway.tools.connector.tenant", &tenant)?,
            }),
            Self::Gitee {
                api_base,
                repository,
            } => Ok(GatewayConnectorConfig::Gitee {
                api_base: normalize_gateway_api_base(&api_base)?,
                repository: normalize_connector_path(
                    "gateway.tools.connector.repository",
                    &repository,
                )?,
            }),
            Self::Coding { api_base, project } => Ok(GatewayConnectorConfig::Coding {
                api_base: normalize_gateway_api_base(&api_base)?,
                project: normalize_connector_path("gateway.tools.connector.project", &project)?,
            }),
            Self::Database {
                engine,
                database_ref,
            } => Ok(GatewayConnectorConfig::Database {
                engine: normalize_connector_token("gateway.tools.connector.engine", &engine)?
                    .to_ascii_lowercase(),
                database_ref: normalize_connector_reference(
                    "gateway.tools.connector.database_ref",
                    &database_ref,
                )?,
            }),
            Self::InternalHttp { api_base, service } => Ok(GatewayConnectorConfig::InternalHttp {
                api_base: normalize_gateway_api_base(&api_base)?,
                service: normalize_connector_token("gateway.tools.connector.service", &service)?,
            }),
            Self::SiemExport { api_base, sink } => Ok(GatewayConnectorConfig::SiemExport {
                api_base: normalize_gateway_api_base(&api_base)?,
                sink: normalize_connector_reference("gateway.tools.connector.sink", &sink)?,
            }),
            Self::Unsupported { kind } => Ok(GatewayConnectorConfig::Unsupported {
                kind: normalize_gateway_segment("gateway.tools.connector.kind", &kind)?,
            }),
        }
    }
}

fn default_github_api_base() -> String {
    "https://api.github.com".into()
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct RawGatewaySecretReference {
    name: String,
    parameter: String,
    scope: String,
}

impl RawGatewaySecretReference {
    fn resolve(self) -> taskfence_core::Result<GatewaySecretReferenceConfig> {
        Ok(GatewaySecretReferenceConfig {
            name: normalize_gateway_segment("gateway.tools.secret_refs.name", &self.name)?,
            parameter: normalize_gateway_parameter(
                "gateway.tools.secret_refs.parameter",
                &self.parameter,
            )?,
            scope: normalize_gateway_segment("gateway.tools.secret_refs.scope", &self.scope)?,
        })
    }
}

#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
struct RawAudit {
    #[serde(default)]
    report: RawReport,
    #[serde(default)]
    capture: CaptureConfig,
}

impl RawAudit {
    fn resolve(self) -> taskfence_core::Result<AuditConfig> {
        Ok(AuditConfig {
            report: self.report.resolve()?,
            capture: self.capture,
        })
    }
}

#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
struct RawReport {
    format: Option<String>,
}

impl RawReport {
    fn resolve(self) -> taskfence_core::Result<ReportConfig> {
        Ok(ReportConfig {
            format: match self.format.as_deref() {
                Some("html") => ReportFormat::Html,
                Some("markdown") | None => ReportFormat::Markdown,
                Some(other) => {
                    return Err(TaskFenceError::Config(format!(
                        "audit.report.format must be markdown or html, got {other}"
                    )));
                }
            },
        })
    }
}

fn resolve_paths(
    base_dir: &Utf8Path,
    workspace: &Utf8Path,
    paths: Vec<Utf8PathBuf>,
) -> taskfence_core::Result<Vec<Utf8PathBuf>> {
    paths
        .iter()
        .map(|path| {
            if escapes_with_parent(path) {
                Err(TaskFenceError::Config(format!(
                    "path must not contain '..': {path}"
                )))
            } else {
                let resolved = resolve_relative_path(base_dir, path)?;
                let validated = validate_allowed_path(workspace, &resolved)?;
                if !validated.starts_with(workspace) {
                    return Err(TaskFenceError::Config(format!(
                        "path must stay inside workspace {workspace}: {validated}"
                    )));
                }
                Ok(validated)
            }
        })
        .collect()
}

fn resolve_existing_workspace(
    base_dir: &Utf8Path,
    workspace: &Utf8Path,
) -> taskfence_core::Result<Utf8PathBuf> {
    let resolved = resolve_relative_path(base_dir, workspace)?;
    let canonical = fs::canonicalize(resolved.as_std_path()).map_err(|err| {
        TaskFenceError::Config(format!("failed to resolve workspace {resolved}: {err}"))
    })?;
    Utf8PathBuf::from_path_buf(canonical).map_err(|path| {
        TaskFenceError::Config(format!(
            "workspace path is not valid UTF-8: {}",
            path.display()
        ))
    })
}

fn validate_allowed_path(
    workspace: &Utf8Path,
    path: &Utf8Path,
) -> taskfence_core::Result<Utf8PathBuf> {
    match fs::canonicalize(path.as_std_path()) {
        Ok(canonical) => Utf8PathBuf::from_path_buf(canonical).map_err(|path| {
            TaskFenceError::Config(format!(
                "configured path is not valid UTF-8: {}",
                path.display()
            ))
        }),
        Err(err) if err.kind() == io::ErrorKind::NotFound => {
            if path.starts_with(workspace) {
                Ok(path.to_path_buf())
            } else {
                Err(TaskFenceError::Config(format!(
                    "path must stay inside workspace {workspace}: {path}"
                )))
            }
        }
        Err(err) => Err(TaskFenceError::Config(format!(
            "failed to resolve configured path {path}: {err}"
        ))),
    }
}

fn resolve_relative_path(
    base_dir: &Utf8Path,
    path: &Utf8Path,
) -> taskfence_core::Result<Utf8PathBuf> {
    if path.is_absolute() {
        Ok(path.to_path_buf())
    } else {
        Ok(base_dir.join(path))
    }
}

fn make_absolute(path: &Utf8Path) -> taskfence_core::Result<Utf8PathBuf> {
    if path.is_absolute() {
        return Ok(path.to_path_buf());
    }

    let cwd = std::env::current_dir().map_err(|err| {
        TaskFenceError::Config(format!("failed to read current directory: {err}"))
    })?;
    let cwd = Utf8PathBuf::from_path_buf(cwd).map_err(|path| {
        TaskFenceError::Config(format!(
            "current directory is not valid UTF-8: {}",
            path.display()
        ))
    })?;
    Ok(cwd.join(path))
}

fn escapes_with_parent(path: &Utf8Path) -> bool {
    path.components()
        .any(|component| component.as_str() == "..")
}

fn validate_domain(domain: String) -> taskfence_core::Result<String> {
    let domain = domain.trim().trim_end_matches('.').to_ascii_lowercase();
    if domain.is_empty()
        || domain.contains('/')
        || domain.contains(':')
        || domain.contains('*')
        || domain.split('.').any(|label| label.is_empty())
    {
        return Err(TaskFenceError::Config(format!(
            "invalid network allow domain: {domain}"
        )));
    }
    Ok(domain)
}

fn normalize_budget_kind(kind: &str) -> taskfence_core::Result<String> {
    let normalized = kind.trim().to_ascii_lowercase();
    if normalized.is_empty() {
        return Err(TaskFenceError::Config(
            "permissions.budget.allow kind must not be empty".into(),
        ));
    }
    Ok(normalized)
}

fn normalize_gateway_segment(field: &str, value: &str) -> taskfence_core::Result<String> {
    let normalized = value.trim().to_ascii_lowercase();
    if normalized.is_empty() {
        return Err(TaskFenceError::Config(format!("{field} must not be empty")));
    }
    Ok(normalized)
}

fn normalize_gateway_parameter(field: &str, value: &str) -> taskfence_core::Result<String> {
    let normalized = value.trim().to_owned();
    if normalized.is_empty() {
        return Err(TaskFenceError::Config(format!("{field} must not be empty")));
    }
    Ok(normalized)
}

fn normalize_gateway_api_base(value: &str) -> taskfence_core::Result<String> {
    let normalized = value.trim().trim_end_matches('/').to_owned();
    if normalized.is_empty() {
        return Err(TaskFenceError::Config(
            "gateway.tools.connector.api_base must not be empty".into(),
        ));
    }
    if !normalized.starts_with("https://") {
        return Err(TaskFenceError::Config(format!(
            "gateway.tools.connector.api_base must use https: {normalized}"
        )));
    }
    let without_scheme = normalized.trim_start_matches("https://");
    if without_scheme.is_empty()
        || without_scheme.contains('@')
        || without_scheme.contains('?')
        || without_scheme.contains('#')
        || without_scheme.chars().any(char::is_whitespace)
    {
        return Err(TaskFenceError::Config(format!(
            "gateway.tools.connector.api_base is not a safe HTTPS base URL: {normalized}"
        )));
    }
    Ok(normalized)
}

fn normalize_github_repository(value: &str) -> taskfence_core::Result<String> {
    let normalized = value.trim();
    let Some((owner, repo)) = normalized.split_once('/') else {
        return Err(TaskFenceError::Config(
            "gateway.tools.connector.repository must be owner/repo".into(),
        ));
    };
    if owner.is_empty()
        || repo.is_empty()
        || repo.contains('/')
        || !is_safe_github_path_segment(owner)
        || !is_safe_github_path_segment(repo)
    {
        return Err(TaskFenceError::Config(format!(
            "gateway.tools.connector.repository must be a safe owner/repo value: {normalized}"
        )));
    }
    Ok(format!("{owner}/{repo}"))
}

fn is_safe_github_path_segment(value: &str) -> bool {
    !value.contains("..")
        && value
            .chars()
            .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '.' | '_' | '-'))
}

fn normalize_connector_token(field: &str, value: &str) -> taskfence_core::Result<String> {
    let normalized = value.trim();
    if normalized.is_empty()
        || normalized.contains("..")
        || !normalized
            .chars()
            .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '.' | '_' | '-'))
    {
        return Err(TaskFenceError::Config(format!(
            "{field} must contain only ASCII letters, digits, '.', '_' or '-'"
        )));
    }
    Ok(normalized.to_owned())
}

fn normalize_connector_path(field: &str, value: &str) -> taskfence_core::Result<String> {
    let normalized = value.trim().trim_matches('/');
    if normalized.is_empty()
        || normalized.contains("..")
        || normalized
            .split('/')
            .any(|segment| normalize_connector_token(field, segment).is_err())
    {
        return Err(TaskFenceError::Config(format!(
            "{field} must be a safe slash-separated connector path"
        )));
    }
    Ok(normalized.to_owned())
}

fn normalize_connector_reference(field: &str, value: &str) -> taskfence_core::Result<String> {
    let normalized = value.trim();
    let lower = normalized.to_ascii_lowercase();
    if normalized.is_empty()
        || normalized.chars().any(char::is_whitespace)
        || normalized.contains('@')
        || normalized.contains('?')
        || normalized.contains('#')
        || lower.contains("password=")
        || lower.contains("token=")
        || lower.contains("secret=")
        || lower.contains("api_key=")
        || lower.contains("authorization=")
        || lower.contains("bearer ")
        || lower.starts_with("postgres://")
        || lower.starts_with("postgresql://")
        || lower.starts_with("mysql://")
        || lower.starts_with("sqlserver://")
    {
        return Err(TaskFenceError::Config(format!(
            "{field} must be a non-secret reference, not an inline credential or DSN"
        )));
    }
    Ok(normalized.to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn parses_example_shape_with_default_deny_network() {
        let temp = tempfile::tempdir().unwrap();
        fs::create_dir(temp.path().join("repo")).unwrap();
        let task_file = Utf8PathBuf::from_path_buf(temp.path().join("task.yaml")).unwrap();
        let yaml = r#"
goal: Test
workspace: ./repo
agent:
  type: generic
  command: codex
sandbox:
  type: docker
"#;

        let task = parse_task_file(&task_file, yaml).unwrap();
        assert_eq!(task.goal, "Test");
        assert_eq!(task.permissions.network.default, NetworkDefault::Deny);
    }

    #[test]
    fn parses_tool_permissions() {
        let temp = tempfile::tempdir().unwrap();
        fs::create_dir(temp.path().join("repo")).unwrap();
        let task_file = Utf8PathBuf::from_path_buf(temp.path().join("task.yaml")).unwrap();
        let yaml = r#"
goal: Test tool policy
workspace: ./repo
agent:
  command: codex
sandbox:
  type: docker
permissions:
  tools:
    allow:
      - "github.read_issue"
    approval_required:
      - "github.create_pr"
    deny:
      - "github.delete_repo"
"#;

        let task = parse_task_file(&task_file, yaml).unwrap();

        assert_eq!(task.permissions.tools.allow, vec!["github.read_issue"]);
        assert_eq!(
            task.permissions.tools.approval_required,
            vec!["github.create_pr"]
        );
        assert_eq!(task.permissions.tools.deny, vec!["github.delete_repo"]);
    }

    #[test]
    fn parses_budget_permissions() {
        let temp = tempfile::tempdir().unwrap();
        fs::create_dir(temp.path().join("repo")).unwrap();
        let task_file = Utf8PathBuf::from_path_buf(temp.path().join("task.yaml")).unwrap();
        let yaml = r#"
goal: Test budget policy
workspace: ./repo
agent:
  command: codex
sandbox:
  type: docker
permissions:
  budget:
    allow:
      - kind: " Tokens "
        max_amount: 1000
      - kind: "usd_cents"
        max_amount: 250
"#;

        let task = parse_task_file(&task_file, yaml).unwrap();

        assert_eq!(task.permissions.budget.allow.len(), 2);
        assert_eq!(task.permissions.budget.allow[0].kind, "tokens");
        assert_eq!(task.permissions.budget.allow[0].max_amount, 1000);
        assert_eq!(task.permissions.budget.allow[1].kind, "usd_cents");
        assert_eq!(task.permissions.budget.allow[1].max_amount, 250);
    }

    #[test]
    fn parses_known_runner_sandbox_types_as_typed_contracts() {
        for (sandbox_type, expected_kind) in [
            ("kubernetes_job", SandboxKind::KubernetesJob),
            ("microvm", SandboxKind::MicroVm),
            ("managed_cloud", SandboxKind::ManagedCloud),
        ] {
            let temp = tempfile::tempdir().unwrap();
            fs::create_dir(temp.path().join("repo")).unwrap();
            let task_file = Utf8PathBuf::from_path_buf(temp.path().join("task.yaml")).unwrap();
            let yaml = format!(
                r#"
goal: Test runner contract
workspace: ./repo
agent:
  command: codex
sandbox:
  type: {sandbox_type}
"#
            );

            let task = parse_task_file(&task_file, &yaml).unwrap();

            assert_eq!(task.sandbox.kind, expected_kind);
        }
    }

    #[test]
    fn parses_remote_ssh_sandbox_contract() {
        let temp = tempfile::tempdir().unwrap();
        fs::create_dir(temp.path().join("repo")).unwrap();
        let task_file = Utf8PathBuf::from_path_buf(temp.path().join("task.yaml")).unwrap();
        let yaml = r#"
goal: Test remote SSH runner
workspace: ./repo
agent:
  command: /usr/bin/true
sandbox:
  type: remote_ssh
  ssh:
    host: runner.example
    user: taskfence
    port: 2222
    workspace: /srv/taskfence/workspaces/task-1
    identity_file: /tmp/taskfence/id_ed25519
    known_hosts_file: /tmp/taskfence/known_hosts
    isolated_workspace: true
    isolated_secrets: true
    terminates_remote_processes: true
    network_policy: uncontrolled_allow
permissions:
  commands:
    allow:
      - /usr/bin/true
  network:
    default: allow
audit:
  capture:
    file_diff: false
"#;

        let task = parse_task_file(&task_file, yaml).unwrap();

        assert_eq!(task.sandbox.kind, SandboxKind::RemoteSsh);
        let ssh = task.sandbox.ssh.unwrap();
        assert_eq!(ssh.host, "runner.example");
        assert_eq!(ssh.user.as_deref(), Some("taskfence"));
        assert_eq!(ssh.port, Some(2222));
        assert_eq!(
            ssh.workspace.unwrap().as_str(),
            "/srv/taskfence/workspaces/task-1"
        );
        assert_eq!(
            ssh.identity_file.unwrap().as_str(),
            "/tmp/taskfence/id_ed25519"
        );
        assert_eq!(
            ssh.known_hosts_file.unwrap().as_str(),
            "/tmp/taskfence/known_hosts"
        );
        assert!(ssh.isolated_workspace);
        assert!(ssh.isolated_secrets);
        assert!(ssh.terminates_remote_processes);
        assert_eq!(
            ssh.network_policy,
            Some(SshNetworkPolicy::UncontrolledAllow)
        );
    }

    #[test]
    fn remote_ssh_requires_ssh_config() {
        let temp = tempfile::tempdir().unwrap();
        fs::create_dir(temp.path().join("repo")).unwrap();
        let task_file = Utf8PathBuf::from_path_buf(temp.path().join("task.yaml")).unwrap();
        let yaml = r#"
goal: Test remote SSH runner
workspace: ./repo
agent:
  command: /usr/bin/true
sandbox:
  type: remote_ssh
"#;

        let err = parse_task_file(&task_file, yaml).unwrap_err();

        assert!(
            matches!(err, TaskFenceError::Config(message) if message.contains("sandbox.ssh is required"))
        );
    }

    #[test]
    fn rejects_ssh_config_on_non_ssh_sandbox() {
        let temp = tempfile::tempdir().unwrap();
        fs::create_dir(temp.path().join("repo")).unwrap();
        let task_file = Utf8PathBuf::from_path_buf(temp.path().join("task.yaml")).unwrap();
        let yaml = r#"
goal: Test bad SSH runner
workspace: ./repo
agent:
  command: /usr/bin/true
sandbox:
  type: docker
  ssh:
    host: runner.example
"#;

        let err = parse_task_file(&task_file, yaml).unwrap_err();

        assert!(
            matches!(err, TaskFenceError::Config(message) if message.contains("only supported when sandbox.type is remote_ssh"))
        );
    }

    #[test]
    fn rejects_unsafe_ssh_segments_and_paths() {
        let temp = tempfile::tempdir().unwrap();
        fs::create_dir(temp.path().join("repo")).unwrap();
        let task_file = Utf8PathBuf::from_path_buf(temp.path().join("task.yaml")).unwrap();
        let yaml = r#"
goal: Test bad SSH runner
workspace: ./repo
agent:
  command: /usr/bin/true
sandbox:
  type: remote_ssh
  ssh:
    host: user@runner.example
    workspace: ../workspace
    identity_file: id_ed25519
"#;

        let err = parse_task_file(&task_file, yaml).unwrap_err();

        assert!(
            matches!(err, TaskFenceError::Config(message) if message.contains("sandbox.ssh.host"))
        );
    }

    #[test]
    fn preserves_unknown_runner_sandbox_types_as_unsupported() {
        let temp = tempfile::tempdir().unwrap();
        fs::create_dir(temp.path().join("repo")).unwrap();
        let task_file = Utf8PathBuf::from_path_buf(temp.path().join("task.yaml")).unwrap();
        let yaml = r#"
goal: Test unsupported runner
workspace: ./repo
agent:
  command: codex
sandbox:
  type: bare_metal
"#;

        let task = parse_task_file(&task_file, yaml).unwrap();

        assert_eq!(
            task.sandbox.kind,
            SandboxKind::Unsupported("bare_metal".into())
        );
    }

    #[test]
    fn parses_gateway_local_fixture_tools() {
        let temp = tempfile::tempdir().unwrap();
        let repo = temp.path().join("repo");
        fs::create_dir(&repo).unwrap();
        fs::create_dir(repo.join("fixtures")).unwrap();
        fs::write(repo.join("fixtures/github.json"), "{}").unwrap();
        let task_file = Utf8PathBuf::from_path_buf(temp.path().join("task.yaml")).unwrap();
        let yaml = r#"
goal: Test gateway fixture
workspace: ./repo
agent:
  command: codex
sandbox:
  type: docker
gateway:
  tools:
    - protocol: " MCP "
      tool: " GitHub "
      operation: " Read_Issue "
      connector:
        type: local_fixture
        kind: " GitHub "
        path: "./repo/fixtures/github.json"
    - protocol: mcp
      tool: github
      operation: create_pr
      connector:
        type: local_fixture
        kind: github
        path: "./repo/fixtures/github.json"
      secret_refs:
        - name: " GitHub_Token "
          parameter: " authorization "
          scope: " GitHub.Create_Pr "
"#;

        let task = parse_task_file(&task_file, yaml).unwrap();

        assert_eq!(task.gateway.tools.len(), 2);
        assert_eq!(task.gateway.tools[0].protocol, "mcp");
        assert_eq!(task.gateway.tools[0].tool, "github");
        assert_eq!(task.gateway.tools[0].operation, "read_issue");
        assert!(matches!(
            &task.gateway.tools[0].connector,
            GatewayConnectorConfig::LocalFixture { kind, path }
                if kind == "github" && path.ends_with("fixtures/github.json")
        ));
        assert_eq!(task.gateway.tools[1].secret_refs[0].name, "github_token");
        assert_eq!(
            task.gateway.tools[1].secret_refs[0].parameter,
            "authorization"
        );
        assert_eq!(
            task.gateway.tools[1].secret_refs[0].scope,
            "github.create_pr"
        );
    }

    #[test]
    fn parses_gateway_github_rest_tools() {
        let temp = tempfile::tempdir().unwrap();
        fs::create_dir(temp.path().join("repo")).unwrap();
        let task_file = Utf8PathBuf::from_path_buf(temp.path().join("task.yaml")).unwrap();
        let yaml = r#"
goal: Test GitHub REST connector
workspace: ./repo
agent:
  command: codex
sandbox:
  type: docker
gateway:
  tools:
    - protocol: mcp
      tool: github
      operation: read_issue
      connector:
        type: github_rest
        repository: TaskFence/TaskFence
      secret_refs:
        - name: " GitHub_Token "
          parameter: authorization
          scope: github.read_issue
    - protocol: http
      tool: github
      operation: comment_issue
      connector:
        type: github_rest
        api_base: "https://api.github.example/"
        repository: owner/repo
"#;

        let task = parse_task_file(&task_file, yaml).unwrap();

        assert_eq!(task.gateway.tools.len(), 2);
        assert!(matches!(
            &task.gateway.tools[0].connector,
            GatewayConnectorConfig::GitHubRest {
                api_base,
                repository,
            } if api_base == "https://api.github.com" && repository == "TaskFence/TaskFence"
        ));
        assert_eq!(task.gateway.tools[0].secret_refs[0].name, "github_token");
        assert!(matches!(
            &task.gateway.tools[1].connector,
            GatewayConnectorConfig::GitHubRest {
                api_base,
                repository,
            } if api_base == "https://api.github.example" && repository == "owner/repo"
        ));
    }

    #[test]
    fn parses_expanded_github_rest_workflow_tools() {
        let temp = tempfile::tempdir().unwrap();
        fs::create_dir(temp.path().join("repo")).unwrap();
        let task_file = Utf8PathBuf::from_path_buf(temp.path().join("task.yaml")).unwrap();
        let yaml = r#"
goal: Test expanded GitHub REST connector
workspace: ./repo
agent:
  command: codex
sandbox:
  type: docker
permissions:
  tools:
    allow:
      - github.read_issue
    approval_required:
      - github.create_branch
      - github.commit_file
      - github.create_pr
      - github.update_pr
      - github.comment_issue
      - github.comment_report
secrets:
  available_to_gateway:
    - name: github_token
      use_for:
        - github.read_issue
        - github.create_branch
        - github.commit_file
        - github.create_pr
        - github.update_pr
        - github.comment_issue
        - github.comment_report
gateway:
  tools:
    - protocol: mcp
      tool: github
      operation: read_issue
      connector:
        type: github_rest
        repository: owner/repo
      secret_refs:
        - name: github_token
          parameter: authorization
          scope: github.read_issue
    - protocol: mcp
      tool: github
      operation: create_branch
      connector:
        type: github_rest
        repository: owner/repo
      secret_refs:
        - name: github_token
          parameter: authorization
          scope: github.create_branch
    - protocol: mcp
      tool: github
      operation: commit_file
      connector:
        type: github_rest
        repository: owner/repo
      secret_refs:
        - name: github_token
          parameter: authorization
          scope: github.commit_file
    - protocol: mcp
      tool: github
      operation: create_pr
      connector:
        type: github_rest
        repository: owner/repo
      secret_refs:
        - name: github_token
          parameter: authorization
          scope: github.create_pr
    - protocol: mcp
      tool: github
      operation: update_pr
      connector:
        type: github_rest
        repository: owner/repo
      secret_refs:
        - name: github_token
          parameter: authorization
          scope: github.update_pr
    - protocol: mcp
      tool: github
      operation: comment_issue
      connector:
        type: github_rest
        repository: owner/repo
      secret_refs:
        - name: github_token
          parameter: authorization
          scope: github.comment_issue
    - protocol: mcp
      tool: github
      operation: comment_report
      connector:
        type: github_rest
        repository: owner/repo
      secret_refs:
        - name: github_token
          parameter: authorization
          scope: github.comment_report
"#;

        let task = parse_task_file(&task_file, yaml).unwrap();

        assert_eq!(
            task.permissions.tools.approval_required,
            vec![
                "github.create_branch",
                "github.commit_file",
                "github.create_pr",
                "github.update_pr",
                "github.comment_issue",
                "github.comment_report"
            ]
        );
        let operations = task
            .gateway
            .tools
            .iter()
            .map(|tool| tool.operation.as_str())
            .collect::<Vec<_>>();
        assert_eq!(
            operations,
            vec![
                "read_issue",
                "create_branch",
                "commit_file",
                "create_pr",
                "update_pr",
                "comment_issue",
                "comment_report"
            ]
        );
        assert!(task
            .gateway
            .tools
            .iter()
            .all(|tool| matches!(tool.connector, GatewayConnectorConfig::GitHubRest { .. })));
        assert_eq!(
            task.gateway.tools[6].secret_refs[0].scope,
            "github.comment_report"
        );
    }

    #[test]
    fn parses_local_listener_gateway_egress_contract() {
        let temp = tempfile::tempdir().unwrap();
        fs::create_dir(temp.path().join("repo")).unwrap();
        let task_file = Utf8PathBuf::from_path_buf(temp.path().join("task.yaml")).unwrap();
        let yaml = r#"
goal: Test gateway egress
workspace: ./repo
agent:
  command: codex
sandbox:
  type: docker
permissions:
  network:
    default: deny
    allow_domains:
      - API.GitHub.COM.
  tools:
    allow:
      - egress.fetch
  budget:
    allow:
      - kind: gateway_calls
        max_amount: 1
gateway:
  mode: local_listener
  egress:
    allow_domains: true
  tools:
    - protocol: http
      tool: egress
      operation: fetch
      connector:
        type: unsupported
        kind: egress
"#;

        let task = parse_task_file(&task_file, yaml).unwrap();

        assert_eq!(task.gateway.mode, GatewayMode::LocalListener);
        assert!(task.gateway.egress.allow_domains);
        assert_eq!(
            task.permissions.network.allow_domains,
            vec!["api.github.com"]
        );
        assert_eq!(task.gateway.tools[0].protocol, "http");
        assert_eq!(task.gateway.tools[0].tool, "egress");
        assert_eq!(task.gateway.tools[0].operation, "fetch");
    }

    #[test]
    fn rejects_gateway_egress_without_local_listener_mode() {
        let temp = tempfile::tempdir().unwrap();
        fs::create_dir(temp.path().join("repo")).unwrap();
        let task_file = Utf8PathBuf::from_path_buf(temp.path().join("task.yaml")).unwrap();
        let yaml = r#"
goal: Test invalid gateway egress
workspace: ./repo
agent:
  command: codex
sandbox:
  type: docker
gateway:
  egress:
    allow_domains: true
"#;

        let err = parse_task_file(&task_file, yaml).unwrap_err();

        assert!(err
            .to_string()
            .contains("gateway.egress.allow_domains requires gateway.mode"));
    }

    #[test]
    fn parses_enterprise_gateway_connector_contracts() {
        let temp = tempfile::tempdir().unwrap();
        fs::create_dir(temp.path().join("repo")).unwrap();
        let task_file = Utf8PathBuf::from_path_buf(temp.path().join("task.yaml")).unwrap();
        let yaml = r#"
goal: Test enterprise gateway connector contracts
workspace: ./repo
agent:
  command: codex
sandbox:
  type: docker
gateway:
  tools:
    - protocol: mcp
      tool: github
      operation: read_issue
      connector:
        type: github_enterprise_rest
        api_base: "https://github.enterprise.example/api/v3"
        repository: owner/repo
    - protocol: mcp
      tool: gitlab
      operation: create_merge_request
      connector:
        type: gitlab
        api_base: "https://gitlab.example/api/v4"
        project: group/subgroup/project
    - protocol: mcp
      tool: jira
      operation: create_issue
      connector:
        type: jira
        api_base: "https://jira.example/rest/api/3"
        project_key: TF
    - protocol: mcp
      tool: feishu
      operation: send_message
      connector:
        type: feishu
        api_base: "https://open.feishu.cn/open-apis"
        app: taskfence_app
    - protocol: mcp
      tool: wecom
      operation: send_message
      connector:
        type: wecom
        api_base: "https://qyapi.weixin.qq.com/cgi-bin"
        corp_id: ww123
    - protocol: mcp
      tool: dingtalk
      operation: send_message
      connector:
        type: dingtalk
        api_base: "https://oapi.dingtalk.com"
        tenant: taskfence
    - protocol: mcp
      tool: gitee
      operation: create_pr
      connector:
        type: gitee
        api_base: "https://gitee.com/api/v5"
        repository: owner/repo
    - protocol: mcp
      tool: coding
      operation: create_merge_request
      connector:
        type: coding
        api_base: "https://example.coding.net/open-api"
        project: team/project
    - protocol: mcp
      tool: database
      operation: write
      connector:
        type: database
        engine: Postgres
        database_ref: taskfence_reporting
    - protocol: http
      tool: internal_http
      operation: call
      connector:
        type: internal_http
        api_base: "https://internal.example/api"
        service: ticket-router
    - protocol: mcp
      tool: siem
      operation: export_events
      connector:
        type: siem_export
        api_base: "https://siem.example/api"
        sink: soc-pipeline
"#;

        let task = parse_task_file(&task_file, yaml).unwrap();

        assert_eq!(task.gateway.tools.len(), 11);
        assert!(matches!(
            &task.gateway.tools[0].connector,
            GatewayConnectorConfig::GitHubEnterpriseRest { api_base, repository }
                if api_base == "https://github.enterprise.example/api/v3"
                    && repository == "owner/repo"
        ));
        assert!(matches!(
            &task.gateway.tools[1].connector,
            GatewayConnectorConfig::GitLab { api_base, project }
                if api_base == "https://gitlab.example/api/v4"
                    && project == "group/subgroup/project"
        ));
        assert!(matches!(
            &task.gateway.tools[8].connector,
            GatewayConnectorConfig::Database { engine, database_ref }
                if engine == "postgres" && database_ref == "taskfence_reporting"
        ));
        assert!(matches!(
            &task.gateway.tools[10].connector,
            GatewayConnectorConfig::SiemExport { api_base, sink }
                if api_base == "https://siem.example/api" && sink == "soc-pipeline"
        ));
    }

    #[test]
    fn rejects_secret_bearing_enterprise_connector_references() {
        for yaml in [
            r#"
goal: Test database secret rejection
workspace: ./repo
agent:
  command: codex
sandbox:
  type: docker
gateway:
  tools:
    - protocol: mcp
      tool: database
      operation: read
      connector:
        type: database
        engine: postgres
        database_ref: "postgres://user:password@db.example/taskfence"
"#,
            r#"
goal: Test SIEM secret rejection
workspace: ./repo
agent:
  command: codex
sandbox:
  type: docker
gateway:
  tools:
    - protocol: mcp
      tool: siem
      operation: export_events
      connector:
        type: siem_export
        api_base: "https://siem.example/api"
        sink: "https://token=secret@example.invalid"
"#,
            r#"
goal: Test unsafe connector path rejection
workspace: ./repo
agent:
  command: codex
sandbox:
  type: docker
gateway:
  tools:
    - protocol: mcp
      tool: gitlab
      operation: create_merge_request
      connector:
        type: gitlab
        api_base: "https://gitlab.example/api/v4"
        project: "../secret"
"#,
        ] {
            let temp = tempfile::tempdir().unwrap();
            fs::create_dir(temp.path().join("repo")).unwrap();
            let task_file = Utf8PathBuf::from_path_buf(temp.path().join("task.yaml")).unwrap();

            assert!(parse_task_file(&task_file, yaml).is_err());
        }
    }

    #[test]
    fn rejects_unknown_fields() {
        let yaml = r#"
goal: Test
workspace: ./repo
unknown: true
agent:
  command: codex
sandbox:
  type: docker
"#;

        assert!(parse_task_file(Utf8Path::new("/tmp/task.yaml"), yaml).is_err());
    }

    #[test]
    fn rejects_unknown_tool_permission_fields() {
        let temp = tempfile::tempdir().unwrap();
        fs::create_dir(temp.path().join("repo")).unwrap();
        let task_file = Utf8PathBuf::from_path_buf(temp.path().join("task.yaml")).unwrap();
        let yaml = r#"
goal: Test
workspace: ./repo
agent:
  command: codex
sandbox:
  type: docker
permissions:
  tools:
    unknown: []
"#;

        assert!(parse_task_file(&task_file, yaml).is_err());
    }

    #[test]
    fn rejects_invalid_gateway_fixture_config() {
        for yaml in [
            r#"
goal: Test
workspace: ./repo
agent:
  command: codex
sandbox:
  type: docker
gateway:
  tools:
    - protocol: " "
      tool: github
      operation: read_issue
      connector:
        type: local_fixture
        kind: github
        path: ./repo/fixtures/github.json
"#,
            r#"
goal: Test
workspace: ./repo
agent:
  command: codex
sandbox:
  type: docker
gateway:
  tools:
    - protocol: mcp
      tool: github
      operation: read_issue
      connector:
        type: local_fixture
        kind: github
        path: ../outside.json
"#,
            r#"
goal: Test
workspace: ./repo
agent:
  command: codex
sandbox:
  type: docker
gateway:
  tools:
    - protocol: mcp
      tool: github
      operation: read_issue
      connector:
        type: local_fixture
        kind: github
        path: ./outside.json
"#,
            r#"
goal: Test
workspace: ./repo
agent:
  command: codex
sandbox:
  type: docker
gateway:
  tools:
    - protocol: mcp
      tool: github
      operation: read_issue
      connector:
        type: local_fixture
        kind: github
        path: ./repo/fixtures/github.json
      secret_refs:
        - name: github_token
          parameter: " "
          scope: github.read_issue
"#,
        ] {
            let temp = tempfile::tempdir().unwrap();
            let repo = temp.path().join("repo");
            fs::create_dir(&repo).unwrap();
            fs::create_dir(repo.join("fixtures")).unwrap();
            fs::write(repo.join("fixtures/github.json"), "{}").unwrap();
            fs::write(temp.path().join("outside.json"), "{}").unwrap();
            let task_file = Utf8PathBuf::from_path_buf(temp.path().join("task.yaml")).unwrap();

            assert!(parse_task_file(&task_file, yaml).is_err());
        }
    }

    #[test]
    fn rejects_invalid_gateway_github_rest_config() {
        for yaml in [
            r#"
goal: Test
workspace: ./repo
agent:
  command: codex
sandbox:
  type: docker
gateway:
  tools:
    - protocol: mcp
      tool: github
      operation: read_issue
      connector:
        type: github_rest
        api_base: "http://api.github.example"
        repository: owner/repo
"#,
            r#"
goal: Test
workspace: ./repo
agent:
  command: codex
sandbox:
  type: docker
gateway:
  tools:
    - protocol: mcp
      tool: github
      operation: read_issue
      connector:
        type: github_rest
        api_base: "https://token@api.github.example"
        repository: owner/repo
"#,
            r#"
goal: Test
workspace: ./repo
agent:
  command: codex
sandbox:
  type: docker
gateway:
  tools:
    - protocol: mcp
      tool: github
      operation: read_issue
      connector:
        type: github_rest
        repository: owner
"#,
            r#"
goal: Test
workspace: ./repo
agent:
  command: codex
sandbox:
  type: docker
gateway:
  tools:
    - protocol: mcp
      tool: github
      operation: read_issue
      connector:
        type: github_rest
        repository: owner/repo/extra
"#,
            r#"
goal: Test
workspace: ./repo
agent:
  command: codex
sandbox:
  type: docker
gateway:
  tools:
    - protocol: mcp
      tool: github
      operation: read_issue
      connector:
        type: github_rest
        repository: ../repo
"#,
        ] {
            let temp = tempfile::tempdir().unwrap();
            fs::create_dir(temp.path().join("repo")).unwrap();
            let task_file = Utf8PathBuf::from_path_buf(temp.path().join("task.yaml")).unwrap();

            assert!(parse_task_file(&task_file, yaml).is_err());
        }
    }

    #[test]
    fn rejects_invalid_budget_permissions() {
        for yaml in [
            r#"
goal: Test
workspace: ./repo
agent:
  command: codex
sandbox:
  type: docker
permissions:
  budget:
    allow:
      - kind: " "
        max_amount: 1
"#,
            r#"
goal: Test
workspace: ./repo
agent:
  command: codex
sandbox:
  type: docker
permissions:
  budget:
    allow:
      - kind: tokens
        max_amount: 0
"#,
        ] {
            let temp = tempfile::tempdir().unwrap();
            fs::create_dir(temp.path().join("repo")).unwrap();
            let task_file = Utf8PathBuf::from_path_buf(temp.path().join("task.yaml")).unwrap();

            assert!(parse_task_file(&task_file, yaml).is_err());
        }
    }

    #[test]
    fn rejects_parent_path_escape() {
        let yaml = r#"
goal: Test
workspace: ../repo
agent:
  command: codex
sandbox:
  type: docker
"#;

        assert!(parse_task_file(Utf8Path::new("/tmp/task.yaml"), yaml).is_err());
    }

    #[test]
    fn rejects_path_outside_workspace() {
        let temp = tempfile::tempdir().unwrap();
        fs::create_dir(temp.path().join("repo")).unwrap();
        fs::create_dir(temp.path().join("outside")).unwrap();
        let task_file = Utf8PathBuf::from_path_buf(temp.path().join("task.yaml")).unwrap();
        let outside = Utf8PathBuf::from_path_buf(temp.path().join("outside")).unwrap();
        let yaml = format!(
            r#"
goal: Test
workspace: ./repo
agent:
  command: codex
sandbox:
  type: docker
permissions:
  paths:
    read:
      - "{outside}"
"#
        );

        let err = parse_task_file(&task_file, &yaml).unwrap_err();

        assert!(
            matches!(err, TaskFenceError::Config(message) if message.contains("inside workspace"))
        );
    }

    #[test]
    fn rejects_invalid_network_report_and_approval_values() {
        for yaml in [
            r#"
goal: Test
workspace: ./repo
agent:
  command: codex
sandbox:
  type: docker
permissions:
  network:
    default: "maybe"
"#,
            r#"
goal: Test
workspace: ./repo
agent:
  command: codex
sandbox:
  type: docker
approval:
  timeout_minutes: 0
"#,
            r#"
goal: Test
workspace: ./repo
agent:
  command: codex
sandbox:
  type: docker
audit:
  report:
    format: pdf
"#,
        ] {
            let temp = tempfile::tempdir().unwrap();
            fs::create_dir(temp.path().join("repo")).unwrap();
            let task_file = Utf8PathBuf::from_path_buf(temp.path().join("task.yaml")).unwrap();

            assert!(parse_task_file(&task_file, yaml).is_err());
        }
    }
}
