---
id: query-value-consolidation
status: to_do
title: Deduplicate the filter engine's comparison and formatting logic
parent: maintenance-review-2026-08
---

## In plain words

The filter engine spells out "is this value greater / less / equal?"
five separate times — once per data type — even though only the value
extraction differs. And "turn a field value into text" exists in four
places that must all agree, with nothing forcing them to: a new kind
of field value means four manual edits. On top of that, when a filter
uses a regular expression, the pattern is recompiled from scratch for
every single item on every evaluation. Consolidate into shared
helpers and compile the pattern once.

## The problem in detail

All in `crates/core/src/query/`:

**Five near-identical typed evaluators** (`eval.rs`):
`eval_integer` (line 197), `eval_float` (223), and `eval_duration`
(251) are the same twelve-arm operator match three times, differing
only in value extraction and right-hand-side parsing; `eval_list`
(331) and `eval_links` (357) are identical modulo element extraction.
Roughly 80 duplicated lines; adding an operator or changing the
absent-value rule touches five sites. A generic
`eval_ordered<T: PartialOrd>` plus one collection helper collapses
them.

**Four hand-copies of field-value stringification**:
`eval.rs:383` (`extract_string`) and `sort.rs:175`
(`extract_sort_string`) are verbatim re-implementations of
`crates/core/src/model/field_value.rs:85` (`format_field_value`) —
all thirteen variants, same date format, same `", "` joins;
`format.rs:164` (`format_value_delimited`) re-enumerates the variants
again (its embedded-separator error is legitimately different, but
the scalar arms could delegate). The compiler forces the match arms
to exist but not the formatting to agree.

**The `/pattern/flags` regex convention is split across three files
and recompiles per item**: `parse.rs:334` encodes the validated regex
back into `Comparison.value` as a string; `eval.rs:425`
(`parse_regex_value`) decodes and `compile_regex` recompiles it *per
item per evaluation*; `clause.rs:151` re-appends it on serialization.
A typed operand on `Comparison` (or a cached compiled regex) gives
the convention one home.

**Two small nits while in the module**:

- `format.rs:49` — `DelimitedError` hand-rolls `Display`/`Error`; the
  only error type in the slice not on `thiserror`.
- `parse.rs:317` — the comment "Validate the field name (reject
  related fields)" says the opposite of what `validate_field_name`
  does (it only rejects empty names; related-field regexes parse fine
  and are presumably intended to).

## Objective

One ordered-comparison helper, one collection helper, all
stringification delegating to `format_field_value`, regexes carried
typed or cached, and the two nits fixed. No public shape changes; the
existing round-trip and operator tests pin behavior.

## Out of scope

- New operators or any change to filter semantics.
- Merging the query language with the expression language — reviewed
  and rejected: they share nothing at the language layer and should
  not (typed per-item arithmetic vs store-wide clause matching); the
  worthwhile sharing is the value layer above.
