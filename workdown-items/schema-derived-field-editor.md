---
id: schema-derived-field-editor
status: in_progress
parent: schema-editor-web
depends_on: [schema-field-editor-shell]
title: Edit compute, when, pull and aggregate blocks in the field editor
---

## In plain words

Some fields work out their own value: a computed number, a colour
picked by condition, a total rolled up from children. The plain field
editor shows those blocks but sends you to `schema.yaml` to change
them. This item makes them editable in the panel.

**Parked until** the plain-field editors ([[schema-field-editor-shell]]
through [[schema-field-editor-relation]]) have shipped and shown what
the panel's UX is. Decided in [[schema-editor-web-design]]: the simple
blocks go first so the derived ones are designed against a real panel,
not a sketch.

## What has to be settled when it is picked up

- **Expressions.** A `compute` line and a `when` condition are text in
  a typed expression language. Free text with the type checker's
  diagnostics shown inline, or a structured builder like the filter
  editor already has for `where:`. The filter builder is the obvious
  starting point; whether it covers arithmetic is the question.
- **`when` branches.** An ordered list of condition → value pairs plus
  a default. A list editor with a condition input and the type's value
  editor per row.
- **`aggregate` and `pull`.** Function picker from the per-type table,
  link-field picker for `pull`.
- **Unlocking the type.** Once the expression is editable, the type
  lock from the shell can be lifted for derived fields, with the type
  check as the guard.

## Not in scope

- New expression grammar. That is [[schema-expressions]].

## Decisions taken

Settled on 2026-10-09 with the user. Four premises checked first:
the type lock stays, because decision 12 of [[schema-editor-web-design]]
removed type changes from the editor altogether; the filter builder's
rows are field, operator, literal and cannot hold arithmetic or a
field-against-field comparison, only its guided-row-plus-raw-row pattern
carries over; the grammar is one comparison or one arithmetic term with
no `and`/`or` yet, every node carrying a span; and the wire serves each
recipe as YAML text while the write leaves recipe keys untouched and
cannot create a field with one. The parser makes `compute`, `when` and
`pull` mutually exclusive; `aggregate` combines with any of them.

Usage checked too: neither this repo's schema nor the shipped default
has a computed field, and every `when` branch in both is a single
field-against-literal comparison.

### Form or text

1. **Conditions are a row, formulas are text.** The objective is a user
   who does not know the syntax. A condition is the case such a user
   writes ("status is done"), so it gets a form: left operand, operator,
   right operand, where an operand is a field, a constant, `$today` or
   a literal entered through the shared `ValueEditor`. That removes the
   quoting trap (`status == done` is an unknown-field error). A formula
   is rare, has no user yet, and the language it is written in is still
   growing ([[schema-expressions]]), so it stays text, made friendly:
   the fields and constants the formula may use are listed beside the
   box and a click inserts one, and the type checker's error is shown
   at the exact character. Rejected for now: a nested row builder
   covering the whole grammar for both. It is the only option that
   meets "no syntax anywhere", costs about twice the rules editor, and
   serves a case nobody has; it is the step to take when formulas have
   users. Also rejected: text for conditions, which would leave the
   quoting trap in the common case.
2. **A condition the row cannot hold is text.** Arithmetic inside an
   operand, or anything beyond one comparison, is shown and edited as a
   raw text row, the filter editor's existing pattern. Rust decides
   which it is, with an exhaustive match on the parsed expression, so a
   new node kind fails to compile until the row shape names it.

### The panel

3. **A "Value" section in the left column.** A selector with four
   entries, entered by hand, computed, conditional, pulled, since the
   parser makes the three recipes exclusive. "Roll up along a link"
   (`aggregate`) is its own section on reducible types, because it
   combines with any of the four. The right column keeps the usage and
   gains the YAML pane the rule panel has. Changing the selector drops
   the old recipe.
4. **Formula options.** `round` only on date fields, `error_on_missing`
   on every type. The writer emits the bare expression string when
   neither is set, the shortest form the parser reads, as the rule
   writer does.
5. **Branches.** An ordered list with Up and Down, since first match
   wins. The `then` cell is `ValueEditor` with the field's own spec, so
   a color field offers the color editor and a choice its values. The
   default control, locked on a conditional field until now, unlocks as
   the fallback: a literal only, no generator, as the parser demands.
