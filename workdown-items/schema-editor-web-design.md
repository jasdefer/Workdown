---
id: schema-editor-web-design
status: done
parent: schema-editor-web
title: Define the schema editor's UX and settle how it writes, then break out the work
---

## In plain words

Editing the schema from the browser is not hard to build; it is hard to
build *safely*. A saved schema is what every item in the project is
checked against, so a careless save can invalidate the whole project at
once — including the page you saved from. [[schema-editor-web]] lists
the questions that raises. This item answers them, defines the feature
end to end, and breaks out the work.

Two premises of the parent were checked against the code on 2026-09-09
and corrected here:

- **There is no schema page today.** The web app fetches
  `GET /api/schema` only to populate item editors. Nothing renders the
  schema. The first cut therefore builds the read view as well as the
  editing.
- **A save preview is cheap.** `parse_schema` takes a string and
  `Store::load_with_resources` takes a schema by reference, so a
  candidate schema can be run through the load spine in memory and its
  diagnostics diffed against the current load. No new machinery.

## UX definition

### The page

A `/schema` route in the app shell, reached from the navigation next to
the views. It has two sections in file order: **Fields** and **Rules**.
Nothing else from the project appears here — resources and constants
live in `resources.yaml` and get their own editor later.

**Fields** is a table in declaration order, one row per field: name,
type, required, default, the first line of the description, and badges
for anything the row cannot express as a plain property — *computed*,
*conditional*, *pulled*, *aggregated*, *resource-backed*. Rows have a
drag handle; dropping a row rewrites the order in the file, because
declaration order drives form layout and board columns. An **Add
field** button sits below the table. Clicking a row opens the field
editor.

**Rules** is a list, one entry per rule: name, description, and the
match and require blocks rendered as they are in the file. Read-only in
the first cut; editing is a later child.

When `schema.yaml` fails to load, the page shows the load diagnostic
and the file path in place of both sections. The rest of the app is in
the `422` tier at that moment anyway (ADR-013); this page is where the
reason is legible. There is no in-browser repair.

### The field editor

A slide-over panel, the same idiom the item detail uses. It has a
fixed header block and a type-specific block beneath it, then a footer.

**Header block**, identical for every type:

- *Name.* Free text for a new field, following the same identifier
  rules the parser enforces. Locked for an existing field: renaming
  orphans every item, view slot, rule and expression that names it, so
  it is not offered.
- *Type.* A selector. For a new field every type is available. For an
  existing field only the current type and the widening changes are
  enabled; the rest are greyed with the hint "change in schema.yaml".
  Switching type keeps the header block, drops the type-specific
  properties after a confirm, and swaps the block beneath.
- *Required.* A toggle.
- *Description.* Free text.
- *Default.* Three-way: none, a fixed value entered in the type's own
  editor (the same component the item panel uses), or a generated value
  chosen from the generators valid for this type.

**Type-specific block.** The twelve types share five shapes, taken from
the property table in the schema model:

| Shape | Types | Controls |
|---|---|---|
| Plain scalar | color, boolean, date | none |
| Numeric | integer, float, duration | min, max |
| Text | string, list | pattern; resource picker over `resources.yaml` |
| Value list | choice, multichoice | ordered values: add, remove, reorder, edit |
| Relation | link, links | allow cycles toggle; inverse picker over the other link fields |

Editing a value in the value list does not rewrite items holding the
old value; the save preview reports how many would warn.

**Derived fields.** A field with `compute`, `when`, `pull` or
`aggregate` shows that block as read-only YAML under its type-specific
block, with "edit in schema.yaml" beside it. Its type is locked because
the expression was type-checked against it. Everything else in the
panel is editable. Editing the block itself is a later child.

**The `id` field.** Shown first, type locked to string, no remove
button. It is the one privileged field.

**Footer.** *Save*, *Cancel*, and for an existing field *Remove*. When
the field's block in `schema.yaml` carries comments, a one-line note
above the footer says they will not be kept.

### Saving

The unit of a write is one field definition. Nothing is written while
typing: a definition is only valid as a whole (a choice with no values
yet does not load), and every write pings every open tab.

Save runs in two tiers, the same split `views.yaml` writes use:

1. The server builds the candidate schema and loads it in memory. If
   it fails to load — a choice without values, an unknown resource, a
   default of the wrong type — the panel shows the parse error and
   nothing is written.
2. If it loads, the server diffs the candidate's diagnostics against
   the current ones. When the change adds warnings — items now holding
   an invalid value, views or rules naming a removed field — a
   confirmation dialog lists them by count and item, in the style of
   the commit dialog. Confirming writes; cancelling returns to the
   panel. With no new warnings the write happens directly.

The write is a splice: the field's mapping entry in `schema.yaml` is
located by byte span and replaced, a new field is appended at the end
of `fields:`, a removed field's entry is cut, a reorder moves entries.
Every byte outside the touched entries survives, including the comment
above the field and the order of everything else. Comments inside the
replaced entry are lost, which is what the footer note warns about.

