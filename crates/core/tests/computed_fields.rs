//! Integration tests for derived fields through the full project
//! loader: constants defined in `resources.yaml` reach the store's
//! derive pass, a check-failed compute config surfaces as exactly one
//! schema diagnostic with no per-item noise, and the required-field
//! check agrees with itself across the coercion/derive seam that the
//! in-memory derive tests bypass.

use std::fs;
use std::path::{Path, PathBuf};

use chrono::NaiveDate;
use tempfile::TempDir;
use workdown_core::model::diagnostic::DiagnosticBody;
use workdown_core::model::FieldValue;
use workdown_core::parser::config::load_config;
use workdown_core::project::{load_project, Project};

// ── Test fixtures ───────────────────────────────────────────────────

const CONFIG_YAML: &str = "\
project:
  name: Test Project
  description: ''
paths:
  work_items: workdown-items
  templates: .workdown/templates
  resources: .workdown/resources.yaml
  views: .workdown/views.yaml
schema: .workdown/schema.yaml
defaults:
  board_field: status
  tree_field: parent
  graph_field: depends_on
";

/// The fields the config's defaults reference, shared by every schema
/// fixture below.
const COMMON_FIELDS: &str = "\
fields:
  status:
    type: choice
    values: [open, done]
  parent:
    type: link
    allow_cycles: false
  depends_on:
    type: links
    allow_cycles: true
";

fn setup_project(
    schema_yaml: &str,
    resources_yaml: &str,
    items: &[(&str, &str)],
) -> (TempDir, PathBuf) {
    let directory = TempDir::new().unwrap();
    let root = directory.path().to_path_buf();

    fs::create_dir_all(root.join(".workdown")).unwrap();
    fs::create_dir_all(root.join("workdown-items")).unwrap();

    fs::write(root.join(".workdown/config.yaml"), CONFIG_YAML).unwrap();
    fs::write(root.join(".workdown/schema.yaml"), schema_yaml).unwrap();
    fs::write(root.join(".workdown/resources.yaml"), resources_yaml).unwrap();

    for (file_name, content) in items {
        fs::write(root.join("workdown-items").join(file_name), content).unwrap();
    }

    (directory, root)
}

fn load(root: &Path) -> Project {
    let config = load_config(&root.join(".workdown/config.yaml")).unwrap();
    load_project(&config, root, Path::new(".workdown/config.yaml"), None).unwrap()
}

fn load_as_of(root: &Path, evaluation_date: NaiveDate) -> Project {
    let config = load_config(&root.join(".workdown/config.yaml")).unwrap();
    load_project(
        &config,
        root,
        Path::new(".workdown/config.yaml"),
        Some(evaluation_date),
    )
    .unwrap()
}

// ── Tests ───────────────────────────────────────────────────────────

#[test]
fn constants_reach_computed_fields_through_the_project_loader() {
    let schema_yaml = format!(
        "{COMMON_FIELDS}  effort:
    type: duration
  cost:
    type: float
    compute: effort / $constants.work_hours_per_day * $constants.daily_rate
"
    );
    let resources_yaml = "\
constants:
  daily_rate:
    type: float
    value: 800
  work_hours_per_day:
    type: duration
    value: \"8h\"
";
    let (_directory, root) = setup_project(
        &schema_yaml,
        resources_yaml,
        &[("task.md", "---\neffort: 16h\n---\nBody.\n")],
    );

    let project = load(&root);

    assert!(
        project.diagnostics.is_empty(),
        "got: {:?}",
        project.diagnostics
    );
    // 16h of effort at 8h per day and a daily rate of 800 costs 1600.
    let task = project.store.get("task").expect("task must load");
    assert_eq!(task.fields.get("cost"), Some(&FieldValue::Float(1600.0)));
}

