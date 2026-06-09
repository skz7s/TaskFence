# TaskFence Documentation

This index is the stable entry point for TaskFence documentation. TaskFence is
currently a local preview project: documents should describe implemented
surfaces and explicit limitations, not future production support.

## First-Time Contributors

- [Quickstart](quickstart.md): no-Docker validation and fixture gateway path.
- [Contributing](../CONTRIBUTING.md): contribution workflow, validation, and
  security expectations.
- [Testing Strategy](testing.md): CI gate, focused crate checks, examples,
  Docker prerequisites, and skipped coverage reporting.
- [Support Policy](../SUPPORT.md): current support boundary.

## Public Preview Interfaces

- [CLI Reference](cli-reference.md): implemented `taskfence` commands and
  evidence lookup workflows.
- [Task File Reference](task-file-reference.md): preview task YAML schema,
  defaults, validation behavior, and fail-closed limits.
- [Examples](../examples/README.md): runnable and contract-only example matrix.
- [Versioning And Compatibility](versioning.md): preview compatibility, MSRV,
  semver, task-file, CLI, and structured evidence policy.

## Runtime And Product Direction

- [Positioning](positioning.md): product wedge and non-goals.
- [Requirements](requirements.md): product and security requirements.
- [Architecture](architecture.md): high-level runtime architecture.
- [Development Design](development-design.md): staged implementation plan and
  crate boundary rationale.
- [Roadmap](roadmap.md): preview-to-production direction without overclaiming
  unsupported surfaces.

## Security, Release, And Maintenance

- [Security Model](security-model.md): protected assets, threat boundaries,
  fail-closed expectations, and current limitations.
- [Security Policy](../SECURITY.md): vulnerability reporting and supported
  preview scope.
- [Supply-Chain Maintenance](supply-chain.md): dependency update, package
  publication, and advisory-tool expectations.
- [Release Process](release.md): preview/beta/stable release gates and release
  note requirements.
- [Maintainer Guide](maintainers.md): review priorities, compatibility
  stewardship, and incident handling.
- [Readiness Checklist](config/readiness-checklist.md): local preview, beta
  candidate, and unsupported production surfaces.

## Operations And Governance

- [Cross-Platform Ops](config/cross-platform-ops.md): supported local operator
  surfaces and deployment assumptions.
- [Environment Inventory](config/env-inventory.md): stable environment
  expectations.
- [Project Structure](codex/project-structure.md): top-level directories,
  repository metadata, and documentation ownership.
- [Structure Contract](codex/structure-contract.md): module ownership and
  boundary rules.
- [Runtime Architecture Facts](codex/runtime-architecture.md): implemented and
  confirmed runtime facts.
- [Governance Change Map](../governance/change-map.md): documents to update
  when behavior, commands, governance, or readiness claims change.
- [Decision Records](decisions/README.md): durable architecture, security,
  compatibility, governance, and release decisions.
