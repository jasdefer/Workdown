//! Integration tests for computed fields through the full project
//! loader: constants defined in `resources.yaml` reach the store's
//! derive pass, and a check-failed compute config surfaces as exactly
//! one schema diagnostic with no per-item noise.

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
