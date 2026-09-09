---
id: test-audit
title: Audit every test block against the rules, delete what fails them, move what is misplaced
status: done
parent: testing-strategy
depends_on: [relocate-operations-tests]
---

## In plain words

Decisions one and two of [[testing-strategy-design]] say what a test is
for and which cases each layer gets. The existing suite was written
before either existed. This item walks the suite against them, largest
block first, and does two things: deletes tests that fail a rule, and
counts before and after so the milestone can say whether it ended with
fewer or better tests than it started with. **Example:**
`view_data/gantt_by_initiative.rs` has 123 lines of product code and
499 lines of tests. Either each of those tests is a distinct placement
or grouping shape, and they stay, or some restate what the gantt tests
below them already prove, and those go. Reading them against the rule
is the work.

The pure moves — the in-file operations tests that belong in
`crates/core/tests/` — are [[relocate-operations-tests]], which runs
first so this item starts from honest in-file numbers.

## What the inventory already found

- **Justified, leave alone:** the large input-space blocks — schema
  parser, `coerce`, query evaluator, views parser, `compute_check`,
  `where_check`, expression typechecker, duration, gantt placement,
  derive, rollup, cycles. Large because their input spaces are.
- **Already moved by [[relocate-operations-tests]]:** the in-file
  operations integration tests, about 3,500 lines. Not this item's
  concern beyond confirming nothing was lost.
- **Look first for excess:** `view_data/gantt_by_initiative.rs` and
  `gantt_by_depth.rs` (about four test lines per product line); the 47
  tests in `crates/server/tests/git_endpoint.rs` (any that assert on
  repository state rather than the HTTP contract); unit tests of glue in
  the `operations/set` submodules.
- **Leave alone:** the git crate's tests. Preview and scope are shared by
  server and CLI and are tested in the crate; commit, pull and push are
  server-only and are tested through the endpoint.

## The rules applied (from decision two)

Delete a test when:

- it is a unit test of glue with one input shape, and an integration
  test in a `tests/` directory runs the same path;
- it restates an assertion a `tests/` file already makes;
- it is a server test asserting on file contents rather than status,
  JSON shape or origin guard;
- it proves the same status code or the same error class as another
  test, for a different input.

Every deletion names, in the commit message, the test that still covers
the behaviour, or states that the behaviour has no user-visible outcome.
A deletion is made because a rule fails, never to reach a number.

## Order of work

1. Record the "before" counts with the method in the design item's
   baseline section. The baseline was taken before the relocation, so
   the per-layer split has shifted; the totals have not.
2. Down the ranked list, one file per commit, largest first. For each
   block: name the function under test, decide input-space or glue,
   then apply the four rules test by test.
3. Record the "after" counts the same way, in [[testing-strategy]].

## Watch out

A deletion that looks like a restatement can be the only test of one
corner the `tests/` file never sends through. Before deleting, find the
integration test that covers the same path and check it reaches the same
branch. If it does not, the test stays, or the case moves into the
integration test first.

## Done when

- Every block in the ranked list has been read against the rules, and
  the item records the verdict per file, one line each.
- Every deletion is justified against a named rule in its commit.
- Before and after counts are in the milestone, taken the same way.

## Verdicts per file (2026-09-09)

Every in-file block and every server test file was read against the
decision-two rules: by me for the blocks the design flagged and for
every deletion candidate plus its covering test, and by three read-only
auditors working the ranked list with the rules as their brief. A
deletion names the rule and the test that still covers the behaviour.

**Deleted (109 tests, 4 more folded into a sibling so their assertions
survive), by file:**