#[test]
fn check_failed_compute_is_one_schema_diagnostic_without_item_noise() {
    // The typo'd reference must surface once, against schema.yaml —
    // not per item, despite `error_on_missing: true`.
    let schema_yaml = format!(
        "{COMMON_FIELDS}  start_date:
    type: date
  end_date:
    type: date
    compute:
      expression: strat_date + duration
      error_on_missing: true
"
    );
    let (_directory, root) = setup_project(
        &schema_yaml,
        "",
        &[
            ("task-a.md", "---\nstart_date: 2026-01-05\n---\n"),
            ("task-b.md", "---\nstart_date: 2026-01-19\n---\n"),
        ],
    );

    let project = load(&root);

    assert_eq!(
        project.diagnostics.len(),
        1,
        "got: {:?}",
        project.diagnostics
    );
    assert!(
        matches!(project.diagnostics[0].body, DiagnosticBody::Config(_)),
        "got: {:?}",
        project.diagnostics[0]
    );
    for id in ["task-a", "task-b"] {
        let item = project.store.get(id).expect("item must load");
        assert_eq!(item.fields.get("end_date"), None);
    }
}

// ── $today at evaluation time ───────────────────────────────────────

/// Schema with a `days_remaining` field computed from `$today` — the
/// motivating case in `evaluation-time-now`, needing no grammar beyond
/// the existing `date - date → duration` rule.
fn days_remaining_schema() -> String {
    format!(
        "{COMMON_FIELDS}  end_date:
    type: date
  days_remaining:
    type: duration
    compute: end_date - $today
"
    )
}

#[test]
fn today_resolves_to_the_pinned_date() {
    let (_directory, root) = setup_project(
        &days_remaining_schema(),
        "",
        &[("task.md", "---\nend_date: 2026-08-10\n---\n")],
    );

    let pinned = NaiveDate::from_ymd_opt(2026, 8, 3).unwrap();
    let project = load_as_of(&root, pinned);

    assert!(
        project.diagnostics.is_empty(),
        "got: {:?}",
        project.diagnostics
    );
    assert_eq!(project.evaluation_date, pinned);
    let task = project.store.get("task").expect("task must load");
    // 2026-08-10 minus 2026-08-03 is 7 days, as canonical seconds.
    assert_eq!(
        task.fields.get("days_remaining"),
        Some(&FieldValue::Duration(7 * 86_400))
    );
}

#[test]
fn a_past_end_date_yields_a_negative_duration() {
    let (_directory, root) = setup_project(
        &days_remaining_schema(),
        "",
        &[("task.md", "---\nend_date: 2026-08-01\n---\n")],
    );

    let project = load_as_of(&root, NaiveDate::from_ymd_opt(2026, 8, 3).unwrap());

    let task = project.store.get("task").expect("task must load");
    assert_eq!(
        task.fields.get("days_remaining"),
        Some(&FieldValue::Duration(-2 * 86_400))
    );
}

#[test]
fn two_pinned_loads_derive_identical_values() {
    let (_directory, root) = setup_project(
        &days_remaining_schema(),
        "",
        &[
            ("task-a.md", "---\nend_date: 2026-08-10\n---\n"),
            ("task-b.md", "---\nend_date: 2026-09-01\n---\n"),
        ],
    );

    let pinned = NaiveDate::from_ymd_opt(2026, 8, 3).unwrap();
    let first = load_as_of(&root, pinned);
    let second = load_as_of(&root, pinned);

    for id in ["task-a", "task-b"] {
        assert_eq!(
            first.store.get(id).unwrap().fields.get("days_remaining"),
            second.store.get(id).unwrap().fields.get("days_remaining"),
            "{id}"
        );
    }
}

#[test]
fn a_hand_written_value_beats_the_today_computation() {
    let (_directory, root) = setup_project(
        &days_remaining_schema(),
        "",
        &[(
            "task.md",
            "---\nend_date: 2026-08-10\ndays_remaining: \"99d\"\n---\n",
        )],
    );

    let project = load_as_of(&root, NaiveDate::from_ymd_opt(2026, 8, 3).unwrap());

    let task = project.store.get("task").expect("task must load");
    assert_eq!(
        task.fields.get("days_remaining"),
        Some(&FieldValue::Duration(99 * 86_400))
    );
}

