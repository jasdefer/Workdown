//! Single-item read projection — the current field values and body of
//! one work item, for the editing surface (`GET /api/items/:id`).
//!
//! Distinct from [`crate::view_data`] (which projects items *through* a
//! view's slots) and [`crate::schema_data`] (field *definitions*). The
//! detail panel and the standalone item page both need an item's current
//! values without a view in context, so this serves them directly.
//!
//! Reuses [`CardField`] for each field —
//! the typed, coerced value, in schema-declaration order. The `id` field
//! is the parser-stripped identity and is returned separately as `id`,
//! not among `fields` (it isn't mutable via `set`).
//!
//! Alongside the values goes one entry per field a fill mechanism is
//! responsible for on this item, saying whether the mechanism supplied
//! the value, was overridden by a written one, or produced nothing.
//! The editor draws its origin badges from that list alone.

use serde::Serialize;

use crate::model::schema::{FillMechanism, Schema};
use crate::model::views::DisplayConfig;
use crate::model::work_item::WorkItem;
use crate::model::{ValueOrigin, WorkItemId};
use crate::store::Store;
use crate::view_data::CardField;

/// One work item's editable state.
#[derive(Debug, Clone, Serialize, ts_rs::TS)]
pub struct ItemDetail {
    pub id: WorkItemId,
    /// Resolved `#rrggbb` the detail surface tints itself with; `None`
    /// when untinted. Same hex convention as
    /// [`Card::background`](crate::view_data::Card::background). The
    /// detail surface has no view in context, so only the project-wide
    /// rungs of the `color` role apply: `defaults.display.color` from
    /// `config.yaml` (including its `none` off switch), then the
    /// first-`color`-field-in-schema-order fallback. Per-view `display:`
    /// blocks and session overrides never reach here.
    pub background: Option<String>,
    /// Each schema-declared field the item has a value for, in schema
    /// order. Fields the item doesn't set are omitted — the editor pulls
    /// the full field list (and how to render absent ones) from
    /// `GET /api/schema`.
    pub fields: Vec<CardField>,
    /// One entry per field a fill mechanism is responsible for on this
    /// item, in schema order. A field absent here is a plain one: its
    /// value, if any, was written and overrides nothing.
    pub derivations: Vec<FieldDerivation>,
    /// The freeform Markdown body, rendered read-only in the UI.
    pub body: String,
}

/// How one derivable field stands on one item.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, ts_rs::TS)]
pub struct FieldDerivation {
    /// The field's name.
    pub field: String,
    /// The mechanism that supplied the value, or the one responsible
    /// for the field here when it supplied none.
    pub mechanism: FillMechanism,
    pub state: DerivationState,
}

/// What became of a fill mechanism's work on one item's field.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, ts_rs::TS)]
#[serde(rename_all = "snake_case")]
pub enum DerivationState {
    /// The mechanism supplied the value shown.
    Derived,
    /// The file carries a value, which wins; the mechanism is switched
    /// off for this item until that value is removed.
    Overridden,
    /// The mechanism is responsible and produced nothing.
    Empty,
}

/// Build the detail projection for a single item. `display_defaults` is
/// the project-wide `defaults.display` from `config.yaml` — the only
/// display-role rung that applies to a surface without a view. `store`
/// is the store `item` was loaded into: which mechanism is responsible
/// for a field depends on the item's place in the link graph.
pub fn build(
    item: &WorkItem,
    store: &Store,
    schema: &Schema,
    display_defaults: &DisplayConfig,
) -> ItemDetail {
    let fields = schema
        .fields
        .iter()
        .filter_map(|(name, config)| {
            item.fields.get(name).map(|value| CardField {
                name: name.clone(),
                value: value.clone(),
                field_type: config.field_type(),
            })
        })
        .collect();

    ItemDetail {
        id: item.id.clone(),
        background: crate::view_data::resolved_background(item, schema, Some(display_defaults)),
        fields,
        derivations: derivations(item, store, schema),
        body: item.body.clone(),
    }
}

