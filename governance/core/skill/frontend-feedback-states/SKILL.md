---
name: frontend-feedback-states
description: Use when implementing or reviewing loading states, skeletons, empty states, toasts, form feedback, optimistic updates, or global frontend interaction feedback.
---

# Frontend Feedback States

## Goal

Keep user feedback stable, visible, accessible, and consistent across product surfaces.

## Loading States

- Use skeleton placeholders for component loading states when layout shape is known.
- Match each skeleton to the final component shape: image areas, table rows, form fields, metadata, and action rows should reserve stable space.
- Preserve final layout dimensions during loading so content does not jump when data arrives.
- Use spinners only for short inline actions or indeterminate operations that do not have a predictable final layout.
- Add `aria-busy` to loading containers when practical.

## Feedback Messages

- Route routine success, warning, and recoverable error messages through the shared global toast or notification surface.
- Toasts should auto-dismiss after a documented interval and provide a visible close action.
- Avoid adding local banners or page-specific message boxes for ordinary form saves, auth outcomes, or recoverable request failures.
- Keep persistent inline messages only when they are part of workflow state, such as validation helper text, a blocking business warning, or a development verification code.

## Empty And Failure States

- Empty states should describe the business state and next action, not the implementation reason.
- Use fallback sample data only after an explicit load failure and pair it with a warning.
- Error states should preserve the surrounding layout and keep retry or recovery actions near the failed surface.

## Implementation

- Prefer shared UI helpers over page-specific skeleton and toast implementations.
- Keep interaction APIs small: one provider/hook for notifications and reusable skeleton primitives or wrappers.
- Verify at least one success path, one warning/error path, and one skeleton loading surface with rendered browser checks when these patterns change.