6. **Aggregate and pull are pickers over the served tables.** Function
   from the per-type table; `over` from the link fields with cycles
   forbidden, `link` only for aggregate, `link` or `links` for pull;
   pull's `field` from the fields whose reduction fits this field's
   type; `error_on_missing` as a toggle. The server remains the judge.
7. **Removing a `when` drops its `default` too.** Beside `when` the
   default is the evaluated fallback; left in place after the `when`
   goes it would silently become a stamped add-time default, a
   different meaning. The panel clears it and says so. The same when
   switching to computed or pulled, which forbid a default.

### The wire

8. **Structured recipes from Rust, mirroring the rules editor.**
   `GET /api/schema/definition` serves each recipe as named parts
   beside the text it serves today: the expression string and its
   options, the branches with each condition as a row or as text, the
   pull and aggregate configs. The write gains `recipe` and `aggregate`
   and the add path accepts them, so a field can be created with a
   recipe. Types reach the browser through `cargo xtask gen-types`.
   Rejected: the browser reading and writing the YAML itself, a second
   copy of the grammar that drifts.
9. **A preview endpoint, `POST /api/schema/field-preview`.** Takes the
   structured draft, runs the candidate schema through the load spine
   (the type check lives in the compute check and needs
   `resources.yaml`), and returns the YAML the write would produce plus
   the verdict. The pane shows the text; the verdict shows in the
   panel before Save. Same pattern and reason as the rule preview:
   produced by the code that saves, it cannot drift from it.
10. **A rejected recipe blocks Save.** A broken expression is a
    diagnostic that disables the field, not a load failure, so a hand
    edit may leave one in the file. In the panel Save is greyed out
    while the preview reports a problem: the person is right there and
    the position is known. Rejected: write, then warn, as items do.

### Build

11. **One item, in the order aggregate, pull, compute, when**, simplest
    form to hardest; each lands usable. Rust half first (structured
    recipe, writer, preview endpoint), then the panel.

Not in this item: compute expressions reading a field in the usage list
(today they appear only as the warning removal would cause), `and`/`or`
in conditions, and a resources editor for constants.

## Built

**2026-10-15, aggregate, the Rust half** (decision 11, step one).

- `AggregateConfig` is the wire type: `GET /api/schema/definition`
  serves it as `aggregate` on every field beside the text in
  `derived.aggregate`; `FieldDefinitionWrite` takes it back (`null`
  clears the block). The writer spells the block in its shortest form
  (`error_on_missing` only when true) from an exhaustive destructuring,
  so a setting added to the config fails to compile until written. The
  add path accepts it, so a field can be created with a rollup.
- `preview_field` and `POST /api/schema/field-preview` (decision 9),
  request `{name, replacing, definition}`. The entry is built by the
  same code the add and the replace use (`new_field_entry`,
  `replace_field_entry`), shown as YAML without the name key, recipe
  keys the write keeps included. The verdict: a name the add would
  refuse, the parser's message, or the schema-pinned diagnostics the
  draft introduces after a full load (decision 10's "rejected recipe";
  nothing an aggregate can trigger yet, the compute step will). Items
  the draft would warn about are not a problem. Its own address so a
  field named `preview` stays addressable.
- The field panel sends the rollup back as served (`FieldDraft.aggregate`),
  so a save keeps the block; `retype` drops it where the new type has
  no aggregate functions.
- `compute`, `when`, `pull` stay as the file has them on a replace.

**2026-10-15, aggregate, the panel** (decision 11, step one, UI half).

- `ui/src/lib/schema/AggregateBlock.svelte`: the "Roll up from the
  items below" section, shown where `properties_by_type` lists
  `aggregate`. Function picker from `aggregate_functions_by_type`,
  relation picker from `rollupRelations` (the `link` fields with
  `allow_cycles: false`), `error_on_missing` as a box. Switching the
  rollup on starts from `newRollup` (first function, first relation);
  the box is disabled with a hint when nothing can be climbed. A
  relation the file names but the picker cannot offer stays as
  "(not climbable)", the server refuses the save. Stateless, like the
  other blocks.
- `FieldPanel.svelte`: the YAML pane ("In schema.yaml") on the right,
  fed by `POST /api/schema/field-preview` after a 300 ms pause, keyed
  by the request so a stale answer is dropped; a `problem` greys Save
  out with the reason (decision 10). Skipped while a new field has no
  name. The read-only "Filled by" blocks are now `compute`, `when`,
  `pull`; the table's "Filled by" column still lists all four.