/// The derivation entries for one item, in schema order.
///
/// A derived value names the mechanism the fill-in phase recorded. A
/// written value is an override exactly where a mechanism would
/// otherwise be responsible, so a hand-entered value on the leaf of a
/// field that only aggregates is plain input and gets no entry.
fn derivations(item: &WorkItem, store: &Store, schema: &Schema) -> Vec<FieldDerivation> {
    schema
        .fields
        .iter()
        .filter_map(|(name, definition)| {
            let in_charge = || store.mechanism_in_charge(&item.id, definition);
            let (mechanism, state) = match item.fields.origin_of(name) {
                Some(ValueOrigin::Derived(mechanism)) => (mechanism, DerivationState::Derived),
                Some(ValueOrigin::Written) => (in_charge()?, DerivationState::Overridden),
                None => (in_charge()?, DerivationState::Empty),
            };
            Some(FieldDerivation {
                field: name.clone(),
                mechanism,
                state,
            })
        })
        .collect()
}

// ── Tests ───────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::schema::{FieldType, FieldTypeConfig};
    use crate::model::views::ColorRole;
    use crate::model::FieldValue;
    use crate::parser::schema::parse_schema;
    use crate::view_data::test_support::{
        make_item, make_schema, make_store, make_store_with_files,
    };

    fn color_schema() -> Schema {
        make_schema(vec![
            (
                "status",
                FieldTypeConfig::Choice {
                    values: vec!["open".into()],
                },
            ),
            ("team_color", FieldTypeConfig::Color),
            ("risk_color", FieldTypeConfig::Color),
        ])
    }

    #[test]
    fn fields_carry_types_in_schema_order_and_omit_absent() {
        let schema = color_schema();
        // Set out of schema order; `risk_color` absent.
        let item = make_item(
            "task-a",
            vec![
                ("team_color", FieldValue::Color("red".into())),
                ("status", FieldValue::Choice("open".into())),
            ],
            "the body",
        );

        let store = make_store(&schema, vec![]);
        let detail = build(&item, &store, &schema, &DisplayConfig::default());

        assert_eq!(detail.id.as_str(), "task-a");
        assert_eq!(detail.body, "the body");
        let names: Vec<&str> = detail
            .fields
            .iter()
            .map(|field| field.name.as_str())
            .collect();
        assert_eq!(names, vec!["status", "team_color"]);
        let types: Vec<FieldType> = detail.fields.iter().map(|field| field.field_type).collect();
        assert_eq!(types, vec![FieldType::Choice, FieldType::Color]);
    }

    #[test]
    fn background_applies_the_config_defaults_rung() {
        // The detail surface has no view in context, so
        // `defaults.display.color` is the only configurable rung: it
        // picks the field, `none` disables, unset falls back to the
        // first color field in schema order.
        let schema = color_schema();
        let item = make_item(
            "task-a",
            vec![
                ("team_color", FieldValue::Color("red".into())),
                ("risk_color", FieldValue::Color("#123456".into())),
            ],
            "",
        );
        let store = make_store(&schema, vec![]);

        let by_default = DisplayConfig {
            color: Some(ColorRole::Field("risk_color".into())),
            ..DisplayConfig::default()
        };
        assert_eq!(
            build(&item, &store, &schema, &by_default)
                .background
                .as_deref(),
            Some("#123456")
        );

        let off = DisplayConfig {
            color: Some(ColorRole::None),
            ..DisplayConfig::default()
        };
        assert_eq!(build(&item, &store, &schema, &off).background, None);

        // Unset: first color field in schema order, palette-resolved.
        assert_eq!(
            build(&item, &store, &schema, &DisplayConfig::default())
                .background
                .as_deref(),
            Some("#ef4444")
        );
    }

    // ── Derivations ─────────────────────────────────────────────────

    /// `duration` only aggregates; `end` is computed everywhere.
    const DERIVING_SCHEMA: &str = "\
fields:
  parent:
    type: link
    allow_cycles: false
  start:
    type: date
  duration:
    type: duration
    aggregate:
      function: sum
      over: parent
  end:
    type: date
    compute: start + duration
