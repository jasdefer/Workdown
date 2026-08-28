# Tree: parent

Hierarchical outline following `parent` upward to roots.

- [Read config.yaml per request so it hot-reloads like everything else](../workdown-items/config-hot-reload.md) — status: to_do
- [Maintenance pass: findings from the 2026-08 codebase review](../workdown-items/maintenance-review-2026-08.md) — status: in_progress
  - [One page that shows how a render flows through the system](../workdown-items/render-flow-doc.md) — status: to_do
  - [Fix the documentation that is actively wrong](../workdown-items/stale-docs-refresh.md) — status: to_do
  - [Test the two stateful areas that currently have no coverage](../workdown-items/stateful-test-gaps.md) — status: to_do
  - [Make the non-Rust schema mirrors fail loudly when they drift](../workdown-items/view-kind-sync-guards.md) — status: to_do
- [Miscellaneous improvements](../workdown-items/misc-work.md) — status: to_do
  - [One clock read per invocation, writes included](../workdown-items/evaluation-date-single-read.md) — status: to_do
  - [A view over tags](../workdown-items/tags-view.md) — status: to_do
  - [Compare dates in filters as dates, not as text](../workdown-items/typed-date-filter-comparison.md) — status: to_do
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
