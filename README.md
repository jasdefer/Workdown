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

Early development. The first installable release is `v0.1.0-alpha.1`. Expect breaking changes before `v1.0.0`.

## License

[MIT](LICENSE)
