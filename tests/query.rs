//! Integration tests for `workdown query`.

use std::fs;
use std::path::PathBuf;

use tempfile::TempDir;
use workdown::model::config::Config;
use workdown::parser::config::load_config;
use workdown::parser::schema::load_schema;
use workdown::query::engine;
use workdown::query::format::{render_delimited, DelimitedError, DelimitedOptions};
use workdown::query::parse::parse_where;
use workdown::query::types::{Predicate, QueryRequest, SortDirection, SortSpec};
use workdown::store::Store;

// ── Test fixtures ───────────────────────────────────────────────────

const TEST_SCHEMA: &str = "\
fields:
  title:
    type: string
    required: false
  type:
    type: choice
    values: [epic, task, bug]
    required: true
    default: task
  status:
    type: choice
    values: [backlog, open, in_progress, done]
    required: true
    default: backlog
  priority:
    type: choice
    values: [critical, high, medium, low]
    required: false
  points:
    type: integer
    required: false
  assignee:
    type: string
    required: false
  tags:
    type: list
    required: false
  parent:
    type: link
    required: false
    allow_cycles: false
    inverse: children
";

const TEST_CONFIG: &str = "\
project:
  name: Test Project
  description: ''
paths:
  work_items: workdown-items
  templates: .workdown/templates
  resources: .workdown/resources.yaml
schema: .workdown/schema.yaml
defaults:
  board_field: status
  tree_field: parent
  graph_field: depends_on
";

fn setup_project() -> (TempDir, PathBuf) {
    let directory = TempDir::new().unwrap();
    let root = directory.path().to_path_buf();

    fs::create_dir_all(root.join(".workdown")).unwrap();
    fs::create_dir_all(root.join("workdown-items")).unwrap();

    fs::write(root.join(".workdown/config.yaml"), TEST_CONFIG).unwrap();
    fs::write(root.join(".workdown/schema.yaml"), TEST_SCHEMA).unwrap();

    // Work items with varied fields for testing queries.
    fs::write(
        root.join("workdown-items/task-a.md"),
        "---\ntitle: Fix Login\ntype: task\nstatus: open\npoints: 3\nassignee: alice\ntags:\n  - auth\n  - backend\n---\nFix the login flow.\n",
    ).unwrap();

    fs::write(
        root.join("workdown-items/task-b.md"),
        "---\ntitle: Add Dashboard\ntype: task\nstatus: in_progress\npoints: 5\nassignee: bob\ntags:\n  - frontend\n---\nBuild the main dashboard.\n",
    ).unwrap();

    fs::write(
        root.join("workdown-items/bug-c.md"),
        "---\ntitle: Crash on Save\ntype: bug\nstatus: open\npriority: critical\npoints: 8\n---\nApp crashes when saving.\n",
    ).unwrap();

    fs::write(
        root.join("workdown-items/epic-d.md"),
        "---\ntitle: Auth Epic\ntype: epic\nstatus: open\n---\nAuthentication initiative.\n",
    ).unwrap();

    fs::write(
        root.join("workdown-items/task-e.md"),
        "---\ntitle: Fix Logout\ntype: task\nstatus: done\npoints: 2\nassignee: alice\nparent: epic-d\n---\nFix the logout flow.\n",
    ).unwrap();

    (directory, root)
}

fn load_test_config(root: &PathBuf) -> Config {
    load_config(&root.join(".workdown/config.yaml")).unwrap()
}

/// Helper: execute a query with the given where clauses, sort, and fields.
fn run_query(
    root: &PathBuf,
    where_clauses: &[&str],
    sort: &[SortSpec],
    fields: &[&str],
) -> workdown::query::types::QueryResult {
    let config = load_test_config(root);
    let schema = load_schema(&root.join(&config.schema)).unwrap();
    let store = Store::load(&root.join(&config.paths.work_items), &schema).unwrap();

    let mut predicates = Vec::new();
    for clause in where_clauses {
        predicates.push(parse_where(clause).unwrap());
    }
    let predicate = match predicates.len() {
        0 => None,
        1 => Some(predicates.remove(0)),
        _ => Some(Predicate::And(predicates)),
    };

    let request = QueryRequest {
        predicate,
        sort: sort.to_vec(),
        fields: fields.iter().map(|field| field.to_string()).collect(),
    };

    engine::execute(&request, &store, &schema).unwrap()
}

