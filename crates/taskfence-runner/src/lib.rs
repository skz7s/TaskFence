use camino::{Utf8Path, Utf8PathBuf};
use std::collections::{BTreeMap, BTreeSet};
use std::io::Read;
use std::process::{Command, Stdio};
use std::sync::{Arc, Mutex, MutexGuard};
use std::thread;
use std::time::{Duration, Instant};

use taskfence_core::{
    AgentInvocation, ExitStatus, GatewayMode, MountMode, MountPlan, NetworkDefault,
    PreparedGateway, PreparedGatewayEgress, PreparedRun, PreparedSshRun, ResolvedTask, RunOutput,
    Runner, RunningTask, SandboxKind, SshNetworkPolicy, SshSandboxConfig, TaskFenceError, TaskId,
    GATEWAY_EGRESS_TOOL_NAME, GATEWAY_EGRESS_TOOL_OPERATION, GATEWAY_EGRESS_TOOL_PROTOCOL,
    GATEWAY_SPOOL_CONTAINER_PATH, GATEWAY_SPOOL_DIR_NAME,
    TASKFENCE_GATEWAY_EGRESS_ALLOW_DOMAINS_ENV, TASKFENCE_GATEWAY_MODE_ENV,
    TASKFENCE_GATEWAY_SPOOL_ENV,
};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DockerRunPlan {
    pub prepared: PreparedRun,
    pub network: DockerNetworkPlan,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DockerNetworkPlan {
    pub mode: DockerNetworkMode,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum DockerNetworkMode {
    None,
    Bridge,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RunnerKind {
    Docker,
    RemoteSsh,
    KubernetesJob,
    MicroVm,
    ManagedCloud,
    Unsupported(String),
}

impl RunnerKind {
    pub fn from_sandbox(kind: &SandboxKind) -> Self {
        match kind {
            SandboxKind::Docker => Self::Docker,
            SandboxKind::RemoteSsh => Self::RemoteSsh,
            SandboxKind::KubernetesJob => Self::KubernetesJob,
            SandboxKind::MicroVm => Self::MicroVm,
            SandboxKind::ManagedCloud => Self::ManagedCloud,
            SandboxKind::Unsupported(kind) => Self::Unsupported(kind.clone()),
        }
    }

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

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RunnerCapabilityReport {
    pub kind: RunnerKind,
    pub available: bool,
    pub can_isolate_filesystem: bool,
    pub can_isolate_secrets: bool,
    pub can_disable_network: bool,
    pub can_enforce_default_deny_network: bool,
    pub can_enforce_domain_allowlist: bool,
    pub can_enforce_limits: bool,
    pub can_capture_output: bool,
    pub can_return_artifacts: bool,
    pub missing: Vec<String>,
}

impl RunnerCapabilityReport {
    pub fn docker(task: &ResolvedTask) -> Self {
        let mut missing = Vec::new();
        let can_enforce_domain_allowlist = docker_gateway_egress_enabled(task);
        if !task.permissions.network.allow_domains.is_empty() && !can_enforce_domain_allowlist {
            missing.push(
                "local Docker cannot enforce domain allowlists without gateway.mode=local_listener, gateway.egress.allow_domains=true, and a registered http egress.fetch tool"
                    .into(),
            );
        }
        Self {
            kind: RunnerKind::Docker,
            available: true,
            can_isolate_filesystem: true,
            can_isolate_secrets: true,
            can_disable_network: true,
            can_enforce_default_deny_network: true,
            can_enforce_domain_allowlist,
            can_enforce_limits: true,
            can_capture_output: true,
            can_return_artifacts: true,
            missing,
        }
    }

    pub fn remote_ssh(task: &ResolvedTask) -> Self {
        let mut missing = Vec::new();
        let Some(ssh) = task.sandbox.ssh.as_ref() else {
            return Self::unavailable(
                RunnerKind::RemoteSsh,
                vec!["sandbox.ssh is required for remote_ssh tasks".into()],
            );
        };

        let can_isolate_filesystem = ssh.isolated_workspace && ssh.workspace.is_some();
        if !can_isolate_filesystem {
            missing.push(
                "remote SSH requires sandbox.ssh.workspace and isolated_workspace=true".into(),
            );
        }
        if !ssh.isolated_secrets {
            missing.push("remote SSH requires isolated_secrets=true".into());
        }
        if ssh.identity_file.is_none() {
            missing.push("remote SSH requires sandbox.ssh.identity_file".into());
        }
        if ssh.known_hosts_file.is_none() {
            missing.push("remote SSH requires sandbox.ssh.known_hosts_file".into());
        }

        let can_enforce_limits = ssh_limits_are_supported(task, ssh);
        if !can_enforce_limits {
            missing.push(
                "remote SSH can enforce timeout only unless enforces_resource_limits=true".into(),
            );
        }
        if !ssh.terminates_remote_processes {
            missing.push(
                "remote SSH requires terminates_remote_processes=true for timeout and cancellation"
                    .into(),
            );
        }

        let can_disable_network = false;
        let can_enforce_default_deny_network = false;
        let can_enforce_domain_allowlist = false;
        match task.permissions.network.default {
            NetworkDefault::Allow => {
                if ssh.network_policy != Some(SshNetworkPolicy::UncontrolledAllow) {
                    missing.push(
                        "remote SSH default allow requires sandbox.ssh.network_policy=uncontrolled_allow"
                            .into(),
                    );
                }
            }
            NetworkDefault::Disabled => {
                missing.push("remote SSH cannot disable remote network access".into());
            }
            NetworkDefault::Deny => {
                missing.push("remote SSH cannot enforce default-deny remote network access".into());
            }
        }
        if !task.permissions.network.allow_domains.is_empty() {
            missing.push("remote SSH cannot enforce domain allowlists".into());
        }
        if !task.permissions.env.allow.is_empty() {
            missing.push("remote SSH does not expose host environment allowlists".into());
        }
        if task.secrets.expose_to_agent {
            missing.push("remote SSH does not expose raw secrets to agents".into());
        }
        if task.gateway.mode != GatewayMode::SpoolOnly || !task.gateway.tools.is_empty() {
            missing.push(
                "remote SSH does not mount the local gateway spool or listener boundary".into(),
            );
        }
        if task.audit.capture.file_diff {
            missing.push(
                "remote SSH artifact return is limited to stdout/stderr/local reports; set audit.capture.file_diff=false"
                    .into(),
            );
        }

        Self {
            kind: RunnerKind::RemoteSsh,
            available: missing.is_empty(),
            can_isolate_filesystem,
            can_isolate_secrets: ssh.isolated_secrets,
            can_disable_network,
            can_enforce_default_deny_network,
            can_enforce_domain_allowlist,
            can_enforce_limits,
            can_capture_output: true,
            can_return_artifacts: !task.audit.capture.file_diff,
            missing,
        }
    }

    pub fn unavailable(kind: RunnerKind, missing: Vec<String>) -> Self {
        Self {
            kind,
            available: false,
            can_isolate_filesystem: false,
            can_isolate_secrets: false,
            can_disable_network: false,
            can_enforce_default_deny_network: false,
            can_enforce_domain_allowlist: false,
            can_enforce_limits: false,
            can_capture_output: false,
            can_return_artifacts: false,
            missing,
        }
    }

    fn is_sufficient_for_task(&self, task: &ResolvedTask) -> bool {
        self.available
            && self.can_isolate_filesystem
            && self.can_isolate_secrets
            && self.can_capture_output
            && self.can_return_artifacts
            && self.can_enforce_limits
            && match task.permissions.network.default {
                NetworkDefault::Disabled => self.can_disable_network,
                NetworkDefault::Deny => self.can_enforce_default_deny_network,
                NetworkDefault::Allow => true,
            }
            && (task.permissions.network.allow_domains.is_empty()
                || self.can_enforce_domain_allowlist)
    }

    pub fn ensure_sufficient_for_task(&self, task: &ResolvedTask) -> taskfence_core::Result<()> {
        if self.is_sufficient_for_task(task) {
            return Ok(());
        }
        let missing = if self.missing.is_empty() {
            "required isolation or network controls".into()
        } else {
            self.missing.join(", ")
        };
        Err(TaskFenceError::Runner(format!(
            "{} runner is unavailable or cannot provide required controls: {missing}",
            self.kind.label()
        )))
    }
}

#[derive(Clone, Debug)]
pub struct UnsupportedRunner {
    kind: RunnerKind,
}

impl UnsupportedRunner {
    pub fn new(kind: RunnerKind) -> Self {
        Self { kind }
    }

    pub fn capability_report(&self) -> RunnerCapabilityReport {
        RunnerCapabilityReport::unavailable(
            self.kind.clone(),
            unsupported_runner_missing_controls(&self.kind),
        )
    }
}

impl Runner for UnsupportedRunner {
    fn prepare(&self, task: &ResolvedTask) -> taskfence_core::Result<PreparedRun> {
        self.capability_report().ensure_sufficient_for_task(task)?;
        Err(TaskFenceError::Runner(format!(
            "{} runner execution is not implemented",
            self.kind.label()
        )))
    }

    fn start(
        &self,
        _prepared: PreparedRun,
        _invocation: AgentInvocation,
    ) -> taskfence_core::Result<RunningTask> {
        Err(TaskFenceError::Runner(format!(
            "{} runner execution is not implemented",
            self.kind.label()
        )))
    }

    fn stop(&self, _running: &RunningTask) -> taskfence_core::Result<()> {
        Err(TaskFenceError::Runner(format!(
            "{} runner execution is not implemented",
            self.kind.label()
        )))
    }

    fn collect_exit(&self, _running: &RunningTask) -> taskfence_core::Result<RunOutput> {
        Err(TaskFenceError::Runner(format!(
            "{} runner execution is not implemented",
            self.kind.label()
        )))
    }
}

#[derive(Clone, Debug)]
pub struct ExpandedRunner {
    docker: DockerRunner,
    remote_ssh: RemoteSshRunner,
}

impl Default for ExpandedRunner {
    fn default() -> Self {
        Self::new()
    }
}

impl ExpandedRunner {
    pub fn new() -> Self {
        Self {
            docker: DockerRunner::new(),
            remote_ssh: RemoteSshRunner::new(),
        }
    }

    pub fn with_docker(docker: DockerRunner) -> Self {
        Self {
            docker,
            remote_ssh: RemoteSshRunner::new(),
        }
    }

    pub fn with_backends(docker: DockerRunner, remote_ssh: RemoteSshRunner) -> Self {
        Self { docker, remote_ssh }
    }

    pub fn capability_report(&self, task: &ResolvedTask) -> RunnerCapabilityReport {
        match RunnerKind::from_sandbox(&task.sandbox.kind) {
            RunnerKind::Docker => RunnerCapabilityReport::docker(task),
            RunnerKind::RemoteSsh => RunnerCapabilityReport::remote_ssh(task),
            kind => UnsupportedRunner::new(kind).capability_report(),
        }
    }

    pub fn ensure_capable(&self, task: &ResolvedTask) -> taskfence_core::Result<()> {
        self.capability_report(task)
            .ensure_sufficient_for_task(task)
    }

    fn unsupported_runner(&self, task: &ResolvedTask) -> UnsupportedRunner {
        UnsupportedRunner::new(RunnerKind::from_sandbox(&task.sandbox.kind))
    }
}

impl Runner for ExpandedRunner {
    fn prepare(&self, task: &ResolvedTask) -> taskfence_core::Result<PreparedRun> {
        self.ensure_capable(task)?;
        match task.sandbox.kind {
            SandboxKind::Docker => self.docker.prepare(task),
            SandboxKind::RemoteSsh => self.remote_ssh.prepare(task),
            _ => self.unsupported_runner(task).prepare(task),
        }
    }

    fn start(
        &self,
        prepared: PreparedRun,
        invocation: AgentInvocation,
    ) -> taskfence_core::Result<RunningTask> {
        match prepared.runner_kind {
            SandboxKind::Docker => self.docker.start(prepared, invocation),
            SandboxKind::RemoteSsh => self.remote_ssh.start(prepared, invocation),
            _ => Err(TaskFenceError::Runner(format!(
                "{} runner execution is not implemented",
                prepared.runner_kind.label()
            ))),
        }
    }

    fn stop(&self, running: &RunningTask) -> taskfence_core::Result<()> {
        if running.runner_ref.starts_with("remote-ssh:") {
            self.remote_ssh.stop(running)
        } else {
            self.docker.stop(running)
        }
    }

    fn collect_exit(&self, running: &RunningTask) -> taskfence_core::Result<RunOutput> {
        if running.runner_ref.starts_with("remote-ssh:") {
            self.remote_ssh.collect_exit(running)
        } else {
            self.docker.collect_exit(running)
        }
    }
}

#[derive(Clone, Debug)]
pub struct SshRunPlan {
    pub prepared: PreparedRun,
}

#[derive(Clone, Debug)]
pub struct RemoteSshRunner {
    ssh_command: String,
    completed: Arc<Mutex<BTreeMap<String, RunOutput>>>,
}

impl Default for RemoteSshRunner {
    fn default() -> Self {
        Self::new()
    }
}

impl RemoteSshRunner {
    pub fn new() -> Self {
        Self {
            ssh_command: "ssh".into(),
            completed: Arc::new(Mutex::new(BTreeMap::new())),
        }
    }

    pub fn with_ssh_command(mut self, ssh_command: impl Into<String>) -> Self {
        self.ssh_command = ssh_command.into();
        self
    }

    pub fn build_run_plan(&self, task: &ResolvedTask) -> taskfence_core::Result<SshRunPlan> {
        ensure_remote_ssh_task(task)?;
        RunnerCapabilityReport::remote_ssh(task).ensure_sufficient_for_task(task)?;
        let ssh = task
            .sandbox
            .ssh
            .as_ref()
            .ok_or_else(|| TaskFenceError::Runner("sandbox.ssh is required".into()))?;
        let prepared_ssh = PreparedSshRun {
            host: ssh.host.clone(),
            user: ssh.user.clone(),
            port: ssh.port,
            workspace: ssh.workspace.clone().ok_or_else(|| {
                TaskFenceError::Runner("sandbox.ssh.workspace is required".into())
            })?,
            identity_file: ssh.identity_file.clone().ok_or_else(|| {
                TaskFenceError::Runner("sandbox.ssh.identity_file is required".into())
            })?,
            known_hosts_file: ssh.known_hosts_file.clone(),
        };

        Ok(SshRunPlan {
            prepared: PreparedRun {
                task_id: task.id.clone(),
                runner_kind: SandboxKind::RemoteSsh,
                image: None,
                mounts: Vec::new(),
                env: BTreeMap::new(),
                network: task.permissions.network.clone(),
                gateway: PreparedGateway::default(),
                limits: task.sandbox.limits.clone(),
                ssh: Some(prepared_ssh),
            },
        })
    }

    fn build_ssh_args(
        &self,
        prepared: &PreparedRun,
        invocation: &AgentInvocation,
    ) -> taskfence_core::Result<Vec<String>> {
        let ssh = prepared
            .ssh
            .as_ref()
            .ok_or_else(|| TaskFenceError::Runner("prepared SSH plan is missing".into()))?;
        reject_ssh_segment("sandbox.ssh.host", &ssh.host)?;
        if let Some(user) = &ssh.user {
            reject_ssh_segment("sandbox.ssh.user", user)?;
        }
        reject_ssh_path("sandbox.ssh.workspace", &ssh.workspace)?;
        reject_ssh_path("sandbox.ssh.identity_file", &ssh.identity_file)?;
        if let Some(path) = &ssh.known_hosts_file {
            reject_ssh_path("sandbox.ssh.known_hosts_file", path)?;
        }
        reject_ssh_arg("agent executable", &invocation.executable)?;
        reject_ssh_path("agent working directory", &invocation.working_dir)?;
        for arg in &invocation.args {
            reject_ssh_arg("agent argument", arg)?;
        }
        if !prepared.env.is_empty() || !invocation.env.is_empty() {
            return Err(TaskFenceError::Runner(
                "remote SSH runner does not forward environment variables".into(),
            ));
        }

        let mut args = vec![
            "-o".into(),
            "BatchMode=yes".into(),
            "-o".into(),
            "ForwardAgent=no".into(),
            "-o".into(),
            "PermitLocalCommand=no".into(),
            "-o".into(),
            "IdentitiesOnly=yes".into(),
            "-o".into(),
            "StrictHostKeyChecking=yes".into(),
            "-i".into(),
            ssh.identity_file.to_string(),
        ];
        if let Some(known_hosts) = &ssh.known_hosts_file {
            args.push("-o".into());
            args.push(format!("UserKnownHostsFile={known_hosts}"));
        }
        if let Some(port) = ssh.port {
            args.push("-p".into());
            args.push(port.to_string());
        }
        args.push("--".into());
        args.push(ssh_target(ssh));
        args.push(remote_command(ssh, invocation)?);
        Ok(args)
    }
}

impl Runner for RemoteSshRunner {
    fn prepare(&self, task: &ResolvedTask) -> taskfence_core::Result<PreparedRun> {
        self.build_run_plan(task).map(|plan| plan.prepared)
    }

    fn start(
        &self,
        prepared: PreparedRun,
        invocation: AgentInvocation,
    ) -> taskfence_core::Result<RunningTask> {
        if prepared.runner_kind != SandboxKind::RemoteSsh {
            return Err(TaskFenceError::Runner(format!(
                "RemoteSshRunner cannot run {} sandbox tasks",
                prepared.runner_kind.label()
            )));
        }
        let runner_ref = remote_ssh_runner_ref(&prepared.task_id);
        let args = self.build_ssh_args(&prepared, &invocation)?;
        let timeout = prepared
            .limits
            .timeout_minutes
            .map(|minutes| Duration::from_secs(minutes.saturating_mul(60)));
        let output = run_command_with_timeout(
            &self.ssh_command,
            &args,
            timeout,
            ProcessMessages {
                unavailable_message: "ssh executable is unavailable",
                start_label: "ssh",
                wait_label: "ssh",
                kill_label: "ssh",
                stdout_label: "ssh stdout",
                stderr_label: "ssh stderr",
            },
            None,
        )?;

        lock_completed(&self.completed, "remote SSH")?.insert(runner_ref.clone(), output);
        Ok(RunningTask {
            task_id: prepared.task_id,
            runner_ref,
        })
    }

    fn stop(&self, _running: &RunningTask) -> taskfence_core::Result<()> {
        Ok(())
    }

    fn collect_exit(&self, running: &RunningTask) -> taskfence_core::Result<RunOutput> {
        lock_completed(&self.completed, "remote SSH")?
            .remove(&running.runner_ref)
            .ok_or_else(|| {
                TaskFenceError::Runner(format!(
                    "no completed remote SSH run was found for {}",
                    running.runner_ref
                ))
            })
    }
}

#[derive(Clone, Debug)]
pub struct DockerRunner {
    host_env: BTreeMap<String, String>,
    host_home: Option<Utf8PathBuf>,
    docker_command: String,
    completed: Arc<Mutex<BTreeMap<String, RunOutput>>>,
}

impl Default for DockerRunner {
    fn default() -> Self {
        Self::new()
    }
}

impl DockerRunner {
    pub fn new() -> Self {
        let host_env = std::env::vars().collect::<BTreeMap<_, _>>();
        let host_home = host_env
            .get("HOME")
            .filter(|home| !home.trim().is_empty())
            .map(|home| Utf8PathBuf::from(home.as_str()));
        Self {
            host_env,
            host_home,
            docker_command: "docker".into(),
            completed: Arc::new(Mutex::new(BTreeMap::new())),
        }
    }

    pub fn with_host_env(host_env: BTreeMap<String, String>) -> Self {
        Self {
            host_env,
            host_home: None,
            docker_command: "docker".into(),
            completed: Arc::new(Mutex::new(BTreeMap::new())),
        }
    }

    pub fn with_host_context(
        host_env: BTreeMap<String, String>,
        host_home: Option<Utf8PathBuf>,
    ) -> Self {
        Self {
            host_env,
            host_home,
            docker_command: "docker".into(),
            completed: Arc::new(Mutex::new(BTreeMap::new())),
        }
    }

    pub fn with_docker_command(mut self, docker_command: impl Into<String>) -> Self {
        self.docker_command = docker_command.into();
        self
    }

    pub fn build_run_plan(&self, task: &ResolvedTask) -> taskfence_core::Result<DockerRunPlan> {
        ensure_docker_task(task)?;
        let mounts = self.build_mount_plan(task)?;
        let env = self.build_env_plan(task)?;
        let gateway = build_prepared_gateway(task);
        let network = self.build_network_plan(task)?;

        Ok(DockerRunPlan {
            prepared: PreparedRun {
                task_id: task.id.clone(),
                runner_kind: SandboxKind::Docker,
                image: task.sandbox.image.clone(),
                mounts,
                env,
                network: task.permissions.network.clone(),
                gateway,
                limits: task.sandbox.limits.clone(),
                ssh: None,
            },
            network,
        })
    }

    pub fn build_mount_plan(&self, task: &ResolvedTask) -> taskfence_core::Result<Vec<MountPlan>> {
        validate_workspace_path(&task.workspace_host_path)?;

        let mut read_paths = BTreeSet::new();
        for path in &task.permissions.paths.read {
            self.validate_mount_path(task, path)?;
            read_paths.insert(path.clone());
        }

        let mut write_paths = BTreeSet::new();
        for path in &task.permissions.paths.write {
            self.validate_mount_path(task, path)?;
            write_paths.insert(path.clone());
        }

        reject_mixed_mode_overlaps(&read_paths, &write_paths)?;

        let mut mounts = Vec::new();
        let gateway_spool_path = if docker_gateway_spool_required(task) {
            let spool_path = gateway_spool_host_path(task)?;
            self.validate_mount_path(task, &spool_path)?;
            reject_permission_mounts_cover_gateway_spool(&read_paths, &write_paths, &spool_path)?;
            Some(spool_path)
        } else {
            None
        };

        for path in read_paths.difference(&write_paths) {
            mounts.push(MountPlan {
                host_path: path.clone(),
                container_path: container_path_for(task, path)?,
                mode: MountMode::ReadOnly,
            });
        }
        for path in write_paths {
            mounts.push(MountPlan {
                host_path: path.clone(),
                container_path: container_path_for(task, &path)?,
                mode: MountMode::ReadWrite,
            });
        }
        if let Some(spool_path) = gateway_spool_path {
            mounts.push(MountPlan {
                host_path: spool_path,
                container_path: Utf8PathBuf::from(GATEWAY_SPOOL_CONTAINER_PATH),
                mode: MountMode::ReadWrite,
            });
        }
        mounts.sort_by(|left, right| {
            left.container_path
                .cmp(&right.container_path)
                .then_with(|| left.host_path.cmp(&right.host_path))
        });
        Ok(mounts)
    }

    pub fn build_env_plan(
        &self,
        task: &ResolvedTask,
    ) -> taskfence_core::Result<BTreeMap<String, String>> {
        let mut env = BTreeMap::new();
        for name in &task.permissions.env.allow {
            validate_env_name(name)?;
            reject_sensitive_env_name(name)?;
            if let Some(value) = self.host_env.get(name) {
                validate_env_value(name, value)?;
                env.insert(name.clone(), value.clone());
            }
        }
        if docker_gateway_spool_required(task) {
            env.insert(
                TASKFENCE_GATEWAY_SPOOL_ENV.into(),
                GATEWAY_SPOOL_CONTAINER_PATH.into(),
            );
        }
        match task.gateway.mode {
            GatewayMode::SpoolOnly => {}
            GatewayMode::LocalListener => {
                env.insert(TASKFENCE_GATEWAY_MODE_ENV.into(), "local_listener".into());
                if docker_gateway_egress_enabled(task) {
                    env.insert(
                        TASKFENCE_GATEWAY_EGRESS_ALLOW_DOMAINS_ENV.into(),
                        task.permissions.network.allow_domains.join(","),
                    );
                }
            }
        }
        Ok(env)
    }

    pub fn build_network_plan(
        &self,
        task: &ResolvedTask,
    ) -> taskfence_core::Result<DockerNetworkPlan> {
        let network = &task.permissions.network;
        if !network.allow_domains.is_empty() {
            if !docker_gateway_egress_enabled(task) {
                return Err(TaskFenceError::Runner(
                    "local Docker cannot enforce domain allowlists without gateway.mode=local_listener, gateway.egress.allow_domains=true, and a registered http egress.fetch tool"
                        .into(),
                ));
            }
            return Ok(DockerNetworkPlan {
                mode: DockerNetworkMode::None,
            });
        }

        let mode = match &network.default {
            NetworkDefault::Disabled | NetworkDefault::Deny => DockerNetworkMode::None,
            NetworkDefault::Allow => DockerNetworkMode::Bridge,
        };
        Ok(DockerNetworkPlan { mode })
    }

    fn validate_mount_path(
        &self,
        task: &ResolvedTask,
        path: &Utf8Path,
    ) -> taskfence_core::Result<()> {
        validate_mount_host_path(path)?;
        if self.exposes_host_home(path) {
            return Err(TaskFenceError::Runner(format!(
                "mount path would expose the host home directory: {path}"
            )));
        }
        if self.exposes_ssh_auth_socket(path) {
            return Err(TaskFenceError::Runner(format!(
                "mount path would expose the host SSH agent socket: {path}"
            )));
        }
        if !path.starts_with(&task.workspace_host_path) {
            return Err(TaskFenceError::Runner(format!(
                "mount path is outside the task workspace: {path}"
            )));
        }
        Ok(())
    }

    fn exposes_host_home(&self, path: &Utf8Path) -> bool {
        self.host_home
            .as_deref()
            .is_some_and(|home| path == home || home.starts_with(path))
    }

    fn exposes_ssh_auth_socket(&self, path: &Utf8Path) -> bool {
        self.host_env
            .get("SSH_AUTH_SOCK")
            .map(|socket| Utf8PathBuf::from(socket.as_str()))
            .is_some_and(|socket| {
                path == socket.as_path()
                    || socket.starts_with(path)
                    || path.starts_with(socket.as_path())
            })
    }
}

impl Runner for DockerRunner {
    fn prepare(&self, task: &ResolvedTask) -> taskfence_core::Result<PreparedRun> {
        self.build_run_plan(task).map(|plan| plan.prepared)
    }

    fn start(
        &self,
        prepared: PreparedRun,
        invocation: AgentInvocation,
    ) -> taskfence_core::Result<RunningTask> {
        let runner_ref = docker_container_name(&prepared.task_id);
        let args = self.build_docker_run_args(&runner_ref, &prepared, &invocation)?;
        let timeout = prepared
            .limits
            .timeout_minutes
            .map(|minutes| Duration::from_secs(minutes.saturating_mul(60)));
        let output = run_docker_command(&self.docker_command, &runner_ref, &args, timeout)?;

        lock_completed(&self.completed, "Docker")?.insert(runner_ref.clone(), output);

        Ok(RunningTask {
            task_id: prepared.task_id,
            runner_ref,
        })
    }

    fn stop(&self, running: &RunningTask) -> taskfence_core::Result<()> {
        let output = Command::new(&self.docker_command)
            .args(["rm", "-f", running.runner_ref.as_str()])
            .output();
        match output {
            Ok(_) => Ok(()),
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => Err(TaskFenceError::Runner(
                "docker executable is unavailable".into(),
            )),
            Err(err) => Err(TaskFenceError::Runner(format!(
                "failed to stop Docker container {}: {err}",
                running.runner_ref
            ))),
        }
    }

    fn collect_exit(&self, running: &RunningTask) -> taskfence_core::Result<RunOutput> {
        lock_completed(&self.completed, "Docker")?
            .remove(&running.runner_ref)
            .ok_or_else(|| {
                TaskFenceError::Runner(format!(
                    "no completed Docker run was found for {}",
                    running.runner_ref
                ))
            })
    }
}

impl DockerRunner {
    fn build_docker_run_args(
        &self,
        runner_ref: &str,
        prepared: &PreparedRun,
        invocation: &AgentInvocation,
    ) -> taskfence_core::Result<Vec<String>> {
        let image = prepared.image.as_deref().ok_or_else(|| {
            TaskFenceError::Runner("sandbox.image is required for Docker execution".into())
        })?;
        reject_docker_arg("sandbox.image", image)?;
        reject_docker_arg("agent executable", &invocation.executable)?;
        for arg in &invocation.args {
            reject_docker_arg("agent argument", arg)?;
        }

        let mut args = vec![
            "run".into(),
            "--rm".into(),
            "--pull=never".into(),
            "--name".into(),
            runner_ref.into(),
            "--workdir".into(),
            invocation.working_dir.to_string(),
            "--network".into(),
            docker_network_arg(prepared)?.into(),
        ];

        for mount in &prepared.mounts {
            args.push("--mount".into());
            args.push(docker_mount_arg(mount)?);
        }

        for (name, value) in &prepared.env {
            reject_docker_arg("environment variable name", name)?;
            reject_docker_arg("environment variable value", value)?;
            args.push("--env".into());
            args.push(format!("{name}={value}"));
        }

        if let Some(cpu) = prepared.limits.cpu {
            args.push("--cpus".into());
            args.push(cpu.to_string());
        }
        if let Some(memory) = &prepared.limits.memory {
            reject_docker_arg("memory limit", memory)?;
            args.push("--memory".into());
            args.push(memory.clone());
        }
        if let Some(disk) = &prepared.limits.disk {
            reject_docker_arg("disk limit", disk)?;
            args.push("--storage-opt".into());
            args.push(format!("size={disk}"));
        }

        args.push(image.into());
        args.push(invocation.executable.clone());
        args.extend(invocation.args.iter().cloned());
        Ok(args)
    }
}

fn run_docker_command(
    docker_command: &str,
    runner_ref: &str,
    args: &[String],
    timeout: Option<Duration>,
) -> taskfence_core::Result<RunOutput> {
    run_command_with_timeout(
        docker_command,
        args,
        timeout,
        ProcessMessages {
            unavailable_message: "docker executable is unavailable",
            start_label: "docker",
            wait_label: "docker",
            kill_label: "docker",
            stdout_label: "docker stdout",
            stderr_label: "docker stderr",
        },
        Some((docker_command, runner_ref)),
    )
}

#[derive(Clone, Copy, Debug)]
struct ProcessMessages {
    unavailable_message: &'static str,
    start_label: &'static str,
    wait_label: &'static str,
    kill_label: &'static str,
    stdout_label: &'static str,
    stderr_label: &'static str,
}

fn run_command_with_timeout(
    executable: &str,
    args: &[String],
    timeout: Option<Duration>,
    messages: ProcessMessages,
    docker_cleanup: Option<(&str, &str)>,
) -> taskfence_core::Result<RunOutput> {
    let mut child = Command::new(executable)
        .args(args)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|err| {
            if err.kind() == std::io::ErrorKind::NotFound {
                TaskFenceError::Runner(messages.unavailable_message.into())
            } else {
                TaskFenceError::Runner(format!("failed to start {}: {err}", messages.start_label))
            }
        })?;

    let stdout = child.stdout.take().ok_or_else(|| {
        TaskFenceError::Runner(format!("failed to capture {}", messages.stdout_label))
    })?;
    let stderr = child.stderr.take().ok_or_else(|| {
        TaskFenceError::Runner(format!("failed to capture {}", messages.stderr_label))
    })?;

    let stdout_reader = thread::spawn(move || read_pipe(stdout));
    let stderr_reader = thread::spawn(move || read_pipe(stderr));
    let deadline = timeout.map(|duration| Instant::now() + duration);
    let mut timed_out = false;

    let status = loop {
        if let Some(status) = child.try_wait().map_err(|err| {
            TaskFenceError::Runner(format!("failed to wait for {}: {err}", messages.wait_label))
        })? {
            break status;
        }

        if deadline.is_some_and(|deadline| Instant::now() >= deadline) {
            timed_out = true;
            child.kill().map_err(|err| {
                TaskFenceError::Runner(format!("failed to kill {}: {err}", messages.kill_label))
            })?;
            if let Some((docker_command, runner_ref)) = docker_cleanup {
                let _ = Command::new(docker_command)
                    .args(["rm", "-f", runner_ref])
                    .output();
            }
            break child.wait().map_err(|err| {
                TaskFenceError::Runner(format!(
                    "failed to wait for killed {}: {err}",
                    messages.wait_label
                ))
            })?;
        }

        thread::sleep(Duration::from_millis(50));
    };

    let stdout = join_reader(stdout_reader, messages.stdout_label)?;
    let stderr = join_reader(stderr_reader, messages.stderr_label)?;
    let exit_status = if timed_out {
        ExitStatus {
            code: None,
            timed_out: true,
            signal: Some("timeout".into()),
        }
    } else {
        ExitStatus {
            code: status.code(),
            timed_out: false,
            signal: exit_signal(&status),
        }
    };

    Ok(RunOutput {
        exit_status,
        stdout,
        stderr,
    })
}

fn read_pipe(mut pipe: impl Read) -> Vec<u8> {
    let mut bytes = Vec::new();
    let _ = pipe.read_to_end(&mut bytes);
    bytes
}

fn join_reader(
    handle: thread::JoinHandle<Vec<u8>>,
    stream: &str,
) -> taskfence_core::Result<String> {
    let bytes = handle
        .join()
        .map_err(|_| TaskFenceError::Runner(format!("{stream} reader panicked")))?;
    Ok(String::from_utf8_lossy(&bytes).into_owned())
}

#[cfg(unix)]
fn exit_signal(status: &std::process::ExitStatus) -> Option<String> {
    use std::os::unix::process::ExitStatusExt;
    status.signal().map(|signal| signal.to_string())
}

#[cfg(not(unix))]
fn exit_signal(_status: &std::process::ExitStatus) -> Option<String> {
    None
}

fn docker_network_arg(prepared: &PreparedRun) -> taskfence_core::Result<&'static str> {
    if !prepared.network.allow_domains.is_empty() {
        if prepared.gateway.egress.is_some() {
            return Ok("none");
        }
        return Err(TaskFenceError::Runner(
            "local Docker cannot enforce domain allowlists without a prepared gateway egress plan"
                .into(),
        ));
    }
    match prepared.network.default {
        NetworkDefault::Disabled | NetworkDefault::Deny => Ok("none"),
        NetworkDefault::Allow => Ok("bridge"),
    }
}

fn docker_mount_arg(mount: &MountPlan) -> taskfence_core::Result<String> {
    reject_docker_mount_path("mount source", &mount.host_path)?;
    reject_docker_mount_path("mount target", &mount.container_path)?;
    let mut arg = format!(
        "type=bind,source={},target={}",
        mount.host_path, mount.container_path
    );
    if mount.mode == MountMode::ReadOnly {
        arg.push_str(",readonly");
    }
    Ok(arg)
}

fn docker_container_name(task_id: &TaskId) -> String {
    let mut name = String::from("taskfence-");
    for ch in task_id.0.chars() {
        if ch.is_ascii_alphanumeric() || matches!(ch, '_' | '-' | '.') {
            name.push(ch);
        } else {
            name.push('-');
        }
    }
    if name.len() > 120 {
        name.truncate(120);
    }
    name
}

fn reject_docker_arg(field: &str, value: &str) -> taskfence_core::Result<()> {
    if value.contains('\0') {
        Err(TaskFenceError::Runner(format!(
            "{field} must not contain NUL"
        )))
    } else {
        Ok(())
    }
}

fn reject_docker_mount_path(field: &str, path: &Utf8Path) -> taskfence_core::Result<()> {
    reject_docker_arg(field, path.as_str())?;
    if path.as_str().contains(',') {
        return Err(TaskFenceError::Runner(format!(
            "{field} must not contain ',' for Docker --mount syntax: {path}"
        )));
    }
    Ok(())
}

fn lock_completed<'a>(
    completed: &'a Mutex<BTreeMap<String, RunOutput>>,
    runner: &str,
) -> taskfence_core::Result<MutexGuard<'a, BTreeMap<String, RunOutput>>> {
    completed.lock().map_err(|_| {
        TaskFenceError::Runner(format!("{runner} runner completion store is poisoned"))
    })
}

