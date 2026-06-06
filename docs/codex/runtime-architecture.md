# 运行时架构

- 本项目的运行时架构应记录已实现或已确认的长期流程、边界和约束。
- `.codex-helper/planning/*` 与 `.codex-helper/runtime.json` 属于 legacy 数据；除非本项目明确重新建模，否则新运行时不写入、不发布、不消费这些自动推进工件。
- 治理增强或自动化流程如需调用本地 `codex` executable，缺失时应显式报错，不要引入隐藏的远程或后台执行回退。
- 未经本项目明确建模和验证，不要把 provider 管理、Assistant 管理、项目级 live workspace、app-server bridge 或 live session host 写进默认运行时边界。
- `.codex-helper/local-env.toml` 是 repo-local、machine-local 的环境事实缓存，不进入 Git；macOS 开发、Linux 部署和多机器切换时优先从这里选择命令。
- 默认 helper-managed skills 以 helper baseline manifest 为准；`managed-project-onboarding` 保持为可选模板
- `governance/profile.toml` 的 `[baseline]` 只保存可提交的 baseline version/source/sync/preflight metadata；不要把它当作机器事实真值
- `codex-helper governance preflight <project-id>` 是只读 release/deploy/baseline-upgrade gate；`codex-helper governance repair <project-id> --dry-run` 只输出修复计划
- secret 风险诊断只输出路径和类别，不打印匹配值；重大治理、部署、provider、依赖源、安全或 lifecycle 策略变化应写入 `docs/decisions/`
- 稳定项目事实继续放在 `docs/codex/`、`docs/config/` 或 `governance/private/*`；SQLite 和 `governance/profile.toml` 不作为机器事实真值。
- 可复用模板源由已安装的本地治理 catalog 管理；外部模板源不再作为默认运行时面，托管项目只同步默认模板和显式选择的本地模板
- 托管项目同步不能以 helper 源仓库自身为模板内容输入；目标项目应接收治理 catalog/template 产物，而不是复制某个项目的私有治理事实。
- 项目保存的 `selected_core_modules` 是默认模板之上的附加选择；即使旧记录只保存了部分默认模板，sync/build 也会补齐 default tier，避免默认 skill 被裁掉。
- 默认开发工作流可以使用 Codex plan mode 组织对话，但计划执行状态只保存在仓库文档中，不进入 helper 运行时状态面。
- 用户显式要求多轮 review、subagents、并行代理、委派代理或 agent workers 时，review 可以使用显式 subagents 作为 bounded helper，以隔离上下文并提升 review 效果。主线程必须保持对 scope、轮次顺序和结论汇总的控制；轮次之间串行推进，同一大轮次内才允许按 review 方向或子系统并行展开。启动多个 subagents 前先检查当前 Codex 配置里的 `[agents].max_threads`；如果没有显式配置，就按默认并发上限 `6` 估算，并且把本轮并行数量控制在剩余可用槽位之内。
- 仓库如果已经有 checked-in 的 debug Docker / Compose 环境，例如 `compose.debug.yaml`，调试和容器化测试都应复用这套 debug 环境，修改相关代码、依赖、环境变量契约或运维脚本后也应同步更新它；如果仓库没有 debug Docker，就忽略该面，不要把它当成默认必改项。debug Docker 中运行项目时应使用文档化的 `dev` 命令保证热更新；生产部署 Docker / Compose 应保持为独立清单，不要和 debug 栈混用。
- `codex review`、结构化非交互输出、MCP、plugins、hooks、cloud tasks 和 subagents 默认保持 opt-in；只有目标仓库明确建模配置、权限、审计和验证入口后才能进入自动化路径

## TaskFence Local Runner

- 当前已实现的运行面是 `cargo run -p taskfence-cli -- run examples/task.yaml`，它通过 CLI 加载任务文件，构造本地 policy / approval / audit / artifact / agent / Docker runner / report / state 组件，并进入 `taskfence-core` orchestrator。
- Docker runner 使用 `docker run --pull=never`，不会在任务运行时静默拉取镜像；演示任务依赖本机已有 `debian:bookworm-slim`。
- 本地 Docker runner 仅声明已实现的网络模式：`disabled` / default deny 使用 `--network none`，default allow 使用 Docker bridge。域名级 allowlist 目前无法由本地 Docker 直接强制执行，配置 `allow_domains` 时必须 fail closed，直到实现 enforcing proxy。
- 运行成功后，本地 artifact store 在任务 workspace 下写入 `.taskfence/tasks/<task-id>/task.resolved.json`、`events.jsonl`、stdout/stderr 日志（有输出时）、`diff.patch` 和 `report.md`。
- Gateway、Web UI、replay、team-server、enterprise control plane 仍是未实现面；当前 gateway crate 只提供规范化和显式 unsupported protocol 合同。
