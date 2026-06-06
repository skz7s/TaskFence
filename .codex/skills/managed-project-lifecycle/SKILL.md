---
name: managed-project-lifecycle
description: Use when onboarding, upgrading, migrating, deploying, archiving, deleting, or detaching a codex-helper managed repository, or when governance health reports lifecycle hints such as missing preflight, baseline drift, stale local env, or missing ops entrypoints.
---

# Managed Project Lifecycle

## Goal

Keep managed repositories governable across setup, machine moves, baseline upgrades, deployment, and archival.

## Lifecycle States

- Onboarding: sync baseline, create private governance source, refresh local env, run health.
- Active development: keep generated outputs current and run the narrowest validation for touched surfaces.
- Machine migration: refresh `.codex-helper/local-env.toml` and run preflight before setup, dev, or deploy.
- Baseline upgrade: review helper baseline version, sync source-first governance, rebuild, then preflight.
- Deployment: refresh local env, run ops doctor, run preflight, then use documented deploy commands.
- Archive or detach: preserve stable docs and decision logs; remove helper runtime state only by operator request.

## Rules

- Project-specific lifecycle rules belong under `governance/private/*`.
- Do not expand global templates for one project's special deployment or archival needs.
- Treat lifecycle hints as prompts for operator review, not automatic repair approval.