/// Extract IDs from a query result, sorted for comparison when order doesn't matter.
fn sorted_ids(result: &workdown::query::types::QueryResult) -> Vec<String> {
    let mut ids: Vec<String> = result.items.iter().map(|row| row.id.clone()).collect();
    ids.sort();
    ids
}

/// Extract IDs from a query result in result order.
fn ordered_ids(result: &workdown::query::types::QueryResult) -> Vec<String> {
    result.items.iter().map(|row| row.id.clone()).collect()
}

// ── Test cases ──────────────────────────────────────────────────────

#[test]
fn query_no_filters_returns_all() {
    let (_directory, root) = setup_project();
    let result = run_query(&root, &[], &[], &[]);
    assert_eq!(result.items.len(), 5);
}

#[test]
fn query_equality_filter() {
    let (_directory, root) = setup_project();
    let result = run_query(&root, &["status=open"], &[], &[]);
    let ids = sorted_ids(&result);
    assert_eq!(ids, vec!["bug-c", "epic-d", "task-a"]);
}

#[test]
fn query_numeric_greater_than() {
    let (_directory, root) = setup_project();
    let result = run_query(&root, &["points>3"], &[], &[]);
    let ids = sorted_ids(&result);
    assert_eq!(ids, vec!["bug-c", "task-b"]);
}

#[test]
fn query_substring_contains() {
    let (_directory, root) = setup_project();
    let result = run_query(&root, &["title~Login"], &[], &[]);
    // "Fix Login" contains "Login", "Fix Logout" does not.
    let ids = sorted_ids(&result);
    assert_eq!(ids, vec!["task-a"]);
}

#[test]
fn query_in_syntax() {
    let (_directory, root) = setup_project();
    let result = run_query(&root, &["status=open,in_progress"], &[], &[]);
    let ids = sorted_ids(&result);
    assert_eq!(ids, vec!["bug-c", "epic-d", "task-a", "task-b"]);
}

#[test]
fn query_is_set() {
    let (_directory, root) = setup_project();
    let result = run_query(&root, &["assignee?"], &[], &[]);
    let ids = sorted_ids(&result);
    assert_eq!(ids, vec!["task-a", "task-b", "task-e"]);
}

#[test]
fn query_is_not_set() {
    let (_directory, root) = setup_project();
    let result = run_query(&root, &["!assignee?"], &[], &[]);
    let ids = sorted_ids(&result);
    assert_eq!(ids, vec!["bug-c", "epic-d"]);
}

#[test]
fn query_multiple_where_and() {
    let (_directory, root) = setup_project();
    let result = run_query(&root, &["status=open", "points>5"], &[], &[]);
    let ids = sorted_ids(&result);
    // Only bug-c has status=open AND points=8 (>5).
    assert_eq!(ids, vec!["bug-c"]);
}

#[test]
fn query_sort_ascending() {
    let (_directory, root) = setup_project();
    let result = run_query(
        &root,
        &["points?"],
        &[SortSpec {
            field: "points".to_owned(),
            direction: SortDirection::Ascending,
        }],
        &["id", "points"],
    );
    let ids = ordered_ids(&result);
    // 2, 3, 5, 8
    assert_eq!(ids, vec!["task-e", "task-a", "task-b", "bug-c"]);
}

#[test]
fn query_sort_descending() {
    let (_directory, root) = setup_project();
    let result = run_query(
        &root,
        &["points?"],
        &[SortSpec {
            field: "points".to_owned(),
            direction: SortDirection::Descending,
        }],
        &["id", "points"],
    );
    let ids = ordered_ids(&result);
    // 8, 5, 3, 2
    assert_eq!(ids, vec!["bug-c", "task-b", "task-a", "task-e"]);
}

#[test]
fn query_custom_fields() {
    let (_directory, root) = setup_project();
    let result = run_query(&root, &["status=open"], &[], &["id", "title", "status"]);
    assert_eq!(result.columns, vec!["id", "title", "status"]);
    // Every row should have 3 values.
    for row in &result.items {
        assert_eq!(row.values.len(), 3);
    }
}

#[test]
fn query_json_format() {
    let (_directory, root) = setup_project();
    let result = run_query(&root, &[], &[], &["id", "status"]);
    let json = workdown::query::format::render_json(&result);
    let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();
    assert!(parsed.is_array());
    assert_eq!(parsed.as_array().unwrap().len(), 5);
}

#[test]
fn query_empty_result() {
    let (_directory, root) = setup_project();
    let result = run_query(&root, &["status=nonexistent"], &[], &[]);
    assert!(result.items.is_empty());
}

// ── Cross-item (related-field) queries ─────────────────────────────

#[test]
fn query_related_forward_link() {
    // task-e has parent: epic-d, and epic-d has status=open.
    let (_directory, root) = setup_project();
    let result = run_query(&root, &["parent.status=open"], &[], &[]);
    let ids = sorted_ids(&result);
    assert_eq!(ids, vec!["task-e"]);
}

#[test]
fn query_related_forward_link_no_match() {
    // No item has a parent with status=done.
    let (_directory, root) = setup_project();
    let result = run_query(&root, &["parent.status=done"], &[], &[]);
    assert!(result.items.is_empty());
}

#[test]
fn query_related_inverse_relation() {
    // epic-d has one child (task-e) with status=done.
    // Using "any" semantics: epic-d matches children.status=done.
    let (_directory, root) = setup_project();
    let result = run_query(&root, &["children.status=done"], &[], &[]);
    let ids = sorted_ids(&result);
    assert_eq!(ids, vec!["epic-d"]);
}

#[test]
fn query_related_is_set() {
    // Only task-e has a parent with any status set.
    let (_directory, root) = setup_project();
    let result = run_query(&root, &["parent.status?"], &[], &[]);
    let ids = sorted_ids(&result);
    assert_eq!(ids, vec!["task-e"]);
}

#[test]
fn query_related_combined_with_local_filter() {
    // task-e has status=done AND parent.status=open.
    let (_directory, root) = setup_project();
    let result = run_query(&root, &["status=done", "parent.status=open"], &[], &[]);
    let ids = sorted_ids(&result);
    assert_eq!(ids, vec!["task-e"]);
}

#[test]
fn query_default_columns_include_id_and_required() {
    let (_directory, root) = setup_project();
    let result = run_query(&root, &[], &[], &[]);
    // Default: id + required fields (type and status are required).
    assert!(result.columns.contains(&"id".to_owned()));
    assert!(result.columns.contains(&"type".to_owned()));
    assert!(result.columns.contains(&"status".to_owned()));
    // Non-required fields should not be in defaults.
    assert!(!result.columns.contains(&"title".to_owned()));
    assert!(!result.columns.contains(&"points".to_owned()));
}

// ── Delimited output (CSV/TSV) ──────────────────────────────────────

fn filtered(
    root: &PathBuf,
    where_clauses: &[&str],
    sort: &[SortSpec],
    fields: &[&str],
) -> (Vec<String>, Vec<workdown::model::WorkItem>) {
    // The engine hands back borrows into the Store. For test ergonomics
    // we clone the items into owned values so the caller can drop the
    // store without lifetime juggling.
    let config = load_test_config(root);
    let schema = load_schema(&root.join(&config.schema)).unwrap();
    let store = Store::load(&root.join(&config.paths.work_items), &schema).unwrap();

    let mut predicates = Vec::new();
    for clause in where_clauses {
        predicates.push(parse_where(clause).unwrap());
    }
    let predicate = match predicates.len() {
        0 => None,
        1 => Some(predicates.remove(0)),
        _ => Some(Predicate::And(predicates)),
    };

    let request = QueryRequest {
        predicate,
        sort: sort.to_vec(),
        fields: fields.iter().map(|field| field.to_string()).collect(),
    };

    let (columns, items) = engine::filter_and_sort(&request, &store, &schema).unwrap();
    // Clone items into owned values — WorkItem has no Clone so we rebuild by hand.
    let owned: Vec<workdown::model::WorkItem> = items
        .into_iter()
        .map(|item| workdown::model::WorkItem {
            id: item.id.clone(),
            fields: item.fields.clone(),
            body: item.body.clone(),
            source_path: item.source_path.clone(),
        })
        .collect();
    (columns, owned)
}

fn tsv_options() -> DelimitedOptions {
    DelimitedOptions {
        delimiter: b'\t',
        header: true,
        list_separator: ';',
    }
}