- `core/src/parser/mod.rs`, `parse_work_item` / `split_frontmatter`: 11 deleted, rule 3. `missing_opening_delimiter`, `missing_closing_delimiter`, `frontmatter_is_a_list`, `invalid_yaml`, `body_preserves_markdown_structure`, `parse_empty_frontmatter` restate the `split_*` tests one level down (`parse_work_item` delegates with `?`). `id_starts_with_digit_accepted`, `id_with_digits`, `id_single_letter`, `id_invalid_format_trailing_hyphen`, `id_invalid_format_underscores` take the single `is_valid_id` branch already taken by `id_from_filename` and `id_invalid_format_uppercase`; the exact strings are pinned in `model/work_item.rs::valid_ids` and `invalid_ids`. Kept `empty_file` as the parser's distinct empty-input shape.
- `core/src/parser/template.rs`, `parse_template_content`: 3 deleted. `template_missing_opening_delimiter_errors` (rule 1, the error is `split_frontmatter`'s, pinned by `split_missing_opening_errors`), `template_body_preserved` (rule 3, `template_without_id_parses` asserts the body), `template_with_literal_id_preserves_it` (rule 3, same branch as `template_with_uuid_token_preserves_raw_token`).
- `core/src/parser/config.rs`, `parse_config`: 2 folded (rule 3). The serve-port and no-serve assertions moved into `parse_default_config` and `parse_minimal_config`, which parse the identical inputs.
- `core/src/parser/views.rs`, `view_from_value`: 1 deleted, `view_from_value_builds_validated_view` (rule 3, the round-trip test drives the same branch).
- `core/src/model/weekday.rs`, serde: 2 folded into `rejects_anything_but_a_full_lowercase_name` (rule 3, one unknown-variant branch).
- `core/src/query/format.rs`, `render_delimited`: 5 deleted, rule 2, each with a same-assertion twin in `tests/query.rs`. `delimited_renders_header_and_basic_row` and `delimited_joins_lists_with_list_separator` (`query_tsv_output_has_header_and_tabs`), `delimited_omits_header_when_disabled` (`query_delimited_without_header`), `delimited_errors_on_embedded_separator_in_list_element` (`query_delimited_errors_on_separator_in_list_element`), `delimited_errors_on_delimiter_list_separator_collision` (`query_delimited_errors_on_delimiter_collision`). Kept the CSV quoting and empty-cell tests, which nothing in `tests/` proves.
- `core/src/query/engine.rs`, `execute`: whole block, 6 deleted, rule 1. Glue over `filter_and_sort` plus row building, each with a counterpart in `tests/query.rs` (`query_no_filters_returns_all`, `query_equality_filter`, `query_default_columns_include_id_and_required`, `query_custom_fields`, `query_sort_ascending`, `query_empty_result`).
- `core/src/query/eval.rs`, `matches_predicate`: 3 deleted. `missing_field_no_match` (rule 3, one row of `missing_field_fails_positive_operators`), `not_in_agrees_with_not_equal_on_absent_field` and `presence_check_restores_strict_exclusion` (rule 2, `tests/query.rs::query_not_in_agrees_with_not_equal` and `query_presence_check_restores_strict_exclusion`).
- `core/src/slug.rs`, `slugify`: 2 deleted. `slugify_special_characters` (rule 2, `tests/add.rs::add_slugifies_title_correctly` runs the same title), `slugify_preserves_internal_digits` (rule 3).
- `core/src/walker.rs`, `target_of_link`: 1 deleted, `target_of_link_returns_none_for_links_plural` (rule 3, same `None` arm as the missing-field test; the plural-terminator contract is `walk_up_in_treats_links_plural_as_terminator`).
- `core/src/generators.rs`: 2 deleted. `tokens_resolves_uuid_exact_match` (rule 3, `tokens_uuid_and_today_resolve_without_slug`), `prettify_with_digits` (rule 3, `prettify_simple_slug`).
- `core/src/store/mod.rs`, `Store::load`: 3 deleted, rule 1. `rollup_fills_parent_with_aggregated_value_after_load` (`tests/computed_fields.rs::when_composes_with_aggregate_on_leaves`; the sum itself is `store/rollup.rs`), `pull_fields_schedule_forward_from_files` (`tests/computed_fields.rs::a_required_field_the_pull_fills_raises_nothing`; the cascade is `store/derive.rs::pull_chain_schedules_forward_from_the_root_anchor`), `all_items_iterates_loaded` (`tests/query.rs::query_no_filters_returns_all`). The loader's own rejection matrix stays.
- `core/src/store/derive.rs`: 2 deleted, rule 2. `check_failed_compute_stays_quiet_even_with_error_on_missing` (`tests/computed_fields.rs::check_failed_compute_is_one_schema_diagnostic_without_item_noise`), `pull_missing_input_with_error_on_missing_names_the_dependency` (`an_incomplete_pull_source_reports_only_the_pull_message`).
- `core/src/store/cycles.rs`: 1 deleted, `all_cycle_diagnostics_are_errors` (rule 3, identical fixture to `two_node_cycle`, and it asserted nothing on an empty vector).
- `core/src/rules/mod.rs`, `evaluate`: 9 deleted, rule 2, a near line-for-line twin of `tests/validate_rules.rs`. `l2_rule_violation`, `l2_rule_passes_when_condition_not_met`, `l2_rule_passes_when_assertion_satisfied` (`l2_in_progress_needs_assignee_violation`, `l2_no_violations_when_all_satisfied`), `rule_no_match_applies_to_all` (`rule_without_match_applies_to_all_items`), `l4_count_violation` and `l4_count_passes` (`l4_wip_limit_violation`, `l4_wip_limit_passes`), `l3_forward_link_rule` (`l3_parent_status_check`), `l3_inverse_quantifier_all` (`l3_quantifier_all_children_done`), `warning_severity_propagated` (`warning_severity_does_not_produce_error`). Kept the inverse-table test, the vacuous-quantifier case and the four `evaluate_as_of` tests, which have no integration counterpart.
- `core/src/operations/add.rs`, `derive_slug`: 3 deleted, rule 2 (`tests/add.rs::add_with_explicit_id_uses_custom_filename`, `add_slugifies_title_correctly`, `add_without_id_or_title_errors`).
- `core/src/operations/init.rs`: 2 deleted. `render_config_replaces_project_name` (rule 2, `tests/init.rs::init_creates_project_structure`), `yaml_safe_name_with_colon` (rule 3, same quoting branch as the hash test; `tests/init.rs::init_special_characters_produce_valid_yaml`).
- `core/src/operations/frontmatter_io.rs`: 6 deleted. `build_emits_fields_in_schema_order` (rule 2, `tests/add.rs::add_frontmatter_follows_schema_order`), `build_skips_id_when_not_user_set` (`tests/rename.rs::explicit_id_key_dropped_after_rename`), `build_emits_id_when_user_set` (`tests/set.rs::explicit_id_in_frontmatter_is_preserved_after_set`), `atomic_write_creates_new_file_with_content` and `atomic_write_replaces_existing_file` (rule 1, every add and set integration test), `parse_choice_field_returns_string` (rule 3, one match arm with the string case).
- `core/src/operations/templates.rs`: whole block, 7 deleted, rule 2. Six restated `tests/templates.rs` case for case; the seventh, skipping non-Markdown files, moved there as `list_templates_skips_files_that_are_not_markdown`.
- `cli/src/cli/schema_args.rs`: 3 deleted, rule 3. `list_field_accepts_repeated_flags` and `list_field_accepts_comma_separated` (both inside `list_field_combines_repeat_and_comma`), `links_field_accepts_repeat_and_comma` (Links shares the List arm). Kept `boolean_explicit_true` and `underscore_field_name_preserved`, which are distinct argument shapes.
- `cli/src/commands/serve.rs`: 1 deleted, `empty_serve_section_uses_default` (rule 3, same fallback branch as the no-section test).
- `cli/src/render/svg_chart.rs`: 4 deleted, rule 3, each the same branch as a kept neighbour. `pick_duration_unit_weeks_for_large_values`, `pick_duration_unit_zero_falls_through_to_seconds`, `axis_label_for_date_is_field_name`, `format_axis_tick_duration_drops_decimal_for_integers`.
- `cli/src/render/bar_chart.rs` and `workload.rs`: 1 each deleted, rule 3. The hours-versus-days choice belongs to `svg_chart::pick_duration_unit`, tested there.
- `cli/src/render/board.rs` and `table.rs`: `uses_configured_item_link_base` deleted in each, rule 3. The sibling link test already passes a non-default base and asserts on it.
- `cli/src/render/line_chart.rs`: 1 deleted (`single_series_emits_svg_with_first_palette_color`, rule 3, `palette_walks_the_received_series_order`). `metric.rs`: 2 deleted (`renders_top_heading`, `integer_number_drops_decimal`, rule 3). `tree.rs`: 1 deleted (`full_output_snapshot`, rule 3, every shape in it has its own test).
- `server/src/lib.rs`: 1 deleted, `robots_txt_is_served` (rule 3, same asset-hit branch as `serves_index_at_root`).
- `server/src/timer.rs`, `TimerService`: 10 deleted, rule 2. The design put the timer state machine's assertions at the endpoint, and each of these has an assertion-for-assertion twin in `tests/timer_endpoint.rs`. Kept the four with no counterpart: backwards clock jump, failed pomodoro write, sticky mode, stopwatch stop.
- `server/tests/origin_guard.rs`: 2 deleted, rule 5. The guard is one layer over `/api`, so `foreign_origin_cannot_mutate_an_item` proves it once; `foreign_origin_cannot_stop_the_timer` and `foreign_origin_cannot_touch_views` proved the same 403 for other routes. Its file-content assertion removed (rule 4).
- `server/tests/items_endpoint.rs`: 1 deleted (`unset_clears_the_field`, rule 5, a second success response with the same JSON shape); five file-content assertions removed (rule 4; the writes are proven in `tests/set.rs` and `tests/add.rs`).
- `server/tests/views_endpoint.rs`: 4 deleted. `get_board_view_returns_board_data` and `get_treemap_view_rolls_up_size_into_synthetic_root` (second and third success responses; the content is `core/tests/view_data.rs::extract_exercises_every_variant`), `display_override_unset_roles_inherit_from_view` (restates `core/src/model/views.rs` unit tests), `malformed_display_parameter_returns_422` (rule 5, same 422 as `views_write_endpoint.rs::preview_with_malformed_filter_returns_422`).
- `server/tests/views_write_endpoint.rs`: 5 deleted. `create_view_blank_name_returns_422`, `malformed_display_on_unrenderable_view_returns_422`, `preview_with_arity_mismatched_condition_returns_422` (rule 5, same status on the same endpoint as a kept test), `config_display_defaults_inherited_by_bare_view` and `view_display_beats_config_defaults` (restate `core/src/model/views.rs` unit tests). Eighteen file-content assertions removed (rule 4), most of them "file must be untouched" after a rejection, which `core/tests/view_write.rs` proves per operation.

**Kept, input-space blocks, no candidate survived reading:** `core/src/parser/{schema,resources}.rs`, `expression/{lexer,parser,typecheck,evaluate}.rs`, `model/{config,views,schema,view_slots,field_value,message,diagnostic,color,work_item,duration,calendar}.rs`, `query/{parse,sort,types,clause}.rs`, `resolve.rs`, `store/{rollup,compute}.rs`, `rules/{condition,assertion}.rs`, `views_check/tests.rs` (two debatable overlaps with `tests/validate_views.rs` kept for the slot-by-kind matrix), `where_check.rs`, `compute_check.rs`, `config_check.rs`, `coerce.rs`, `change_summary.rs`, `schema_data.rs`, `item_data.rs`, `timer_data.rs`, `operations/{install_hooks,rename}.rs`, every `view_data/*.rs` (the two gantt-by files are nine and seven distinct grouping shapes; the four-to-one ratio is fixture verbosity), `cli/src/render/{description,gantt,gantt_by_depth,gantt_by_initiative,graph,heatmap,treemap}.rs`, `cli/src/main.rs`, `server/src/{origin,watcher}.rs`, `server/src/api/events.rs`, `git/src/{lib,scope}.rs`.

**Kept, server contract files:** `git_endpoint.rs` and `timer_endpoint.rs` (the server-owned exception; the git file's two repository-state reads are inside the pull flow), `project_endpoint.rs`, `schema_endpoint.rs`, `watcher_live.rs`.

**Gaps noticed, not this item's to fill:** `walker::walk_up` has no direct test; `model/field_value.rs` formats 4 of 12 variants under test; `cli/src/render/markdown.rs` and `mermaid_gantt.rs` have no test block; `schema_args.rs` has no `Multichoice` case; the port scan in `serve.rs` is untested.

## Counts (2026-09-09, same method as the baseline)

| Layer | Baseline | After |
|---|---|---|
| Rust unit, in-file | 32,300 | 26,161 |
| Core integration, `crates/core/tests/` | 5,300 | 9,183 |
| Server integration, `crates/server/tests/` | 4,700 | 4,346 |
| CLI binary, `crates/cli/tests/` | 0 | 1,374 |
| Web app unit | 2,100 | 2,066 |
| **Total** | **44,400** | **43,130** |

| | Baseline | After |
|---|---|---|
| Rust test functions | 1,960 | 1,939 (88 added, 109 removed) |
| Web app test functions | 207 | 207 |

Gate after the audit: fmt, clippy over all targets, doc, the full
workspace test run (1,939 passed), Vitest (207), and the completeness
check, all green in the dev container.
