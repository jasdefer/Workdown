---
id: ui-foundation
type: issue
status: done
title: UI foundation — conventions and scaffolding before the first view
parent: server
depends_on: [walking-skeleton]
effort: "16h"
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

### Type sharing between Rust and TypeScript — **`ts-rs`, generated files gitignored**

The view-data surface is ~30 public types across `crates/core/src/view_data/` plus the wire-relevant types from `crates/core/src/model/` (`WorkItem`, `Schema`, `Diagnostic`, `FieldValue`). Many are tagged enums where a hand-written TypeScript copy drifts silently — a typo in a discriminant string doesn't fail to compile, it just produces a wrong type at runtime. `ts-rs` puts the source of truth in the Rust struct and emits the TypeScript mechanically.

Rejected: hand-written TypeScript types (drift compounds across 30+ types and many tagged enums); `specta` (same shape as `ts-rs`, smaller ecosystem, no reason to prefer it here); OpenAPI + codegen (annotation burden on every handler buys HTTP-semantics features we don't need for a single-consumer internal API); runtime validation via Zod (doesn't solve drift, duplicates serde's work).

#### Generated-file handling

The emitted `.ts` files are build artifacts, conceptually equivalent to `target/` or `ui/dist/`. They are:

- Written to `ui/src/lib/api/generated/`.
- Gitignored. There is exactly one source of truth (the Rust struct) — nothing in the repo to drift from it.
- Produced by `cargo xtask gen-types`, wired into `cargo xtask build` so the standard build path always regenerates them before `npm run check` runs.
- Materialized on devcontainer start (`postCreateCommand` or similar) so the directory is populated immediately, before anyone runs `npm run check` standalone.

CI: `cargo xtask build` runs first and produces the types as a side effect; the existing `npm run check` step then finds them in place. No drift check needed — there is no committed copy that could drift.

#### Deferred — settle at scaffolding time

- Which types derive `TS` — only wire-level types (view data, error envelope), not the internal model. Reviewed when the first endpoint lands.
- Output shape — one bundled `.ts` file vs one per Rust type. Default to `ts-rs`'s out-of-the-box behavior unless friction surfaces.

### API conventions — **light envelope `{data?, diagnostics}`, reuse the CLI `Diagnostic` type, pragmatic status codes, generic `/api/views/:id`**

The CLI already has a structured vocabulary for validation findings: the `Diagnostic` type with a `severity` field and a five-variant tagged `body` (`File`, `Item`, `Files`, `Collection`, `Config`), each variant carrying source context plus a typed `kind` enum with ~25 variants between them (`BrokenLink`, `RuleViolation`, `AggregateChainConflict`, `ViewUnknownField`, `ViewGanttEndAndDurationConflict`, ...). The API does not invent an error vocabulary — it reuses this one.

That immediately rules out the "naked JSON" envelope (`{columns: [...], unplaced: [...]}` directly): diagnostics need a stable home that's the same across every endpoint. A light envelope earns its keep precisely because every response carries diagnostics — successful renders may still have warnings, rejected writes carry errors, both use the same shape.

Rejected: naked JSON (no home for diagnostics); RFC 7807 problem details (flat fields are weaker than the existing typed `Diagnostic` variants; we'd be downgrading our own model to fit the standard); strict REST status codes (forces every validation failure into a `4xx` body shape that fights the diagnostics-as-data model); versioned URL prefix (`/api/v1/`) (premature for an unreleased internal API); view-kind-specific URLs (`/api/board/:id`) (fights the schema-driven view system where any `views.yaml` entry is one URL).

#### Envelope shape

```json
{
  "data": <endpoint-specific shape, omitted when there's nothing to return>,
  "diagnostics": [<Diagnostic>...]
}
```

- `diagnostics` is **always present**, often `[]`. Saves the UI from optional-chaining; `response.diagnostics.length` always works.
- `data` is **omitted** (not `null`) when the response has no payload — outright rejection of a write, or a delete that succeeded. Absent vs null disambiguates "rejected" from "explicit null result."
- `Diagnostic` is exactly the existing Rust type, exported via `ts-rs`. Same shape as `workdown check --json` produces (or will produce).
- The envelope shape is shared between the HTTP API and the CLI's `--format json` output. A future "command result" type lives at `crates/core` and both surfaces serialize it. `GET /api/views/items-by-status` and `workdown render items-by-status --format json` return essentially the same bytes.

#### Status code mapping

| Status | Used for |
| --- | --- |
| `200 OK` | Read succeeded; or write succeeded. `diagnostics` may still carry warnings about the data. |
| `201 Created` | POST that created a new work item / view. |
| `400 Bad Request` | Request body malformed (invalid JSON, missing required request-shape fields — not work-item-field validation). |
| `404 Not Found` | Endpoint or `:id` resource doesn't exist. |
| `422 Unprocessable Entity` | Request well-formed but rejected for validation reasons. Body always carries `diagnostics` explaining why. |
| `500 Internal Server Error` | Server panic. Body may have a generic diagnostic or just `{diagnostics: []}`. |

Rule of thumb: HTTP status answers "did the thing happen?" (fast gate for `response.ok`). `diagnostics` answers "what should the user know?" (rich, typed, the same vocabulary the CLI uses).

`204 No Content` is **not** used — we always send `200` with `{diagnostics: []}` for uniformity (costs ~20 bytes; saves the UI a special case).

#### URL shape

- `/api/views/:id` — generic, the view kind comes back inside `data.kind` so the UI dispatches on response payload, not URL.
- `/api/items/:id` — single work item, frontmatter + body.
- `/api/items` (POST), `/api/items/:id` (PATCH/DELETE) — standard REST shape for mutations.
- `/api/items/:id/rename`, `/api/items/:id/move` — verb-shaped sub-resources for cascading operations that aren't plain field edits.
- `/api/schema`, `/api/resources`, `/api/views`, `/api/diagnostics` — collection-level read endpoints.
- No `v1` prefix. Internal API, no compatibility burden today.

#### Deferred — settled later

- **Concurrent modification / stale state.** The only genuinely web-app-specific concern: the browser holds editing state while files may be modified externally (other tab, text editor, git pull). Three options on the table: last-writer-wins (no detection), ETag / `If-Match` version tokens, or live-update push (already scoped as the `live-updates` issue). Plan: start with last-writer-wins, treat staleness as the problem `live-updates` structurally fixes; revisit ETags only if the window between read and write turns out to bite. When detection is added, it becomes a new `Diagnostic` variant + `412 Precondition Failed`; the envelope shape doesn't change.
- **Filesystem write failures.** Need a `WriteError` variant under `File` scope (currently only `ReadError` exists). Add when the first write endpoint lands.
- **Endpoint / resource-not-found diagnostic.** Today we return `404` with no body. If the UI needs richer "view 'X' is not configured" detail, add it as a `Diagnostic` variant later.

### Backend handler organization — **flat by resource**

One Rust file per REST resource. ~7 files under `api/`, each holding all the verbs (and types where they're handler-local) for one resource. Split further only if a file grows past ~300 lines.

Rejected: feature folders / vertical slices (the API has a single `/api/views/:id` endpoint that serves every view kind — feature folders would either contain almost nothing while a shared dispatcher does the work, or fight the schema-driven view system by inventing per-kind URLs; feature folders work better on the *frontend* where each view kind is a self-contained component); one file per endpoint (too granular at this scale, loses the cohesion of "all the verbs for one resource live together").

Concrete layout:

```
crates/server/src/
  lib.rs              -- entry point, builds the Axum router, holds AppState
  state.rs            -- AppState (config, store, watcher handle, ...)
  envelope.rs         -- Response<T> wrapper and status code conventions
  error.rs            -- IntoResponse for diagnostics, panic handler
  api.rs              -- pure wiring: declares children, builds /api router
  api/
    views.rs          -- all /api/views/* endpoints
    items.rs          -- all /api/items/* endpoints
    schema.rs         -- GET /api/schema
    resources.rs      -- GET /api/resources
    diagnostics.rs    -- GET /api/diagnostics
    events.rs         -- GET /api/events (SSE, lands with live-updates)
```

`api.rs` (sibling to the `api/` folder, preferred over `api/mod.rs` for visibility in the file tree) is wiring only — `mod` declarations plus a `router()` function that nests each child's router under its URL segment.

The server crate stays thin: handlers route requests, call into `crates/core/`, wrap the result in `Response<T>`. Any handler growing past ~30 lines of real logic is a signal the logic belongs in `crates/core/`, not the server.

### Frontend folder structure under `ui/src/lib/` — **hybrid: by-type for primitives, by-feature for view kinds**

By-type at the top level (`api/`, `ui/`, `stores/`), by-feature within `views/` and `features/`. Each kind of code goes where it naturally belongs: truly generic primitives in `ui/`, view-specific components in `views/<kind>/`, non-view features in `features/<name>/`, the typed fetch layer in `api/`, cross-cutting state in `stores/`.

Rejected: pure by-type (option A — scatters each view's code across `api/`, `components/`, `views/` for no benefit, since each view kind genuinely *is* a coherent unit); pure by-feature (option B — creates the "where does shared code live" friction for primitives that don't belong to any one feature).

The asymmetry with the backend (flat by resource, no feature folders) is intentional. The backend dispatches all view kinds through one endpoint and delegates heavy lifting to `crates/core/src/view_data/`; the frontend has one *visually distinct* component per view kind. Different shape, different organization. The HTTP envelope is the seam that absorbs the difference.

Starting scaffold:

```
ui/src/lib/
  api/
    client.ts                  -- typed fetch wrapper
    generated/                 -- ts-rs output, gitignored
  ui/                          -- generic primitives (Badge, Button, DiagnosticBanner, ...)
  views/                       -- one folder per view kind, created on demand
    board/                     -- lands with first-view-end-to-end
      BoardView.svelte
      Column.svelte
      Card.svelte
  features/                    -- non-view features (diagnostics panel, item editor, ...)
  stores/
    theme.svelte.ts            -- lands with the dark-mode decision
```

Folders are created on demand, not upfront — empty folders are noise. `views/board/` lands when slice 2 builds the board; other view kinds land when each one ships.


### API client pattern (frontend) — **hand-written typed wrapper + SvelteKit `load()` + `{#await}`**

One central `ui/src/lib/api/client.ts` exports a typed `api` object (`api.getView(id)`, `api.patchItem(id, patch)`, ...) that encapsulates the envelope shape and HTTP status handling once. Route-level data fetching goes through SvelteKit `load()` functions calling into `api`; in-component fetches use Svelte's `{#await}` blocks with the same `api` calls. No third-party data-fetching library.

Rejected: raw `fetch` at every call site (boilerplate duplication, envelope handling drifts across components); TanStack Query for Svelte (most of its value — cache, stale-while-revalidate, refetch-on-focus — is already covered by `live-updates`'s SSE push and SvelteKit's `load()` re-running on navigation; ~20 KB and a sizable API surface for the ~10% of features we'd actually use).

#### Shape

```ts
// ui/src/lib/api/client.ts
import type { ViewData, WorkItem, Diagnostic } from "./generated";

export type ApiResult<T> = {
  data?: T;
  diagnostics: Diagnostic[];
  status: number;
};

async function request<T>(method: string, path: string, body?: unknown): Promise<ApiResult<T>> {
  const response = await fetch(path, {
    method,
    headers: body ? { "content-type": "application/json" } : undefined,
    body: body ? JSON.stringify(body) : undefined,
  });
  const envelope = await response.json();
  return { data: envelope.data, diagnostics: envelope.diagnostics ?? [], status: response.status };
}

export const api = {
  getView: (id: string) => request<ViewData>("GET", `/api/views/${id}`),
  // one line per endpoint
};
```

Route-level fetch (the page-render path):

```ts
// ui/src/routes/views/[id]/+page.ts
import { api } from "$lib/api/client";
export async function load({ params }) {
  return { result: await api.getView(params.id) };
}
```

Ad-hoc / in-component fetch:

```svelte
<script>
  let promise = $state(api.getView("board"));
</script>
{#await promise}<Loading />{:then result}...{:catch error}<Error />{/await}
```

#### Deferred — settle when first encountered

- **Mutation invalidation pattern.** SvelteKit's `invalidate()` / `invalidateAll()` re-runs `load()` after a PATCH. Likely sufficient; revisit if it gets noisy.
- **`fetch()` rejection → `ApiResult` shape.** Network failures currently propagate to `{:catch}`. May want to convert to a synthetic diagnostic so the UI has one unified handling path. Decide with the first real error case.
- **Optimistic updates.** Drag-a-card style — UI updates immediately, server confirms or rolls back. Deferred until the first interactive mutation; await + invalidate is fine for forms.


### Frontend state management — **runes everywhere, shared state in `.svelte.ts` modules**

Component-local state uses `$state` directly inside `.svelte` files. Genuinely shared state (theme today, schema cache when needed) lives in `.svelte.ts` modules under `ui/src/lib/stores/`, each exporting a small object with a getter and setter. No `writable`/`readable` stores. No external state-management library.

Rejected: Svelte stores (`writable`/`readable`) — works, but mixing stores with runes creates two reactivity systems in the same codebase; runes are the post-Svelte-5 default; pick one and stick to it. External library (Pinia/MobX/Zustand-port) — our shared state is small and not deeply derived; a library is 80% unused weight.

Rule of thumb: state stays component-local unless something forces it to be shared. Don't preemptively centralize. No single `appState.svelte.ts` "god store" — co-locate state by scope (`theme.svelte.ts`, future `schema.svelte.ts`), not by being-shared-ness.

Server data is mostly **not** held in state at all: SvelteKit's `load()` caches per-route, `live-updates`'s SSE will invalidate via `invalidate()`. The only persistent client cache likely worth holding is the schema (small, mostly static, hit on every form). That's deferred until the first form lands.

Starting set:

```
ui/src/lib/stores/
  theme.svelte.ts        -- light/dark, persisted to localStorage (lands with the theme decision)
```

#### Deferred — settle when first encountered

- **Schema caching.** Add `schema.svelte.ts` when the first form/edit component needs it; invalidate on the SSE `schema_changed` event once `live-updates` is in.
- **Per-route UI state persistence.** Sidebar collapse, filter values, etc. — `localStorage`, URL query params (better for bookmarking/sharing), or both. Decide per feature when it lands.
- **SSE → invalidation glue.** The wiring between SSE events and `invalidate()` / store updates lives somewhere — likely the root `+layout.svelte` or a dedicated `lib/live-updates.svelte.ts`. Settled with the `live-updates` issue.


### URL / route structure — **`/` redirects to default view, views at `/views/:id`, items at `/items/:id` plus `?item=` query overlay**

These are the page URLs (the SvelteKit router), not the API URLs (those are `/api/*`, already settled).

| Question | Pick |
| --- | --- |
| Landing `/` | Redirect to the default view (from `config.yaml`). User opens the tool to see their work, not a meta-page. |
| View page URL | `/views/:id`. Mirrors the API URL, future-proof against new top-level concepts. |
| Item standalone | `/items/:id`. Dedicated full-page route. |
| Item in-context | `/views/:viewId?item=:itemId`. View stays rendered behind, item opens on top. Closing drops the query param; back/forward and bookmarking both work. |
| Prefix | None. The app *is* the site. |

Rejected: `/v/:id` (saves characters, costs clarity); `/:id` at the root (collides with any future top-level concept); `?focus=` instead of `?item=` (item is the noun, focus is the behaviour); `/app/` prefix (no marketing/docs surface to reserve `/` for); modal-only in-context display with no URL reflection (loses deep linkability).

#### Resulting routes tree

```
ui/src/routes/
  +layout.svelte           -- app shell: header, theme toggle, view list
  +page.ts                 -- redirect to the default view
  views/
    [id]/
      +page.svelte         -- renders the view; mounts ItemPanel when ?item= is set
      +page.ts             -- load() calls api.getView(params.id)
  items/
    [id]/
      +page.svelte         -- standalone item detail/edit
      +page.ts             -- load() calls api.getItem(params.id)
```

#### In-context display form — deferred, slide-over panel is the leading candidate

The URL convention is settled now; the *visual form* of the in-context item display (when `?item=` is set) is a UI decision that's safer to settle when we actually build it.

Candidates ranked for Workdown's item shape (structured frontmatter + freeform Markdown body):

| Form | Workdown fit |
| --- | --- |
| Slide-over panel from the right (Linear/Jira-style) | **Leading.** View context stays visible, body has vertical room, structured fields fit narrow width. |
| Modal overlay | Cramped — Markdown body wants more room than a centered modal. |
| Persistent split-pane | Wastes screen when nothing selected; awkward for board's wide columns. |
| Inline expand | Doesn't fit a full edit form. |
| Header / breadcrumb badge | Useful as *additional* persistent chrome ("currently viewing X"), not as the editing surface. |
| Full-page swap | Loses context. Already covered by `/items/:id`. |

For now, `views/[id]/+page.svelte` conditionally renders a stub `ItemPanel.svelte` when `?item=` is set; the stub shows item ID and an "open standalone" link. The real form lands with `first-view-end-to-end` or whenever the first interactive editing surface is built.

#### Deferred — settle when first encountered

- **Default view source.** Likely a `default_view:` field in `config.yaml`. Quick check at scaffolding time.
- **`/schema`, `/resources`, `/diagnostics` routes.** Stub or skip — not needed for first-view. Each lands with its real panel.
- **404 / error page.** `ui/src/routes/+error.svelte` — bare Svelte default for now; real "view not configured" message wants to read an API diagnostic. Defer.
- **Slide-over panel form vs other in-context display.** See table above; soft-locked to slide-over panel pending the first real use case.


### Linter and formatter — **ESLint + Prettier**

The conventional pair. Single-tool alternatives (Biome) are tempting on the Rust-everywhere aesthetic, but Svelte's tooling (language server, `svelte-check`, every plugin and tutorial) is built around ESLint + Prettier. At ~50 TS/Svelte files Biome's speed advantage is invisible; its weaker Svelte-specific lint rules are a real cost.

Rejected: Biome (Svelte support not first-class in 2026; smaller plugin ecosystem); Prettier alone with no linter (misses unused imports, dead code, no-explicit-any patterns the type checker doesn't catch); nothing (style drift + diff noise on every PR).

#### Config

```
ui/
  eslint.config.js       -- flat config (post-ESLint-9 format)
  .prettierrc
  .prettierignore        -- node_modules, dist, src/lib/api/generated/
```

ESLint base: `@typescript-eslint/strict-type-checked` + `eslint-plugin-svelte` recommended set. Prettier: 2-space indent, double quotes, semicolons on, print width 100, plus `prettier-plugin-svelte` for `.svelte` files.

`package.json` scripts:

```json
{
  "lint": "eslint .",
  "format": "prettier --write .",
  "check": "svelte-check && eslint . && prettier --check ."
}
```

`npm run check` becomes the CI gate (runs type-check + lint + format-check). CI: a new step in `.github/workflows/ci.yml` runs `npm run check` after `cargo xtask build` produces the ts-rs types.

#### Approach

Start strict and prune anything that turns out to be noise — much easier than ratcheting rules up over an existing codebase. The generated `src/lib/api/generated/` directory is ignored by both tools (it's a build artifact; ts-rs's output style is its own concern).

#### Deferred

- **Pre-commit hook (Husky + lint-staged).** Skip for now — VSCode formats on save, CI catches misses. Add when PRs start arriving unformatted.


### TypeScript strictness beyond `strict: true` — **add six flags on top of `strict`**

Turn on now while the codebase is small. Same flags would be painful to retrofit once a year of code has accumulated.

```json
{
  "extends": "./.svelte-kit/tsconfig.json",
  "compilerOptions": {
    "strict": true,
    "noUncheckedIndexedAccess": true,
    "exactOptionalPropertyTypes": true,
    "verbatimModuleSyntax": true,
    "noImplicitReturns": true,
    "noUnusedLocals": true,
    "noUnusedParameters": true
  }
}
```

Each flag's payoff:

| Flag | Catches |
| --- | --- |
| `noUncheckedIndexedAccess` | `arr[i]` is typed `T \| undefined` (off-by-one, missing key). |
| `exactOptionalPropertyTypes` | Distinguishes `{x?: T}` from `{x: T \| undefined}`. Matters for the API envelope's `data?: T` shape. |
| `verbatimModuleSyntax` | Forces explicit `import type` — clean signal for bundlers, matches ts-rs's type-only outputs. |
| `noImplicitReturns` | All code paths in a function explicitly return. |
| `noUnusedLocals` / `noUnusedParameters` | Dead variables. Overlaps with ESLint; redundancy is cheap, fires in `svelte-check` too. |

Skipped: `noImplicitOverride` (marginal value with little class hierarchy in TS), `allowJs: false` (SvelteKit config files are `.js` by convention; not worth fighting), `noPropertyAccessFromIndexSignature` (verbose without strong payoff).


### Theme support and dark mode — **manual binary toggle, default light, persisted**

Two states: `"light" | "dark"`. Default is light on first visit. User can toggle from the header; choice persists to `localStorage`. No system-preference detection — opening the app does not consult `prefers-color-scheme`.

Rejected: system-only (no user agency); system-as-default-with-override (more state and more code for a marginal benefit at this scale — users who want dark will toggle once and keep it; `prefers-color-scheme` integration can be added later as a one-flag extension without restructuring the store).

The styling mechanics are already in place from the CSS framework decision: `tokens.css` defines light tokens as the default and `[data-theme="dark"]` redefines them. Every `var(--color-*)` re-themes when the attribute flips.

#### Shape

```ts
// ui/src/lib/stores/theme.svelte.ts
type Theme = "light" | "dark";

let theme = $state<Theme>(
  (localStorage.getItem("theme") as Theme) ?? "light"
);

$effect(() => {
  document.documentElement.dataset.theme = theme;
  localStorage.setItem("theme", theme);
});

export const themeStore = {
  get value() { return theme; },
  set(next: Theme) { theme = next; },
  toggle() { theme = theme === "light" ? "dark" : "light"; },
};
```

Toggle button in the app-shell header (sun ⇄ moon icon), top-right by convention.

#### First-paint flicker (FOUC)

Even with default light, a user who picked dark and refreshes sees a white flash before JS reads `localStorage`. Mitigated by an inline script in `app.html` (SvelteKit's HTML template) that synchronously reads `localStorage.theme` and sets `<html data-theme="...">` before stylesheets are parsed. ~5 lines of inline JavaScript, standard pattern.

#### Deferred

- **System-preference detection.** Can layer on later as a `"system"` third state without restructuring — `theme.svelte.ts` becomes tri-state, a `$derived` resolves to `"light" | "dark"`. Revisit when a user actually asks for it.
- **Toggle UI form.** Cycle button vs dropdown vs preference page. Cycle button (sun/moon) at scaffolding time; upgrade if the header gains other settings.



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