#[derive(Clone, Debug)]
pub struct FakeRunner {
    exit_status: ExitStatus,
    prepare_error: Option<String>,
    start_error: Option<String>,
    collect_error: Option<String>,
    state: Arc<Mutex<FakeRunnerState>>,
}

impl Default for FakeRunner {
    fn default() -> Self {
        Self::succeeding()
    }
}

impl FakeRunner {
    pub fn succeeding() -> Self {
        Self::with_exit_status(ExitStatus {
            code: Some(0),
            timed_out: false,
            signal: None,
        })
    }

    pub fn failing(code: i32) -> Self {
        Self::with_exit_status(ExitStatus {
            code: Some(code),
            timed_out: false,
            signal: None,
        })
    }

    pub fn timed_out() -> Self {
        Self::with_exit_status(ExitStatus {
            code: None,
            timed_out: true,
            signal: Some("timeout".into()),
        })
    }

    pub fn with_exit_status(exit_status: ExitStatus) -> Self {
        Self {
            exit_status,
            prepare_error: None,
            start_error: None,
            collect_error: None,
            state: Arc::new(Mutex::new(FakeRunnerState::default())),
        }
    }

    pub fn with_prepare_error(mut self, message: impl Into<String>) -> Self {
        self.prepare_error = Some(message.into());
        self
    }

