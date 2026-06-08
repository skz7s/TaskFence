use std::collections::BTreeMap;

use taskfence_core::{
    AgentAdapter, AgentInvocation, AgentKind, NetworkDefault, ResolvedTask, TaskFenceError,
};

#[derive(Clone, Copy, Debug, Default)]
pub struct GenericAgentAdapter;

impl AgentAdapter for GenericAgentAdapter {
    fn build_invocation(&self, task: &ResolvedTask) -> taskfence_core::Result<AgentInvocation> {
        build_generic_invocation(task)
    }
}

#[derive(Clone, Copy, Debug, Default)]
pub struct SpecializedAgentAdapter;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SpecializedAgentProfile {
    CodexCli,
    ClaudeCode,
    GeminiCli,
    OpenHands,
}

impl SpecializedAgentProfile {
    pub fn parse(kind: &str) -> Option<Self> {
        match normalize_agent_kind(kind).as_str() {
            "codex" | "codex_cli" | "codex-cli" => Some(Self::CodexCli),
            "claude_code" | "claude-code" | "claude" => Some(Self::ClaudeCode),
            "gemini" | "gemini_cli" | "gemini-cli" => Some(Self::GeminiCli),
            "openhands" | "open_hands" | "open-hands" => Some(Self::OpenHands),
            _ => None,
        }
    }

