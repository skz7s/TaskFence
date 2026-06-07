use camino::{Utf8Path, Utf8PathBuf};
use std::collections::{BTreeMap, BTreeSet};
use std::io::Read;
use std::process::{Command, Stdio};
use std::sync::{Arc, Mutex, MutexGuard};
use std::thread;
use std::time::{Duration, Instant};

use taskfence_core::{
    AgentInvocation, ExitStatus, MountMode, MountPlan, NetworkDefault, NetworkPermissions,
    PreparedRun, ResolvedTask, RunOutput, Runner, RunningTask, SandboxKind, TaskFenceError, TaskId,
    GATEWAY_SPOOL_CONTAINER_PATH, GATEWAY_SPOOL_DIR_NAME,
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
    pub missing: Vec<String>,
}

impl RunnerCapabilityReport {
    pub fn docker(network: &NetworkPermissions) -> Self {
        let mut missing = Vec::new();
        if !network.allow_domains.is_empty() {
            missing.push("domain allowlist enforcement".into());
        }
        Self {
            kind: RunnerKind::Docker,
            available: true,
            can_isolate_filesystem: true,
            can_isolate_secrets: true,
            can_disable_network: true,
            can_enforce_default_deny_network: true,
            can_enforce_domain_allowlist: false,
            can_enforce_limits: true,
            can_capture_output: true,
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
            missing,
        }
    }

    fn is_sufficient_for_task(&self, task: &ResolvedTask) -> bool {
        self.available
            && self.can_isolate_filesystem
            && self.can_isolate_secrets
            && self.can_capture_output
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
        }
    }

    pub fn with_docker(docker: DockerRunner) -> Self {
        Self { docker }
    }

    pub fn capability_report(&self, task: &ResolvedTask) -> RunnerCapabilityReport {
        match RunnerKind::from_sandbox(&task.sandbox.kind) {
            RunnerKind::Docker => RunnerCapabilityReport::docker(&task.permissions.network),
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
            _ => self.unsupported_runner(task).prepare(task),
        }
    }

    fn start(
        &self,
        prepared: PreparedRun,
        invocation: AgentInvocation,
    ) -> taskfence_core::Result<RunningTask> {
        self.docker.start(prepared, invocation)
    }

    fn stop(&self, running: &RunningTask) -> taskfence_core::Result<()> {
        self.docker.stop(running)
    }

    fn collect_exit(&self, running: &RunningTask) -> taskfence_core::Result<RunOutput> {
        self.docker.collect_exit(running)
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
        let network = self.build_network_plan(&task.permissions.network)?;

        Ok(DockerRunPlan {
            prepared: PreparedRun {
                task_id: task.id.clone(),
                image: task.sandbox.image.clone(),
                mounts,
                env,
                network: task.permissions.network.clone(),
                limits: task.sandbox.limits.clone(),
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
        let gateway_spool_path = if !task.gateway.tools.is_empty() {
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
        Ok(env)
    }

    pub fn build_network_plan(
        &self,
        network: &NetworkPermissions,
    ) -> taskfence_core::Result<DockerNetworkPlan> {
        if !network.allow_domains.is_empty() {
            return Err(TaskFenceError::Runner(
                "local Docker cannot enforce domain allowlists; configure an enforcing proxy before allowing domains"
                    .into(),
            ));
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

        lock_completed(&self.completed)?.insert(runner_ref.clone(), output);

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
        lock_completed(&self.completed)?
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
            docker_network_arg(&prepared.network)?.into(),
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
    let mut child = Command::new(docker_command)
        .args(args)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|err| {
            if err.kind() == std::io::ErrorKind::NotFound {
                TaskFenceError::Runner("docker executable is unavailable".into())
            } else {
                TaskFenceError::Runner(format!("failed to start docker: {err}"))
            }
        })?;

    let stdout = child
        .stdout
        .take()
        .ok_or_else(|| TaskFenceError::Runner("failed to capture docker stdout".into()))?;
    let stderr = child
        .stderr
        .take()
        .ok_or_else(|| TaskFenceError::Runner("failed to capture docker stderr".into()))?;

    let stdout_reader = thread::spawn(move || read_pipe(stdout));
    let stderr_reader = thread::spawn(move || read_pipe(stderr));
    let deadline = timeout.map(|duration| Instant::now() + duration);
    let mut timed_out = false;

    let status = loop {
        if let Some(status) = child
            .try_wait()
            .map_err(|err| TaskFenceError::Runner(format!("failed to wait for docker: {err}")))?
        {
            break status;
        }

        if deadline.is_some_and(|deadline| Instant::now() >= deadline) {
            timed_out = true;
            child
                .kill()
                .map_err(|err| TaskFenceError::Runner(format!("failed to kill docker: {err}")))?;
            let _ = Command::new(docker_command)
                .args(["rm", "-f", runner_ref])
                .output();
            break child.wait().map_err(|err| {
                TaskFenceError::Runner(format!("failed to wait for killed docker: {err}"))
            })?;
        }

        thread::sleep(Duration::from_millis(50));
    };

    let stdout = join_reader(stdout_reader, "stdout")?;
    let stderr = join_reader(stderr_reader, "stderr")?;
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
        .map_err(|_| TaskFenceError::Runner(format!("docker {stream} reader panicked")))?;
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

fn docker_network_arg(network: &NetworkPermissions) -> taskfence_core::Result<&'static str> {
    if !network.allow_domains.is_empty() {
        return Err(TaskFenceError::Runner(
            "local Docker cannot enforce domain allowlists; configure an enforcing proxy before allowing domains"
                .into(),
        ));
    }
    match network.default {
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

fn lock_completed(
    completed: &Mutex<BTreeMap<String, RunOutput>>,
) -> taskfence_core::Result<MutexGuard<'_, BTreeMap<String, RunOutput>>> {
    completed
        .lock()
        .map_err(|_| TaskFenceError::Runner("Docker runner completion store is poisoned".into()))
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
            image: task.sandbox.image.clone(),
            mounts: Vec::new(),
            env: BTreeMap::new(),
            network: task.permissions.network.clone(),
            limits: task.sandbox.limits.clone(),
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
    use taskfence_core::{
        AgentConfig, AgentKind, ApprovalConfig, AuditConfig, EnvPermissions, GatewayConfig,
        GatewayConnectorConfig, GatewayToolConfig, LimitConfig, PathPermissions, PermissionConfig,
        SandboxConfig, SecretConfig, TaskId,
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
        let plan = runner()
            .build_network_plan(&NetworkPermissions {
                default: NetworkDefault::Allow,
                allow_domains: Vec::new(),
            })
            .unwrap();

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
            .contains(&"domain allowlist enforcement".into()));
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
    fn expanded_runner_fails_closed_for_remote_runner_families() {
        for (kind, expected) in [
            (SandboxKind::RemoteSsh, "remote SSH isolation contract"),
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
