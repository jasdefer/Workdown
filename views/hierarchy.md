# Tree: parent

Hierarchical outline following `parent` upward to roots.

- [Maintenance pass: findings from the 2026-08 codebase review](../workdown-items/maintenance-review-2026-08.md) — status: in_progress
  - [Grab bag of small consistency fixes from the review](../workdown-items/assorted-small-fixes.md) — status: to_do
  - [Make the terminal chart renderers share what they each rebuilt](../workdown-items/chart-renderer-sharing.md) — status: to_do
  - [One voice for validation messages](../workdown-items/message-style-consistency.md) — status: to_do
  - [Stop validating views and metric rows with two copies of every check](../workdown-items/metric-row-check-unification.md) — status: to_do
  - [Define the "parent" roll-up default exactly once](../workdown-items/over-default-single-definition.md) — status: to_do
  - [Deduplicate the filter engine's comparison and formatting logic](../workdown-items/query-value-consolidation.md) — status: to_do
  - [One page that shows how a render flows through the system](../workdown-items/render-flow-doc.md) — status: to_do
  - [Table-drive the "is this property allowed on this field type?" check](../workdown-items/schema-property-table.md) — status: to_do
  - [Fix the documentation that is actively wrong](../workdown-items/stale-docs-refresh.md) — status: to_do
  - [Test the two stateful areas that currently have no coverage](../workdown-items/stateful-test-gaps.md) — status: to_do
  - [Decide where the required-field check belongs in the load pipeline](../workdown-items/validation-phase-boundaries.md) — status: in_progress
  - [Make the non-Rust view-kind mirrors fail loudly when they drift](../workdown-items/view-kind-sync-guards.md) — status: to_do
  - [Sort and group view items in one place, not per renderer](../workdown-items/view-order-in-extractor.md) — status: to_do
  - [Write down the web layer's design decisions as an ADR](../workdown-items/web-layer-adr.md) — status: to_do
- [Miscellaneous improvements](../workdown-items/misc-work.md) — status: to_do
  - [One clock read per invocation, writes included](../workdown-items/evaluation-date-single-read.md) — status: to_do
  - [A view over tags](../workdown-items/tags-view.md) — status: to_do
  - [Move value coercion out of the store to break the parser↔store cycle](../workdown-items/value-coercion-layering.md) — status: to_do
  - [Collapse the metric-row duplicates of the generic view checks](../workdown-items/views-check-metric-row-dedup.md) — status: to_do
- [Multi-project support](../workdown-items/multi-project-support.md) — status: to_do
  - [Design multi-project support — set decisions and break out follow-up work](../workdown-items/multi-project-design.md) — status: to_do
- [Cache the project load in the server (when it starts to hurt)](../workdown-items/project-load-cache.md) — status: to_do
- [Derived field expressions](../workdown-items/schema-expressions.md) — status: to_do
  - [Integer precision and NaN in comparison evaluation](../workdown-items/expression-comparison-corner-cases.md) — status: to_do
  - [`and` / `or` / `not` in the expression grammar](../workdown-items/expression-logical-combinators.md) — status: to_do
  - [`map:` — lookup-table shorthand over the `when:` evaluator](../workdown-items/when-map-shorthand.md) — status: to_do
  - [`then:` values beyond literals — `$today`, fields, expressions](../workdown-items/when-then-value-expressions.md) — status: to_do
