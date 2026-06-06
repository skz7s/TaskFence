---
name: decision-log-policy
description: Use when changing durable governance, deployment, provider, dependency source, baseline, security, lifecycle, or architecture policy and the reason should remain reviewable after the implementation.
---

# Decision Log Policy

## Goal

Keep important governance and operations decisions explainable without turning runtime prompts into history documents.

## When To Record

- Default governance, baseline, generated-artifact, or skill policy changes.
- Deployment model, service manager, port, or environment strategy changes.
- Provider configuration, token injection, readiness, or failover policy changes.
- Dependency source, mirror, proxy, or package manager policy changes.
- Security and secret-handling policy changes.

## Format

Write lightweight records in the decisions docs directory using `YYYY-MM-DD-topic.md` filenames with:

- context
- decision
- consequences
- validation or rollback notes

## Boundaries

- Do not write routine code edits or local machine facts as decisions.
- Do not put secrets, private tokens, or host-specific paths in decision records.