#[test]
fn boolean_fields_compute_from_predicates() {
    let schema_yaml = format!(
        "{COMMON_FIELDS}  end_date:
    type: date
  is_overdue:
    type: boolean
    compute: end_date < $today
  is_done:
    type: boolean
    compute: status == \"done\"
"
    );
    let (_directory, root) = setup_project(
        &schema_yaml,
        "",
        &[
            ("late.md", "---\nstatus: open\nend_date: 2026-07-01\n---\n"),
            (
                "upcoming.md",
                "---\nstatus: done\nend_date: 2026-09-01\n---\n",
            ),
        ],
    );

    let project = load_as_of(&root, NaiveDate::from_ymd_opt(2026, 8, 3).unwrap());

    assert!(
        project.diagnostics.is_empty(),
        "got: {:?}",
        project.diagnostics
    );
    let late = project.store.get("late").expect("late must load");
    assert_eq!(
        late.fields.get("is_overdue"),
        Some(&FieldValue::Boolean(true))
    );
    assert_eq!(
        late.fields.get("is_done"),
        Some(&FieldValue::Boolean(false))
    );
    let upcoming = project.store.get("upcoming").expect("upcoming must load");
    assert_eq!(
        upcoming.fields.get("is_overdue"),
        Some(&FieldValue::Boolean(false))
    );
    assert_eq!(
        upcoming.fields.get("is_done"),
        Some(&FieldValue::Boolean(true))
    );
}

// ── when: conditional fields ────────────────────────────────────────

/// The acceptance example from `conditional-field-value`: a color
/// picked by first matching condition, with an evaluated fallback.
fn urgency_color_schema() -> String {
    format!(
        "{COMMON_FIELDS}  end_date:
    type: date
  urgency_color:
    type: color
    when:
      - if: status == \"done\"
        then: green
      - if: end_date < $today
        then: red
    default: gray
"
    )
}

fn color_of(project: &Project, id: &str) -> Option<FieldValue> {
    project
        .store
        .get(id)
        .expect("item must load")
        .fields
        .get("urgency_color")
        .cloned()
}

#[test]
fn when_picks_the_first_matching_branch() {
    let (_directory, root) = setup_project(
        &urgency_color_schema(),
        "",
        &[
            // done AND overdue: first branch wins.
            ("done.md", "---\nstatus: done\nend_date: 2026-07-01\n---\n"),
            ("late.md", "---\nstatus: open\nend_date: 2026-07-01\n---\n"),
            (
                "upcoming.md",
                "---\nstatus: open\nend_date: 2026-09-01\n---\n",
            ),
        ],
    );

    let project = load_as_of(&root, NaiveDate::from_ymd_opt(2026, 8, 3).unwrap());

    assert!(
        project.diagnostics.is_empty(),
        "got: {:?}",
        project.diagnostics
    );
    assert_eq!(
        color_of(&project, "done"),
        Some(FieldValue::Color("green".to_owned()))
    );
    assert_eq!(
        color_of(&project, "late"),
        Some(FieldValue::Color("red".to_owned()))
    );
    assert_eq!(
        color_of(&project, "upcoming"),
        Some(FieldValue::Color("gray".to_owned()))
    );
}

#[test]
fn when_branch_with_absent_input_falls_through() {
    let (_directory, root) = setup_project(
        &urgency_color_schema(),
        "",
        &[
            // No status: branch 1 cannot be answered, branch 2 catches it.
            ("late.md", "---\nend_date: 2026-07-01\n---\n"),
            // Neither status nor end_date: the default catches it.
            ("blank.md", "---\ntitle: Blank\n---\n"),
        ],
    );

    let project = load_as_of(&root, NaiveDate::from_ymd_opt(2026, 8, 3).unwrap());

    assert_eq!(
        color_of(&project, "late"),
        Some(FieldValue::Color("red".to_owned()))
    );
    assert_eq!(
        color_of(&project, "blank"),
        Some(FieldValue::Color("gray".to_owned()))
    );
}

