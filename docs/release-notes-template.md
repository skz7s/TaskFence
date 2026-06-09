# Release Notes Template

Use this template for preview, beta, and stable TaskFence releases. Keep
implemented behavior separate from future work, and record unavailable
integration coverage explicitly.

## Release

- Version:
- Release type: Preview / Beta / Stable
- Date:
- Commit or tag:

## Summary

- 

## Implemented Surfaces

- CLI:
- Task-file schema:
- Policy and approval:
- Audit, artifacts, and reports:
- Runner backends:
- Gateway connectors:
- Local review, replay, and state:
- Team state:
- Governance and documentation:

## Unsupported Surfaces

List surfaces that remain unsupported or contract-only, especially:

- production API daemon
- deployed team server
- production Web UI
- production MCP server
- arbitrary HTTP proxy
- SDK/webhook connectors
- SSO
- object storage
- Kubernetes, microVM, or managed-cloud live execution
- live replay of externally visible connector effects

## Security-Relevant Changes

- Fail-closed behavior:
- Secret handling:
- Sandbox or runner isolation:
- Gateway mediation:
- Approval behavior:
- Audit, artifact, or report integrity:
- Known limitations:

## Compatibility And Migration

- Rust MSRV impact:
- CLI impact:
- Task-file impact:
- Structured evidence impact:
- Gateway, runner, state, or report impact:
- Required migration steps:

## Validation

Record commands run and results.

```bash
bash deploy/manage.sh readiness
bash -n deploy/manage.sh
cargo fmt --all --check
cargo check --workspace --locked
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo test --workspace --locked
RUSTDOCFLAGS="-D warnings" cargo doc --workspace --no-deps --locked
cargo package --workspace --no-verify --locked
python3 scripts/governance/build_agents.py --check
python3 scripts/governance/check_codex_governance.py
```

## Integration Coverage

- Docker runner:
- Database:
- Remote SSH runner:
- Live GitHub or GitHub Enterprise:
- Other live enterprise connectors:
- Browser/UI:
- External advisory, license, or source checks:

For skipped coverage, record the reason and affected risk. Do not imply a check
passed when the required service, credential, image, database, or tool was
unavailable.

## Dependency And Supply-Chain Notes

- Dependency changes:
- MSRV changes from dependencies:
- Package publication checks:
- `cargo audit` result or skip reason:
- `cargo deny` result or skip reason:
- Signing or provenance notes:

## Known Issues

- 

## Operator Approval

- Release branch merge approved by:
- Tag or artifact publication approved by:
- Crate publication approved by:
