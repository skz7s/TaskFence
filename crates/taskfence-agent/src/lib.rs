use std::collections::BTreeMap;

use taskfence_core::{AgentAdapter, AgentInvocation, AgentKind, ResolvedTask, TaskFenceError};

#[derive(Clone, Copy, Debug, Default)]
pub struct GenericAgentAdapter;

impl AgentAdapter for GenericAgentAdapter {
    fn build_invocation(&self, task: &ResolvedTask) -> taskfence_core::Result<AgentInvocation> {
        build_generic_invocation(task)
    }
}

pub fn build_generic_invocation(task: &ResolvedTask) -> taskfence_core::Result<AgentInvocation> {
    match &task.agent.kind {
        AgentKind::Generic => {}
        AgentKind::Specialized(kind) => {
            return Err(TaskFenceError::Unsupported(format!(
                "generic adapter cannot run specialized agent type {kind}"
            )));
        }
    }

    let executable = task.agent.command.trim();
    if executable.is_empty() {
        return Err(TaskFenceError::Config(
            "agent.command must not be empty".into(),
        ));
    }
    if executable.chars().any(char::is_whitespace) {
        return Err(TaskFenceError::Config(
            "agent.command must be an executable; put arguments in agent.args".into(),
        ));
    }
    reject_nul("agent.command", executable)?;
    for arg in &task.agent.args {
        reject_nul("agent.args", arg)?;
    }

    Ok(AgentInvocation {
        executable: executable.to_owned(),
        args: task.agent.args.clone(),
        env: BTreeMap::new(),
        working_dir: task.workspace_container_path.clone(),
    })
}

fn reject_nul(field: &str, value: &str) -> taskfence_core::Result<()> {
    if value.contains('\0') {
        Err(TaskFenceError::Config(format!(
            "{field} must not contain NUL"
        )))
    } else {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use taskfence_core::{
        AgentConfig, ApprovalConfig, AuditConfig, LimitConfig, PermissionConfig, SandboxConfig,
        SandboxKind, SecretConfig, TaskId,
    };

    fn task_with_agent(agent: AgentConfig) -> ResolvedTask {
        ResolvedTask {
            id: TaskId("task-1".into()),
            task_file: "/tmp/task.yaml".into(),
            goal: "test".into(),
            workspace_host_path: "/tmp/repo".into(),
            workspace_container_path: "/workspace".into(),
            agent,
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

    #[test]
    fn generic_adapter_builds_runner_invocation_without_host_env() {
        let task = task_with_agent(AgentConfig {
            kind: AgentKind::Generic,
            command: "codex".into(),
            args: vec!["--ask-for-approval".into(), "never".into()],
        });

        let invocation = GenericAgentAdapter.build_invocation(&task).unwrap();

        assert_eq!(invocation.executable, "codex");
        assert_eq!(invocation.args, vec!["--ask-for-approval", "never"]);
        assert_eq!(invocation.working_dir, "/workspace");
        assert!(invocation.env.is_empty());
    }

    #[test]
    fn generic_adapter_rejects_specialized_agent_kind() {
        let task = task_with_agent(AgentConfig {
            kind: AgentKind::Specialized("codex".into()),
            command: "codex".into(),
            args: Vec::new(),
        });

        assert!(GenericAgentAdapter.build_invocation(&task).is_err());
    }

    #[test]
    fn generic_adapter_rejects_arguments_embedded_in_command() {
        let task = task_with_agent(AgentConfig {
            kind: AgentKind::Generic,
            command: "codex exec".into(),
            args: Vec::new(),
        });

        assert!(GenericAgentAdapter.build_invocation(&task).is_err());
    }
}