#[test]
fn required_when_aggregate_non_leaf_gets_the_classic_message() {
    // The conditional pass is leaves-only when the field also
    // aggregates, so a parent's branches never ran — "no 'when:' branch
    // matched" would be false there. The parent gets the classic
    // missing-required message; the child, whose branches genuinely all
    // failed, keeps the specific one.
    let schema_yaml = format!(
        "{COMMON_FIELDS}  review_date:
    type: date
    required: true
    when:
      - if: status == \"done\"
        then: 2026-01-01
    aggregate:
      function: max
"
    );
    let (_directory, root) = setup_project(
        &schema_yaml,
        "",
        &[
            ("epic.md", "---\nstatus: open\n---\n"),
            ("task.md", "---\nstatus: open\nparent: epic\n---\n"),
        ],
    );

    let project = load(&root);

    let message_for = |file_name: &str| -> String {
        let matches: Vec<_> = project
            .diagnostics
            .iter()
            .filter(|diagnostic| {
                diagnostic
                    .source_path()
                    .is_some_and(|path| path.ends_with(file_name))
            })
            .collect();
        assert_eq!(matches.len(), 1, "{file_name}: {:?}", project.diagnostics);
        matches[0].message.clone()
    };

    assert!(
        message_for("epic.md").contains("required field 'review_date' is missing"),
        "the non-leaf must not be told its branches failed"
    );
    assert!(message_for("task.md").contains("no 'when:' branch matched"));
}

#[test]
fn when_branch_with_failing_condition_warns_and_falls_through() {
    // A condition that fails *at runtime* on this item's actual values
    // (here: division by a zero duration) must warn, skip the branch,
    // and keep going — later branches and the default still apply.
    let schema_yaml = format!(
        "{COMMON_FIELDS}  spent:
    type: duration
  estimate:
    type: duration
  flag:
    type: color
    when:
      - if: spent / estimate > 0.5
        then: red
      - if: spent > estimate
        then: blue
    default: green
"
    );
    let (_directory, root) = setup_project(
        &schema_yaml,
        "",
        &[("task.md", "---\nspent: 2h\nestimate: 0s\n---\n")],
    );

    let project = load(&root);

    let failures: Vec<_> = project
        .diagnostics
        .iter()
        .filter(|diagnostic| diagnostic.message.contains("branch 1 condition failed"))
        .collect();
    assert_eq!(failures.len(), 1, "got: {:?}", project.diagnostics);

    // Branch 2 still matched: the failure cost one branch, not the field.
    let task = project.store.get("task").expect("task must load");
    assert_eq!(
        task.fields.get("flag"),
        Some(&FieldValue::Color("blue".to_owned()))
    );
}

#[test]
fn a_hand_written_value_beats_every_branch() {
    let (_directory, root) = setup_project(
        &urgency_color_schema(),
        "",
        &[(
            "done.md",
            "---\nstatus: done\nurgency_color: \"#123456\"\n---\n",
        )],
    );

    let project = load_as_of(&root, NaiveDate::from_ymd_opt(2026, 8, 3).unwrap());

    assert_eq!(
        color_of(&project, "done"),
        Some(FieldValue::Color("#123456".to_owned()))
    );
}

#[test]
fn required_when_without_default_reports_the_unmatched_item() {
    let schema_yaml = format!(
        "{COMMON_FIELDS}  urgency_color:
    type: color
    required: true
    when:
      - if: status == \"done\"
        then: green
"
    );
    let (_directory, root) = setup_project(
        &schema_yaml,
        "",
        &[
            ("open.md", "---\nstatus: open\n---\n"),
            ("no-status.md", "---\ntitle: X\n---\n"),
        ],
    );

    let project = load_as_of(&root, NaiveDate::from_ymd_opt(2026, 8, 3).unwrap());

    let unmatched: Vec<&workdown_core::model::diagnostic::Diagnostic> = project
        .diagnostics
        .iter()
        .filter(|diagnostic| diagnostic.message.contains("no 'when:' branch matched"))
        .collect();
    assert_eq!(unmatched.len(), 2, "got: {:?}", project.diagnostics);
    // The item without a status has the absent input named — the reason
    // the branch could not be answered.
    assert!(project.diagnostics.iter().any(|diagnostic| diagnostic
        .message
        .contains("absent condition inputs: 'status'")));
}