";

    /// The derivation entries of one item as `(field, mechanism, state)`.
    fn derivations_of(
        files: Vec<(&str, &str)>,
        item_id: &str,
    ) -> Vec<(String, FillMechanism, DerivationState)> {
        let schema = parse_schema(DERIVING_SCHEMA).expect("test schema must parse");
        let (_directory, store) = make_store_with_files(&schema, files);
        let item = store.get(item_id).expect("item loads");
        build(item, &store, &schema, &DisplayConfig::default())
            .derivations
            .into_iter()
            .map(|entry| (entry.field, entry.mechanism, entry.state))
            .collect()
    }

    #[test]
    fn a_derived_value_names_the_mechanism_that_supplied_it() {
        let files = vec![
            ("epic.md", "---\nstart: 2026-01-05\n---\n"),
            ("task.md", "---\nparent: epic\nduration: 7d\n---\n"),
        ];

        // The rollup supplied `duration`, the formula `end` from it.
        assert_eq!(
            derivations_of(files, "epic"),
            vec![
                (
                    "duration".to_owned(),
                    FillMechanism::Aggregate,
                    DerivationState::Derived
                ),
                (
                    "end".to_owned(),
                    FillMechanism::Compute,
                    DerivationState::Derived
                ),
            ]
        );
    }

    #[test]
    fn a_written_value_where_a_mechanism_is_responsible_is_an_override() {
        // `end` is written although the formula would run; `duration`
        // is written on an item with a child, where the rollup would.
        let files = vec![
            (
                "epic.md",
                "---\nstart: 2026-01-05\nduration: 14d\nend: 2026-03-01\n---\n",
            ),
            ("task.md", "---\nparent: epic\n---\n"),
        ];

        assert_eq!(
            derivations_of(files, "epic"),
            vec![
                (
                    "duration".to_owned(),
                    FillMechanism::Aggregate,
                    DerivationState::Overridden
                ),
                (
                    "end".to_owned(),
                    FillMechanism::Compute,
                    DerivationState::Overridden
                ),
            ]
        );
    }

    #[test]
    fn a_value_written_where_nothing_would_fill_it_gets_no_entry() {
        // A leaf of a field that only aggregates: the written
        // `duration` is plain input, not an override. `start` is a
        // plain field. Only the computed `end` is listed.
        let files = vec![("task.md", "---\nstart: 2026-01-05\nduration: 7d\n---\n")];

        assert_eq!(
            derivations_of(files, "task"),
            vec![(
                "end".to_owned(),
                FillMechanism::Compute,
                DerivationState::Derived
            )]
        );
    }

    #[test]
    fn a_responsible_mechanism_that_produced_nothing_is_listed_as_empty() {
        // No `duration` anywhere: the formula on both items and the
        // rollup on the epic are responsible and came up empty. The
        // leaf's own blank `duration` is nobody's to fill.
        let files = vec![
            ("epic.md", "---\nstart: 2026-01-05\n---\n"),
            ("task.md", "---\nparent: epic\nstart: 2026-01-05\n---\n"),
        ];

        assert_eq!(
            derivations_of(files.clone(), "epic"),
            vec![
                (
                    "duration".to_owned(),
                    FillMechanism::Aggregate,
                    DerivationState::Empty
                ),
                (
                    "end".to_owned(),
                    FillMechanism::Compute,
                    DerivationState::Empty
                ),
            ]
        );
        assert_eq!(
            derivations_of(files, "task"),
            vec![(
                "end".to_owned(),
                FillMechanism::Compute,
                DerivationState::Empty
            )]
        );
    }
}
