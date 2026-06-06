# 结构契约

- 治理增强或自动化流程应先明确边界、权限和验证入口，再进入长期运行时契约。
- 不重新引入 Plan -> Dispatch -> Worker -> Goals 自动流水线。
- 可以把 Codex plan mode 作为默认协作方法，但不得把它实现成 helper-owned runtime、后台状态机、队列或自动推进链路。
- 不让治理增强自动生成 active dispatch、phase queue、worker handoff 或 `.codex-helper/runtime.json`。
- helper-managed Codex role 只保留 `governance`。
- 不要把 provider 管理、Assistant 管理、项目级 live workspace 或 app-server lifecycle 写进默认 baseline；只有本项目明确需要并建模后才能加入。
- `.codex-helper/local-env.toml` 是 ignored machine-local 环境事实缓存；不要把其中的 host-specific 路径、版本、依赖源、镜像、代理或包管理器状态写进可同步治理真值。
- 默认治理 skills 以 helper baseline manifest 为准；新增默认 skill 必须同步 `governance/skill-routing.toml`、`governance/skill-maintenance.md` 和生成输出。`managed-project-onboarding` 保持为显式 opt-in。
- `governance/profile.toml` 的 `[baseline]` 只保存可提交的 baseline version/source/sync/preflight metadata；baseline 缺失、过期或未知来源必须作为治理 health 风险暴露。
- `codex-helper governance preflight <project-id>` 是 release、deploy 和 baseline-upgrade 的只读聚合 gate；`codex-helper governance repair <project-id> --dry-run` 只能输出修复计划，不自动改文件。
- docs、governance、generated outputs 和 `.codex-helper/local-env.toml` 不允许保存 provider token、registry token、Bearer/Auth header、URL userinfo 或真实私钥类敏感值。
- 重大治理、部署、provider、依赖源、安全或 lifecycle 策略变化要写入 `docs/decisions/`。
- 项目私有 agent/skill 的标准流程是：先写 `governance/private/*` 源文件，再登记 `governance/modules.toml`，必要时把 agent 片段挂进 `governance/bundles.toml`，然后运行 `python3 scripts/governance/build_agents.py` 和 `python3 scripts/governance/check_codex_governance.py`。
- `AGENTS.md`、`.codex/skills/*`、`governance/core/*` 都是 generated-but-committed 生成物；如果长期需要修改，必须回写源文件后重新 build。
- `codex review`、结构化非交互输出、MCP、plugins、hooks、cloud tasks 和 subagents 默认只作为 operator 手动输入或显式 opt-in 集成；不要写进默认 inject baseline
- 显式 opt-in 的 review subagents 只能作为主线程编排的辅助能力使用；用户显式要求多轮 review 即授权 review subagents 以隔离上下文。多轮 review 必须逐轮串行汇总，同一大轮次内才允许按方向并行；启动多个 subagents 前先检查当前 Codex 配置里的 `[agents].max_threads`，未配置时按默认上限 `6` 估算，并避免一次并行启动超过剩余可用槽位；后续修复应使用新的实现型 subagents 而不是沿用 reviewer 上下文