    pub fn with_start_error(mut self, message: impl Into<String>) -> Self {
        self.start_error = Some(message.into());
        self
    }

    pub fn with_collect_error(mut self, message: impl Into<String>) -> Self {
        self.collect_error = Some(message.into());
        self
    }

    pub fn prepared_runs(&self) -> taskfence_core::Result<Vec<PreparedRun>> {
        Ok(lock_state(&self.state)?.prepared_runs.clone())
    }

    pub fn invocations(&self) -> taskfence_core::Result<Vec<AgentInvocation>> {
        Ok(lock_state(&self.state)?.invocations.clone())
    }

    pub fn stopped_tasks(&self) -> taskfence_core::Result<Vec<RunningTask>> {
        Ok(lock_state(&self.state)?.stopped_tasks.clone())
    }
}

impl Runner for FakeRunner {
    fn prepare(&self, task: &ResolvedTask) -> taskfence_core::Result<PreparedRun> {
        if let Some(message) = &self.prepare_error {
            return Err(TaskFenceError::Runner(message.clone()));
        }
        Ok(PreparedRun {
            task_id: task.id.clone(),
            runner_kind: task.sandbox.kind.clone(),
            image: task.sandbox.image.clone(),
            mounts: Vec::new(),
            env: BTreeMap::new(),
            network: task.permissions.network.clone(),
            gateway: build_prepared_gateway(task),
            limits: task.sandbox.limits.clone(),
            ssh: None,
        })
    }

