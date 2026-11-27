//! What the colors of a view mean.
//!
//! A view tints its items with the field the `color` display role
//! picks. This module says what those tints stand for, read from the
//! field's recipe in the schema rather than guessed from the items:
//!
//! - **`when:`** is a table from conditions to literal colors, so it
//!   gets a full legend: one entry per color that is on screen, with
//!   the conditions that produce it. Two branches with the same `then`
//!   share an entry, and so does the evaluated `default`.
//! - **`pull:`** copies the color of a linked item. The schema knows
//!   where the color comes from and nothing about what it stands for,
//!   so the answer is a hint, not a legend.
//! - A field no mechanism fills has no stated meaning at all, and
//!   there is no legend.
//!
//! The recipe supplies the wording and the items supply the selection:
//! only colors of items the view presents appear, each with the number
//! of items wearing it. A hand-written color on a `when:` field is not
//! explained by any condition, so it is counted apart.
//!
//! No part of this needs to know which branch an item matched. A
//! `then:` is a literal, so a derived color names its entry by itself.
//! What that leaves open is an entry several branches share, or a
//! branch and the `default`: the recipe says what *can* make an item
//! gray, not what made the ones on screen gray. Where every condition
//! tests the same field for equality, the legend answers with the
//! values that field has on the counted items.
//!
//! Wording stays with the renderers (ADR-006): a condition travels as
//! the [`ConditionShape`] the schema editor already reads.

use std::collections::HashSet;

use serde::Serialize;

use crate::condition_shape::{ConditionShape, Operand};
use crate::expression::ComparisonOperator;
use crate::model::color::resolve_color_to_hex;
use crate::model::field_value::format_field_value;
use crate::model::schema::{FieldTypeConfig, FillMechanism, Schema, WhenConfig};
use crate::model::views::View;
use crate::model::{FieldValue, WorkItem};
use crate::store::Store;

use super::common::resolve_color_field;
use super::tree::TreeNode;
use super::treemap::TreemapNode;
use super::ViewData;

// ── Wire types ────────────────────────────────────────────────────────

/// The meaning of the tints in one view.
#[derive(Debug, Clone, PartialEq, Serialize, ts_rs::TS)]
pub struct ColorLegend {
    /// The `color`-typed field the view tints with.
    pub field: String,
    pub meaning: ColorMeaning,
}

/// How much the schema can say about a color field's values.
#[derive(Debug, Clone, PartialEq, Serialize, ts_rs::TS)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ColorMeaning {
    /// A `when:` field: every derived color has its conditions.
    Conditions {
        /// The field every condition on screen tests for equality with
        /// a literal, when they all test the same one: the legend then
        /// reads "Color by Status" and each entry is labelled with its
        /// `values`. `None` when the conditions differ in field or
        /// form.
        subject: Option<String>,
        /// One entry per derived color on screen, in the order the
        /// recipe first names the color.
        entries: Vec<ConditionEntry>,
        /// Colors written by hand on items of the view, which no
        /// condition explains. In the view's item order.
        written: Vec<ColorCount>,
    },
    /// A `pull:` field: the color is the one `source_field` has on the
    /// item `over` points at.
    Inherited { over: String, source_field: String },
    /// A recipe that reads `fields` and whose mapping to colors cannot
    /// be listed. No mechanism produces this for a color field today;
    /// it is the answer a future one gets until it can do better.
    DependsOn { fields: Vec<String> },
}

/// One color of a `when:` field and what makes an item take it.
#[derive(Debug, Clone, PartialEq, Serialize, ts_rs::TS)]
pub struct ConditionEntry {
    /// Resolved `#rrggbb`, the same form as an item's `background`.
    pub color: String,
    /// Items of the view that derived this color.
    pub count: usize,
    /// The conditions of every branch whose `then` is this color, in
    /// branch order. Empty when only the `default` produces it.
    pub conditions: Vec<ConditionShape>,
    /// Whether the recipe's `default` is this color as well, so items
    /// that matched no branch are among the counted ones.
    pub otherwise: bool,
    /// With a subject: the values that field has on the counted items,
    /// `None` for an item without one. This is what is on screen, not
    /// what the recipe allows, so a view filtered down to the items of
    /// the `default` names their values instead of the branch that
    /// shares the color. In the field's declared order for a `choice`,
    /// else as the view lists them; a missing value last. Empty
    /// without a subject.
    pub values: Vec<Option<String>>,
}