#[test]
fn when_composes_with_aggregate_on_leaves() {
    let schema_yaml = format!(
        "{COMMON_FIELDS}  end_date:
    type: date
  priority_score:
    type: integer
    when:
      - if: end_date < $today
        then: 50
    default: 10
    aggregate:
      function: max
"
    );
    let (_directory, root) = setup_project(
        &schema_yaml,
        "",
        &[
            ("epic.md", "---\ntitle: Epic\n---\n"),
            (
                "late-child.md",
                "---\nparent: epic\nend_date: 2026-07-01\n---\n",
            ),
            ("calm-child.md", "---\nparent: epic\n---\n"),
        ],
    );

    let project = load_as_of(&root, NaiveDate::from_ymd_opt(2026, 8, 3).unwrap());

    let score = |id: &str| {
        project
            .store
            .get(id)
            .unwrap()
            .fields
            .get("priority_score")
            .cloned()
    };
    // Leaves get their conditional score; the parent aggregates the max
    // instead of evaluating conditions of its own.
    assert_eq!(score("late-child"), Some(FieldValue::Integer(50)));
    assert_eq!(score("calm-child"), Some(FieldValue::Integer(10)));
    assert_eq!(score("epic"), Some(FieldValue::Integer(50)));
}

#[test]
fn when_conditions_read_computed_fields() {
    let schema_yaml = format!(
        "{COMMON_FIELDS}  end_date:
    type: date
  days_remaining:
    type: duration
    compute: end_date - $today
  urgency_color:
    type: color
    when:
      - if: days_remaining < $constants.warning_threshold
        then: red
    default: gray
"
    );
    let resources_yaml = "\
constants:
  warning_threshold:
    type: duration
    value: \"1w\"
";
    let (_directory, root) = setup_project(
        &schema_yaml,
        resources_yaml,
        &[
            ("soon.md", "---\nend_date: 2026-08-05\n---\n"),
            ("far.md", "---\nend_date: 2026-10-01\n---\n"),
        ],
    );

    let project = load_as_of(&root, NaiveDate::from_ymd_opt(2026, 8, 3).unwrap());

    assert!(
        project.diagnostics.is_empty(),
        "got: {:?}",
        project.diagnostics
    );
    assert_eq!(
        color_of(&project, "soon"),
        Some(FieldValue::Color("red".to_owned()))
    );
    assert_eq!(
        color_of(&project, "far"),
        Some(FieldValue::Color("gray".to_owned()))
    );
}

#[test]
fn without_an_override_the_evaluation_date_is_today() {
    let (_directory, root) = setup_project(
        &days_remaining_schema(),
        "",
        &[("task.md", "---\nend_date: 2026-08-10\n---\n")],
    );

    let project = load(&root);

    // Sandwich the load between two clock reads instead of asserting an
    // exact date, so the test survives a midnight rollover mid-run.
    let after = workdown_core::generators::current_local_date();
    assert!(project.evaluation_date <= after);
    assert!(
        after
            .signed_duration_since(project.evaluation_date)
            .num_days()
            .abs()
            <= 1
    );
}

// ── Required + pull, across the coercion/derive seam ─────────────────
//
// These three cross the seam the derive unit tests bypass by
// construction: they build items in memory, so `coerce_fields` — which
// owns the early half of the required-field check — never runs. Every
// case below turns on the two halves agreeing about `pull`.
// See `validation-phase-boundaries` for the standing question of
// whether that check should be split at all.

/// A required date pulled one hop forward over `parent`. `depends_on`
/// cannot carry a pull — the config demands `allow_cycles: false`.
fn pulled_target_date_schema(error_on_missing: bool) -> String {
    format!(
        "{COMMON_FIELDS}  target_date:
    type: date
    required: true
    pull:
      over: parent
      field: target_date
      function: max
      error_on_missing: {error_on_missing}
"
    )
}

