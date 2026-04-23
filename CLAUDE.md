# Workdown

Git-based project management CLI tool. Installed into user repos via `workdown init`. Work items are structured Markdown files (YAML frontmatter + freeform body). The repo is the single source of truth.

## Tech Stack

- **Language:** Rust
- **CLI framework:** clap
- **Serialization:** serde + serde_yaml

## This Repo vs Consumer Projects

This repo is the **tool itself** (Rust CLI). Consumer projects run `workdown init` which scaffolds:

```
.workdown/
  config.yaml          # Project config (from defaults/config.yaml)
  schema.yaml          # Field definitions and rules (from defaults/schema.yaml)
  resources.yaml       # Resource lists: people, teams, etc. (from defaults/resources.yaml)
  views.yaml           # Persisted view definitions (from defaults/views.yaml)
  templates/           # Work item templates
workdown-items/
  *.md                 # Work item files
```

## Configuration Files (consumer project)

- **`config.yaml`** — Entry point for the CLI. Defines project metadata, file paths (where work items live, where templates are, where resources are), and CLI defaults (which field to use for board/tree/graph views).
- **`schema.yaml`** — User-editable. Defines fields, their types, validation rules, defaults, and aggregate behavior. This is what makes each project's work items structured differently. Fields can reference resources via `resource: <name>`.
- **`resources.yaml`** — User-editable. Named lists of entities (people, teams, sprints, etc.) that work item fields can reference. A field with `resource: people` only accepts values matching an `id` from the `people` section.
- **`views.yaml`** — User-editable. Declares persisted views rendered by `workdown render` (board, tree, graph, table, gantt, charts, etc.). Each view references schema fields.
- **`schema.schema.json`** (shipped with CLI, not in consumer project) — JSON Schema that formally defines the structure of `schema.yaml`. Used by editors for autocomplete. Not loaded by the CLI at runtime — see ADR-005.
- **`resources.schema.json`** (shipped with CLI, not in consumer project) — JSON Schema that formally defines the structure of `resources.yaml`. Not user-editable.
- **`views.schema.json`** (shipped with CLI, not in consumer project) — JSON Schema that formally defines the structure of `views.yaml`. Used by editors for autocomplete. Not loaded by the CLI at runtime — see ADR-005.

## Work Item File Format

Each work item is exactly one `.md` file. Structure:

```markdown
---
title: Implement user login
type: task
status: open
parent: auth-epic
---

Freeform Markdown body. Description, notes, acceptance criteria — anything.
```

- Frontmatter: YAML between `---` delimiters. All structured metadata.
- Body: everything below the frontmatter. No structure enforced by the CLI.
- One work item per file. No multi-item files.

## Key Design Decisions

- **Generic type system:** Field types (not names) drive CLI behavior. Any `choice` field can be a board, any `link` field can be a tree, any `links` field can be a graph. No field name is "magic" except `id`. See ADR-002.
- **Built-in type system:** Types (string, choice, multichoice, integer, float, date, boolean, list, link, links) are built into the CLI. Formally defined in `defaults/schema.schema.json`. Users define fields and pick types in their `schema.yaml`.
- **Snapshot-only validation:** The CLI validates current file state, not git history. No state transition enforcement. See ADR-001.
- **Hybrid ID:** `id` is the one special field — filename (minus `.md`) by default, frontmatter `id` overrides it. Uniqueness enforced.
- **Title fallback:** `title` is optional. Falls back to prettified filename.
- **Default generators:** Fields can have generated defaults (`$filename`, `$filename_pretty`, `$uuid`, `$today`, `$max_plus_one`) applied at `workdown add` time.
- **Computed/aggregated fields:** Fields with an `aggregate` config are set manually on leaf items and computed automatically up the parent chain. Two items in the same ancestor chain both setting manually is a validation error.
- **Relations are generic:** `link`/`links` fields with `allow_cycles` config. Default relations: `parent`, `depends_on`, `related_to`, `duplicates`. Inverses are derived by the CLI.
- **Flat structure:** All work items in a single directory (default: `workdown-items/`).
- **Validation:** Broken references and cycle detection (where `allow_cycles: false`).

## Project Structure (this repo)

```
crates/
  core/                # Shared business logic (model, parser, operations)
    src/               # Rust source
    defaults/          # Default files for `workdown init`
      config.yaml      # Default project config
      schema.yaml      # Default field definitions and validation rules
      resources.yaml   # Default resource lists (people, teams, etc.)
      views.yaml       # Default persisted view definitions
      schema.schema.json    # JSON Schema: formal definition of schema.yaml structure (not user-editable)
      resources.schema.json # JSON Schema: formal definition of resources.yaml structure (not user-editable)
      views.schema.json     # JSON Schema: formal definition of views.yaml structure (not user-editable)
    tests/             # Integration tests for the core crate
  cli/                 # CLI binary (clap)
  server/              # Local web server (`workdown serve`)
docs/
  adr/                 # Architecture Decision Records
```

## Architecture Decision Records

Key design decisions are recorded in `docs/adr/`. Create new ADRs for fundamental architectural decisions only, keep them slim.

## Conventions

- Work item filenames: kebab-case, e.g. `implement-login.md`
- References use the ID (filename without `.md`), e.g. `parent: implement-auth-epic`
- One work item per file
- Frontmatter = structure, Markdown body = freeform content
