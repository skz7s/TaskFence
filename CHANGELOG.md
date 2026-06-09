# Changelog

All notable changes to TaskFence will be documented in this file.

The project has not made its first stable release. Until then, entries describe
development snapshots and readiness milestones.

## Unreleased

### Added

- Open-source collaboration documentation for contribution, support, security,
  release, and maintainer workflows.
- GitHub pull request, issue, and CI scaffolding for standard repository
  validation.
- Public security model, versioning and compatibility policy, and supply-chain
  maintenance policy for preview releases.
- Public CLI and task-file reference documentation for the implemented preview
  interface.
- Rustdoc generation with warnings denied in the documented release and CI
  gates.
- Public testing strategy for default CI, focused crate checks, examples,
  Docker integration prerequisites, and skipped coverage reporting.

### Current Preview Scope

- Local Rust CLI, task validation, Docker runner path, remote SSH runner under
  an explicit capability contract, bounded gateway connector foundations,
  structured audit/artifact/report generation, local review/replay tooling, and
  local team-state foundations.

### Not Production Supported

- Long-lived production API daemon, deployed team server, production Web UI,
  production MCP server, arbitrary HTTP proxy, SDK/webhook connectors, SSO,
  object storage, Kubernetes/microVM/managed-cloud live execution, and live
  replay of externally visible connector effects.