After the write the panel closes, the file watcher pings, and the page
and every other tab refetch as they do for any YAML change. Undo is a
git revert; the git pill shows the changed file.

### Removing

*Remove* goes through the same preview: how many items hold the field
and will get an unknown-field warning, and which views and rules name
it. Items are not rewritten. A field named by a role in `config.yaml`
(board, tree, graph, display, effort) cannot be removed from the page,
because config is read at boot and the break would surface only on
restart.

### Changing type

Allowed only when every value valid under the old type is valid under
the new one by construction:

- integer → float
- integer, float, date, duration, boolean, choice, color → string
- multichoice, links → list

The table lives in Rust and is served with the property table. String
→ choice, the most wanted conversion, is a follow-up: it needs the
values pre-filled from what items already hold.

### Concurrency

The panel's save carries a hash of the `schema.yaml` it was loaded
from. If the file changed underneath — a second tab, or an editor —
the server answers `409` and the panel offers to reload the field,
discarding the pending edits. A splice over changed text would
otherwise silently overwrite someone's edit to the same field.

## Decisions taken

1. **First cut edits plain fields, shows everything.** Add, edit,
   remove and reorder fields with their plain properties. Derived
   blocks and rules are displayed, not edited. Each gets its own child
   later, once the plain editor has shown what the UX looks like.
2. **Form, not text.** A YAML textarea in the browser is strictly worse
   than an editor with the shipped JSON-schema autocomplete. The form is
   what adds value for the person who does not know the YAML.
3. **Splice write.** Whole-file re-serialize, which `views.yaml` uses,
   would drop 92 of the 198 lines of the shipped default schema. A
   comment-preserving YAML writer for Rust would need to be found and
   trusted. Splicing by span keeps everything outside the edited entry
   byte-for-byte and needs only a span-aware parser (`serde_yaml` has no
   spans and is archived upstream; `saphyr` or similar for span
   discovery, `serde_yaml` unchanged for everything else).
4. **Two save tiers.** Unloadable candidate: rejected, nothing written.
   Loadable with new warnings: previewed behind a confirm. Same rule
   `views.yaml` writes follow, with the preview added because the
   schema's blast radius is the whole project.
5. **No rename.** Its consequences span items, views, rules and
   expressions. A rename that rewrites all of them is its own item.
6. **Remove is remove.** Items keep the key and get the existing
   unknown-field warning. No cascading rewrite of items.
7. **No CLI counterpart.** The CLI user has a text editor with
   autocomplete, which is the better tool for them. The write lives in
   core as `operations/schema_write.rs` beside `view_write.rs`, so a
   command later is a thin wrapper. Confirms the `views.yaml` choice on
   its merits.
8. **One source for type knowledge.** A new `GET /api/schema/definition`
   serves the full persisted field definitions, the property table
   (`allowed_field_properties`), the aggregate-function table, the
   generator-per-type table and the widening table, typed through
   `ts_rs`. The generator pairing currently lives in hand-written parser
   checks and moves into the table first. `GET /api/schema` is
   unchanged so item editors do not pay for it. ADR-005 stands.
9. **`409` on a changed file.** Justified by the splice.
10. **Save per field definition.** Not live, not per page.
11. **Reorder in the first cut.** Cheap on the splice write, and order
    is user-visible in every form and board.
12. **Type changes: forbid by default, allow widening.** A hand-kept
    table in Rust of changes that cannot invalidate a value.
13. **Only fields and rules on the page.** Resources and constants wait
    for a resources editor.

## Breakdown

Cut on 2026-09-09 as children of [[schema-editor-web]], in build order:

1. [[schema-definition-api]] — `GET /api/schema/definition` with the
   tables in decision 8; generator pairing moved into the property
   table; `ts_rs` types.
2. [[schema-page-read-view]] — the `/schema` route, field table with
   badges, rules list, broken state. No editing yet.
3. [[schema-field-write-backend]] — `operations/schema_write.rs`:
   splice add/replace/remove/reorder, candidate load, diagnostic diff,
   hash check; `POST`/`PUT`/`DELETE` under `/api/schema/fields`; the
   reorder endpoint.
4. [[schema-field-editor-shell]] — the slide-over with the header
   block, default control, save flow with preview dialog, remove, `409`
   handling, comment note. Covers the plain-scalar shape as its first
   type-specific block.
5. [[schema-field-editor-numeric]] — min/max, typed per type.
6. [[schema-field-editor-text]] — pattern, resource picker.
7. [[schema-field-editor-values]] — the ordered value-list editor.
8. [[schema-field-editor-relation]] — allow cycles, inverse picker.
9. [[schema-field-reorder]] — drag handle on the table.
10. [[schema-field-type-change]] — widening table, greyed selector,
    confirm on dropped properties.

Parked (`on_hold`) until the above has shaped the UX:

- [[schema-derived-field-editor]] — `compute`, `when`, `pull`,
  `aggregate`.
- [[schema-rules-editor]].
- [[schema-field-rename]] — rename with rewrite of items, views, rules
  and expressions.
- [[schema-string-to-choice]] — conversion with values pre-filled from
  existing item values.
