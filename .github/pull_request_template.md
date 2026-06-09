## Summary

- 

## Validation

- [ ] `cargo fmt --all --check`
- [ ] `cargo check --workspace --locked`
- [ ] `cargo clippy --workspace --all-targets --locked -- -D warnings`
- [ ] `cargo test --workspace --locked`
- [ ] `RUSTDOCFLAGS="-D warnings" cargo doc --workspace --no-deps --locked`
- [ ] `python3 scripts/docs/check_markdown_links.py` when public docs or GitHub templates changed
- [ ] Governance checks, if governance files changed
- [ ] Integration limitations recorded, if Docker/database/remote/live connector coverage was unavailable
- [ ] Testing scope follows `docs/testing.md`

## Security Boundary

- [ ] This change preserves fail-closed behavior for unsupported or unknown actions.
- [ ] This change does not expose host secrets, home directories, Docker sockets, SSH agents, package tokens, or cloud credentials to sandboxes by default.
- [ ] Documentation does not claim unsupported production behavior.

## Notes
