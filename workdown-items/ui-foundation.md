---
id: ui-foundation
type: issue
status: to_do
title: UI foundation — conventions and scaffolding before the first view
parent: server
depends_on: [walking-skeleton]
---

Before building any real view, settle the conventions that will shape every future view and ship the scaffolding so slice 2 (`first-view-end-to-end`) can focus purely on feature code. Each individual decision is small, but each is hard to retrofit once views start piling on.

Same playbook as `walking-skeleton`: discuss each decision in turn, record the rationale (in code where natural, in this body where it isn't), install the minimum scaffold for each. No feature code lands in this issue — that's slice 2.

## Decisions to make (in roughly the order they unlock each other)

- **CSS framework / styling approach.** Tailwind / UnoCSS / Pico / DaisyUI / vanilla CSS + Svelte scoped styles. Affects every component, retrofit is painful.
- **Type sharing between Rust and TypeScript.** Hand-written TS types (drift risk) / `ts-rs` codegen (Rust attribute → TS file) / `specta` (similar) / OpenAPI + codegen. Sets the discipline for the "Rust adds a field, UI sees it" loop.
- **API conventions.** JSON envelope shape (naked vs `{ data, ... }`), error format (RFC 7807 problem details vs custom), HTTP status code conventions, URL shape (`/api/views/:id` vs versioned vs other).
- **Backend handler organization.** `crates/server/src/` is currently one `lib.rs`. Options: one file per resource (`api/views.rs`, `api/items.rs`), feature folders (`features/board/`), or flat-by-type. Ties to the vertical-slice-vs-layered question on the backend side.
- **Frontend folder structure under `ui/src/lib/`.** Feature folders (`lib/board/{api.ts, BoardView.svelte}`) vs by-type (`lib/api/`, `lib/components/`, `lib/stores/`). Vertical-slice-vs-layered on the UI side.
- **API client pattern (frontend).** Raw fetch / hand-written typed wrapper / Tanstack Query for Svelte / SvelteKit `$app/fetch`. Includes how loading / error / empty states are represented.
- **Frontend state management.** Svelte 5 runes alone / runes-in-context / Svelte stores / external library. For shared state — cached API responses, current view, theme, etc.
- **URL / route structure.** `/views/:id` vs `/board/:id` vs `/v/:id`. What lives at `/`? How are item deep links shaped (`/items/:id`)?
- **Linter and formatter.** We turned ESLint and Prettier off during the SvelteKit scaffold to defer the decision. Slice 2 is the right moment to land them — before lots of code accumulates and the auto-format diff explodes.

## Scope

- Each decision discussed and recorded — in code where natural (Tailwind config files, ts-rs build step, module-level comments, ESLint config), in this body where the rationale outlasts the code.
- Minimum viable scaffold installed for each: CSS framework + base styles, type-generation tooling (with one example exported type), an example empty API handler module, a typed fetch helper, the folder structure created with placeholders, lint/format running locally and in CI.
- No feature code — every line that responds to a real work item lives in `first-view-end-to-end`.

## Acceptance

- A reader of the repo can identify the CSS framework, type-sharing strategy, API envelope shape, and folder conventions by inspecting the code, with no fresh prose docs required.
- `cargo xtask build` and `npm run check` still pass.
- CI is updated to run any new check (lint, format, type generation) so regressions are caught on every PR.

## Out of scope

- The board view, or any other view (next slice: `first-view-end-to-end`).
- UI test framework — vitest / Playwright / Svelte component tests. Defer; revisit when the first non-trivial component exists, because the testing pattern is much easier to choose with a real thing to test.
- Formal accessibility baseline (WCAG target, axe-core in dev, etc.). Start informal in slice 2 with semantic HTML and obvious ARIA; pick a formal target once there's more surface area.
- Auth, multi-user concerns, remote serve — explicit non-goals of the whole milestone.
