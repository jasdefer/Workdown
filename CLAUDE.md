# Workdown

Git-based project management system. Work items are structured Markdown files (YAML frontmatter + body) stored in the repository. The repo is the single source of truth.

## Tech Stack

- **Language:** Rust
- **CLI framework:** clap
- **Serialization:** serde + serde_yaml

## Key Design Decisions

- **Flat structure:** All work items live in a single directory (default: `work-items/`)
- **Hybrid ID:** Filename (without `.md`) is the ID by default. An optional `id` field in frontmatter overrides it.
- **Modular schema:** Field definitions, types, and validation rules are configured in `.workdown/schema.yaml`. Users can define their own frontmatter structure.
- **Configurable state machine:** States and transitions are defined in the schema config. Enforcement is opt-in.
- **Relations — single source:**
  - `parent` is declared on the child only. Children lists are derived.
  - `depends_on` (predecessors) is declared on the dependent only. Successors are derived.
  - No bidirectional declarations — avoids contradictions.
- **Validation and logic live outside the data** — the CLI handles all validation, querying, and rendering.

## Project Structure

```
.workdown/
  config.yaml          # Project-level configuration
  schema.yaml          # Field definitions, state machine, validation rules
  templates/           # Work item templates
work-items/
  *.md                 # Work item files
```

## Conventions

- Work item filenames: kebab-case, e.g. `implement-login.md`
- References use the ID (filename without `.md`), e.g. `parent: implement-auth-epic`
- One work item per file
- Frontmatter = structure, Markdown body = content
