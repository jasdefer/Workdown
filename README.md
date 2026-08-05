# Workdown

A lightweight, git-native project management framework. Work items are structured Markdown files stored directly in your repository — no external database, no cloud service. The repository is the single source of truth.

## Install

PowerShell (Windows):

```powershell
irm https://github.com/jasdefer/Workdown/releases/latest/download/workdown-installer.ps1 | iex
```

Shell (macOS / Linux):

```sh
curl --proto '=https' --tlsv1.2 -LsSf https://github.com/jasdefer/Workdown/releases/latest/download/workdown-installer.sh | sh
```

After installation, `workdown` is on your `PATH`.

> Workdown is distributed as a prebuilt binary only — there is no `cargo install workdown` path. The binary ships with the web UI embedded; building from a `cargo install` would require a Node toolchain on the install machine, which conflicts with the "one tool, no extra runtimes" goal. Use the installer above on any supported platform.

### Update

Workdown ships an updater alongside the main binary:

```powershell
workdown-update
```

This checks GitHub for the latest release and replaces the installed binary in place. Re-running the original installer command works too — it'll overwrite the existing install with the latest version.

## Quick start

From inside an existing git repository:

```powershell
workdown init
workdown add --type task --title "Implement user login"
workdown validate
workdown render
```

`workdown init` scaffolds two directories:

- `.workdown/` — configuration (schema, resources, views, templates)
- `workdown-items/` — your work item Markdown files

## Running the web UI

`workdown serve` boots a local web UI for browsing and editing work items:

```sh
workdown serve              # default port 3141
workdown serve --port 8080  # pick a specific port
workdown serve --open       # also launch your default browser
```

If port 3141 is busy, workdown scans the next ten ports (3142, 3143, …) and uses the first free one. Pass `--port N` to pin a specific port; in that mode workdown won't fall back — it fails if `N` is taken.

To pin a default port for a project (committed to the repo, shared by everyone):

```yaml
# .workdown/config.yaml
serve:
  port: 3142
```

Inside a devcontainer or remote SSH session, `--open` will silently fail to launch a browser (there's no display); VS Code's auto-forwarded-port notification handles the same job. The UI is local-only — `workdown serve` binds to `127.0.0.1` and never exposes anything to the network.

## Work item format

Each work item is a single Markdown file. YAML frontmatter holds structured fields; the body is freeform Markdown.

```markdown
---
title: Implement user login
type: task
status: open
parent: auth-epic
---

Description, notes, acceptance criteria — anything you want.
```

Filename (minus `.md`) is the work item's ID. References to other items use that same ID, e.g. `parent: auth-epic`.

## Views and rendering

Views turn work items into boards, trees, tables, gantt charts and more. They are declared once in `.workdown/views.yaml` and consumed two ways: `workdown serve` shows them live in the web UI, and `workdown render` writes each one as a Markdown file (charts as embedded SVG) into `views/`, meant to be committed alongside the items.

```yaml
# .workdown/views.yaml
views:
  - id: status-board
    type: board
    field: status
```

`workdown render` turns that into `views/status-board.md`:

```markdown
# Board: status

Cards grouped into columns by `status`.

## open
- [Implement user login](../workdown-items/implement-user-login.md)
- [Add password reset](../workdown-items/add-password-reset.md)

## in_progress
_(no cards)_

## done
- [Set up CI](../workdown-items/set-up-ci.md)
```

Because rendered views are plain files in the repository, they change in the same commit as the work items: a PR that finishes a task also moves its card to the done column, reviewable in the diff and readable on GitHub without any tooling. Boards are one kind of many — tree, graph, table, gantt (and variants), treemap, workload, bar and line charts, heatmap and metric views are all cataloged with their options in [docs/views.md](docs/views.md).

Rendered views go stale the moment an item changes. `workdown install-hooks` (or `workdown init --install-hooks`) installs a git pre-commit hook that re-renders and stages them whenever a commit touches work items or workdown configuration — pass `--check` to have it fail the commit instead of staging. It never overwrites a pre-commit hook it didn't write.

This repository manages its own development with workdown: [`workdown-items/`](workdown-items/) holds the real work items, [`views/`](views/) their rendered views.

## Configuration

Everything under `.workdown/` is plain YAML and user-editable:

| File              | Purpose                                                                  |
| ----------------- | ------------------------------------------------------------------------ |
| `config.yaml`     | Project metadata and file paths                                          |
| `schema.yaml`     | Field definitions, types, validation rules, defaults                     |
| `resources.yaml`  | Named lists (people, teams, sprints) that fields can reference           |
| `views.yaml`      | Persisted views: boards, trees, graphs, tables, gantt charts, etc.       |
| `templates/`      | Work item templates                                                      |

Fields are typed (string, choice, integer, date, link, links, …). Any `choice` field can drive a board view; any `link` field can drive a tree view; any `links` field can drive a graph view. There's no "magic" field name except `id`.

## Documentation

- [Schema guide](docs/schema.md) — field types, validation rules, defaults, computed, aggregated, and pull fields.
- [Views guide](docs/views.md) — every view kind and its options.
- [Architecture Decision Records](docs/adr/) — the *why* behind the core design choices.

## Working on workdown itself

Contributors only — most users can skip this section.

The workspace is one Cargo workspace plus a separate SvelteKit project for the web UI:

| Path        | What's in it                                              |
| ----------- | --------------------------------------------------------- |
| `crates/core`   | Pure library: parsing, validation, mutation               |
| `crates/cli`    | `workdown` binary — clap subcommands wrapping `core` |
| `crates/server` | axum-based local web server with embedded SvelteKit bundle |
| `crates/xtask`  | Build orchestrator — runs `npm` then `cargo` for release builds |
| `ui/`           | SvelteKit project (TypeScript, `adapter-static` in SPA mode) |

A devcontainer is provided with Rust and Node 20 preinstalled — open the repo in VS Code and "Reopen in Container".

**UI iteration loop** (fast feedback with HMR):

```sh
# Terminal 1 — Vite dev server with hot-module reload
cd ui && npm run dev

# Terminal 2 — backend (debug mode reads ui/dist/ from disk)
cargo run -- serve
```

Vite serves the UI at `http://localhost:5173` and proxies `/api/*` to the backend on `localhost:3141`. Edits to Svelte components hot-reload in the browser without restarting either side.

**Production build** (UI embedded in the binary):

```sh
cargo xtask build      # npm ci + npm run check + npm run build + cargo build --release
./target/release/workdown serve
```

This is the same pipeline CI runs on every PR, so local breakage of the release path is caught before pushing.

Plain `cargo check`, `cargo test`, and `cargo clippy` stay pure-Rust and do not invoke Node — `rust-embed`'s `debug-embed = false` default means debug builds read `ui/dist/` from disk at runtime instead of baking it in.

## Status

Early development. Prerelease versions are published on the [releases page](https://github.com/jasdefer/Workdown/releases). Expect breaking changes before `v1.0.0`.

## License

[MIT](LICENSE)