/// Every diagnostic raised against one item file, in load order.
fn messages_for(project: &Project, file_name: &str) -> Vec<String> {
    project
        .diagnostics
        .iter()
        .filter(|diagnostic| {
            diagnostic
                .source_path()
                .is_some_and(|path| path.ends_with(file_name))
        })
        .map(|diagnostic| diagnostic.message.clone())
        .collect()
}

#[test]
fn a_required_field_the_pull_fills_raises_nothing() {
    // The regression: coercion used to raise `MissingRequired` here
    // before the pull pass ran, so a field that ends up correctly
    // filled was still reported missing.
    let (_directory, root) = setup_project(
        &pulled_target_date_schema(false),
        "",
        &[
            ("epic.md", "---\ntarget_date: 2026-03-01\n---\n"),
            ("task.md", "---\nparent: epic\n---\n"),
        ],
    );

    let project = load(&root);

    assert!(
        project.diagnostics.is_empty(),
        "got: {:?}",
        project.diagnostics
    );
    let task = project.store.get("task").expect("task must load");
    assert_eq!(
        task.fields.get("target_date"),
        Some(&FieldValue::Date(
            NaiveDate::from_ymd_opt(2026, 3, 1).unwrap()
        ))
    );
}

#[test]
fn a_required_field_the_pull_cannot_fill_is_reported_once() {
    // An unanchored root: nothing to pull from and no hand-written
    // value. One complaint, from the post-derive half — not one from
    // each half.
    let (_directory, root) = setup_project(
        &pulled_target_date_schema(false),
        "",
        &[("root.md", "---\nstatus: open\n---\n")],
    );

    let project = load(&root);

    assert_eq!(
        messages_for(&project, "root.md"),
        vec!["required field 'target_date' is missing".to_owned()],
        "got: {:?}",
        project.diagnostics
    );
}

#[test]
fn an_incomplete_pull_source_reports_only_the_pull_message() {
    // The parent exists but has no `target_date`, so the pull has an
    // incomplete input to name. The child must get that specific
    // message alone — not the generic missing-required one as well.
    // The parent, unanchored, keeps its own generic message.
    let (_directory, root) = setup_project(
        &pulled_target_date_schema(true),
        "",
        &[
            ("epic.md", "---\nstatus: open\n---\n"),
            ("task.md", "---\nparent: epic\n---\n"),
        ],
    );

    let project = load(&root);

    assert_eq!(
        messages_for(&project, "task.md"),
        vec![
            "pull field 'target_date' could not be evaluated: missing 'epic.target_date'"
                .to_owned()
        ],
        "got: {:?}",
        project.diagnostics
    );
    assert_eq!(
        messages_for(&project, "epic.md"),
        vec!["required field 'target_date' is missing".to_owned()],
        "got: {:?}",
        project.diagnostics
    );
}

// ── The consolidated required check (validation-phase-boundaries) ────
//
// One check, after the fill-in phase, consulting coercion's record of
// written-but-invalid fields. These tests pin the three behaviors the
// consolidation decided: no false "missing" on top of an invalid
// value, no fill-in overriding a broken hand-written value, and
// item-first report ordering.

#[test]
fn a_written_but_invalid_required_field_reports_only_the_invalid_value() {
    // "Written but invalid" and "never written" both end up as an
    // absent key after coercion drops the broken value. Only the
    // failure record lets the required check tell them apart — without
    // it, this file would get a second, false, "missing" complaint.
    let schema_yaml = format!(
        "{COMMON_FIELDS}  target_date:
    type: date
    required: true
"
    );
    let (_directory, root) = setup_project(
        &schema_yaml,
        "",
        &[("task.md", "---\ntarget_date: not-a-date\n---\n")],
    );

    let project = load(&root);

    assert_eq!(
        messages_for(&project, "task.md"),
        vec![
            "field 'target_date': 'not-a-date' is not a valid date (expected YYYY-MM-DD)"
                .to_owned()
        ],
        "got: {:?}",
        project.diagnostics
    );
}

