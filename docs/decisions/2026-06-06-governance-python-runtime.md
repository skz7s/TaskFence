# Governance Python Runtime Selection

## Context

The documented governance commands use `python3 scripts/governance/...`. On the
current macOS checkout, `python3` can resolve to Python 3.9, which does not
include `tomllib`. The governance build and asset checks need TOML parsing, so
the documented command failed before it could validate generated governance.

The checkout also has newer Python executables available. Relying on an exact
host path would violate the repository policy that host-specific tool paths stay
in `.codex-helper/local-env.toml`.

## Decision

Governance scripts that require TOML parsing may re-execute themselves with the
first detected compatible Python runtime from the repo virtualenv,
`python3.13`, `python3.12`, `python3.11`, or `python3`.

The public command remains `python3 scripts/governance/...`; the compatibility
logic stays inside the governance scripts.

## Consequences

- Operators can keep using the documented governance commands on hosts where
  shell `python3` is older than 3.11.
- The scripts do not hard-code host-specific paths or commit machine-local
  runtime facts.
- Hosts without any Python runtime that provides `tomllib` still fail closed
  with an explicit runtime error.

## Validation And Rollback

Validation:

- `python3 scripts/governance/build_agents.py --check`
- `python3 scripts/governance/check_codex_governance.py`

Rollback is to remove the re-exec logic and require operators to invoke a
specific Python 3.11+ command directly, but that would also require updating
the documented governance command contract.
