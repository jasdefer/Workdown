# Tree: parent

Hierarchical outline following `parent` upward to roots.

- [A chart that shows progress over time (burndown or similar)](../workdown-items/burndown-chart.md) — status: to_do
  - [Decide where the burndown's time axis comes from](../workdown-items/burndown-chart-design.md) — status: to_do
- [Read config.yaml per request so it hot-reloads like everything else](../workdown-items/config-hot-reload.md) — status: to_do
- [The full git loop, without leaving the board](../workdown-items/full-git-loop.md) — status: to_do
  - [Let the web app commit, so the git loop is not broken in the middle](../workdown-items/commit-from-web-ui.md) — status: to_do
  - [Switch on git controls and the effort timer in this repo's own config](../workdown-items/dogfood-git-controls-config.md) — status: to_do
- [Miscellaneous improvements](../workdown-items/misc-work.md) — status: in_progress
  - [Dragging a card on a multichoice board wipes its other values](../workdown-items/board-drop-multichoice.md) — status: to_do
  - [One clock read per invocation, writes included](../workdown-items/evaluation-date-single-read.md) — status: to_do
  - [Compare dates in filters as dates, not as text](../workdown-items/typed-date-filter-comparison.md) — status: to_do
- [Multi-project support](../workdown-items/multi-project-support.md) — status: to_do
  - [Design multi-project support — set decisions and break out follow-up work](../workdown-items/multi-project-design.md) — status: to_do
- [Cache the project load in the server (when it starts to hurt)](../workdown-items/project-load-cache.md) — status: to_do
- [Animated project tour in the web UI](../workdown-items/project-tour.md) — status: in_progress
- [Extract the recording indicator the six item-presenting views each rebuilt](../workdown-items/recording-dot-extraction.md) — status: to_do
- [Apply the same-origin check to every mutating endpoint, not just the git ones](../workdown-items/same-origin-guard-everywhere.md) — status: to_do
- [See and edit the schema in the web app](../workdown-items/schema-editor-web.md) — status: to_do
  - [Decide how much of the schema the web app edits, and what a breaking save does](../workdown-items/schema-editor-web-design.md) — status: to_do
- [Derived field expressions](../workdown-items/schema-expressions.md) — status: to_do
  - [Decide which field types may declare compute and pull](../workdown-items/compute-type-support-mismatch.md) — status: to_do
  - [Integer precision and NaN in comparison evaluation](../workdown-items/expression-comparison-corner-cases.md) — status: to_do
  - [`and` / `or` / `not` in the expression grammar](../workdown-items/expression-logical-combinators.md) — status: to_do
  - [`map:` — lookup-table shorthand over the `when:` evaluator](../workdown-items/when-map-shorthand.md) — status: to_do
  - [`then:` values beyond literals — `$today`, fields, expressions](../workdown-items/when-then-value-expressions.md) — status: to_do
- [Fill in a date when a status changes, instead of typing it by hand](../workdown-items/status-transition-dates.md) — status: to_do
- [Decide what our tests are for, and restructure them accordingly](../workdown-items/testing-strategy.md) — status: to_do
  - [Work out the testing approach and break the milestone into items](../workdown-items/testing-strategy-design.md) — status: to_do