    fn start(
        &self,
        prepared: PreparedRun,
        invocation: AgentInvocation,
    ) -> taskfence_core::Result<RunningTask> {
        if let Some(message) = &self.start_error {
            return Err(TaskFenceError::Runner(message.clone()));
        }
        let running = RunningTask {
            runner_ref: format!("fake-runner:{}", prepared.task_id.0),
            task_id: prepared.task_id.clone(),
        };
        let mut state = lock_state(&self.state)?;
        state.prepared_runs.push(prepared);
        state.invocations.push(invocation);
        Ok(running)
    }

    fn stop(&self, running: &RunningTask) -> taskfence_core::Result<()> {
        lock_state(&self.state)?.stopped_tasks.push(running.clone());
        Ok(())
    }

    fn collect_exit(&self, _running: &RunningTask) -> taskfence_core::Result<RunOutput> {
        if let Some(message) = &self.collect_error {
            return Err(TaskFenceError::Runner(message.clone()));
        }
        Ok(RunOutput {
            exit_status: self.exit_status.clone(),
            stdout: String::new(),
            stderr: String::new(),
        })
    }
}

#[derive(Debug, Default)]
struct FakeRunnerState {
    prepared_runs: Vec<PreparedRun>,
    invocations: Vec<AgentInvocation>,
    stopped_tasks: Vec<RunningTask>,
}

