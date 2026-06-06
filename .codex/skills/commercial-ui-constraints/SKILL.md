---
name: commercial-ui-constraints
description: Use when designing, reviewing, or implementing product-facing web UI so default layout, responsive behavior, dialog usage, and business-language copy stay aligned with commercial product expectations.
---

# Commercial UI Constraints

## Goal

Keep default web UI decisions practical, business-facing, and commercially readable instead of drifting toward nested cards, technical labels, or dashboard-style metric panels.

## Default Rules

- Prefer tables for list views unless the user explicitly asks for a different presentation.
- Prefer dialogs for view and edit flows unless the workflow clearly requires a full page.
- Treat mobile adaptation as a default requirement for web UI, not an optional enhancement.
- Do not add statistics panels, KPI chips, or summary counters unless the page is explicitly a dashboard or the user asks for business metrics.
- Use commercial, operational, or marketing language in the interface by default.
- Do not expose technical wording, implementation jargon, schema language, or engineering-facing labels in product UI copy unless the user explicitly requests that language.
- When the user provides preferred wording, use that wording directly unless it would create obvious ambiguity or inconsistency.
- For new product UI, major redesigns, or substantial pre-design work, prefer a GPT-generated bitmap concept as the first visual design artifact when image generation is available. Use existing design-system implementation directly for narrow fixes, copy edits, and small layout adjustments.
- Do not substitute an HTML mockup plus screenshot for the pre-design image concept when the task is mainly visual direction. HTML and screenshots are implementation or verification artifacts after the visual direction is clear.

## Layout Guidance

- Avoid card nesting as the default layout pattern for routine management pages.
- Avoid the default Codex habit of wrapping a whole page in a large summary card and then nesting smaller cards for each section or record.
- Prefer flat sections with spacing, dividers, toolbars, tables, and inline groups before introducing a prominent container.
- Use a clear page heading, primary actions, filters, and a table-based content area before introducing decorative containers.
- Keep information density stable across desktop and mobile instead of turning each row into a stack of mini-cards.
- When actions belong to a row, keep them close to the row content and avoid wrapping the same record in multiple visual shells.
- If a container is needed, keep it structural and singular. Do not stack outer hero cards, inner section cards, and per-item cards by default on the same routine page.

## Copy Guidance

- Write labels, helper text, empty states, and calls to action in business language that describes outcomes, value, and operator intent.
- Prefer phrases such as customer, project, record, progress, status, plan, publish, confirm, or complete over technical nouns that describe implementation details.
- Avoid surfacing terms such as API, JSON, schema, payload, mutation, component, hook, service, cache, job, or database in end-user UI copy unless the surface is explicitly technical.
- If the user's request already contains the desired product language, preserve it instead of translating it into engineering terminology.

## Review Checklist

- Is the main list shown as a table?
- Are view and edit actions handled in dialogs by default?
- Does the page work on mobile without overflow, clipped actions, or unreadable columns?
- Did the design avoid default metric panels and unnecessary card nesting?
- Did the design avoid the large-card-within-card composition that Codex tends to overproduce on admin pages?
- Does the copy read like a commercial product instead of an internal engineering tool?
- For substantial UI design, was the visual direction captured with image generation or an existing approved design image before implementation screenshots were used as evidence?

## Boundaries

- These rules are defaults, not absolute bans. A dashboard, analytics page, kanban workflow, or explicitly requested visual pattern can override them.
- When an existing product design system already defines a different stable pattern, preserve that pattern unless the user asks for a redesign.