/// A color and the number of items of the view wearing it.
#[derive(Debug, Clone, PartialEq, Serialize, ts_rs::TS)]
pub struct ColorCount {
    /// Resolved `#rrggbb`.
    pub color: String,
    pub count: usize,
}

// ── Building ──────────────────────────────────────────────────────────

/// The legend for `view`'s tints over the items `data` presents, or
/// `None` when there is nothing to explain: tinting is off, the color
/// field has no recipe, or no presented item has a color.
///
/// `view` must be the view `data` was extracted from, display roles
/// resolved, so both agree on the color field.
pub fn color_legend(
    view: &View,
    data: &ViewData,
    store: &Store,
    schema: &Schema,
) -> Option<ColorLegend> {
    let field_name = resolve_color_field(schema, Some(&view.display))?;
    let definition = schema.fields.get(field_name)?;
    let mechanism = definition.recipe_mechanism()?;

    let tints = presented_tints(data, store, field_name);
    if tints.is_empty() {
        return None;
    }

    // Exhaustive, so a new mechanism has to say what it can tell a
    // reader about its colors before it compiles.
    let meaning = match mechanism {
        FillMechanism::When => conditions_meaning(definition.when.as_ref()?, &tints, schema),
        FillMechanism::Pull => {
            let pull = definition.pull.as_ref()?;
            ColorMeaning::Inherited {
                over: pull.over.clone(),
                source_field: pull.field.clone(),
            }
        }
        FillMechanism::Compute => {
            let mut fields: Vec<String> = Vec::new();
            for name in definition.derived_references() {
                if !fields.iter().any(|known| known == name) {
                    fields.push(name.to_owned());
                }
            }
            ColorMeaning::DependsOn { fields }
        }
        // A rollup is not a recipe; `recipe_mechanism` never names it.
        FillMechanism::Aggregate => return None,
    };

    Some(ColorLegend {
        field: field_name.to_owned(),
        meaning,
    })
}

/// The tint of one presented item.
struct Tint<'store> {
    hex: String,
    /// Written in the item's file rather than derived.
    written: bool,
    item: &'store WorkItem,
}

/// The tint of every item `data` presents that has one, each item
/// once, in the order the view lists them.
fn presented_tints<'store>(
    data: &ViewData,
    store: &'store Store,
    field_name: &str,
) -> Vec<Tint<'store>> {
    let mut seen: HashSet<&str> = HashSet::new();
    tinted_item_ids(data)
        .into_iter()
        .filter(|id| seen.insert(*id))
        .filter_map(|id| {
            let item = store.get(id)?;
            let FieldValue::Color(canonical) = item.fields.get(field_name)? else {
                return None;
            };
            Some(Tint {
                hex: resolve_color_to_hex(canonical)?,
                written: item.fields.is_written(field_name),
                item,
            })
        })
        .collect()
}

/// The ids of the items a view kind draws with a tint, repeats
/// included. Unplaced items are left out: they are listed below the
/// view untinted. The chart kinds aggregate items and tint none.
///
/// Exhaustive over [`ViewData`]: a new kind has to say which of its
/// items wear the color.
fn tinted_item_ids(data: &ViewData) -> Vec<&str> {
    match data {
        ViewData::Board(board) => board
            .columns
            .iter()
            .flat_map(|column| &column.cards)
            .map(|card| card.id.as_str())
            .collect(),
        ViewData::Gantt(gantt) => gantt.bars.iter().map(|bar| bar.card.id.as_str()).collect(),
        ViewData::GanttByDepth(gantt) => gantt
            .levels
            .iter()
            .flat_map(|level| &level.bars)
            .map(|bar| bar.card.id.as_str())
            .collect(),
        ViewData::GanttByInitiative(gantt) => gantt
            .initiatives
            .iter()
            .flat_map(|initiative| &initiative.bars)
            .map(|bar| bar.card.id.as_str())
            .collect(),
        ViewData::Graph(graph) => graph.nodes.iter().map(|card| card.id.as_str()).collect(),
        ViewData::Table(table) => table.rows.iter().map(|row| row.id.as_str()).collect(),
        ViewData::Tree(tree) => {
            let mut ids = Vec::new();
            for root in &tree.roots {
                collect_tree_ids(root, &mut ids);
            }
            ids
        }
        ViewData::Treemap(treemap) => {
            let mut ids = Vec::new();
            collect_treemap_ids(&treemap.root, &mut ids);
            ids
        }
        ViewData::BarChart(_)
        | ViewData::Heatmap(_)
        | ViewData::LineChart(_)
        | ViewData::Metric(_)
        | ViewData::Workload(_) => Vec::new(),
    }
}