fn ensure_docker_task(task: &ResolvedTask) -> taskfence_core::Result<()> {
    match &task.sandbox.kind {
        SandboxKind::Docker => Ok(()),
        SandboxKind::RemoteSsh => Err(TaskFenceError::Unsupported(
            "DockerRunner cannot run remote_ssh sandbox tasks".into(),
        )),
        SandboxKind::KubernetesJob => Err(TaskFenceError::Unsupported(
            "DockerRunner cannot run kubernetes_job sandbox tasks".into(),
        )),
        SandboxKind::MicroVm => Err(TaskFenceError::Unsupported(
            "DockerRunner cannot run microvm sandbox tasks".into(),
        )),
        SandboxKind::ManagedCloud => Err(TaskFenceError::Unsupported(
            "DockerRunner cannot run managed_cloud sandbox tasks".into(),
        )),
        SandboxKind::Unsupported(kind) => Err(TaskFenceError::Unsupported(format!(
            "DockerRunner cannot run unsupported sandbox type {kind}"
        ))),
    }
}

fn validate_workspace_path(path: &Utf8Path) -> taskfence_core::Result<()> {
    if !path.is_absolute() {
        return Err(TaskFenceError::Runner(format!(
            "workspace host path must be absolute: {path}"
        )));
    }
    if contains_parent_component(path) {
        return Err(TaskFenceError::Runner(format!(
            "workspace host path must not contain '..': {path}"
        )));
    }
    Ok(())
}

fn validate_mount_host_path(path: &Utf8Path) -> taskfence_core::Result<()> {
    if !path.is_absolute() {
        return Err(TaskFenceError::Runner(format!(
            "mount path must be absolute: {path}"
        )));
    }
    if contains_parent_component(path) {
        return Err(TaskFenceError::Runner(format!(
            "mount path must not contain '..': {path}"
        )));
    }
    if is_docker_socket_path(path) {
        return Err(TaskFenceError::Runner(format!(
            "mount path would expose the host Docker socket: {path}"
        )));
    }
    Ok(())
}

fn contains_parent_component(path: &Utf8Path) -> bool {
    path.components()
        .any(|component| component.as_str() == "..")
}

fn is_docker_socket_path(path: &Utf8Path) -> bool {
    let lower = path.as_str().to_ascii_lowercase();
    lower == "/var/run/docker.sock"
        || lower == "/run/docker.sock"
        || lower.ends_with("/docker.sock")
}

fn reject_mixed_mode_overlaps(
    read_paths: &BTreeSet<Utf8PathBuf>,
    write_paths: &BTreeSet<Utf8PathBuf>,
) -> taskfence_core::Result<()> {
    for read in read_paths {
        for write in write_paths {
            if read == write {
                continue;
            }
            if read.starts_with(write) || write.starts_with(read) {
                return Err(TaskFenceError::Runner(format!(
                    "ambiguous read-only/read-write mount overlap: {read} and {write}"
                )));
            }
        }
    }
    Ok(())
}

fn reject_permission_mounts_cover_gateway_spool(
    read_paths: &BTreeSet<Utf8PathBuf>,
    write_paths: &BTreeSet<Utf8PathBuf>,
    spool_path: &Utf8Path,
) -> taskfence_core::Result<()> {
    for path in read_paths.iter().chain(write_paths.iter()) {
        if path == spool_path || spool_path.starts_with(path) || path.starts_with(spool_path) {
            return Err(TaskFenceError::Runner(format!(
                "gateway spool must be exposed only through its dedicated mount: {spool_path}"
            )));
        }
    }
    Ok(())
}

fn container_path_for(
    task: &ResolvedTask,
    host_path: &Utf8Path,
) -> taskfence_core::Result<Utf8PathBuf> {
    if host_path == task.workspace_host_path.as_path() {
        return Ok(task.workspace_container_path.clone());
    }

    let relative = host_path
        .strip_prefix(&task.workspace_host_path)
        .map_err(|_| {
            TaskFenceError::Runner(format!(
                "mount path is outside the task workspace: {host_path}"
            ))
        })?;
    Ok(task.workspace_container_path.join(relative))
}

fn gateway_spool_host_path(task: &ResolvedTask) -> taskfence_core::Result<Utf8PathBuf> {
    if task.id.0.is_empty()
        || task.id.0 == "."
        || task.id.0 == ".."
        || task.id.0.contains('/')
        || task.id.0.contains('\\')
        || task.id.0.chars().any(char::is_control)
    {
        return Err(TaskFenceError::Runner(format!(
            "task id is not a safe gateway spool path component: {:?}",
            task.id.0
        )));
    }
    Ok(task
        .workspace_host_path
        .join(".taskfence")
        .join("tasks")
        .join(task.id.0.as_str())
        .join(GATEWAY_SPOOL_DIR_NAME))
}

fn docker_gateway_spool_required(task: &ResolvedTask) -> bool {
    !task.gateway.tools.is_empty() || docker_gateway_egress_enabled(task)
}

fn docker_gateway_egress_enabled(task: &ResolvedTask) -> bool {
    task.gateway.mode == GatewayMode::LocalListener
        && task.gateway.egress.allow_domains
        && !task.permissions.network.allow_domains.is_empty()
        && gateway_egress_tool_configured(task)
}

