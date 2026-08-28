//! Drift guard for the vocabulary tables in `docs/`.
//!
//! The guides carry a row per view kind and a row per field type. Those are
//! the same two lists the CLI has as enums, restated in prose for a reader —
//! and a reader has no way to tell that the list is one short. Nothing else
//! in the test suite reads the guides, so a kind added to the code and not
//! to its table ships silently.
//!
//! Only the *set of names* is checked here. What each row says about its
//! kind — required slots, renderer status, type-specific options — is prose
//! a test cannot verify; `crates/core/tests/schema_schema.rs` and
//! `views_schema.rs` pin the machine-readable statements of those same
//! rules instead.

use std::collections::BTreeSet;

use strum::VariantArray;
use workdown_core::model::schema::FieldType;
use workdown_core::model::views::ViewType;

const VIEWS_GUIDE: &str = include_str!("../../../docs/views.md");
const SCHEMA_GUIDE: &str = include_str!("../../../docs/schema.md");

/// The first cell of every row of the Markdown table whose header row
/// starts with `header_prefix`, with the code backticks stripped.
fn first_column_of_table(markdown: &str, header_prefix: &str) -> BTreeSet<String> {
    let mut lines = markdown
        .lines()
        .skip_while(|line| !line.starts_with(header_prefix));
    let header = lines.next();
    assert!(
        header.is_some(),
        "no table found with a header starting `{header_prefix}` — the guide was restructured, \
         so this test needs to follow it"
    );
    lines.next(); // The `|---|---|` separator row.
    lines
        .take_while(|line| line.starts_with('|'))
        .map(|row| {
            row.split('|')
                .nth(1)
                .expect("a table row has a first cell")
                .trim()
                .trim_matches('`')
                .to_owned()
        })
        .collect()
}

#[test]
fn views_guide_documents_every_view_kind() {
    let documented = first_column_of_table(VIEWS_GUIDE, "| Type | Required slots |");
    let in_code: BTreeSet<String> = ViewType::VARIANTS.iter().map(ToString::to_string).collect();

    assert_eq!(
        documented, in_code,
        "the view-type table in docs/views.md and the ViewType enum disagree about which view \
         kinds exist"
    );
}

#[test]
fn schema_guide_documents_every_field_type() {
    let documented = first_column_of_table(SCHEMA_GUIDE, "| Type | Description |");
    let in_code: BTreeSet<String> = FieldType::VARIANTS
        .iter()
        .map(ToString::to_string)
        .collect();

    assert_eq!(
        documented, in_code,
        "the field-type table in docs/schema.md and the FieldType enum disagree about which field \
         types exist"
    );
}
