//! Integration tests for rule evaluation in `workdown validate`.
//!
//! Each test sets up a temp directory with a schema (including rules) and
//! work item files, loads via the public Store + Schema API, and checks
//! that the expected diagnostics are produced.

use std::fs;

use tempfile::TempDir;
use workdown_core::model::diagnostic::DiagnosticKind;
use workdown_core::model::schema::Severity;
use workdown_core::parser::schema::parse_schema;
use workdown_core::rules::evaluate;
use workdown_core::store::Store;

/// Create a temp directory and write work item files into it.
fn setup(items: Vec<(&str, &str)>) -> (TempDir, std::path::PathBuf) {
    let dir = TempDir::new().unwrap();
    let path = dir.path().to_path_buf();
    for (name, content) in items {
        fs::write(path.join(name), content).unwrap();
    }
    (dir, path)
}

// ── L2: Cross-field rules ───────────────────────────────────────────

#[test]
fn l2_in_progress_needs_assignee_violation() {
    let schema = parse_schema(
        "\
fields:
  status:
    type: choice
    values: [open, in_progress, done]
    required: true
  assignee:
    type: string
rules:
  - name: in-progress-needs-assignee
    description: Work items in progress must have an assignee
    match:
      status: in_progress
    require:
      assignee: required
",
    )
    .unwrap();

    let (_dir, path) = setup(vec![
        ("task-a.md", "---\nstatus: in_progress\n---\nNo assignee!\n"),
        (
            "task-b.md",
            "---\nstatus: in_progress\nassignee: alice\n---\n",
        ),
        ("task-c.md", "---\nstatus: open\n---\n"),
    ]);

    let store = Store::load(&path, &schema).unwrap();
    let diags = evaluate(&store, &schema);

    // Only task-a should violate (in_progress + no assignee).
    let violations: Vec<_> = diags
        .iter()
        .filter(|d| matches!(&d.kind, DiagnosticKind::RuleViolation { .. }))
        .collect();
    assert_eq!(violations.len(), 1);

    match &violations[0].kind {
        DiagnosticKind::RuleViolation {
            item_id,
            rule,
            detail,
            ..
        } => {
            assert_eq!(item_id, "task-a");
            assert_eq!(rule, "in-progress-needs-assignee");
            assert!(detail.contains("assignee"));
            assert!(detail.contains("required"));
        }
        _ => unreachable!(),
    }
    assert_eq!(violations[0].severity, Severity::Error);
}

#[test]
fn l2_bugs_need_priority_violation() {
    let schema = parse_schema(
        "\
fields:
  status:
    type: choice
    values: [open, done]
    required: true
  type_field:
    type: choice
    values: [task, bug]
  priority:
    type: choice
    values: [high, medium, low]
rules:
  - name: bugs-need-priority
    match:
      type_field: bug
    require:
      priority: required
",
    )
    .unwrap();

    let (_dir, path) = setup(vec![
        ("bug-a.md", "---\nstatus: open\ntype_field: bug\n---\n"),
        (
            "bug-b.md",
            "---\nstatus: open\ntype_field: bug\npriority: high\n---\n",
        ),
        ("task-a.md", "---\nstatus: open\ntype_field: task\n---\n"),
    ]);

    let store = Store::load(&path, &schema).unwrap();
    let diags = evaluate(&store, &schema);

    let violations: Vec<_> = diags
        .iter()
        .filter(|d| matches!(&d.kind, DiagnosticKind::RuleViolation { .. }))
        .collect();
    assert_eq!(violations.len(), 1);

    match &violations[0].kind {
        DiagnosticKind::RuleViolation { item_id, .. } => {
            assert_eq!(item_id, "bug-a");
        }
        _ => unreachable!(),
    }
}

#[test]
fn l2_no_violations_when_all_satisfied() {
    let schema = parse_schema(
        "\
fields:
  status:
    type: choice
    values: [open, in_progress]
    required: true
  assignee:
    type: string
rules:
  - name: in-progress-needs-assignee
    match:
      status: in_progress
    require:
      assignee: required
",
    )
    .unwrap();

    let (_dir, path) = setup(vec![
        (
            "task-a.md",
            "---\nstatus: in_progress\nassignee: alice\n---\n",
        ),
        ("task-b.md", "---\nstatus: open\n---\n"),
    ]);

    let store = Store::load(&path, &schema).unwrap();
    let diags = evaluate(&store, &schema);

    assert!(diags.is_empty());
}

// ── L3: Relationship-based rules ────────────────────────────────────

