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
- Repository metadata defaults for editor settings, Git text normalization,
  generated-governance language statistics, and binary/archive attributes.
- Documentation index organized by contributor task, public preview interface,
  runtime direction, security/release maintenance, and governance references.
- Release notes template for implemented surfaces, unsupported surfaces,
  security changes, compatibility impact, validation evidence, skipped
  integration coverage, supply-chain notes, known issues, and publication
  approvals.
- README badges for CI, Apache-2.0 licensing, Rust MSRV, and local-preview
  readiness status.
- GitHub issue routing for documentation issues and support questions, with
  blank issues disabled to keep security and support reports on documented
  paths.
- CODEOWNERS default review routing for public pull request triage.
- Crate-level rustdoc introductions for workspace crates and the CLI binary,
  documenting crate responsibilities and preview boundaries.
- GitHub Actions concurrency cancellation and job timeouts for bounded public
  pull request validation.
- Installation documentation for the supported source-build path, local binary
  commands, runtime prerequisites, and unsupported distribution channels.

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