fn build_prepared_gateway(task: &ResolvedTask) -> PreparedGateway {
    let spool_container_path = docker_gateway_spool_required(task)
        .then(|| Utf8PathBuf::from(GATEWAY_SPOOL_CONTAINER_PATH));
    let egress = docker_gateway_egress_enabled(task).then(|| PreparedGatewayEgress {
        allow_domains: task.permissions.network.allow_domains.clone(),
    });
    PreparedGateway {
        mode: task.gateway.mode.clone(),
        spool_container_path,
        egress,
    }
}

pub fn gateway_egress_tool_configured(task: &ResolvedTask) -> bool {
    task.gateway.tools.iter().any(|tool| {
        tool.protocol == GATEWAY_EGRESS_TOOL_PROTOCOL
            && tool.tool == GATEWAY_EGRESS_TOOL_NAME
            && tool.operation == GATEWAY_EGRESS_TOOL_OPERATION
    })
}

fn validate_env_name(name: &str) -> taskfence_core::Result<()> {
    if name.is_empty() || name.contains('=') || name.contains('\0') {
        Err(TaskFenceError::Runner(format!(
            "invalid environment variable name: {name:?}"
        )))
    } else {
        Ok(())
    }
}

fn reject_sensitive_env_name(name: &str) -> taskfence_core::Result<()> {
    match name {
        "HOME" | "SSH_AUTH_SOCK" | "DOCKER_HOST" => Err(TaskFenceError::Runner(format!(
            "environment variable {name} is not allowed in the Docker runner plan"
        ))),
        _ => Ok(()),
    }
}

fn validate_env_value(name: &str, value: &str) -> taskfence_core::Result<()> {
    if value.contains('\0') {
        return Err(TaskFenceError::Runner(format!(
            "environment variable {name} must not contain NUL"
        )));
    }
    if value.contains("docker.sock") || value.starts_with("unix://") {
        return Err(TaskFenceError::Runner(format!(
            "environment variable {name} would expose a host socket"
        )));
    }
    Ok(())
}

fn ensure_remote_ssh_task(task: &ResolvedTask) -> taskfence_core::Result<()> {
    match &task.sandbox.kind {
        SandboxKind::RemoteSsh => Ok(()),
        kind => Err(TaskFenceError::Unsupported(format!(
            "RemoteSshRunner cannot run {} sandbox tasks",
            kind.label()
        ))),
    }
}

fn ssh_limits_are_supported(task: &ResolvedTask, ssh: &SshSandboxConfig) -> bool {
    let resource_limits_requested = task.sandbox.limits.cpu.is_some()
        || task.sandbox.limits.memory.is_some()
        || task.sandbox.limits.disk.is_some();
    (!resource_limits_requested || ssh.enforces_resource_limits)
        && (task.sandbox.limits.timeout_minutes.is_none() || ssh.terminates_remote_processes)
}

fn remote_ssh_runner_ref(task_id: &TaskId) -> String {
    format!("remote-ssh:{}", docker_container_name(task_id))
}

fn ssh_target(ssh: &PreparedSshRun) -> String {
    match &ssh.user {
        Some(user) => format!("{user}@{}", ssh.host),
        None => ssh.host.clone(),
    }
}

fn remote_command(
    ssh: &PreparedSshRun,
    invocation: &AgentInvocation,
) -> taskfence_core::Result<String> {
    let mut parts = vec![shell_quote(&invocation.executable)?];
    for arg in &invocation.args {
        parts.push(shell_quote(arg)?);
    }
    Ok(format!(
        "cd {} && exec {}",
        shell_quote(ssh.workspace.as_str())?,
        parts.join(" ")
    ))
}

fn shell_quote(value: &str) -> taskfence_core::Result<String> {
    reject_ssh_arg("remote shell argument", value)?;
    Ok(format!("'{}'", value.replace('\'', "'\\''")))
}

fn reject_ssh_segment(field: &str, value: &str) -> taskfence_core::Result<()> {
    if value.is_empty()
        || value.contains('@')
        || value.contains('\0')
        || value.chars().any(char::is_control)
    {
        return Err(TaskFenceError::Runner(format!(
            "{field} must be a non-empty SSH segment without control characters or '@'"
        )));
    }
    Ok(())
}

fn reject_ssh_path(field: &str, path: &Utf8Path) -> taskfence_core::Result<()> {
    reject_ssh_arg(field, path.as_str())?;
    if !path.is_absolute() || contains_parent_component(path) {
        return Err(TaskFenceError::Runner(format!(
            "{field} must be an absolute path without '..': {path}"
        )));
    }
    Ok(())
}

fn reject_ssh_arg(field: &str, value: &str) -> taskfence_core::Result<()> {
    if value.contains('\0') || value.chars().any(|ch| matches!(ch, '\r' | '\n')) {
        return Err(TaskFenceError::Runner(format!(
            "{field} must not contain NUL or newlines"
        )));
    }
    Ok(())
}

fn lock_state(
    state: &Mutex<FakeRunnerState>,
) -> taskfence_core::Result<MutexGuard<'_, FakeRunnerState>> {
    state
        .lock()
        .map_err(|_| TaskFenceError::Runner("fake runner state lock poisoned".into()))
}

