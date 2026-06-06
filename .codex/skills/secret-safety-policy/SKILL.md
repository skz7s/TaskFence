---
name: secret-safety-policy
description: Use when reviewing, recording, scanning, redacting, or diagnosing provider tokens, API keys, Bearer headers, registry credentials, URL userinfo, secret-bearing config, or sensitive values in docs, governance, generated outputs, or local environment facts.
---

# Secret Safety Policy

## Goal

Prevent credentials from becoming durable governance or documentation facts.

## Rules

- Do not store provider tokens, API keys, cookies, SSH keys, registry passwords, or private config contents in docs, governance, generated outputs, or `.codex-helper/local-env.toml`.
- Secret-bearing dependency sources must be redacted before writing local env facts.
- Docs may mention variable names, secret manager paths, and placeholder values, but not real values.
- Health and preflight diagnostics must report only path and category; never print the matched value.

## Workflow

1. Before committing governance, docs, generated output, or local env changes, run governance preflight when available.
2. If a secret risk appears, move the value to the configured provider store, secret manager, shell environment, or operator-owned config.
3. Replace committed text with a placeholder such as `PROVIDER_TOKEN`, `SECRET_MANAGER_PATH`, or `[redacted]`.
4. Re-run preflight and the normal governance checks.

## Boundaries

- Secret scanning is a guardrail, not a replacement for a dedicated secret scanning service.
- Do not auto-rewrite secret-bearing files; require manual review.
