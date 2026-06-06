---
name: seo-visibility-policy
description: Use when creating, reviewing, or changing SEO-facing pages, metadata, crawl/index controls, structured data, sitemap, canonical URLs, social previews, or localized search surfaces.
---

# SEO Visibility Policy

## Goal

Make public pages crawlable, canonical, index-safe, and commercially clear without leaking private or operational surfaces.

## Page Classification

- Classify each changed route as public indexable, public noindex, authenticated, admin/operator, API, asset, or health/status.
- Only public product, marketing, documentation, article, catalog, and share pages should be indexable by default.
- Admin, account, payment status internals, preview-only pages, staging routes, API, callback, and health endpoints should not become SEO targets.

## Metadata Contract

- Each indexable page needs a stable title, meta description, canonical URL, Open Graph title/description/image, and meaningful visible H1.
- Localized pages must pair with `i18n-localization-policy` and include correct `hreflang` alternates.
- Canonical URLs must reflect production host, locale strategy, trailing slash policy, and query parameter policy.
- Social preview images should show the product, place, object, or content state directly, not a generic dark or blurred background.

## Crawl And Index Controls

- Keep robots.txt, sitemap, canonical tags, and noindex rules consistent.
- Do not include authenticated, admin, staging, callback, or private user URLs in sitemaps.
- Preserve important compatibility redirects, but point canonical URLs to the final route.
- When route structure changes, document redirect behavior and verify that legacy routes do not create duplicate indexable pages.

## Content Quality

- Titles and descriptions should describe the user-facing offer, page content, or business value, not implementation details.
- Avoid duplicate titles/descriptions across many pages unless the pages are intentionally equivalent.
- Structured data is useful when the page has a genuine supported entity such as article, product, organization, breadcrumb, FAQ, or app. Do not add fake schema markup.

## Validation

- Inspect rendered HTML for title, description, canonical, Open Graph, robots/noindex, H1, and structured data.
- Check sitemap and robots paths when those files change.
- For frontend apps, verify metadata after the route renders in the mode search crawlers or SSR/static generation will see.