fn collect_tree_ids<'data>(node: &'data TreeNode, ids: &mut Vec<&'data str>) {
    ids.push(node.id.as_str());
    for child in &node.children {
        collect_tree_ids(child, ids);
    }
}

fn collect_treemap_ids<'data>(node: &'data TreemapNode, ids: &mut Vec<&'data str>) {
    if let Some(card) = &node.card {
        ids.push(card.id.as_str());
    }
    for child in &node.children {
        collect_treemap_ids(child, ids);
    }
}

/// The legend of a `when:` field: the recipe's colors with their
/// conditions, narrowed to the colors in `tints` and counted.
fn conditions_meaning(when: &WhenConfig, tints: &[Tint<'_>], schema: &Schema) -> ColorMeaning {
    let mut entries: Vec<ConditionEntry> = Vec::new();
    for branch in &when.branches {
        if let Some(hex) = hex_of(&branch.value) {
            let index = entry_index(&mut entries, hex);
            entries[index].conditions.push(ConditionShape::of(
                &branch.condition,
                &branch.condition_source,
            ));
        }
    }
    if let Some(hex) = when.default.as_ref().and_then(hex_of) {
        let index = entry_index(&mut entries, hex);
        entries[index].otherwise = true;
    }

    let mut written: Vec<ColorCount> = Vec::new();
    for tint in tints {
        if tint.written {
            match written.iter_mut().find(|known| known.color == tint.hex) {
                Some(known) => known.count += 1,
                None => written.push(ColorCount {
                    color: tint.hex.clone(),
                    count: 1,
                }),
            }
        } else if let Some(entry) = entries.iter_mut().find(|entry| entry.color == tint.hex) {
            entry.count += 1;
        }
    }
    entries.retain(|entry| entry.count > 0);

    let subject = shared_subject(&entries).map(str::to_owned);
    if let Some(subject) = &subject {
        for tint in tints.iter().filter(|tint| !tint.written) {
            let Some(entry) = entries.iter_mut().find(|entry| entry.color == tint.hex) else {
                continue;
            };
            let value = tint.item.fields.get(subject).map(format_field_value);
            if !entry.values.contains(&value) {
                entry.values.push(value);
            }
        }
        let declared = declared_values(schema, subject);
        for entry in &mut entries {
            // Stable, so values the field does not declare keep the
            // view's order among themselves.
            entry.values.sort_by_key(|value| match value {
                Some(value) => declared
                    .iter()
                    .position(|known| known == value)
                    .unwrap_or(declared.len()),
                None => usize::MAX,
            });
        }
    }

    ColorMeaning::Conditions {
        subject,
        entries,
        written,
    }
}

/// The field every condition of `entries` tests for equality with a
/// literal, when there is a condition and they all test the same one.
fn shared_subject(entries: &[ConditionEntry]) -> Option<&str> {
    let mut subject: Option<&str> = None;
    for condition in entries.iter().flat_map(|entry| &entry.conditions) {
        let tested = equality_subject(condition)?;
        if subject.is_some_and(|known| known != tested) {
            return None;
        }
        subject = Some(tested);
    }
    subject
}

/// The field of a `field == literal` row, in either operand order.
fn equality_subject(condition: &ConditionShape) -> Option<&str> {
    let ConditionShape::Row {
        left,
        operator: ComparisonOperator::Equal,
        right,
    } = condition
    else {
        return None;
    };
    match (left, right) {
        (Operand::Field { name }, Operand::Literal { .. })
        | (Operand::Literal { .. }, Operand::Field { name }) => Some(name),
        _ => None,
    }
}

/// The values a `choice` field declares, in order; none for any other
/// field.
fn declared_values<'schema>(schema: &'schema Schema, field_name: &str) -> &'schema [String] {
    match schema
        .fields
        .get(field_name)
        .map(|field| &field.type_config)
    {
        Some(FieldTypeConfig::Choice { values }) => values,
        _ => &[],
    }
}

/// The position of `hex`'s entry, appended empty when the recipe has
/// not named the color before.
fn entry_index(entries: &mut Vec<ConditionEntry>, hex: String) -> usize {
    match entries.iter().position(|entry| entry.color == hex) {
        Some(index) => index,
        None => {
            entries.push(ConditionEntry {
                color: hex,
                count: 0,
                conditions: Vec::new(),
                otherwise: false,
                values: Vec::new(),
            });
            entries.len() - 1
        }
    }
}

