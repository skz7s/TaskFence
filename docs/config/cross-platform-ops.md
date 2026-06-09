# Cross-Platform Ops

This document is the project-owned stable operations fact record. Helper sync may seed this file
when it is missing, but confirmed project facts must be maintained here and must not be overwritten
or generalized by governance sync.

## Local Environment Facts

- Read `.codex-helper/local-env.toml` before choosing Python, uv, Node, npm, Codex, Vite, Vitest, or deployment commands.
- If the file is missing or stale, use the `project-env-baseline` skill to refresh it.
- Do not commit the file or copy its host-specific paths into `governance/profile.toml`.

## Operations Scripts

- `deploy/manage.sh` is the only supported project-local operations entrypoint.
- No legacy `setup.sh`, `deploy.sh`, or `build.sh` wrappers are currently part of the supported contract.
- Default dependency policy is reuse-first: discover tools, record paths, and install missing tools only when the operator explicitly asks.
- Current local development target is the Rust 1.88+ workspace rooted at
  `Cargo.toml`.
- Current installation path is source checkout plus local Cargo build; see
  `docs/installation.md`. Do not document package-manager or binary
  installation until artifacts exist.
- Current deployment target is not implemented. Do not add systemd, launchd, Docker image deployment, or persistent service behavior until a concrete TaskFence runtime command is ready.
- The team-state foundation is a library and local CLI surface, including local audit-export artifact generation. It does not add a supported deployed API server, worker service, managed Postgres service, background audit-export service, port, launchd unit, systemd unit, or deployment command.
- Do not generalize Ubuntu-only, Debian-only, macOS-only, WSL-specific, or other OS-specific deployment facts into generic Linux systemd support.
- Store stable deployment facts and operator runbooks in `docs/config/*`; do not store local tool paths there.
- Use `bash deploy/manage.sh doctor` for read-only diagnostics; do not let doctor rewrite global package manager configuration.

Supported `deploy/manage.sh` commands:

- `detect-env`: detect local tools and write ignored `.codex-helper/local-env.toml`
- `doctor`: print detected facts and missing dependency guidance
- `setup`: prepare detected repo-local dependencies and verify Rust tooling when supported
- `dev`: run the foreground Rust development check for specialized agent adapters and example validation
- `build`: run `cargo build --workspace` and detected build scripts, or write explicit systemd service files only when invoked with documented options
- `readiness`: print the local preview, beta-candidate, unsupported production,
  and release-gate checklist from `docs/config/readiness-checklist.md`

Rust validation gates:

```bash
cargo fmt --all
cargo check --workspace --locked
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo test --workspace --locked
RUSTDOCFLAGS="-D warnings" cargo doc --workspace --no-deps --locked
```

Local runner smoke test:

```bash
cargo run -p taskfence-cli -- run examples/task.yaml
```

This smoke test requires Docker and the task image to be present locally. The
current demo uses `debian:bookworm-slim`, and the runner passes `--pull=never`
so operators must pre-pull or pre-load images explicitly. Generated demo
artifacts are written under `examples/repo/.taskfence/tasks/local-demo/` and are
ignored runtime evidence, not source files.

Governance validation gates:

```bash
python3 scripts/governance/build_agents.py
python3 scripts/governance/build_agents.py --check
python3 scripts/governance/check_codex_governance.py
```

Governance scripts require Python 3.11+ or another Python runtime with `tomllib`. The scripts may re-execute themselves with a detected `python3.13`, `python3.12`, `python3.11`, or repo virtualenv Python when the shell `python3` is older.

Readiness checklist:

```bash
bash deploy/manage.sh readiness
```

The checklist separates local preview, beta-candidate, and not-production-supported surfaces. It
does not start services or install dependencies.

## Dependency Isolation

- Python dependencies should live in a repo `.venv`.
- Web dependencies should live in the owning web package directory, such as `web/node_modules`.
- Runtime state should live in helper-managed state directories rather than shared user-level Codex state.

## Docker CI And Image Deployment

For managed projects that use Docker deployment, keep source hosting, CI execution, and image
delivery as separate choices:

- prefer GitHub Actions as the primary CI path when a project can use GitHub
- use Gitee as a mirror, domestic collaboration entrypoint, or fallback when access requires it
- use Gitee CI/Gitee Go as primary CI only when the project documents a concrete operational or compliance reason
- publish immutable image tags from CI and deploy by pulling those tags on the host
- place release images in a registry close to the deployment host, such as an internal registry or domestic registry for mainland China servers
- use BuildKit/buildx cache for repeat builds when the CI platform supports it
- keep deployment wrappers able to skip local source rebuilds when prebuilt images are available

Debug Docker is a separate path. Debug startup should reuse existing images and named dependency
volumes when lockfiles are unchanged, with explicit rebuild or force-sync flags for toolchain or
dependency resets.

## Script Scaffold

If this project does not already have an operations entrypoint, use the default operations skill to
create one:

```bash
python3 .codex/skills/ops-script-maintenance/scripts/scaffold_manage.py --root . --check
```

Review the generated `deploy/manage.sh` before extending deployment. The scaffold detects common
Python and Node layouts and keeps setup/dev/build commands project-local. It writes only
`deploy/manage.sh` by default. Pass `--with-wrappers` only when legacy wrapper compatibility is a
confirmed project requirement. Systemd service files require explicit `--systemd --exec-command`;
the scaffold does not infer long-running service commands or OS support for unfamiliar projects.

## Governance Health

Use the Web governance page or CLI health/preflight commands before deployment and after adding
private governance modules:

```bash
codex-helper governance health <project-id>
codex-helper governance preflight <project-id>
codex-helper governance repair <project-id> --dry-run
```

The checks are read-only. They report missing default skills, stale generated outputs, unregistered
private sources, unbundled private agent fragments, local-env ignore status, scattered generated
skills, baseline version state, secret risks, lifecycle hints, repair candidates, and the
`deploy/manage.sh` status.