#[test]
fn l3_parent_status_check() {
    let schema = parse_schema(
        "\
fields:
  status:
    type: choice
    values: [backlog, open, in_progress, done]
    required: true
  parent:
    type: link
    allow_cycles: false
    inverse: children
rules:
  - name: parent-not-backlog-when-child-active
    match:
      status: in_progress
    require:
      parent.status:
        not: backlog
",
    )
    .unwrap();

    let (_dir, path) = setup(vec![
        ("epic.md", "---\nstatus: backlog\n---\n"),
        ("task-a.md", "---\nstatus: in_progress\nparent: epic\n---\n"),
    ]);

    let store = Store::load(&path, &schema).unwrap();
    let diags = evaluate(&store, &schema);

    let violations: Vec<_> = diags
        .iter()
        .filter(|d| matches!(&d.kind, DiagnosticKind::RuleViolation { .. }))
        .collect();
    assert_eq!(violations.len(), 1);

    match &violations[0].kind {
        DiagnosticKind::RuleViolation { item_id, rule, .. } => {
            assert_eq!(item_id, "task-a");
            assert_eq!(rule, "parent-not-backlog-when-child-active");
        }
        _ => unreachable!(),
    }
}

#[test]
fn l3_parent_status_no_parent_skipped() {
    // Items without a parent should not violate parent.status checks
    // because the resolved value is null (condition on null -> false for not).
    let schema = parse_schema(
        "\
fields:
  status:
    type: choice
    values: [backlog, open, in_progress]
    required: true
  parent:
    type: link
    allow_cycles: false
    inverse: children
rules:
  - name: parent-not-backlog
    match:
      status: in_progress
    require:
      parent.status:
        not: backlog
",
    )
    .unwrap();

    let (_dir, path) = setup(vec![("task-a.md", "---\nstatus: in_progress\n---\n")]);

    let store = Store::load(&path, &schema).unwrap();
    let diags = evaluate(&store, &schema);

    // No violation — parent is null, so parent.status resolves to null.
    // The `not: backlog` assertion on null is skipped (not a violation).
    assert!(diags.is_empty());
}

#[test]
fn l3_quantifier_all_children_done() {
    let schema = parse_schema(
        "\
fields:
  status:
    type: choice
    values: [open, in_progress, done]
    required: true
  parent:
    type: link
    allow_cycles: false
    inverse: children
rules:
  - name: close-parent-when-children-done
    severity: warning
    match:
      children.status:
        all: done
    require:
      status:
        values: [done]
",
    )
    .unwrap();

    let (_dir, path) = setup(vec![
        ("epic.md", "---\nstatus: open\n---\n"),
        ("child-a.md", "---\nstatus: done\nparent: epic\n---\n"),
        ("child-b.md", "---\nstatus: done\nparent: epic\n---\n"),
    ]);

    let store = Store::load(&path, &schema).unwrap();
    let diags = evaluate(&store, &schema);

    // epic has all children done but is itself "open" — warning violation.
    let violations: Vec<_> = diags
        .iter()
        .filter(|d| matches!(&d.kind, DiagnosticKind::RuleViolation { .. }))
        .collect();
    assert_eq!(violations.len(), 1);
    assert_eq!(violations[0].severity, Severity::Warning);

    match &violations[0].kind {
        DiagnosticKind::RuleViolation { item_id, .. } => {
            assert_eq!(item_id, "epic");
        }
        _ => unreachable!(),
    }
}

#[test]
fn l3_min_count_children() {
    let schema = parse_schema(
        "\
fields:
  status:
    type: choice
    values: [open, done]
    required: true
  type_field:
    type: choice
    values: [task, epic]
  parent:
    type: link
    allow_cycles: false
    inverse: children
rules:
  - name: epics-need-children
    match:
      type_field: epic
    require:
      children:
        min_count: 1
",
    )
    .unwrap();

    let (_dir, path) = setup(vec![
        (
            "lonely-epic.md",
            "---\nstatus: open\ntype_field: epic\n---\n",
        ),
        ("good-epic.md", "---\nstatus: open\ntype_field: epic\n---\n"),
        (
            "child.md",
            "---\nstatus: open\ntype_field: task\nparent: good-epic\n---\n",
        ),
    ]);

    let store = Store::load(&path, &schema).unwrap();
    let diags = evaluate(&store, &schema);

    let violations: Vec<_> = diags
        .iter()
        .filter(|d| matches!(&d.kind, DiagnosticKind::RuleViolation { .. }))
        .collect();
    assert_eq!(violations.len(), 1);

    match &violations[0].kind {
        DiagnosticKind::RuleViolation { item_id, .. } => {
            assert_eq!(item_id, "lonely-epic");
        }
        _ => unreachable!(),
    }
}

// ── L4: Collection-wide count constraints ───────────────────────────

#[test]
fn l4_wip_limit_violation() {
    let schema = parse_schema(
        "\
fields:
  status:
    type: choice
    values: [open, in_progress, done]
    required: true
rules:
  - name: wip-limit
    description: At most 2 items in progress at once
    match:
      status: in_progress
    count:
      max: 2
",
    )
    .unwrap();

    let (_dir, path) = setup(vec![
        ("a.md", "---\nstatus: in_progress\n---\n"),
        ("b.md", "---\nstatus: in_progress\n---\n"),
        ("c.md", "---\nstatus: in_progress\n---\n"),
    ]);

    let store = Store::load(&path, &schema).unwrap();
    let diags = evaluate(&store, &schema);

    assert!(diags.iter().any(|d| matches!(
        &d.kind,
        DiagnosticKind::CountViolation { rule, count, max, .. }
        if rule == "wip-limit" && *count == 3 && *max == Some(2)
    )));
}

