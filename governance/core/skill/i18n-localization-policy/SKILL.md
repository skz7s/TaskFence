---
name: i18n-localization-policy
description: Use when designing or implementing multilingual routes, localized UI copy, language switching, hreflang/canonical metadata, or admin/user surface language boundaries.
---

# I18n Localization Policy

## Goal

Keep multilingual products consistent across routing, SEO metadata, UI copy, content ownership, and layout.

## Route Model

- Choose one canonical locale strategy before implementation: language-prefixed routes, locale subdomains, or domain-per-locale.
- For SEO-facing public pages, prefer explicit language-prefixed routes such as `/<locale>/...` unless the project already documents another canonical model.
- Keep admin or operator surfaces in the product's documented operating language unless product strategy explicitly requires runtime language switching.
- Preserve the active language segment across public or creator/user navigation, but do not leak it into fixed-language admin routes.
- Keep legacy unprefixed routes as redirects or compatibility normalization only, not as canonical SEO URLs.

## Metadata

- Localized public pages must set language-specific `html lang`, title, description, canonical URL, Open Graph metadata, and `hreflang` alternates.
- Canonical and alternate links must match the final deployed route structure, including trailing slash policy.
- Server, static host, and SPA fallback rules must serve language-prefixed routes to the app while leaving API, health, asset, and admin backend endpoints untouched.

## Copy And Content

- Transcreate marketing slogans, hero titles, navigation labels, and page titles for tone, rhythm, and product positioning instead of literal translation.
- Add durable UI strings to the project's i18n resource layer. Short transitional wrappers are acceptable only during migration from hardcoded copy.
- Treat backend, user, imported, or LLM-generated content as content data. Do not translate or rewrite it in the frontend unless the project has dedicated localized fields or a translation workflow.
- Keep product terms, plan names, legal copy, payment wording, and support language consistent with the source-of-truth docs.

## Layout

- Design localized layouts as equivalent experiences, not identical text blocks.
- Account for English length, CJK line height, number/date formats, and button label expansion with language-aware CSS tokens, responsive wrapping, stable button dimensions, and no negative letter spacing.
- Verify the longest supported labels in mobile and desktop viewports before treating the surface as complete.

## Validation

- Check representative routes for every supported locale.
- Verify canonical, `hreflang`, Open Graph, and `html lang` values.
- Run rendered checks for at least one dense page, one public marketing page, and one form or checkout-like flow when those surfaces exist.
