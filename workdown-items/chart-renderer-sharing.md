---
id: chart-renderer-sharing
status: to_do
title: Make the terminal chart renderers share what they each rebuilt
parent: maintenance-review-2026-08
depends_on:
- view-order-in-extractor
---

## In plain words

The terminal chart renderers each rebuilt pieces that a shared helper
file exists to provide. The "Unplaced" section listing items a chart
could not place is copy-pasted into four renderers — and two other
renderers do it in a different style, so there are three competing
conventions for a new renderer to choose from. The line chart rebuilt
the axis machinery, and one function exists twice, character for
character. Consolidate into the shared helpers so a change is made
once.

## The problem in detail

All in `crates/cli/src/render/` unless noted:

**The "Unplaced" footer, four copies and three conventions.**
`bar_chart.rs:68-87`, `line_chart.rs:66-85`, `heatmap.rs:106-125`,
and `workload.rs:81-117` contain the same ~20-line loop (same
exhaustive match over `UnplacedReason`, same explanatory comment,
verbatim). Meanwhile the gantt family renders unplaced as a blockquote
via `mermaid_gantt.rs:111-198`, metric as per-row blockquotes
(`metric.rs:69-91`), and treemap as its own section style
(`treemap.rs:49-59`). A new `UnplacedReason` variant forces edits in
four near-identical places plus two divergent ones. Extract one
helper; decide deliberately which convention each view family keeps.

**Line chart re-implements the shared axis machinery.**
`line_chart.rs:175-247` (`axis_kind_x`, `axis_kind_y`,
`axis_to_f64`, `size_to_f64`) are structural clones of
`svg_chart.rs:188-224` (`axis_kind_for`, `value_to_f64`). Root cause:
core exposes three near-identical value enums (`AggregateValue`,
`AxisValue`, `SizeValue`), so the duration-unit-picking match is
written three times, panics included — a change to duration-unit
policy must be found in three places. The clean fix (a small trait or
enum unification) likely touches core; a CLI-side consolidation is
the cheaper fallback.

**Exact duplicates.**

- `heatmap.rs:251-268` (`compute_extent`) is character-for-character
  `svg_chart.rs:98-115` (`numeric_extent`) — and heatmap already
  imports other helpers from that file.
- `mermaid_gantt.rs:200-213` (`format_titles`) is identical to
  `metric.rs:103-116`; belongs in `markdown.rs` next to the escape
  helpers.

**Repeated phrase-building.** The aggregate label ("count" /
"`{agg}` of `{value}`") is built three ways inside `bar_chart.rs`
(lines 92-99, 116-124, 204-213) and once more in
`heatmap.rs:130-141` — one helper keeps the H1, the table header, and
the axis title from drifting.

**A silent exhaustiveness hole.**
`crates/cli/src/commands/render.rs:212-224`
(`emit_unplaced_warnings`) enumerates nine `ViewData` variants and
swallows the rest with `_ => 0` — contradicting the crate's own
convention of matching exhaustively so new variants fail compilation.
A future chart kind with unplaced items would compile and silently
skip the warning this function exists for.

**Pluralization typo defended by tests.** `mermaid_gantt.rs:164` and
`metric.rs:76` emit "1 items dropped"; tests assert the wrong form
(`gantt.rs:233`, `gantt_by_depth.rs:151`, `metric.rs:354`), and
`commands/render.rs:226-229` repeats the phrase terminal-side.

## Objective

One unplaced-footer helper with a deliberate per-family convention,
axis machinery shared (core enum unification preferred), the exact
duplicates deleted, the aggregate-label phrase built once, the
`_ => 0` catch-all replaced with an exhaustive match, and "1 item
dropped" pluralized correctly with tests updated.

## Out of scope

- Visual redesign of any chart. Snapshot tests pin the output; where
  a convention decision changes output, the change is the decision,
  not a side effect.

Depends on [[view-order-in-extractor]] because that item moves logic
out of `treemap.rs` and `line_chart.rs` first — consolidating around
code that is about to be relocated would be redone work.