#[test]
fn l4_wip_limit_passes() {
    let schema = parse_schema(
        "\
fields:
  status:
    type: choice
    values: [open, in_progress, done]
    required: true
rules:
  - name: wip-limit
    match:
      status: in_progress
    count:
      max: 5
",
    )
    .unwrap();

    let (_dir, path) = setup(vec![
        ("a.md", "---\nstatus: in_progress\n---\n"),
        ("b.md", "---\nstatus: open\n---\n"),
    ]);

    let store = Store::load(&path, &schema).unwrap();
    let diags = evaluate(&store, &schema);

    assert!(!diags
        .iter()
        .any(|d| matches!(&d.kind, DiagnosticKind::CountViolation { .. })));
}

// ── Combined require + count ────────────────────────────────────────

#[test]
fn combined_require_and_count() {
    let schema = parse_schema(
        "\
fields:
  status:
    type: choice
    values: [open, in_progress, done]
    required: true
  assignee:
    type: string
rules:
  - name: wip-with-assignee
    match:
      status: in_progress
    require:
      assignee: required
    count:
      max: 2
",
    )
    .unwrap();

    let (_dir, path) = setup(vec![
        ("a.md", "---\nstatus: in_progress\n---\n"),
        ("b.md", "---\nstatus: in_progress\nassignee: bob\n---\n"),
        ("c.md", "---\nstatus: in_progress\nassignee: charlie\n---\n"),
    ]);

    let store = Store::load(&path, &schema).unwrap();
    let diags = evaluate(&store, &schema);

    // a has no assignee -> RuleViolation
    assert!(diags.iter().any(|d| matches!(
        &d.kind,
        DiagnosticKind::RuleViolation { item_id, .. } if item_id == "a"
    )));

    // 3 items in_progress, max 2 -> CountViolation
    assert!(diags
        .iter()
        .any(|d| matches!(&d.kind, DiagnosticKind::CountViolation { .. })));
}

// ── Warning severity ────────────────────────────────────────────────

#[test]
fn warning_severity_does_not_produce_error() {
    let schema = parse_schema(
        "\
fields:
  status:
    type: choice
    values: [open, in_progress]
    required: true
  assignee:
    type: string
rules:
  - name: soft-check
    severity: warning
    match:
      status: in_progress
    require:
      assignee: required
",
    )
    .unwrap();

    let (_dir, path) = setup(vec![("task.md", "---\nstatus: in_progress\n---\n")]);

    let store = Store::load(&path, &schema).unwrap();
    let diags = evaluate(&store, &schema);

    assert_eq!(diags.len(), 1);
    assert_eq!(diags[0].severity, Severity::Warning);

    // No errors — only warnings.
    assert!(!diags.iter().any(|d| d.severity == Severity::Error));
}

// ── Rule with no match ──────────────────────────────────────────────

#[test]
fn rule_without_match_applies_to_all_items() {
    let schema = parse_schema(
        "\
fields:
  status:
    type: choice
    values: [open, done]
    required: true
  title:
    type: string
rules:
  - name: all-need-title
    severity: warning
    require:
      title: required
",
    )
    .unwrap();

    let (_dir, path) = setup(vec![
        ("a.md", "---\ntitle: Has Title\nstatus: open\n---\n"),
        ("b.md", "---\nstatus: done\n---\n"),
        ("c.md", "---\nstatus: open\n---\n"),
    ]);

    let store = Store::load(&path, &schema).unwrap();
    let diags = evaluate(&store, &schema);

    let violations: Vec<_> = diags
        .iter()
        .filter(|d| matches!(&d.kind, DiagnosticKind::RuleViolation { .. }))
        .collect();
    // b and c have no title
    assert_eq!(violations.len(), 2);
}

// ── No violations when rules are satisfied ──────────────────────────

#[test]
fn no_diagnostics_when_all_rules_satisfied() {
    let schema = parse_schema(
        "\
fields:
  status:
    type: choice
    values: [open, in_progress, done]
    required: true
  assignee:
    type: string
rules:
  - name: in-progress-needs-assignee
    match:
      status: in_progress
    require:
      assignee: required
  - name: wip-limit
    match:
      status: in_progress
    count:
      max: 5
",
    )
    .unwrap();

    let (_dir, path) = setup(vec![
        ("a.md", "---\nstatus: in_progress\nassignee: alice\n---\n"),
        ("b.md", "---\nstatus: open\n---\n"),
        ("c.md", "---\nstatus: done\n---\n"),
    ]);

    let store = Store::load(&path, &schema).unwrap();
    let diags = evaluate(&store, &schema);

    assert!(diags.is_empty());
}
