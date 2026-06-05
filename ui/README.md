# Workdown web UI

The frontend for the Workdown web app — a [SvelteKit](https://svelte.dev/docs/kit)
(Svelte 5 + TypeScript) single-page app that renders a project's views **read-only**.
It is served by the `workdown serve` subcommand, which embeds this app's built bundle
directly into the Rust binary (via `rust-embed`) and exposes project data over a small
JSON API under `/api`.

You normally don't build this directory by hand — the workspace `xtask` orchestrator
does it as part of a release build (see [Building](#building)). Work here when developing
the frontend itself.

## Architecture

- **Wire types are generated from Rust.** The TypeScript types in `src/lib/api/generated/`
  are emitted from the core crate's wire structs via `ts-rs` — don't edit them by hand.
  Regenerate with `cargo xtask gen-types`.
- **One module per view kind.** `src/lib/views/<kind>/` mirrors the Rust `view_data` and
  CLI `render` splits (board, table, tree, graph, gantt, charts, …).
- **Read-only.** Mutations, item detail pages, and live file-watching are tracked as
  follow-up work and not implemented yet.

## Developing

Prerequisites: Node.js 20 and npm (provided automatically in the dev container).

```sh
npm install
```

The dev server needs a running backend for its `/api` calls. In one terminal, serve a
Workdown project:

```sh
workdown serve            # listens on http://localhost:3141 by default
```

In another, start the Vite dev server — it proxies `/api` to `localhost:3141`
(see `vite.config.ts`):

```sh
npm run dev               # or: npm run dev -- --open
```

> If imports from `$lib/api/generated/` fail to resolve, the generated types are missing —
> run `cargo xtask gen-types`.

## Checks

```sh
npm run check             # svelte-check (types) + eslint + prettier --check
npm run lint              # eslint
npm run format            # prettier --write
npm run test              # vitest
```

## Building

The production bundle is built and embedded into the `workdown` binary by the workspace
`xtask`:

```sh
cargo xtask build-ui      # gen-types + npm ci + npm run check + npm run build
cargo xtask build         # build-ui, then `cargo build --release`
```

`npm run build` alone emits the static bundle to `dist/` (via `@sveltejs/adapter-static`);
`workdown serve` embeds that directory at compile time. A plain `cargo build` stays
pure-Rust and does **not** rebuild the UI.