/// A color value as resolved `#rrggbb`; `None` for any other value.
fn hex_of(value: &FieldValue) -> Option<String> {
    match value {
        FieldValue::Color(canonical) => resolve_color_to_hex(canonical),
        _ => None,
    }
}

// ── Tests ─────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    use crate::condition_shape::ConditionLiteral;
    use crate::model::views::{ColorRole, DisplayConfig, ViewKind};
    use crate::parser::schema::parse_schema;
    use crate::view_data::test_support::make_store_with_files;
    use crate::view_data::{table, tree};

    const GREEN: &str = "#22c55e";
    const BLUE: &str = "#3b82f6";
    const GRAY: &str = "#6b7280";
    const RED: &str = "#ef4444";

    const STATUS_SCHEMA: &str = "\
fields:
  status:
    type: choice
    values: [to_do, in_progress, done, removed]
  parent:
    type: link
    allow_cycles: false
  color:
    type: color
    when:
      - if: status == \"done\"
        then: green
      - if: status == \"removed\"
        then: gray
      - if: status == \"in_progress\"
        then: blue
    default: gray
";

    fn view_of(kind: ViewKind) -> View {
        View {
            id: "v".to_owned(),
            where_clauses: Vec::new(),
            sort_keys: vec![],
            display: DisplayConfig::default(),
            kind,
        }
    }

    fn item_file(frontmatter: &str) -> String {
        format!("---\n{frontmatter}---\n")
    }

    /// The legend of a table over `files` under `schema_yaml`.
    fn table_legend(schema_yaml: &str, files: &[(&str, &str)], view: &View) -> Option<ColorLegend> {
        let schema = parse_schema(schema_yaml).expect("schema parses");
        let contents: Vec<(&str, String)> = files
            .iter()
            .map(|(name, frontmatter)| (*name, item_file(frontmatter)))
            .collect();
        let (_directory, store) = make_store_with_files(
            &schema,
            contents
                .iter()
                .map(|(name, content)| (*name, content.as_str()))
                .collect(),
        );
        let data = ViewData::Table(table::extract_table(view, &store, &schema));
        color_legend(view, &data, &store, &schema)
    }

    fn status_is(value: &str) -> ConditionShape {
        ConditionShape::Row {
            left: Operand::Field {
                name: "status".to_owned(),
            },
            operator: ComparisonOperator::Equal,
            right: Operand::Literal {
                value: ConditionLiteral::Text(value.to_owned()),
            },
        }
    }

    #[test]
    fn a_when_field_lists_only_the_colors_on_screen_with_their_conditions() {
        let legend = table_legend(
            STATUS_SCHEMA,
            &[
                ("a.md", "status: done\n"),
                ("b.md", "status: done\n"),
                ("c.md", "status: to_do\n"),
                ("d.md", "status: removed\n"),
            ],
            &view_of(ViewKind::Table),
        )
        .expect("a when field has a legend");

        assert_eq!(legend.field, "color");
        // Blue is in the recipe and on no item, so it is absent. Gray
        // comes from a branch and from the default: one entry.
        assert_eq!(
            legend.meaning,
            ColorMeaning::Conditions {
                subject: Some("status".to_owned()),
                entries: vec![
                    ConditionEntry {
                        color: GREEN.to_owned(),
                        count: 2,
                        conditions: vec![status_is("done")],
                        otherwise: false,
                        values: vec![Some("done".to_owned())],
                    },
                    ConditionEntry {
                        color: GRAY.to_owned(),
                        count: 2,
                        conditions: vec![status_is("removed")],
                        otherwise: true,
                        values: vec![Some("to_do".to_owned()), Some("removed".to_owned())],
                    },
                ],
                written: vec![],
            }
        );
    }

    #[test]
    fn an_entry_names_the_values_on_screen_not_the_branch_that_shares_its_color() {
        // Gray is `removed` and the default. No removed item is in the
        // view, so the entry must not read as if one were: it names
        // what the gray items are, an unset status last.
        let legend = table_legend(
            STATUS_SCHEMA,
            &[("a.md", "parent: b\n"), ("b.md", "status: to_do\n")],
            &view_of(ViewKind::Table),
        )
        .expect("legend");

        let ColorMeaning::Conditions { entries, .. } = legend.meaning else {
            panic!("a when field yields conditions");
        };
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].color, GRAY);
        assert_eq!(entries[0].values, vec![Some("to_do".to_owned()), None]);
    }
    #[test]
    fn a_hand_written_color_is_counted_apart_from_the_conditions() {
        // `b` is in progress and would derive blue; it wears a written
        // blue instead, which no condition explains.
        let legend = table_legend(
            STATUS_SCHEMA,
            &[
                ("a.md", "status: in_progress\n"),
                ("b.md", "status: in_progress\ncolor: blue\n"),
                ("c.md", "status: done\ncolor: red\n"),
            ],
            &view_of(ViewKind::Table),
        )
        .expect("legend");

        assert_eq!(
            legend.meaning,
            ColorMeaning::Conditions {
                subject: Some("status".to_owned()),
                entries: vec![ConditionEntry {
                    color: BLUE.to_owned(),
                    count: 1,
                    conditions: vec![status_is("in_progress")],
                    otherwise: false,
                    values: vec![Some("in_progress".to_owned())],
                }],
                written: vec![
                    ColorCount {
                        color: BLUE.to_owned(),
                        count: 1,
                    },
                    ColorCount {
                        color: RED.to_owned(),
                        count: 1,
                    },
                ],
            }
        );
    }

    #[test]
    fn a_condition_that_is_not_one_plain_comparison_travels_as_text() {
        let schema = "\
fields:
  effort:
    type: integer
  budget:
    type: integer
  color:
    type: color
    when:
      - if: effort * 2 > budget
        then: red
";
        let legend = table_legend(
            schema,
            &[("a.md", "effort: 5\nbudget: 8\n")],
            &view_of(ViewKind::Table),
        )
        .expect("legend");

        assert_eq!(
            legend.meaning,
            ColorMeaning::Conditions {
                subject: None,
                entries: vec![ConditionEntry {
                    color: RED.to_owned(),
                    count: 1,
                    conditions: vec![ConditionShape::Text {
                        source: "effort * 2 > budget".to_owned(),
                    }],
                    otherwise: false,
                    values: vec![],
                }],
                written: vec![],
            }
        );
    }

    #[test]
    fn a_pulled_color_is_a_hint_at_where_it_comes_from() {
        let schema = "\
fields:
  parent:
    type: link
    allow_cycles: false
  color:
    type: color
    pull:
      over: parent
      field: color
";
        let legend = table_legend(
            schema,
            &[("root.md", "color: red\n"), ("child.md", "parent: root\n")],
            &view_of(ViewKind::Table),
        )
        .expect("a pulled color has a hint");

        assert_eq!(
            legend.meaning,
            ColorMeaning::Inherited {
                over: "parent".to_owned(),
                source_field: "color".to_owned(),
            }
        );
    }

    #[test]
    fn a_color_field_without_a_recipe_has_no_legend() {
        let schema = "\
fields:
  color:
    type: color
";
        assert_eq!(
            table_legend(
                schema,
                &[("a.md", "color: red\n")],
                &view_of(ViewKind::Table)
            ),
            None
        );
    }

    #[test]
    fn an_untinted_view_has_no_legend() {
        let mut view = view_of(ViewKind::Table);
        view.display.color = Some(ColorRole::None);
        assert_eq!(
            table_legend(STATUS_SCHEMA, &[("a.md", "status: done\n")], &view),
            None
        );
    }

    #[test]
    fn a_view_whose_items_have_no_color_has_no_legend() {
        let schema = "\
fields:
  status:
    type: choice
    values: [to_do, done]
  color:
    type: color
    when:
      - if: status == \"done\"
        then: green
";
        assert_eq!(
            table_legend(
                schema,
                &[("a.md", "status: to_do\n")],
                &view_of(ViewKind::Table)
            ),
            None
        );
    }

    #[test]
    fn nested_items_of_a_tree_are_counted() {
        let schema = parse_schema(STATUS_SCHEMA).expect("schema parses");
        let (_directory, store) = make_store_with_files(
            &schema,
            vec![
                ("root.md", "---\nstatus: done\n---\n"),
                ("child.md", "---\nstatus: done\nparent: root\n---\n"),
                ("grandchild.md", "---\nstatus: done\nparent: child\n---\n"),
            ],
        );
        let view = view_of(ViewKind::Tree {
            field: "parent".to_owned(),
        });
        let data = ViewData::Tree(tree::extract_tree(&view, &store, &schema));

        let legend = color_legend(&view, &data, &store, &schema).expect("legend");
        let ColorMeaning::Conditions { entries, .. } = legend.meaning else {
            panic!("a when field yields conditions");
        };
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].count, 3);
    }
}