#[test]
fn query_tsv_output_has_header_and_tabs() {
    let (_directory, root) = setup_project();
    let (columns, items) = filtered(
        &root,
        &["title~Fix Login"],
        &[],
        &["id", "title", "status", "tags"],
    );
    let refs: Vec<&workdown::model::WorkItem> = items.iter().collect();
    let output = render_delimited(&refs, &columns, &tsv_options()).unwrap();
    assert_eq!(output, "id\ttitle\tstatus\ttags\ntask-a\tFix Login\topen\tauth;backend\n");
}

#[test]
fn query_csv_output_quotes_embedded_commas() {
    let (_directory, root) = setup_project();
    let (columns, items) = filtered(&root, &["title~Fix Login"], &[], &["id", "tags"]);
    let refs: Vec<&workdown::model::WorkItem> = items.iter().collect();
    let options = DelimitedOptions {
        delimiter: b',',
        header: true,
        list_separator: ';',
    };
    let output = render_delimited(&refs, &columns, &options).unwrap();
    // Tags joined with ';' — no comma inside the cell, so no quoting needed.
    assert_eq!(output, "id,tags\ntask-a,auth;backend\n");
}

#[test]
fn query_csv_quotes_title_containing_comma() {
    // Add an item with a comma in the title to exercise quoting.
    let (_directory, root) = setup_project();
    fs::write(
        root.join("workdown-items/task-f.md"),
        "---\ntitle: \"Hello, world\"\ntype: task\nstatus: open\n---\nBody.\n",
    )
    .unwrap();

    let (columns, items) = filtered(&root, &["title~Hello"], &[], &["id", "title"]);
    let refs: Vec<&workdown::model::WorkItem> = items.iter().collect();
    let options = DelimitedOptions {
        delimiter: b',',
        header: true,
        list_separator: ';',
    };
    let output = render_delimited(&refs, &columns, &options).unwrap();
    assert_eq!(output, "id,title\ntask-f,\"Hello, world\"\n");
}

#[test]
fn query_delimited_without_header() {
    let (_directory, root) = setup_project();
    let (columns, items) = filtered(&root, &["title~Fix Login"], &[], &["id", "status"]);
    let refs: Vec<&workdown::model::WorkItem> = items.iter().collect();
    let options = DelimitedOptions {
        header: false,
        ..tsv_options()
    };
    let output = render_delimited(&refs, &columns, &options).unwrap();
    assert_eq!(output, "task-a\topen\n");
}

#[test]
fn query_delimited_custom_delimiter() {
    let (_directory, root) = setup_project();
    let (columns, items) = filtered(&root, &["title~Fix Login"], &[], &["id", "status"]);
    let refs: Vec<&workdown::model::WorkItem> = items.iter().collect();
    let options = DelimitedOptions {
        delimiter: b'|',
        header: true,
        list_separator: ';',
    };
    let output = render_delimited(&refs, &columns, &options).unwrap();
    assert_eq!(output, "id|status\ntask-a|open\n");
}

#[test]
fn query_delimited_errors_on_separator_in_list_element() {
    // tag "a;b" contains the default list separator ';' → must error.
    let (_directory, root) = setup_project();
    fs::write(
        root.join("workdown-items/task-g.md"),
        "---\ntitle: NastyTags\ntype: task\nstatus: open\ntags:\n  - \"a;b\"\n---\n",
    )
    .unwrap();

    let (columns, items) = filtered(&root, &["title~NastyTags"], &[], &["id", "tags"]);
    let refs: Vec<&workdown::model::WorkItem> = items.iter().collect();
    let result = render_delimited(&refs, &columns, &tsv_options());
    match result {
        Err(DelimitedError::EmbeddedSeparator {
            item_id,
            field,
            separator,
        }) => {
            assert_eq!(item_id, "task-g");
            assert_eq!(field, "tags");
            assert_eq!(separator, ';');
        }
        other => panic!("expected EmbeddedSeparator, got {other:?}"),
    }
}

#[test]
fn query_delimited_errors_on_delimiter_collision() {
    let (_directory, root) = setup_project();
    let (columns, items) = filtered(&root, &["title~Fix Login"], &[], &["id", "tags"]);
    let refs: Vec<&workdown::model::WorkItem> = items.iter().collect();
    let options = DelimitedOptions {
        delimiter: b';',
        header: true,
        list_separator: ';',
    };
    let result = render_delimited(&refs, &columns, &options);
    assert!(matches!(
        result,
        Err(DelimitedError::DelimiterConflict { .. })
    ));
}
