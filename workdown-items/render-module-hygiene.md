---
id: render-module-hygiene
type: issue
status: done
title: Render module hygiene — escape helpers, test fixtures, common.rs naming
parent: code-quality
---

The renderer set under `crates/cli/src/render/` accumulated quickly. Several rough edges surfaced once it stabilized:

- **Escape helpers are scattered.** `card_link` in `render/common.rs` escapes link text; individual renderers re-do their own Markdown/HTML escaping inline. Pull the primitives into one place so a renderer never has to think about the rules.
- **Test fixtures are duplicated.** Each renderer's tests build a `ViewData` value by hand with similar boilerplate (cards, schema, group lists). Share a small fixture helper so tests stay readable and adding a renderer doesn't mean copy-pasting setup.
- **`common.rs` naming is overloaded.** Three "common" files coexist: `render/common.rs`, `render/chart_common.rs`, `render/gantt_common.rs`. The split is by audience but the naming doesn't make that obvious. Rename or restructure so a reader can tell at a glance what each one is for.

## Objective

Renderer code should pull shared concerns from a clearly-named place, and adding a new renderer should involve writing renderer-specific code only — no copy-pasting escape rules or test scaffolding.

## Out of scope

- Behavior changes to any renderer's output.
- Moving renderers between crates.
