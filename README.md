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

## Status

Early development. The first installable release is `v0.1.0-alpha`. Expect breaking changes before `v1.0.0`.

## License

[MIT](LICENSE)
