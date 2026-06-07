use camino::Utf8PathBuf;
use std::collections::BTreeMap;
use std::fs;
use std::process::Command;
use taskfence_core::{
    AgentConfig, AgentInvocation, AgentKind, ApprovalConfig, AuditConfig, EnvPermissions,
    LimitConfig, NetworkDefault, NetworkPermissions, PathPermissions, PermissionConfig, Runner,
    SandboxConfig, SandboxKind, SecretConfig, TaskId,
};
use taskfence_runner::DockerRunner;

const IMAGE: &str = "debian:bookworm-slim";

#[test]
#[ignore = "requires Docker daemon and a locally available test image"]
fn docker_runner_captures_exit_logs_missing_image_and_timeout() {
    if !docker_available() {
        eprintln!("skipping Docker integration: Docker daemon is unavailable");
        return;
    }
    if !docker_image_available(IMAGE) {
        eprintln!("skipping Docker integration: local image {IMAGE} is unavailable");
        return;
    }

    let runner = DockerRunner::with_host_context(BTreeMap::new(), None);
    let (_success_temp, task) = docker_task("docker-success", IMAGE, Some(5));
    let prepared = runner.prepare(&task).unwrap();
    let running = runner
        .start(
            prepared,
            AgentInvocation {
                executable: "/bin/sh".into(),
                args: vec![
                    "-c".into(),
                    "printf taskfence-stdout; printf taskfence-stderr >&2; exit 7".into(),
                ],
                env: BTreeMap::new(),
                working_dir: "/workspace".into(),
            },
        )
        .unwrap();
    let output = runner.collect_exit(&running).unwrap();

    assert_eq!(output.exit_status.code, Some(7));
    assert!(!output.exit_status.timed_out);
    assert_eq!(output.stdout, "taskfence-stdout");
    assert_eq!(output.stderr, "taskfence-stderr");

    let (_missing_temp, missing_task) = docker_task(
        "docker-missing-image",
        "taskfence/missing-image:phase3",
        Some(5),
    );
    let missing_prepared = runner.prepare(&missing_task).unwrap();
    let missing_running = runner
        .start(
            missing_prepared,
            AgentInvocation {
                executable: "true".into(),
                args: Vec::new(),
                env: BTreeMap::new(),
                working_dir: "/workspace".into(),
            },
        )
        .unwrap();
    let missing = runner.collect_exit(&missing_running).unwrap();

    assert_ne!(missing.exit_status.code, Some(0));
    assert!(
        missing.stderr.contains("No such image")
            || missing.stderr.contains("not found")
            || missing.stderr.contains("Unable to find image locally"),
        "missing image stderr was: {}",
        missing.stderr
    );

    let (_timeout_temp, timeout_task) = docker_task("docker-timeout", IMAGE, Some(0));
    let timeout_prepared = runner.prepare(&timeout_task).unwrap();
    let timeout_running = runner
        .start(
            timeout_prepared,
            AgentInvocation {
                executable: "/bin/sh".into(),
                args: vec!["-c".into(), "sleep 5".into()],
                env: BTreeMap::new(),
                working_dir: "/workspace".into(),
            },
        )
        .unwrap();
    let timed_out = runner.collect_exit(&timeout_running).unwrap();

    assert!(timed_out.exit_status.timed_out);
    assert_eq!(timed_out.exit_status.code, None);
}

fn docker_task(
    id: &str,
    image: &str,
    timeout_minutes: Option<u64>,
) -> (tempfile::TempDir, taskfence_core::ResolvedTask) {
    let temp_root = std::env::current_dir().unwrap();
    let temp = tempfile::Builder::new()
        .prefix("taskfence-docker-")
        .tempdir_in(temp_root)
        .unwrap();
    let workspace = Utf8PathBuf::from_path_buf(temp.path().join("repo")).unwrap();
    fs::create_dir(&workspace).unwrap();
    fs::write(workspace.join("README.md"), "readme\n").unwrap();
    fs::create_dir(workspace.join("src")).unwrap();

    let task = taskfence_core::ResolvedTask {
        id: TaskId(id.into()),
        task_file: workspace.join("task.yaml"),
        goal: "Docker integration".into(),
        workspace_host_path: workspace.clone(),
        workspace_container_path: "/workspace".into(),
        agent: AgentConfig {
            kind: AgentKind::Generic,
            command: "true".into(),
            args: Vec::new(),
        },
        sandbox: SandboxConfig {
            kind: SandboxKind::Docker,
            image: Some(image.into()),
            limits: LimitConfig {
                timeout_minutes,
                cpu: None,
                memory: None,
                disk: None,
            },
        },
        permissions: PermissionConfig {
            paths: PathPermissions {
                read: vec![workspace.join("README.md")],
                write: vec![workspace.join("src")],
            },
            commands: Default::default(),
            network: NetworkPermissions {
                default: NetworkDefault::Disabled,
                allow_domains: Vec::new(),
            },
            env: EnvPermissions::default(),
            tools: Default::default(),
            budget: Default::default(),
        },
        secrets: SecretConfig::default(),
        approval: ApprovalConfig::default(),
        audit: AuditConfig::default(),
    };
    (temp, task)
}

fn docker_available() -> bool {
    Command::new("docker")
        .arg("info")
        .output()
        .is_ok_and(|output| output.status.success())
}

fn docker_image_available(image: &str) -> bool {
    Command::new("docker")
        .args(["image", "inspect", image])
        .output()
        .is_ok_and(|output| output.status.success())
}