- `fieldDraft.ts`: `takesAggregate`, `aggregateFunctionsFor`,
  `rollupRelations`, `newRollup`; `TypeTables` gained
  `aggregate_functions_by_type`.
- Clicked through against this repo's schema: rollup switched on and off
  on an integer field, function and relation picked, the YAML pane and
  the Save verdict followed. Nothing was saved.

**2026-10-15, pull, the Rust half** (decision 11, step two).

- `FieldRecipe`, a tagged union in `schema_definition_data`, is how a
  recipe travels: `GET /api/schema/definition` serves it as `recipe`
  on every field (`None` by hand; `compute` and `when` still `None`
  until their step, their text in `derived`), `FieldDefinitionWrite`
  takes it back (`None` clears `pull`; `compute` and `when` in the file
  stay). One value for the "Value" selector's choice and its block, so
  two recipes cannot be sent at once. `PullConfig` is the variant's
  payload and the wire type, like `AggregateConfig`.
- The writer spells `pull` in its shortest form from an exhaustive
  destructuring, before `aggregate`; `WRITTEN_KEYS` gained `pull`.
- `pull_sources_by_type`: for each field type, the source types a pull
  into it may read, each with its functions, from `pull_functions` in
  the model. The "integer fits float" rule moved into
  `aggregate_result_fits`, which the pull check and the table share,
  so the browser copies no type rule (decision 6: pull's `field` from
  the fields whose reduction fits).
- The preview reports its first "rejected recipe" (decision 10): a
  pull whose reduction does not fit the field is a load diagnostic,
  not a parse error, and comes back as the problem.
- The field panel sends the recipe back as served (`FieldDraft.recipe`),
  so a save keeps a pull; `retype` drops it where the new type has no
  `pull` property.

**2026-10-15, pull, the panel** (decision 11, step two, UI half).

- The "Value" section (decision 3) in `FieldPanel.svelte`, after the
  type-specific block: a selector with "Set by hand" and "Pulled from
  linked items", shown where the type can be pulled into. A computed
  or conditional field shows "Computed" / "Conditional", locked, with
  its text still on the right; those entries unlock in their steps.
- `ui/src/lib/schema/PullBlock.svelte`: Through (the `link` and
  `links` fields that forbid cycles, `pullRelations`), Read (the
  fields whose type the served `pull_sources_by_type` lists,
  `pullSources`), Combine with (that field's functions), and the
  `error_on_missing` box with what it changes spelled out. A choice
  the file names but the pickers cannot offer stays as "(not
  followable)" / "(cannot be read)" / "(does not fit)"; the preview
  reports the problem. Switching the field read keeps the function
  where the new field offers it (`withPullSource`). Stateless.
- Choosing "pulled" starts from `newPull` (first relation, first
  readable field, its first function), drops the default and says so
  in the Default row (decision 7). "Set by hand" clears the recipe.
- `fieldDraft.ts`: `takesPull`, `pullRelations`, `pullSources`,
  `newPull`, `withPullSource`; `TypeTables` gained
  `pull_sources_by_type`.
- Clicked through against this repo's schema: an integer field added,
  a second one pulling it along `depends_on`, and a date field whose
  pull of it the preview refused with Save greyed out. The rewritten
  `schema.yaml` was restored afterwards.

**2026-10-15, compute, the Rust half** (decision 11, step three).

- `FieldRecipe::Compute { expression, round, error_on_missing }`: the
  formula as text (decision 1) with its options, served from
  `ComputeConfig.source` and taken back by the write. The writer
  spells the bare expression until an option is set (decision 4).
  `RoundMode` is on the wire in lowercase. `WRITTEN_KEYS` gained
  `compute`; `when` is the one key a write still leaves alone.
- The column (decision 1, "the error at the exact character"):
  `FieldPreview.problem_column`, the one-based column the problem's
  text names, as data. A syntax error's column comes from the
  expression parser, asked directly because the schema parser only
  relays its text; a type error's from the load's diagnostic, which
  now carries `column` (`ComputeInvalidExpression`). Both error types
  gained a `column()` accessor; an empty or early-ending formula has
  none.
