# Tree: parent

Hierarchical outline following `parent` upward to roots.

- [A chart that shows progress over time (burndown or similar)](../workdown-items/burndown-chart.md) — status: to_do
  - [Decide where the burndown's time axis comes from](../workdown-items/burndown-chart-design.md) — status: to_do
- [Miscellaneous improvements](../workdown-items/misc-work.md) — status: in_progress
  - [One clock read per invocation, writes included](../workdown-items/evaluation-date-single-read.md) — status: to_do
  - [Prefill terminal commits with the generated message](../workdown-items/prepare-commit-msg-hook.md) — status: to_do
  - [Cache the project load in the server (when it starts to hurt)](../workdown-items/project-load-cache.md) — status: on_hold
  - [Extract the recording indicator the six item-presenting views each rebuilt](../workdown-items/recording-dot-extraction.md) — status: to_do
- [Multi-project support](../workdown-items/multi-project-support.md) — status: to_do
  - [Design multi-project support — set decisions and break out follow-up work](../workdown-items/multi-project-design.md) — status: to_do
- [See and edit the schema in the web app](../workdown-items/schema-editor-web.md) — status: in_progress
  - [Serve the full schema definition and the type tables to the web app](../workdown-items/schema-definition-api.md) — status: to_do
  - [Edit compute, when, pull and aggregate blocks in the field editor](../workdown-items/schema-derived-field-editor.md) — status: on_hold
  - [Field editor block for integer, float and duration](../workdown-items/schema-field-editor-numeric.md) — status: to_do
  - [Field editor block for link and links](../workdown-items/schema-field-editor-relation.md) — status: to_do
  - [The field editor panel, with the shared header and the save flow](../workdown-items/schema-field-editor-shell.md) — status: to_do
  - [Field editor block for string and list, with the resource picker](../workdown-items/schema-field-editor-text.md) — status: to_do
  - [Field editor block for choice and multichoice — the ordered value list](../workdown-items/schema-field-editor-values.md) — status: to_do
  - [Rename a field and everything that names it](../workdown-items/schema-field-rename.md) — status: on_hold
  - [Drag fields into a new order on the schema page](../workdown-items/schema-field-reorder.md) — status: to_do
  - [Let an existing field change type, but only to a type that keeps every value valid](../workdown-items/schema-field-type-change.md) — status: to_do
  - [Write one field definition into schema.yaml without touching the rest](../workdown-items/schema-field-write-backend.md) — status: to_do
  - [A schema page that shows the fields and rules](../workdown-items/schema-page-read-view.md) — status: to_do
  - [Edit rules on the schema page](../workdown-items/schema-rules-editor.md) — status: on_hold
  - [Turn a free-text field into a choice, with the values it already holds](../workdown-items/schema-string-to-choice.md) — status: on_hold
- [Derived field expressions](../workdown-items/schema-expressions.md) — status: in_progress
  - [`and` / `or` / `not` in the expression grammar](../workdown-items/expression-logical-combinators.md) — status: on_hold
  - [`map:` — lookup-table shorthand over the `when:` evaluator](../workdown-items/when-map-shorthand.md) — status: on_hold
  - [`then:` values beyond literals — `$today`, fields, expressions](../workdown-items/when-then-value-expressions.md) — status: on_hold
- [Fill in a date when a status changes, instead of typing it by hand](../workdown-items/status-transition-dates.md) — status: to_do