#[test]
fn a_written_but_invalid_computed_field_is_not_filled_over() {
    // The author wrote a value; that it failed conversion means the
    // file must be fixed — not that the compute pass may quietly
    // replace it. The field stays absent and the only complaint is
    // about the written value.
    let schema_yaml = format!(
        "{COMMON_FIELDS}  duration:
    type: duration
  end_date:
    type: date
    required: true
    compute: start_date + duration
  start_date:
    type: date
"
    );
    let (_directory, root) = setup_project(
        &schema_yaml,
        "",
        &[(
            "task.md",
            "---\nstart_date: 2026-03-01\nduration: 5d\nend_date: not-a-date\n---\n",
        )],
    );

    let project = load(&root);

    let task = project.store.get("task").expect("task must load");
    assert_eq!(task.fields.get("end_date"), None, "must not be filled");
    assert_eq!(
        messages_for(&project, "task.md"),
        vec!["field 'end_date': 'not-a-date' is not a valid date (expected YYYY-MM-DD)".to_owned()],
        "got: {:?}",
        project.diagnostics
    );
}

#[test]
fn an_invalid_value_on_an_aggregating_ancestor_is_not_overwritten() {
    // The middle item's hand-written (broken) effort must not be
    // replaced by the rollup — but its child's contribution still
    // passes through to the grandparent, so one broken file does not
    // cut its subtree off from the rest of the tree.
    let schema_yaml = format!(
        "{COMMON_FIELDS}  effort:
    type: integer
    aggregate:
      function: sum
      over: parent
"
    );
    let (_directory, root) = setup_project(
        &schema_yaml,
        "",
        &[
            ("epic.md", "---\nstatus: open\n---\n"),
            ("story.md", "---\nparent: epic\neffort: broken\n---\n"),
            ("task.md", "---\nparent: story\neffort: 3\n---\n"),
        ],
    );

    let project = load(&root);

    let story = project.store.get("story").expect("story must load");
    assert_eq!(story.fields.get("effort"), None, "must not be filled");
    let epic = project.store.get("epic").expect("epic must load");
    assert_eq!(epic.fields.get("effort"), Some(&FieldValue::Integer(3)));
    assert_eq!(messages_for(&project, "story.md").len(), 1);
    assert!(messages_for(&project, "story.md")[0].starts_with("field 'effort':"));
}

#[test]
fn missing_required_findings_are_ordered_item_first() {
    // The consolidated check reports by item, then by schema
    // declaration order within the item — users fix files, not schema
    // fields. Pinned because consolidation changed this order (the old
    // late half reported field-by-field) and the changelog documents
    // the new one.
    let schema_yaml = format!(
        "{COMMON_FIELDS}  owner:
    type: string
    required: true
  target_date:
    type: date
    required: true
"
    );
    let (_directory, root) = setup_project(
        &schema_yaml,
        "",
        &[
            ("alpha.md", "---\nstatus: open\n---\n"),
            ("beta.md", "---\nstatus: open\n---\n"),
        ],
    );

    let project = load(&root);

    let required_messages: Vec<(String, String)> = project
        .diagnostics
        .iter()
        .filter(|diagnostic| diagnostic.message.contains("required field"))
        .map(|diagnostic| {
            let file_name = diagnostic
                .source_path()
                .and_then(|path| path.file_name())
                .map(|name| name.to_string_lossy().into_owned())
                .unwrap_or_default();
            (file_name, diagnostic.message.clone())
        })
        .collect();
    assert_eq!(
        required_messages,
        vec![
            (
                "alpha.md".to_owned(),
                "required field 'owner' is missing".to_owned()
            ),
            (
                "alpha.md".to_owned(),
                "required field 'target_date' is missing".to_owned()
            ),
            (
                "beta.md".to_owned(),
                "required field 'owner' is missing".to_owned()
            ),
            (
                "beta.md".to_owned(),
                "required field 'target_date' is missing".to_owned()
            ),
        ],
        "got: {:?}",
        project.diagnostics
    );
}