- For the formula's insert list: `expression_readable_types` on the
  definition payload (the types with a place in the algebra, from
  `expression_type_of`), and `constants` on `GET /api/schema` with
  their typed values, since that payload already reads
  `resources.yaml` and the definition payload reads `schema.yaml`
  alone by design.
- The panel already sends the recipe back as served, so a save keeps
  a formula; the "Value" selector still shows "Computed" locked.

**2026-10-16, compute, the panel** (decision 11, step three, UI half).

- "Computed by a formula" in the "Value" selector, where the type
  takes `compute`; choosing it drops the default and says so
  (decision 7). Only `when` is still locked.
- `ui/src/lib/schema/ComputeBlock.svelte`: the formula as a monospace
  text box (decision 1); under it the server's objection with a caret
  line under the column it names (`problem_column`), the insert list
  — the readable fields from `expression_readable_types`, the field
  itself left out, the constants from `GET /api/schema` with their
  values, `$today` — where a click puts the token at the caret with
  the spaces a run-on would need (`insertAt`); `round` on a date
  field only, `error_on_missing` as a box with what it changes spelled
  out. The grammar stays in Rust: nothing in the browser parses the
  text.
- `fieldDraft.ts`: `ComputeRecipe`, `takesCompute`, `newCompute`,
  `expressionReferences`, `insertAt`; `TypeTables` gained
  `expression_readable_types`. `schemaStore.constants`.
- Clicked through in a browser against this repo's schema; nothing to
  report, and `schema.yaml` came out unchanged.

**2026-10-16, when, the Rust half** (decision 11, step four).

- `crates/core/src/condition_shape.rs`: a branch condition as a row
  (left operand, operator, right operand; an operand a field, a
  constant, `$today` or a literal) when the parsed expression is one
  comparison between plain operands, as text otherwise, by an
  exhaustive match on `Expression` (decisions 1 and 2); `to_source`
  spells a row back, a fraction keeping its dot. `ComparisonOperator`
  is on the wire in `snake_case`, with the grammar's symbols served
  as `comparison_operators`.
- `FieldRecipe::When { branches }`, each `WhenBranchData { condition,
  then }`; the fallback travels through the `default` slot, served as
  the coerced literal, written as sent, the parser refusing a
  generator there (decision 5). `FieldValue::to_yaml` spells a coerced
  value as a file does. `conditional_types` on the definition payload
  from `when_allowed`, which the parser's own check now uses too.
- The writer spells `when` as one `if`/`then` mapping per branch and
  no longer treats `default` as untouchable beside a `when`: the editor
  sends the fallback with the recipe, and a draft without the recipe
  drops both (decision 7). Every recipe key is now written; nothing is
  left alone.
- The panel sends a `when` back as served and keeps its "Conditional"
  entry locked until the panel half; `retype` asks `takesRecipe`.

**2026-10-16, when, the panel** (decision 11, step four, UI half; the
last step).

- "Conditional: by the first condition that holds" in the "Value"
  selector where the type is in `conditional_types`. Nothing in the
  panel is locked any more; the read-only "Filled by" section is gone.
- `ui/src/lib/schema/WhenBlock.svelte`: one row per branch with Up,
  Down and Remove (decision 5), Add branch below. A row condition is
  two operand pickers around an operator picker (symbols from
  `comparison_operators`); an operand is a readable field (the field
  itself left out), a constant, `$today`, or "a value" edited with
  `ValueEditor` built for the other side's type (`literalSpecFor`: a
  choice offers its values, a constant's type is read off its value,
  `$today` makes it a date). A text condition is a monospace input. A
  row turns into a text line and back, each starting empty: the
  browser does not spell the grammar, so it does not translate between
  them (decision 2, kept strict). `then` is `ValueEditor` with the
  field's own spec. The default control stays, relabelled "Default
  when no branch holds", with the generator choice withheld.
- Decision 7 in the panel: leaving "Conditional" clears the default
  and says why; choosing it drops a generated default and says so.
- `fieldDraft.ts`: `WhenRecipe`, `takesWhen`, `operandChoices`,
  `operandKey`, `literalSpecFor`, `literalOf`, `valueOfLiteral`,
  `thenOf`, `newBranch`, `newWhen`, `withBranch*`; `moved` shared
  with the values list.
- Not clicked through in a browser at the time of writing.
