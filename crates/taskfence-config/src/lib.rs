use camino::{Utf8Path, Utf8PathBuf};
use serde::Deserialize;
use std::fs;
use std::io;
use taskfence_core::{
    AgentConfig, AgentKind, ApprovalConfig, AuditConfig, BudgetLimit, BudgetPermissions,
    CaptureConfig, CommandPermissions, EnvPermissions, LimitConfig, NetworkDefault,
    NetworkPermissions, PathPermissions, PermissionConfig, ReportConfig, ReportFormat,
    ResolvedTask, SandboxConfig, SandboxKind, SecretConfig, SecretGrant, TaskFenceError, TaskId,
    ToolPermissions,
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
    #[serde(default)]
    limits: LimitConfig,
}

impl RawSandbox {
    fn resolve(self) -> taskfence_core::Result<SandboxConfig> {
        let kind = match self.kind.as_deref() {
            Some("docker") => SandboxKind::Docker,
            Some(other) => SandboxKind::Unsupported(other.to_owned()),
            None => return Err(TaskFenceError::Config("sandbox.type is required".into())),
        };

        if self.limits.timeout_minutes == Some(0) {
            return Err(TaskFenceError::Config(
                "sandbox.limits.timeout_minutes must be positive".into(),
            ));
        }

        Ok(SandboxConfig {
            kind,
            image: self.image,
            limits: self.limits,
        })
    }
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