    pub fn label(&self) -> &'static str {
        match self {
            Self::CodexCli => "codex_cli",
            Self::ClaudeCode => "claude_code",
            Self::GeminiCli => "gemini_cli",
            Self::OpenHands => "openhands",
        }
    }

    pub fn default_command(&self) -> &'static str {
        match self {
            Self::CodexCli => "codex",
            Self::ClaudeCode => "claude",
            Self::GeminiCli => "gemini",
            Self::OpenHands => "openhands",
        }
    }

    fn prompt_env_name(&self) -> &'static str {
        match self {
            Self::CodexCli => "TASKFENCE_CODEX_PROMPT",
            Self::ClaudeCode => "TASKFENCE_CLAUDE_CODE_PROMPT",
            Self::GeminiCli => "TASKFENCE_GEMINI_PROMPT",
            Self::OpenHands => "TASKFENCE_OPENHANDS_PROMPT",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CodingAgentPolicyTemplate {
    pub agent_type: String,
    pub command_allow: Vec<String>,
    pub command_approval_required: Vec<String>,
    pub command_deny: Vec<String>,
    pub adapter_env: Vec<String>,
    pub network_default: NetworkDefault,
    pub notes: Vec<String>,
}

pub fn coding_agent_policy_template(kind: &str) -> Option<CodingAgentPolicyTemplate> {
    let profile = SpecializedAgentProfile::parse(kind)?;
    Some(CodingAgentPolicyTemplate {
        agent_type: profile.label().into(),
        command_allow: vec![profile.default_command().into()],
        command_approval_required: Vec::new(),
        command_deny: vec!["sudo *".into(), "docker *".into(), "ssh *".into()],
        adapter_env: vec![
            "TASKFENCE_AGENT_PROFILE".into(),
            profile.prompt_env_name().into(),
            "TASKFENCE_WORKSPACE".into(),
            "TASKFENCE_GATEWAY_MODE".into(),
        ],
        network_default: NetworkDefault::Disabled,
        notes: vec![
            "template is guidance for explicit task permissions and is not applied automatically"
                .into(),
            "adapter_env entries are generated inside the runner and do not permit host env passthrough"
                .into(),
            "add gateway tool permissions explicitly when the agent needs mediated external actions"
                .into(),
        ],
    })
}

impl AgentAdapter for SpecializedAgentAdapter {
    fn build_invocation(&self, task: &ResolvedTask) -> taskfence_core::Result<AgentInvocation> {
        build_specialized_invocation(task)
    }
}

pub fn build_specialized_invocation(
    task: &ResolvedTask,
) -> taskfence_core::Result<AgentInvocation> {
    let AgentKind::Specialized(kind) = &task.agent.kind else {
        return build_generic_invocation(task);
    };
    let Some(profile) = SpecializedAgentProfile::parse(kind) else {
        return Err(TaskFenceError::Unsupported(format!(
            "specialized agent type {kind} is not supported; use generic or one of codex_cli, claude_code, gemini_cli, openhands"
        )));
    };

    let executable = task.agent.command.trim();
    let executable = if executable.is_empty() {
        profile.default_command()
    } else {
        executable
    };
    validate_executable("agent.command", executable)?;
    for arg in &task.agent.args {
        reject_nul("agent.args", arg)?;
    }

    let mut env = BTreeMap::new();
    env.insert("TASKFENCE_AGENT_PROFILE".into(), profile.label().into());
    env.insert(profile.prompt_env_name().into(), task.goal.clone());
    env.insert(
        "TASKFENCE_WORKSPACE".into(),
        task.workspace_container_path.to_string(),
    );
    if !task.gateway.tools.is_empty() {
        env.insert("TASKFENCE_GATEWAY_MODE".into(), "configured".into());
    }

    Ok(AgentInvocation {
        executable: executable.to_owned(),
        args: task.agent.args.clone(),
        env,
        working_dir: task.workspace_container_path.clone(),
    })
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
    validate_executable("agent.command", executable)?;
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

fn validate_executable(field: &str, value: &str) -> taskfence_core::Result<()> {
    if value.chars().any(char::is_whitespace) {
        return Err(TaskFenceError::Config(format!(
            "{field} must be an executable; put arguments in agent.args"
        )));
    }
    reject_nul(field, value)
}

fn normalize_agent_kind(kind: &str) -> String {
    kind.trim().to_ascii_lowercase()
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

    #[test]
    fn specialized_codex_adapter_adds_non_secret_runtime_hints() {
        let mut task = task_with_agent(AgentConfig {
            kind: AgentKind::Specialized("codex_cli".into()),
            command: "codex".into(),
            args: vec!["exec".into()],
        });
        task.goal = "fix the test".into();

        let invocation = SpecializedAgentAdapter.build_invocation(&task).unwrap();

        assert_eq!(invocation.executable, "codex");
        assert_eq!(invocation.args, vec!["exec"]);
        assert_eq!(invocation.working_dir, "/workspace");
        assert_eq!(
            invocation.env.get("TASKFENCE_AGENT_PROFILE"),
            Some(&"codex_cli".into())
        );
        assert_eq!(
            invocation.env.get("TASKFENCE_CODEX_PROMPT"),
            Some(&"fix the test".into())
        );
        assert!(!invocation.env.keys().any(|key| key.contains("TOKEN")));
    }

    #[test]
    fn specialized_adapter_accepts_known_coding_agent_aliases() {
        for kind in ["claude-code", "gemini_cli", "openhands"] {
            let task = task_with_agent(AgentConfig {
                kind: AgentKind::Specialized(kind.into()),
                command: "agent-bin".into(),
                args: Vec::new(),
            });

            let invocation = SpecializedAgentAdapter.build_invocation(&task).unwrap();
            assert_eq!(invocation.executable, "agent-bin");
        }
    }

    #[test]
    fn specialized_adapter_rejects_unknown_agent_kind() {
        let task = task_with_agent(AgentConfig {
            kind: AgentKind::Specialized("unknown-agent".into()),
            command: "agent".into(),
            args: Vec::new(),
        });

        assert!(matches!(
            SpecializedAgentAdapter.build_invocation(&task),
            Err(TaskFenceError::Unsupported(message))
                if message.contains("specialized agent type unknown-agent")
        ));
    }

    #[test]
    fn coding_agent_policy_template_is_conservative_and_explicit() {
        let template = coding_agent_policy_template("claude-code").unwrap();

        assert_eq!(template.agent_type, "claude_code");
        assert_eq!(template.command_allow, vec!["claude"]);
        assert!(template
            .command_deny
            .iter()
            .any(|pattern| pattern == "sudo *"));
        assert_eq!(template.network_default, NetworkDefault::Disabled);
        assert!(template
            .adapter_env
            .contains(&"TASKFENCE_CLAUDE_CODE_PROMPT".into()));
        assert!(template
            .notes
            .iter()
            .any(|note| note.contains("not applied automatically")));
    }

    #[test]
    fn coding_agent_policy_template_rejects_unknown_profiles() {
        assert!(coding_agent_policy_template("unsupported-agent").is_none());
    }
}
