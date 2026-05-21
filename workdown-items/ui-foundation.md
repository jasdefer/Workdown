---
id: ui-foundation
type: issue
status: in_progress
title: UI foundation — conventions and scaffolding before the first view
parent: server
depends_on: [walking-skeleton]
---

Before building any real view, settle the conventions that will shape every future view and ship the scaffolding so slice 2 (`first-view-end-to-end`) can focus purely on feature code. Each individual decision is small, but each is hard to retrofit once views start piling on.

Same playbook as `walking-skeleton`: discuss each decision in turn, record the rationale (in code where natural, in this body where it isn't), install the minimum scaffold for each. No feature code lands in this issue — that's slice 2.

## Decisions

### CSS framework / styling approach — **vanilla CSS + CSS variables + Svelte scoped styles**

Rejected Tailwind primarily because (a) theme switching via CSS variables is more elegant and DRY than `dark:` prefixes at every call site — one block of variable overrides flips every component, no per-property duplication; (b) most CSS is going to be AI-written, which weakens Tailwind's main human-facing wins (decision fatigue, naming ceremony, design-token guardrails); (c) HTML stays semantic and grep-friendly, which matters more for a solo project with many view types than utility-class density would. Also rejected: UnoCSS (Tailwind-shaped, smaller ecosystem — no reason to pick it over either extreme); Pico / classless frameworks (wrong shape — won't help with kanban columns, gantt bars, graph nodes); DaisyUI / component libs (layer on top of a system, not instead of one — defer until a real need surfaces).

Retrofit cost if we're wrong is bounded: `svelte-add tailwindcss` and our custom classes coexist during a migration.

#### Core CSS plan

Files:

```
ui/src/
  app.css                    -- imported once in +layout.svelte
  lib/styles/
    reset.css                -- modern element reset (Andy Bell-flavored)
    tokens.css               -- design tokens (CSS variables) + theme overrides
    base.css                 -- global element defaults
```