fn unsupported_runner_missing_controls(kind: &RunnerKind) -> Vec<String> {
    match kind {
        RunnerKind::RemoteSsh => vec![
            "remote SSH isolation contract".into(),
            "remote filesystem mount policy enforcement".into(),
            "remote secret isolation".into(),
            "remote network control enforcement".into(),
            "remote limit enforcement".into(),
        ],
        RunnerKind::KubernetesJob => vec![
            "Kubernetes job namespace/pod security contract".into(),
            "Kubernetes network policy enforcement".into(),
            "Kubernetes secret isolation".into(),
            "Kubernetes artifact collection contract".into(),
        ],
        RunnerKind::MicroVm => vec![
            "microVM image contract".into(),
            "microVM filesystem sharing policy".into(),
            "microVM network policy enforcement".into(),
            "microVM lifecycle and artifact collection".into(),
        ],
        RunnerKind::ManagedCloud => vec![
            "managed cloud runner provider contract".into(),
            "managed cloud credential boundary".into(),
            "managed cloud network policy enforcement".into(),
            "managed cloud artifact collection contract".into(),
        ],
        RunnerKind::Unsupported(kind) => vec![format!("unsupported sandbox type {kind}")],
        RunnerKind::Docker => Vec::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use taskfence_core::{
        AgentConfig, AgentKind, ApprovalConfig, AuditConfig, EnvPermissions, GatewayConfig,
        GatewayConnectorConfig, GatewayEgressConfig, GatewayToolConfig, LimitConfig,
        NetworkPermissions, PathPermissions, PermissionConfig, SandboxConfig, SecretConfig,
        SshNetworkPolicy, SshSandboxConfig, TaskId,
    };

    fn task() -> ResolvedTask {
        ResolvedTask {
            id: TaskId("task-1".into()),
            task_file: "/tmp/task.yaml".into(),
            goal: "test".into(),
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
                limits: LimitConfig {
                    timeout_minutes: Some(10),
                    cpu: Some(2),
                    memory: Some("2g".into()),
                    disk: None,
                },
            },
            permissions: PermissionConfig {
                paths: PathPermissions {
                    read: vec!["/tmp/repo/README.md".into()],
                    write: vec!["/tmp/repo/src".into()],
                },
                network: NetworkPermissions {
                    default: NetworkDefault::Disabled,
                    allow_domains: Vec::new(),
                },
                env: EnvPermissions {
                    allow: vec!["CI".into()],
                },
                ..PermissionConfig::default()
            },
            secrets: SecretConfig::default(),
            approval: ApprovalConfig::default(),
            gateway: Default::default(),
            audit: AuditConfig::default(),
        }
    }

    fn remote_ssh_task() -> ResolvedTask {
        let mut task = task();
        task.sandbox = SandboxConfig {
            kind: SandboxKind::RemoteSsh,
            image: None,
            ssh: Some(SshSandboxConfig {
                host: "runner.example".into(),
                user: Some("taskfence".into()),
                port: Some(2222),
                workspace: Some("/srv/taskfence/workspaces/task-1".into()),
                identity_file: Some("/tmp/taskfence/id_ed25519".into()),
                known_hosts_file: Some("/tmp/taskfence/known_hosts".into()),
                isolated_workspace: true,
                isolated_secrets: true,
                terminates_remote_processes: true,
                enforces_resource_limits: false,
                network_policy: Some(SshNetworkPolicy::UncontrolledAllow),
            }),
            limits: LimitConfig {
                timeout_minutes: Some(10),
                cpu: None,
                memory: None,
                disk: None,
            },
        };
        task.permissions.paths.read.clear();
        task.permissions.paths.write.clear();
        task.permissions.env.allow.clear();
        task.permissions.network = NetworkPermissions {
            default: NetworkDefault::Allow,
            allow_domains: Vec::new(),
        };
        task.audit.capture.file_diff = false;
        task
    }

    fn runner() -> DockerRunner {
        DockerRunner::with_host_context(
            BTreeMap::from([
                ("CI".into(), "true".into()),
                ("SECRET_TOKEN".into(), "not-forwarded".into()),
            ]),
            Some("/Users/tester".into()),
        )
    }

    #[test]
    fn docker_runner_plans_mounts_env_network_and_limits() {
        let plan = runner().build_run_plan(&task()).unwrap();

        assert_eq!(plan.network.mode, DockerNetworkMode::None);
        assert_eq!(
            plan.prepared.env,
            BTreeMap::from([("CI".into(), "true".into())])
        );
        assert_eq!(plan.prepared.limits.timeout_minutes, Some(10));
        assert_eq!(plan.prepared.mounts.len(), 2);
        assert_eq!(
            plan.prepared.mounts[0].container_path.as_str(),
            "/workspace/README.md"
        );
        assert_eq!(plan.prepared.mounts[0].mode, MountMode::ReadOnly);
        assert_eq!(
            plan.prepared.mounts[1].container_path.as_str(),
            "/workspace/src"
        );
        assert_eq!(plan.prepared.mounts[1].mode, MountMode::ReadWrite);
    }

    #[test]
    fn docker_runner_adds_dedicated_gateway_spool_mount_for_gateway_tools() {
        let mut task = task();
        task.permissions.paths.read = vec!["/tmp/repo/README.md".into()];
        task.permissions.paths.write = vec!["/tmp/repo/src".into()];
        task.gateway = GatewayConfig {
            mode: GatewayMode::SpoolOnly,
            egress: GatewayEgressConfig::default(),
            tools: vec![GatewayToolConfig {
                protocol: "mcp".into(),
                tool: "github".into(),
                operation: "read_issue".into(),
                connector: GatewayConnectorConfig::Unsupported {
                    kind: "github".into(),
                },
                secret_refs: Vec::new(),
            }],
        };

        let plan = runner().build_run_plan(&task).unwrap();

        assert_eq!(plan.network.mode, DockerNetworkMode::None);
        let spool_mount = plan
            .prepared
            .mounts
            .iter()
            .find(|mount| mount.container_path.as_str() == GATEWAY_SPOOL_CONTAINER_PATH)
            .expect("gateway spool mount should be present");
        assert_eq!(
            spool_mount.host_path.as_str(),
            "/tmp/repo/.taskfence/tasks/task-1/gateway-spool"
        );
        assert_eq!(spool_mount.mode, MountMode::ReadWrite);
    }

    #[test]
    fn gateway_spool_rejects_broad_permission_mounts_that_cover_spool() {
        let mut task = task();
        task.permissions.paths.read.clear();
        task.permissions.paths.write = vec!["/tmp/repo".into()];
        task.gateway = GatewayConfig {
            mode: GatewayMode::SpoolOnly,
            egress: GatewayEgressConfig::default(),
            tools: vec![GatewayToolConfig {
                protocol: "mcp".into(),
                tool: "github".into(),
                operation: "read_issue".into(),
                connector: GatewayConnectorConfig::Unsupported {
                    kind: "github".into(),
                },
                secret_refs: Vec::new(),
            }],
        };

        let err = runner().prepare(&task).unwrap_err();

        assert!(err
            .to_string()
            .contains("gateway spool must be exposed only through its dedicated mount"));
    }

    #[test]
    fn host_env_is_not_inherited_without_allowlist() {
        let mut task = task();
        task.permissions.env.allow.clear();

        let prepared = runner().prepare(&task).unwrap();

        assert!(prepared.env.is_empty());
    }

    #[test]
    fn unsupported_domain_allowlist_fails_closed() {
        let mut task = task();
        task.permissions.network.default = NetworkDefault::Deny;
        task.permissions.network.allow_domains = vec!["api.github.com".into()];

        let err = runner().prepare(&task).unwrap_err();

        assert!(err.to_string().contains("domain allowlists"));
    }

    #[test]
    fn gateway_egress_domain_allowlist_uses_dedicated_gateway_path() {
        let mut task = task();
        task.permissions.network.default = NetworkDefault::Deny;
        task.permissions.network.allow_domains = vec!["api.github.com".into()];
        task.gateway = GatewayConfig {
            mode: GatewayMode::LocalListener,
            egress: GatewayEgressConfig {
                allow_domains: true,
            },
            tools: vec![GatewayToolConfig {
                protocol: GATEWAY_EGRESS_TOOL_PROTOCOL.into(),
                tool: GATEWAY_EGRESS_TOOL_NAME.into(),
                operation: GATEWAY_EGRESS_TOOL_OPERATION.into(),
                connector: GatewayConnectorConfig::Unsupported {
                    kind: "egress".into(),
                },
                secret_refs: Vec::new(),
            }],
        };

        let plan = runner().build_run_plan(&task).unwrap();
        let args = runner()
            .build_docker_run_args(
                "taskfence-test",
                &plan.prepared,
                &AgentInvocation {
                    executable: "codex".into(),
                    args: Vec::new(),
                    env: BTreeMap::new(),
                    working_dir: "/workspace".into(),
                },
            )
            .unwrap();
        let joined = args.join(" ");

        assert_eq!(plan.network.mode, DockerNetworkMode::None);
        assert_eq!(
            plan.prepared.gateway.egress.unwrap().allow_domains,
            vec!["api.github.com"]
        );
        assert_eq!(
            plan.prepared.env.get(TASKFENCE_GATEWAY_MODE_ENV),
            Some(&"local_listener".into())
        );
        assert_eq!(
            plan.prepared
                .env
                .get(TASKFENCE_GATEWAY_EGRESS_ALLOW_DOMAINS_ENV),
            Some(&"api.github.com".into())
        );
        assert!(joined.contains("--network none"));
        assert!(joined.contains(GATEWAY_SPOOL_CONTAINER_PATH));
    }

    #[test]
    fn host_home_mount_fails_closed() {
        let mut task = task();
        task.workspace_host_path = "/Users/tester".into();
        task.permissions.paths.read = vec!["/Users/tester".into()];
        task.permissions.paths.write.clear();

        let err = runner().prepare(&task).unwrap_err();

        assert!(err.to_string().contains("host home"));
    }

    #[test]
    fn docker_socket_mount_fails_closed() {
        let mut task = task();
        task.workspace_host_path = "/var/run".into();
        task.permissions.paths.read = vec!["/var/run/docker.sock".into()];
        task.permissions.paths.write.clear();

        let err = runner().prepare(&task).unwrap_err();

        assert!(err.to_string().contains("Docker socket"));
    }

    #[test]
    fn ssh_auth_socket_mount_fails_closed() {
        let mut task = task();
        task.workspace_host_path = "/tmp".into();
        task.permissions.paths.read = vec!["/tmp/ssh-agent.sock".into()];
        task.permissions.paths.write.clear();
        let runner = DockerRunner::with_host_context(
            BTreeMap::from([("SSH_AUTH_SOCK".into(), "/tmp/ssh-agent.sock".into())]),
            None,
        );

        let err = runner.prepare(&task).unwrap_err();

        assert!(err.to_string().contains("SSH agent socket"));
    }

    #[test]
    fn readonly_readwrite_overlap_fails_closed() {
        let mut task = task();
        task.permissions.paths.read = vec!["/tmp/repo".into()];
        task.permissions.paths.write = vec!["/tmp/repo/src".into()];

        let err = runner().prepare(&task).unwrap_err();

        assert!(err.to_string().contains("mount overlap"));
    }

    #[test]
    fn sensitive_env_allowlist_fails_closed() {
        let mut task = task();
        task.permissions.env.allow = vec!["SSH_AUTH_SOCK".into()];
        let runner = DockerRunner::with_host_context(
            BTreeMap::from([("SSH_AUTH_SOCK".into(), "/tmp/ssh-agent.sock".into())]),
            None,
        );

        let err = runner.prepare(&task).unwrap_err();

        assert!(err.to_string().contains("SSH_AUTH_SOCK"));
    }

    #[test]
    fn default_allow_network_uses_bridge_when_no_domain_rules_exist() {
        let mut task = task();
        task.permissions.network = NetworkPermissions {
            default: NetworkDefault::Allow,
            allow_domains: Vec::new(),
        };
        let plan = runner().build_network_plan(&task).unwrap();

        assert_eq!(plan.mode, DockerNetworkMode::Bridge);
    }

    #[test]
    fn fake_runner_records_invocation_and_returns_exit_status() {
        let fake = FakeRunner::failing(7);
        let prepared = fake.prepare(&task()).unwrap();
        let invocation = AgentInvocation {
            executable: "codex".into(),
            args: vec!["exec".into()],
            env: BTreeMap::new(),
            working_dir: "/workspace".into(),
        };
        let running = fake.start(prepared, invocation.clone()).unwrap();

        assert_eq!(
            fake.collect_exit(&running).unwrap().exit_status.code,
            Some(7)
        );
        assert_eq!(fake.prepared_runs().unwrap().len(), 1);
        assert_eq!(fake.invocations().unwrap(), vec![invocation]);
    }

    #[test]
    fn fake_runner_can_model_timeout() {
        let fake = FakeRunner::timed_out();
        let prepared = fake.prepare(&task()).unwrap();
        let invocation = AgentInvocation {
            executable: "codex".into(),
            args: Vec::new(),
            env: BTreeMap::new(),
            working_dir: "/workspace".into(),
        };
        let running = fake.start(prepared, invocation).unwrap();

        let output = fake.collect_exit(&running).unwrap();

        assert!(output.exit_status.timed_out);
        assert_eq!(output.exit_status.code, None);
    }

    #[test]
    fn docker_run_args_include_sandbox_controls_without_host_secrets() {
        let prepared = runner().prepare(&task()).unwrap();
        let invocation = AgentInvocation {
            executable: "codex".into(),
            args: vec!["exec".into()],
            env: BTreeMap::from([("IGNORED".into(), "not-forwarded".into())]),
            working_dir: "/workspace".into(),
        };

        let args = runner()
            .build_docker_run_args("taskfence-test", &prepared, &invocation)
            .unwrap();
        let joined = args.join(" ");

        assert!(joined.contains("--pull=never"));
        assert!(joined.contains("--network none"));
        assert!(joined.contains(
            "--mount type=bind,source=/tmp/repo/README.md,target=/workspace/README.md,readonly"
        ));
        assert!(joined.contains("--mount type=bind,source=/tmp/repo/src,target=/workspace/src"));
        assert!(joined.contains("--env CI=true"));
        assert!(joined.contains("--cpus 2"));
        assert!(joined.contains("--memory 2g"));
        assert!(joined.ends_with("taskfence/runner:latest codex exec"));
        assert!(!joined.contains("SECRET_TOKEN"));
        assert!(!joined.contains("IGNORED"));
    }

    #[test]
    fn missing_docker_executable_returns_runner_error() {
        let runner = DockerRunner::with_host_context(BTreeMap::new(), None)
            .with_docker_command("/tmp/taskfence-missing-docker");
        let mut task = task();
        task.sandbox.image = Some("debian:bookworm-slim".into());
        task.permissions.env.allow.clear();
        let prepared = runner.prepare(&task).unwrap();
        let invocation = AgentInvocation {
            executable: "true".into(),
            args: Vec::new(),
            env: BTreeMap::new(),
            working_dir: "/workspace".into(),
        };

        let err = runner.start(prepared, invocation).unwrap_err();

        assert!(
            matches!(err, TaskFenceError::Runner(message) if message.contains("docker executable"))
        );
    }

    #[test]
    fn expanded_runner_reports_docker_capabilities_and_domain_gap() {
        let mut task = task();
        task.permissions.network.default = NetworkDefault::Deny;
        task.permissions.network.allow_domains = vec!["api.github.com".into()];
        let runner = ExpandedRunner::with_docker(runner());

        let report = runner.capability_report(&task);

        assert_eq!(report.kind, RunnerKind::Docker);
        assert!(report.available);
        assert!(report.can_enforce_default_deny_network);
        assert!(!report.can_enforce_domain_allowlist);
        assert!(report
            .missing
            .iter()
            .any(|missing| missing.contains("cannot enforce domain allowlists")));
        assert!(runner.ensure_capable(&task).is_err());
    }

    #[test]
    fn expanded_runner_delegates_docker_prepare() {
        let runner = ExpandedRunner::with_docker(runner());

        let prepared = runner.prepare(&task()).unwrap();

        assert_eq!(prepared.task_id, TaskId("task-1".into()));
        assert_eq!(prepared.mounts.len(), 2);
    }

    #[test]
    fn remote_ssh_capability_requires_explicit_runner_contract() {
        let mut task = remote_ssh_task();
        let ssh = task.sandbox.ssh.as_mut().unwrap();
        ssh.isolated_workspace = false;
        ssh.isolated_secrets = false;
        ssh.known_hosts_file = None;
        task.permissions.network.default = NetworkDefault::Deny;
        task.audit.capture.file_diff = true;
        let runner = ExpandedRunner::with_docker(runner());

        let report = runner.capability_report(&task);
        let err = runner.prepare(&task).unwrap_err();

        assert_eq!(report.kind, RunnerKind::RemoteSsh);
        assert!(!report.available);
        assert!(report
            .missing
            .iter()
            .any(|missing| missing.contains("isolated_workspace=true")));
        assert!(report
            .missing
            .iter()
            .any(|missing| missing.contains("known_hosts_file")));
        assert!(report
            .missing
            .iter()
            .any(|missing| missing.contains("default-deny")));
        assert!(report
            .missing
            .iter()
            .any(|missing| missing.contains("file_diff=false")));
        assert!(err.to_string().contains("remote SSH requires"));
    }

    #[test]
    fn remote_ssh_runner_prepares_bounded_plan() {
        let task = remote_ssh_task();
        let runner = RemoteSshRunner::new();

        let prepared = runner.prepare(&task).unwrap();

        assert_eq!(prepared.runner_kind, SandboxKind::RemoteSsh);
        assert!(prepared.image.is_none());
        assert!(prepared.mounts.is_empty());
        assert!(prepared.env.is_empty());
        let ssh = prepared.ssh.unwrap();
        assert_eq!(ssh.host, "runner.example");
        assert_eq!(ssh.user.as_deref(), Some("taskfence"));
        assert_eq!(ssh.port, Some(2222));
        assert_eq!(ssh.workspace.as_str(), "/srv/taskfence/workspaces/task-1");
        assert_eq!(ssh.identity_file.as_str(), "/tmp/taskfence/id_ed25519");
    }

    #[test]
    fn remote_ssh_args_quote_remote_command_and_disable_forwarding() {
        let task = remote_ssh_task();
        let prepared = RemoteSshRunner::new().prepare(&task).unwrap();
        let invocation = AgentInvocation {
            executable: "/usr/bin/printf".into(),
            args: vec!["hello world".into(), "it's safe".into()],
            env: BTreeMap::new(),
            working_dir: "/workspace".into(),
        };

        let args = RemoteSshRunner::new()
            .build_ssh_args(&prepared, &invocation)
            .unwrap();
        let remote_command = args.last().unwrap();

        assert!(args.windows(2).any(|pair| pair == ["-o", "BatchMode=yes"]));
        assert!(args
            .windows(2)
            .any(|pair| pair == ["-o", "ForwardAgent=no"]));
        assert!(args
            .windows(2)
            .any(|pair| pair == ["-o", "StrictHostKeyChecking=yes"]));
        assert!(args.contains(&"taskfence@runner.example".into()));
        assert!(remote_command.starts_with("cd '/srv/taskfence/workspaces/task-1' && exec "));
        assert!(remote_command.contains("'/usr/bin/printf' 'hello world' 'it'\\''s safe'"));
    }

    #[test]
    fn remote_ssh_runner_executes_mock_ssh_and_captures_output() {
        let temp = tempfile::tempdir().unwrap();
        let script = Utf8PathBuf::from_path_buf(temp.path().join("mock-ssh")).unwrap();
        fs::write(
            script.as_std_path(),
            "#!/bin/sh\necho remote-ok\necho remote-err >&2\nexit 3\n",
        )
        .unwrap();
        make_executable(&script);
        let runner = RemoteSshRunner::new().with_ssh_command(script.to_string());
        let prepared = runner.prepare(&remote_ssh_task()).unwrap();
        let invocation = AgentInvocation {
            executable: "/usr/bin/true".into(),
            args: Vec::new(),
            env: BTreeMap::new(),
            working_dir: "/workspace".into(),
        };

        let running = runner.start(prepared, invocation).unwrap();
        let output = runner.collect_exit(&running).unwrap();

        assert_eq!(output.exit_status.code, Some(3));
        assert_eq!(output.stdout, "remote-ok\n");
        assert_eq!(output.stderr, "remote-err\n");
    }

    #[cfg(unix)]
    fn make_executable(path: &Utf8Path) {
        use std::os::unix::fs::PermissionsExt;
        let mut permissions = fs::metadata(path.as_std_path()).unwrap().permissions();
        permissions.set_mode(0o700);
        fs::set_permissions(path.as_std_path(), permissions).unwrap();
    }

    #[cfg(not(unix))]
    fn make_executable(_path: &Utf8Path) {}

    #[test]
    fn expanded_runner_fails_closed_for_remote_runner_families() {
        for (kind, expected) in [
            (
                SandboxKind::KubernetesJob,
                "Kubernetes job namespace/pod security contract",
            ),
            (SandboxKind::MicroVm, "microVM image contract"),
            (
                SandboxKind::ManagedCloud,
                "managed cloud runner provider contract",
            ),
        ] {
            let mut task = task();
            task.sandbox.kind = kind;
            task.sandbox.ssh = None;
            let runner = ExpandedRunner::with_docker(runner());

            let report = runner.capability_report(&task);
            let err = runner.prepare(&task).unwrap_err();

            assert!(!report.available);
            assert!(report.missing.iter().any(|missing| missing == expected));
            assert!(err.to_string().contains(expected), "{err}");
        }
    }

    #[test]
    fn unsupported_runner_fails_closed_for_unknown_sandbox_types() {
        let mut task = task();
        task.sandbox.kind = SandboxKind::Unsupported("bare-metal".into());
        let runner = ExpandedRunner::with_docker(runner());

        let report = runner.capability_report(&task);
        let err = runner.prepare(&task).unwrap_err();

        assert_eq!(report.kind, RunnerKind::Unsupported("bare-metal".into()));
        assert!(!report.available);
        assert!(err
            .to_string()
            .contains("unsupported sandbox type bare-metal"));
    }
}
