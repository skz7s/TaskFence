# 项目结构

本文档记录当前仓库的顶层目录、主要职责，以及结构变化时需要同步的治理面。

## 当前顶层目录

- `Cargo.toml`
- `.editorconfig`
- `.gitattributes`
- `.github/`
- `.codex/skills/`
- `.codex-helper/local-env.toml`
- `crates/`
- `deploy/`
- `governance/`
- `docs/codex/`
- `docs/config/`
- `docs/decisions/`
- `examples/`
- `.codex-helper/design/`
- `scripts/`

## Rust Workspace Layout

- `crates/taskfence-cli/`: `taskfence` command-line interface and terminal UX.
- `crates/taskfence-core/`: shared domain types, ports/traits, errors, and orchestrator boundary.
- `crates/taskfence-config/`: task YAML parsing, validation, defaulting, and path resolution.
- `crates/taskfence-policy/`: built-in allow/deny/approval policy evaluation.
- `crates/taskfence-approval/`: approval records and local approval engine behavior.
- `crates/taskfence-audit/`: append-only JSONL audit logging and redaction/sanitization helpers.
- `crates/taskfence-artifacts/`: local task artifact directories, resolved task output, baseline, and diff collection.
- `crates/taskfence-runner/`: Docker run planning, runner port implementations, and fake runner.
- `crates/taskfence-agent/`: generic agent invocation construction.
- `crates/taskfence-gateway/`: gateway action normalization and mediation contracts.
- `crates/taskfence-report/`: Markdown report generation from structured task evidence.
- `crates/taskfence-state/`: queryable task state store implementations.
- `crates/taskfence-testkit/`: reusable fakes, fixtures, and test helpers.

## Repository Metadata

- `.editorconfig`: editor defaults for UTF-8, LF endings, final newlines,
  trailing-whitespace behavior, and common indentation sizes.
- `.gitattributes`: Git text normalization, generated-governance linguist
  hints, and binary/archive treatment.
- `.github/`: pull request template, issue templates, Dependabot config, and
  CI workflows for the public repository.
- `.gitignore`: build outputs, runtime task state, example demo output, and
  machine-local helper environment facts.

## 文档与治理布局

- 稳定项目事实：`docs/codex/*.md` 与 `docs/config/*.md`
- 文档位置规则：
  - `README.md` 只放项目入口、核心边界、最小命令和文档地图
  - `docs/codex/project-structure.md` 放目录职责和文档布局
  - `docs/codex/structure-contract.md` 放模块边界、禁止耦合和长期架构约束
  - `docs/codex/runtime-architecture.md` 放已实现或已确认的运行时流程
  - `docs/codex/plans/*.md` 放 Codex plan mode 的活跃或阻塞持久化执行计划和 phase 状态
  - `docs/codex/plan_archived/*.md` 放已完成并记录最终证据的计划归档，保留原文件名
- `docs/config/*.md` 放环境变量、部署和操作参数
- `docs/decisions/*.md` 放持久治理、部署、依赖源、安全或架构决策记录
- `docs/quickstart.md` 放首次贡献者无需 Docker/token 的成功路径
- `docs/cli-reference.md` 放当前 preview CLI 命令、参数和本地证据工作流参考
- `docs/task-file-reference.md` 放当前 preview task YAML 字段、默认值和 fail-closed 限制
- `docs/testing.md` 放默认 CI、聚焦 crate 测试、示例验证、Docker/live 集成前提和 coverage 记录策略
- `docs/security-model.md` 放面向外部审查的安全边界、受保护资产、威胁范围和当前限制
- `docs/versioning.md` 放 preview 版本、MSRV、task-file、CLI、结构化证据和弃用兼容策略
- `docs/supply-chain.md` 放依赖更新、外部审计工具、包发布和 secret/source hygiene 策略
- `examples/README.md` 放示例矩阵、运行要求和验证命令
  - `governance/*` 放治理生成、skill routing、profile 和变更同步规则
- 运行时治理源码：`governance/*`
- 可复用治理模板：由已安装的本地治理 catalog 提供；目标项目不要把 helper 源仓库本身当作模板真值
- 机器本地环境事实：`.codex-helper/local-env.toml`，由 `project-env-baseline` 或 `deploy/manage.sh detect-env` 生成，不进入 Git。
- 可提交 baseline 事实：`governance/profile.toml` 的 `[baseline]`，只记录 helper baseline version/source/sync/preflight metadata，不记录本机路径。
- 治理决策记录：decisions docs directory 下的 `YYYY-MM-DD-topic.md` 文件，只记录稳定决策，不记录 secret 或 host-specific 路径。
- 项目设计资产：`.codex-helper/design/draft/` 与 `.codex-helper/design/ui-library/`
- 可复用工作流：`.codex/skills/*`；默认安装列表以 helper baseline manifest 为准，新增默认 skill 必须同步路由和生成输出。`managed-project-onboarding` 保持为显式选择的可选模板
- 项目私有 agent/skill 源码：`governance/private/*`，不要直接修改生成输出
- `scripts/` 建议按 `bootstrap/`、`governance/`、`test/` 分层
- 当前测试主要跟随 Rust crate 放在各 crate 的 `src/*.rs` 单元测试和 `taskfence-testkit` fixtures 中；只有出现跨 crate 或端到端测试需要时再新增顶层 `tests/`。