Splitting lets `tokens.css` evolve independently (it'll be the file touched most often as the design grows) and keeps each file under ~60 lines.

`tokens.css` — start small, grow with need:

```css
:root {
  color-scheme: light dark;

  /* Colors — light theme defaults */
  --color-bg:       #ffffff;
  --color-surface:  #f7f7f8;
  --color-fg:       #18181b;
  --color-fg-muted: #71717a;
  --color-border:   #e4e4e7;
  --color-accent:   #3b82f6;

  /* Spacing — 4px scale */
  --space-1: 0.25rem;   --space-2: 0.5rem;
  --space-3: 0.75rem;   --space-4: 1rem;
  --space-6: 1.5rem;    --space-8: 2rem;

  /* Radii */
  --radius-md: 0.375rem;
  --radius-full: 9999px;

  /* Shadows */
  --shadow-sm: 0 1px 2px rgb(0 0 0 / 0.06);

  /* Typography */
  --font-sans: ui-sans-serif, system-ui, -apple-system, "Segoe UI", sans-serif;
  --font-mono: ui-monospace, "Cascadia Mono", Consolas, monospace;
  --text-sm: 0.875rem;  --text-base: 1rem;  --text-lg: 1.125rem;
}

[data-theme="dark"] {
  --color-bg:       #09090b;
  --color-surface:  #18181b;
  --color-fg:       #e4e4e7;
  --color-fg-muted: #a1a1aa;
  --color-border:   #27272a;
  --color-accent:   #60a5fa;
}
```

`base.css` — body using tokens, `:focus-visible` ring, `.sr-only` utility, `prefers-reduced-motion` block disabling animations.

`reset.css` — modern reset: box-sizing border-box, body margin 0, image/form sensible defaults. ~25 lines copy-paste from a known-good source.

Theme switching: set `data-theme="dark"` on `<html>`. Variables cascade, every `var(--color-bg)` descendant updates. No FOUC concern since `ssr = false` (SPA-only). Persistence and toggle UI land with the theme/dark-mode decision below.

No global utility classes beyond `.sr-only`. No "grid system" — modern CSS Grid + Flexbox handle layout per-component in scoped styles. The app shell layout lives in `+layout.svelte`, not core CSS.

#### Deferred — add when first needed

- Semantic colors (success / warning / danger / info) — add with the first badge / toast / validation message.
- Z-index scale — add when the first modal or popover lands.
- Full type scale (xs / xl / 2xl / 3xl) — extend as headings / displays appear.
- More shadow tiers (md / lg / popover) — add when surfaces stack.
- Animation tokens (`--ease-out`, `--duration-fast`) — when the first non-trivial transition lands.

## Remaining decisions (in roughly the order they unlock each other)

- **Type sharing between Rust and TypeScript.** Hand-written TS types (drift risk) / `ts-rs` codegen (Rust attribute → TS file) / `specta` (similar) / OpenAPI + codegen. Sets the discipline for the "Rust adds a field, UI sees it" loop.
- **API conventions.** JSON envelope shape (naked vs `{ data, ... }`), error format (RFC 7807 problem details vs custom), HTTP status code conventions, URL shape (`/api/views/:id` vs versioned vs other).
- **Backend handler organization.** `crates/server/src/` is currently one `lib.rs`. Options: one file per resource (`api/views.rs`, `api/items.rs`), feature folders (`features/board/`), or flat-by-type. Ties to the vertical-slice-vs-layered question on the backend side.
- **Frontend folder structure under `ui/src/lib/`.** Feature folders (`lib/board/{api.ts, BoardView.svelte}`) vs by-type (`lib/api/`, `lib/components/`, `lib/stores/`). Vertical-slice-vs-layered on the UI side.
- **API client pattern (frontend).** Raw fetch / hand-written typed wrapper / Tanstack Query for Svelte / SvelteKit `$app/fetch`. Includes how loading / error / empty states are represented.
- **Frontend state management.** Svelte 5 runes alone / runes-in-context / Svelte stores / external library. For shared state — cached API responses, current view, theme, etc.
- **URL / route structure.** `/views/:id` vs `/board/:id` vs `/v/:id`. What lives at `/`? How are item deep links shaped (`/items/:id`)?
- **Linter and formatter.** We turned ESLint and Prettier off during the SvelteKit scaffold to defer the decision. Slice 2 is the right moment to land them — before lots of code accumulates and the auto-format diff explodes.
- **TypeScript strictness beyond `strict: true`.** Current `ui/tsconfig.json` is SvelteKit defaults. Candidates to turn on: `noUncheckedIndexedAccess`, `noUnusedLocals`, `noUnusedParameters`, `exactOptionalPropertyTypes`, `verbatimModuleSyntax`, `allowJs: false`. Cheap upfront, painful to retrofit once code accumulates.
- **Theme support and dark mode.** Decide whether to bake in a theme store (Svelte 5 rune, persisted to `localStorage`) and dark-mode discipline from day one, or defer. Cheap upfront because every new component is written with the dark variant in mind; painful to retrofit across many components later. Ties to the CSS-framework decision (Tailwind's `darkMode: 'class'` strategy, vs CSS variables, vs nothing).

## Scope

- Each decision discussed and recorded — in code where natural (Tailwind config files, ts-rs build step, module-level comments, ESLint config), in this body where the rationale outlasts the code.
- Minimum viable scaffold installed for each: CSS framework + base styles, type-generation tooling (with one example exported type), an example empty API handler module, a typed fetch helper, the folder structure created with placeholders, lint/format running locally and in CI.
- Minimal app shell: header (app name + theme toggle if theme support is in) and a main slot that `first-view-end-to-end` can render into. No nav, no sidebar — just enough chrome that slice 2 doesn't have to invent layout while it's building the board.
- Devcontainer: persistent volume mount for `ui/node_modules` (mirrors the existing pattern for the Rust `target/` volume) so rebuilds don't re-install on every container start.
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
